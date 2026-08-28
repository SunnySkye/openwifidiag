use std::collections::VecDeque;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::model::WifiNetwork;
use crate::stress::StressTest;

pub const PROBE_TARGET: &str = "1.1.1.1";
const HISTORY_LENGTH: usize = 120;
const PROBE_INTERVAL: Duration = Duration::from_secs(1);

enum ProbeResult {
    Reply(f64),
    Timeout,
    Failed(String),
}

pub struct LiveDiagnostic {
    pub target: WifiNetwork,
    signal_history: VecDeque<Option<i32>>,
    latency_history: VecDeque<Option<f64>>,
    packets_sent: u64,
    packets_received: u64,
    last_seen: Option<Instant>,
    last_probe: Option<Instant>,
    probe_rx: Option<Receiver<ProbeResult>>,
    pub probe_error: Option<String>,
    /// Optional downstream stress load running alongside the probes.
    pub stress: StressTest,
}

impl LiveDiagnostic {
    pub fn new(target: WifiNetwork) -> Self {
        let mut signal_history = VecDeque::new();
        signal_history.push_back(Some(target.rssi));
        Self {
            target,
            signal_history,
            latency_history: VecDeque::new(),
            packets_sent: 0,
            packets_received: 0,
            last_seen: Some(Instant::now()),
            last_probe: None,
            probe_rx: None,
            probe_error: None,
            stress: StressTest::new(),
        }
    }

    pub fn on_tick(&mut self) {
        self.poll_probe();
        self.stress.on_tick();
        let probe_due = self
            .last_probe
            .map(|last| last.elapsed() >= PROBE_INTERVAL)
            .unwrap_or(true);
        if self.probe_rx.is_none() && probe_due {
            self.start_probe();
        }
    }

    pub fn record_scan(&mut self, network: Option<&WifiNetwork>) {
        if let Some(network) = network {
            self.target.channel = network.channel;
            self.target.band = network.band;
            self.target.rssi = network.rssi;
            self.last_seen = Some(Instant::now());
            push_bounded(&mut self.signal_history, Some(network.rssi));
        } else {
            push_bounded(&mut self.signal_history, None);
        }
    }

    pub fn signal_data(&self) -> Vec<u64> {
        self.signal_history
            .iter()
            .map(|sample| {
                sample
                    .map(|rssi| (rssi.clamp(-90, -30) + 90) as u64)
                    .unwrap_or(0)
            })
            .collect()
    }

    pub fn latency_data(&self) -> Vec<u64> {
        self.latency_history
            .iter()
            .map(|sample| sample.map(|ms| ms.round().max(1.0) as u64).unwrap_or(0))
            .collect()
    }

    pub fn current_signal(&self) -> Option<i32> {
        self.signal_history.back().copied().flatten()
    }

    pub fn signal_stats(&self) -> Option<(i32, i32, f64)> {
        let samples: Vec<i32> = self.signal_history.iter().flatten().copied().collect();
        if samples.is_empty() {
            return None;
        }
        let min = *samples.iter().min()?;
        let max = *samples.iter().max()?;
        let average = samples.iter().map(|value| *value as f64).sum::<f64>() / samples.len() as f64;
        Some((min, max, average))
    }

    pub fn current_latency(&self) -> Option<f64> {
        self.latency_history.back().copied().flatten()
    }

    pub fn latency_stats(&self) -> Option<(f64, f64, f64)> {
        let samples: Vec<f64> = self.latency_history.iter().flatten().copied().collect();
        if samples.is_empty() {
            return None;
        }
        let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
        let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let average = samples.iter().sum::<f64>() / samples.len() as f64;
        Some((min, max, average))
    }

    pub fn packets(&self) -> (u64, u64) {
        (self.packets_sent, self.packets_received)
    }

    pub fn packet_loss(&self) -> f64 {
        if self.packets_sent == 0 {
            return 0.0;
        }
        (self.packets_sent - self.packets_received) as f64 * 100.0 / self.packets_sent as f64
    }

    pub fn seconds_since_seen(&self) -> Option<u64> {
        self.last_seen.map(|seen| seen.elapsed().as_secs())
    }

    pub fn probing(&self) -> bool {
        self.probe_rx.is_some()
    }

    fn start_probe(&mut self) {
        let (tx, rx) = channel();
        self.probe_rx = Some(rx);
        self.last_probe = Some(Instant::now());
        thread::spawn(move || {
            let _ = tx.send(ping_once(PROBE_TARGET));
        });
    }

    fn poll_probe(&mut self) {
        let result = self.probe_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(result) = result else {
            return;
        };
        self.probe_rx = None;
        match result {
            ProbeResult::Reply(milliseconds) => {
                self.packets_sent += 1;
                self.packets_received += 1;
                self.probe_error = None;
                push_bounded(&mut self.latency_history, Some(milliseconds));
            }
            ProbeResult::Timeout => {
                self.packets_sent += 1;
                self.probe_error = None;
                push_bounded(&mut self.latency_history, None);
            }
            ProbeResult::Failed(error) => {
                self.probe_error = Some(error);
            }
        }
    }
}

fn push_bounded<T>(history: &mut VecDeque<T>, value: T) {
    if history.len() == HISTORY_LENGTH {
        history.pop_front();
    }
    history.push_back(value);
}

fn ping_once(target: &str) -> ProbeResult {
    let mut command = Command::new("ping");
    #[cfg(target_os = "windows")]
    command.args(["-n", "1", "-w", "1000", target]);
    #[cfg(target_os = "macos")]
    command.args(["-n", "-c", "1", "-W", "1000", target]);
    #[cfg(all(unix, not(target_os = "macos")))]
    command.args(["-n", "-c", "1", "-W", "1", target]);

    let output = match command.output() {
        Ok(output) => output,
        Err(error) => return ProbeResult::Failed(format!("failed to run ping: {error}")),
    };
    if !output.status.success() {
        return ProbeResult::Timeout;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ping_latency(&stdout)
        .map(ProbeResult::Reply)
        .unwrap_or_else(|| ProbeResult::Failed("ping replied without a readable latency".into()))
}

fn parse_ping_latency(output: &str) -> Option<f64> {
    let marker = output.find("time=").or_else(|| output.find("time<"))?;
    let value = &output[marker + 5..];
    let number: String = value
        .chars()
        .skip_while(|character| character.is_whitespace())
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    number.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_ping_latency;

    #[test]
    fn parses_unix_ping_latency() {
        assert_eq!(parse_ping_latency("64 bytes: time=12.345 ms"), Some(12.345));
    }

    #[test]
    fn parses_windows_sub_millisecond_latency() {
        assert_eq!(parse_ping_latency("Reply: time<1ms TTL=57"), Some(1.0));
    }
}
