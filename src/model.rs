use std::fmt;

use serde::Serialize;

/// Security protocol of a network.
#[derive(Clone, Debug, Serialize)]
pub enum Security {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    /// We could only determine "encrypted" vs "open" (e.g. CoreWLAN API).
    Encrypted,
    Unknown,
}

impl Security {
    pub fn label(&self) -> &'static str {
        match self {
            Security::Open => "Open",
            Security::Wep => "WEP",
            Security::Wpa => "WPA",
            Security::Wpa2 => "WPA2",
            Security::Wpa3 => "WPA3",
            Security::Encrypted => "Encrypted",
            Security::Unknown => "?",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum Band {
    TwoGhz,
    FiveGhz,
    SixGhz,
    Unknown,
}

impl Band {
    pub fn from_freq(freq_mhz: Option<f64>, channel: Option<u32>) -> Self {
        if let Some(f) = freq_mhz {
            if f < 3000.0 {
                return Band::TwoGhz;
            }
            if f > 6000.0 {
                return Band::SixGhz;
            }
            return Band::FiveGhz;
        }
        match channel {
            Some(c) if c <= 14 => Band::TwoGhz,
            Some(c) if (32..=233).contains(&c) => Band::FiveGhz,
            _ => Band::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Band::TwoGhz => "2.4",
            Band::FiveGhz => "5",
            Band::SixGhz => "6",
            Band::Unknown => "?",
        }
    }
}

/// One discovered access point (one row of the table).
#[derive(Clone, Debug, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: String,
    pub channel: Option<u32>,
    pub band: Band,
    /// Signal strength in dBm (negative; closer to 0 is stronger).
    pub rssi: i32,
    pub security: Security,
}

impl WifiNetwork {
    pub fn new(ssid: String, bssid: String, channel: Option<u32>, rssi: i32, security: Security) -> Self {
        let band = Band::from_freq(None, channel);
        WifiNetwork { ssid, bssid, channel, band, rssi, security }
    }
}

impl fmt::Display for Security {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
