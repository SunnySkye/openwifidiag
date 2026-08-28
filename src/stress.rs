//! Downstream stress test: saturates the link with parallel downloads while
//! the live diagnostic keeps sampling, so latent problems (bufferbloat, loss,
//! driver drops) surface under load.
//!
//! Each worker repeatedly downloads a fixed-size chunk from Cloudflare's
//! speed-test endpoint via `curl` (shipped with macOS, most Linux distros and
//! Windows 10+), discards the payload, and reports the transferred bytes.

use std::collections::VecDeque;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const STRESS_URL: &str = "https://speed.cloudflare.com/__down?bytes=10485760";
pub const STRESS_HOST: &str = "speed.cloudflare.com";
pub const STRESS_WORKERS: usize = 4;
const HISTORY_LENGTH: usize = 120;
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub struct StressTest {
    running: Option<Arc<AtomicBool>>,
    bytes: Option<Arc<AtomicU64>>,
    worker_error: Option<Arc<Mutex<Option<String>>>>,
    workers: Vec<JoinHandle<()>>,
    started: Option<Instant>,
    last_sample: Option<(Instant, u64)>,
    throughput_history: VecDeque<Option<u64>>,
    last_error: Option<String>,
}

impl StressTest {
    pub fn new() -> Self {
        StressTest {
            running: None,
            bytes: None,
            worker_error: None,
            workers: Vec::new(),
            started: None,
            last_sample: None,
            throughput_history: VecDeque::new(),
            last_error: None,
        }
    }

    pub fn start(&mut self) {
        if self.running() {
            return;
        }
        let running = Arc::new(AtomicBool::new(true));
        let bytes = Arc::new(AtomicU64::new(0));
        let error = Arc::new(Mutex::new(None));
        self.workers = (0..STRESS_WORKERS)
            .map(|_| {
                let running = Arc::clone(&running);
                let bytes = Arc::clone(&bytes);
                let error = Arc::clone(&error);
                thread::spawn(move || worker(running, bytes, error))
            })
            .collect();
        self.running = Some(running);
        self.bytes = Some(bytes);
        self.worker_error = Some(error);
        self.started = Some(Instant::now());
        self.last_sample = None;
        self.throughput_history.clear();
    }

    pub fn stop(&mut self) {
        if let Some(running) = &self.running {
            running.store(false, Ordering::Relaxed);
        }
        // Detach: workers exit after their current chunk finishes; we never
        // block the UI waiting on an in-flight download.
        self.workers.clear();
    }

    pub fn toggle(&mut self) {
        if self.running() {
            self.stop();
        } else {
            self.start();
        }
    }

    pub fn running(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    pub fn on_tick(&mut self) {
        if let Some(error) = &self.worker_error {
            if let Ok(guard) = error.lock() {
                if guard.is_some() {
                    self.last_error = guard.clone();
                }
            }
        }
        if !self.running() {
            return;
        }
        let Some((last_time, last_bytes)) = self.last_sample else {
            self.last_sample = Some((Instant::now(), self.total_bytes()));
            return;
        };
        if last_time.elapsed() < SAMPLE_INTERVAL {
            return;
        }
        let total = self.total_bytes();
        let mbps = (total.saturating_sub(last_bytes)) as f64 * 8.0 / last_time.elapsed().as_secs_f64()
            / 1e6;
        push_bounded(&mut self.throughput_history, Some(mbps.round().max(1.0) as u64));
        self.last_sample = Some((Instant::now(), total));
    }

    pub fn error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn total_bytes(&self) -> u64 {
        self.bytes
            .as_ref()
            .map(|bytes| bytes.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn elapsed_secs(&self) -> Option<u64> {
        self.started.map(|started| started.elapsed().as_secs())
    }

    pub fn current_mbps(&self) -> Option<u64> {
        self.throughput_history.back().copied().flatten()
    }

    pub fn average_mbps(&self) -> Option<f64> {
        let elapsed = self.elapsed_secs()?;
        if elapsed == 0 {
            return None;
        }
        Some(self.total_bytes() as f64 * 8.0 / elapsed as f64 / 1e6)
    }

    pub fn throughput_data(&self) -> Vec<u64> {
        self.throughput_history
            .iter()
            .map(|sample| sample.unwrap_or(0))
            .collect()
    }
}

impl Drop for StressTest {
    fn drop(&mut self) {
        self.stop();
    }
}

fn worker(running: Arc<AtomicBool>, bytes: Arc<AtomicU64>, error: Arc<Mutex<Option<String>>>) {
    let mut command = Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--output",
        null_device(),
        "--write-out",
        "%{size_download}",
        "--max-time",
        "120",
        STRESS_URL,
    ]);
    while running.load(Ordering::Relaxed) {
        match command.output() {
            Ok(output) => {
                let size = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
                    .filter(|_| output.status.success());
                match size {
                    Some(size) => {
                        bytes.fetch_add(size, Ordering::Relaxed);
                    }
                    None => {
                        if let Ok(mut guard) = error.lock() {
                            let detail = String::from_utf8_lossy(&output.stderr);
                            *guard = Some(if detail.trim().is_empty() {
                                format!("stress download failed (exit {})", output.status)
                            } else {
                                format!("stress download failed: {}", detail.trim())
                            });
                        }
                        thread::sleep(Duration::from_millis(500));
                    }
                }
            }
            Err(_) => {
                if let Ok(mut guard) = error.lock() {
                    *guard = Some("curl is required for the stress test but was not found".into());
                }
                break;
            }
        }
    }
}

fn null_device() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "NUL"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "/dev/null"
    }
}

fn push_bounded(history: &mut VecDeque<Option<u64>>, value: Option<u64>) {
    if history.len() == HISTORY_LENGTH {
        history.pop_front();
    }
    history.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded() {
        let mut history = VecDeque::new();
        for index in 0..(HISTORY_LENGTH as u64 + 10) {
            push_bounded(&mut history, Some(index));
        }
        assert_eq!(history.len(), HISTORY_LENGTH);
        assert_eq!(history.back().copied().flatten(), Some(HISTORY_LENGTH as u64 + 9));
    }

    #[test]
    fn average_needs_elapsed_time() {
        let mut stress = StressTest::new();
        assert_eq!(stress.average_mbps(), None);
        stress.started = Instant::now().checked_sub(Duration::from_secs(10));
        assert_eq!(stress.average_mbps(), Some(0.0));
    }
}
