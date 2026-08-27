use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::diagnostics::LiveDiagnostic;
use crate::model::WifiNetwork;
use crate::scanner::platform_scanner;

/// Result of a background scan: backend name plus networks, or an error string.
pub enum ScanEvent {
    Done(Result<(String, Vec<WifiNetwork>), String>),
}

pub enum SortMode {
    Signal,
    Ssid,
    Channel,
    Security,
}

impl SortMode {
    pub fn label(&self) -> &'static str {
        match self {
            SortMode::Signal => "signal",
            SortMode::Ssid => "ssid",
            SortMode::Channel => "channel",
            SortMode::Security => "security",
        }
    }

    pub fn next(&self) -> SortMode {
        match self {
            SortMode::Signal => SortMode::Ssid,
            SortMode::Ssid => SortMode::Channel,
            SortMode::Channel => SortMode::Security,
            SortMode::Security => SortMode::Signal,
        }
    }
}

pub struct App {
    pub networks: Vec<WifiNetwork>,
    pub selected: usize,
    pub backend: String,
    pub last_error: Option<String>,
    pub advisory: Option<String>,
    pub sort: SortMode,
    pub interval: Duration,
    pub last_scan: Option<Instant>,
    pub scanning: bool,
    pub spinner_tick: usize,
    pub diagnostic: Option<LiveDiagnostic>,
    rx: Option<Receiver<ScanEvent>>,
    iface: Option<String>,
    pub should_quit: bool,
}

impl App {
    pub fn new(interval: Duration, sort: SortMode, iface: Option<String>) -> Self {
        App {
            networks: Vec::new(),
            selected: 0,
            backend: "…".into(),
            last_error: None,
            advisory: None,
            sort,
            interval,
            last_scan: None,
            scanning: false,
            spinner_tick: 0,
            diagnostic: None,
            rx: None,
            iface,
            should_quit: false,
        }
    }

    pub fn start_scan(&mut self) {
        if self.scanning {
            return;
        }
        let iface = self.iface.clone();
        let (tx, rx) = channel();
        self.scanning = true;
        self.last_scan = Some(Instant::now());
        self.rx = Some(rx);
        thread::spawn(move || {
            let result = match platform_scanner(iface.as_deref()) {
                Ok(scanner) => {
                    let name = scanner.backend_name().to_owned();
                    scanner
                        .scan()
                        .map(|nets| (name, nets))
                        .map_err(|e| format!("{:#}", e))
                }
                Err(e) => Err(format!("{:#}", e)),
            };
            let _ = tx.send(ScanEvent::Done(result));
        });
    }

    /// Poll the scan channel; integrates results.
    pub fn poll(&mut self) {
        let done = self.rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(ScanEvent::Done(res)) => Some(res),
            Err(_) => None,
        });
        if let Some(res) = done {
            self.scanning = false;
            self.rx = None;
            match res {
                Ok((backend, nets)) => {
                    self.backend = backend;
                    self.last_error = None;
                    self.apply(nets);
                }
                Err(e) => {
                    self.last_error = Some(e);
                }
            }
        }
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
    }

    fn apply(&mut self, nets: Vec<WifiNetwork>) {
        // macOS/Location-Services advisory: all SSIDs redacted.
        if self.backend == "CoreWLAN" && !nets.is_empty() && nets.iter().all(|n| n.ssid == "<hidden>") {
            self.advisory = Some(
                "macOS redacts SSIDs without Location Services — allow openwifidiag in Privacy & Security > Location Services, then press r.".into(),
            );
        } else {
            self.advisory = None;
        }
        if let Some(diagnostic) = &mut self.diagnostic {
            let target = if diagnostic.target.bssid.is_empty() {
                nets.iter().find(|network| {
                    diagnostic.target.ssid != "<hidden>" && network.ssid == diagnostic.target.ssid
                })
            } else {
                nets.iter().find(|network| {
                    network
                        .bssid
                        .eq_ignore_ascii_case(&diagnostic.target.bssid)
                })
            };
            diagnostic.record_scan(target);
        }
        self.networks = nets;
        self.sort();
        if self.selected >= self.networks.len() {
            self.selected = self.networks.len().saturating_sub(1);
        }
    }

    pub fn sort(&mut self) {
        match self.sort {
            SortMode::Signal => self.networks.sort_by_key(|n| std::cmp::Reverse(n.rssi)),
            SortMode::Ssid => self
                .networks
                .sort_by(|a, b| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase())),
            SortMode::Channel => self.networks.sort_by_key(|n| n.channel.unwrap_or(u32::MAX)),
            SortMode::Security => self
                .networks
                .sort_by(|a, b| a.security.label().cmp(b.security.label())),
        }
    }

    pub fn on_tick(&mut self) {
        if let Some(diagnostic) = &mut self.diagnostic {
            diagnostic.on_tick();
        }
        let scan_interval = if self.diagnostic.is_some() {
            Duration::from_secs(1)
        } else {
            self.interval
        };
        if !self.scanning {
            if let Some(last) = self.last_scan {
                if last.elapsed() >= scan_interval {
                    self.start_scan();
                }
            } else {
                self.start_scan();
            }
        }
        self.poll();
    }

    pub fn countdown_secs(&self) -> Option<u64> {
        self.last_scan.map(|l| {
            self.interval.as_secs().saturating_sub(l.elapsed().as_secs())
        })
    }

    pub fn start_diagnostic(&mut self) {
        let Some(network) = self.networks.get(self.selected).cloned() else {
            return;
        };
        self.diagnostic = Some(LiveDiagnostic::new(network));
        self.last_scan = None;
    }

    pub fn stop_diagnostic(&mut self) {
        self.diagnostic = None;
        self.last_scan = None;
    }
}
