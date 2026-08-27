//! Pure parsers for the various platform CLI tools (`iw`/`nmcli` on Linux,
//! `airport` on legacy macOS, `netsh` on Windows). Kept platform-agnostic so
//! parsers can be unit-tested anywhere.
#![allow(dead_code)] // backends used vary per target platform

use crate::model::{Security, WifiNetwork};

fn classify_security(flags: &SecurityFlags, capability_privacy: bool) -> Security {
    let mut result = match (flags.rsn, flags.wpa) {
        (true, _) => Security::Wpa2,
        (false, true) => Security::Wpa,
        (false, false) => {
            if capability_privacy {
                Security::Wep
            } else {
                Security::Open
            }
        }
    };
    if flags.rsn && flags.sae {
        result = Security::Wpa3;
    }
    result
}

#[derive(Default)]
struct SecurityFlags {
    wpa: bool,
    rsn: bool,
    sae: bool,
}

/// Parse `iw dev <iface> scan` output.
pub fn parse_iw(raw: &str) -> Vec<WifiNetwork> {
    let mut networks = Vec::new();
    let mut cur: Option<IwAccum> = None;
    for line in raw.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("BSS ") {
            if let Some(acc) = cur.take() {
                networks.push(acc.build());
            }
            let bssid = rest.split(['(', ' ']).next().unwrap_or("").to_owned();
            cur = Some(IwAccum { bssid, ..Default::default() });
            continue;
        }
        let acc = match cur.as_mut() {
            Some(a) => a,
            None => continue,
        };
        if let Some(f) = line.strip_prefix("freq:") {
            acc.freq = f.trim().parse::<f64>().ok();
        } else if let Some(s) = line.strip_prefix("signal:") {
            acc.signal = s.trim().trim_end_matches(" dBm").parse::<f32>().ok();
        } else if let Some(ss) = line.strip_prefix("SSID:") {
            acc.ssid = ss.trim().to_owned();
        } else if line.contains("capability:") {
            acc.privacy = line.contains("Privacy");
        } else if line.starts_with("RSN     *") {
            acc.flags.rsn = true;
        } else if line.starts_with("WPA     *") {
            acc.flags.wpa = true;
        }
        if line.contains("SAE") || line.contains("* CCMP") && line.contains("PSK") && line.contains("SAE") {
            acc.flags.sae = true;
        }
    }
    if let Some(acc) = cur.take() {
        networks.push(acc.build());
    }
    networks
}

#[derive(Default)]
struct IwAccum {
    bssid: String,
    ssid: String,
    freq: Option<f64>,
    signal: Option<f32>,
    privacy: bool,
    flags: SecurityFlags,
}

impl IwAccum {
    fn build(self) -> WifiNetwork {
        let (channel, band_from_freq) = freq_to_channel(self.freq);
        let security = classify_security(&self.flags, self.privacy);
        WifiNetwork {
            ssid: if self.ssid.is_empty() { "<hidden>".into() } else { self.ssid },
            bssid: self.bssid,
            channel,
            band: band_from_freq,
            rssi: self.signal.unwrap_or(-100.0) as i32,
            security,
        }
    }
}

fn freq_to_channel(freq: Option<f64>) -> (Option<u32>, crate::model::Band) {
    let f = match freq {
        Some(f) => f,
        None => return (None, crate::model::Band::Unknown),
    };
    let ch = if f < 3000.0 {
        ((f - 2407.0) / 5.0) as u32
    } else if f > 5950.0 {
        ((f - 5950.0) / 5.0) as u32
    } else {
        ((f - 5000.0) / 5.0) as u32
    };
    (Some(ch), crate::model::Band::from_freq(freq, Some(ch)))
}

/// Parse `nmcli -t -f SSID,BSSID,CHAN,SIGNAL,SECURITY dev wifi` output.
/// Fields are colon-separated; BSSID colons are escaped as `\:`.
pub fn parse_nmcli(raw: &str) -> Vec<WifiNetwork> {
    raw.lines()
        .filter_map(|line| {
            if line.trim().is_empty() {
                return None;
            }
            // split on non-escaped colons
            let mut parts = Vec::new();
            let mut buf = String::new();
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    buf.push(c);
                    if let Some(n) = chars.next() {
                        buf.push(n);
                    }
                } else if c == ':' {
                    parts.push(std::mem::take(&mut buf));
                } else {
                    buf.push(c);
                }
            }
            parts.push(buf);
            if parts.len() < 4 {
                return None;
            }
            let ssid = parts[0].replace("\\:", ":");
            let bssid = parts[1].replace("\\:", ":");
            let channel = parts[2].trim().parse::<u32>().ok();
            let percent = parts[3].trim().parse::<f32>().ok();
            let security_field = parts.get(4).map(|s| s.as_str()).unwrap_or("");
            let rssi = percent.map(|p| (p as i32 / 2) - 100).unwrap_or(-100);
            let security = nmcli_security(security_field);
            Some(WifiNetwork::new(ssid, bssid, channel, rssi, security))
        })
        .collect()
}

fn nmcli_security(field: &str) -> Security {
    let up = field.to_uppercase();
    if up.contains("WPA3") {
        Security::Wpa3
    } else if up.contains("WPA2") {
        Security::Wpa2
    } else if up.contains("WPA") {
        Security::Wpa
    } else if up.contains("WEP") {
        Security::Wep
    } else if up.trim().is_empty() || up == "--" {
        Security::Open
    } else {
        Security::Unknown
    }
}

/// Parse `netsh wlan show networks mode=bssid` output (English locale).
pub fn parse_netsh(raw: &str) -> Vec<WifiNetwork> {
    let mut networks = Vec::new();
    let mut cur: Option<NetshAccum> = None;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("SSID ") && t.contains(':') {
            if let Some(acc) = cur.take() {
                networks.push(acc.build());
            }
            let name = t.splitn(2, ':').nth(1).unwrap_or("").trim().to_owned();
            cur = Some(NetshAccum { ssid: name, ..Default::default() });
            continue;
        }
        let acc = match cur.as_mut() {
            Some(a) => a,
            None => continue,
        };
        if let Some(v) = split_value(t, "Authentication") {
            acc.security = netsh_security(&v);
        } else if t.starts_with("BSSID ") {
            acc.bssid = t.splitn(2, ':').nth(1).unwrap_or("").trim().to_owned();
        } else if let Some(v) = split_value(t, "Signal") {
            let pct = v.trim_end_matches('%').trim().parse::<i32>().ok();
            acc.rssi = pct.map(|p| p / 2 - 100).unwrap_or(-100);
        } else if let Some(v) = split_value(t, "Channel") {
            acc.channel = v.trim().parse::<u32>().ok();
        }
    }
    if let Some(acc) = cur.take() {
        networks.push(acc.build());
    }
    networks
}

fn split_value(line: &str, key: &str) -> Option<String> {
    line.strip_prefix(key)
        .map(|rest| rest.trim())
        .and_then(|rest| rest.strip_prefix(':'))
        .map(|v| v.trim().to_owned())
}

fn netsh_security(auth: &str) -> Security {
    let up = auth.to_uppercase();
    if up.contains("WPA3") {
        Security::Wpa3
    } else if up.contains("WPA2") {
        Security::Wpa2
    } else if up.contains("WPA") {
        Security::Wpa
    } else if up.contains("WEP") {
        Security::Wep
    } else if up.contains("OPEN") || up.contains("NONE") {
        Security::Open
    } else {
        Security::Unknown
    }
}

#[derive(Default)]
struct NetshAccum {
    ssid: String,
    bssid: String,
    channel: Option<u32>,
    rssi: i32,
    security: Security,
}

impl Default for Security {
    fn default() -> Self {
        Security::Unknown
    }
}

impl NetshAccum {
    fn build(self) -> WifiNetwork {
        let ssid = if self.ssid.is_empty() { "<hidden>".into() } else { self.ssid };
        WifiNetwork {
            ssid,
            bssid: self.bssid,
            channel: self.channel,
            band: crate::model::Band::from_freq(None, self.channel),
            rssi: self.rssi,
            security: self.security,
        }
    }
}

/// Parse legacy `airport -s` output (pre-modern-macOS).
pub fn parse_airport(raw: &str) -> Vec<WifiNetwork> {
    raw.lines()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let bssid_idx = tokens.iter().position(|t| is_bssid(t))?;
            let ssid = tokens[..bssid_idx].join(" ");
            let rest = &tokens[bssid_idx + 1..];
            let rssi = rest.first().and_then(|t| t.parse::<i32>().ok()).unwrap_or(-100);
            let channel = rest.get(1).and_then(|t| {
                t.trim_matches(['+', ','])
                    .split('+')
                    .next()
                    .unwrap_or("")
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .parse::<u32>()
                    .ok()
            });
            let sec_field = rest.get(2..).map(|s| s.join(" ")).unwrap_or_default();
            Some(WifiNetwork::new(ssid, tokens[bssid_idx].to_owned(), channel, rssi, airport_security(&sec_field)))
        })
        .collect()
}

fn airport_security(field: &str) -> Security {
    let up = field.to_uppercase();
    if up.contains("WPA3") {
        Security::Wpa3
    } else if up.contains("WPA2") {
        Security::Wpa2
    } else if up.contains("WPA") {
        Security::Wpa
    } else if up.contains("WEP") {
        Security::Wep
    } else if up.contains("NONE") || up.contains("OPEN") || up.trim().is_empty() {
        Security::Open
    } else {
        Security::Encrypted
    }
}

fn is_bssid(t: &str) -> bool {
    let parts: Vec<&str> = t.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iw_sample() {
        let sample = r#"
BSS 5c:ce:25:c7:cf:58(on wlan0)
	TSF: 7012281004 usec (0d,00:11:42)
	freq: 5180
	beacon interval: 100 TUs
	capability: 0x1111
	signal: -50.00 dBm
	last seen: 852 ms ago
	SSID: CoffeeNet
	RSN     * Version: 1
		 * Group cipher: CCMP
		 * Pairwise ciphers: CCMP
		 * Authentication suites: PSK
		 * Capabilities: 1-PTKSA-RC 1-GPSA-RC (0x000c)
BSS ab:12:34:56:78:9a(on wlan0)
	freq: 2462
	signal: -71.00 dBm
	capability: 0x0421
	SSID: OpenCafe
BSS ff:ee:dd:cc:bb:aa(on wlan0)
	freq: 2462
	signal: -33.00 dBm
	capability: 0x0431 Privacy
	WPA     * Version: 1
"#;
        let nets = parse_iw(sample);
        assert_eq!(nets.len(), 3);
        assert_eq!(nets[0].ssid, "CoffeeNet");
        assert_eq!(nets[0].bssid, "5c:ce:25:c7:cf:58");
        assert_eq!(nets[0].channel, Some(36));
        assert_eq!(nets[0].rssi, -50);
        assert!(matches!(nets[0].security, Security::Wpa2));
        assert!(matches!(nets[1].security, Security::Open));
        assert!(matches!(nets[2].security, Security::Wpa));
    }

    #[test]
    fn nmcli_sample() {
        let sample = "CoffeeNet:5c\\:ce\\:25\\:c7\\:cf\\:58:36:100:WPA2\nPub:aa\\:bb\\:cc\\:dd\\:ee\\:ff:11:84:\n";
        let nets = parse_nmcli(sample);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].ssid, "CoffeeNet");
        assert_eq!(nets[0].bssid, "5c:ce:25:c7:cf:58");
        assert_eq!(nets[0].channel, Some(36));
        assert_eq!(nets[0].rssi, -50);
        assert!(matches!(nets[0].security, Security::Wpa2));
        assert!(matches!(nets[1].security, Security::Open));
        assert_eq!(nets[1].rssi, -58);
    }

    #[test]
    fn netsh_sample() {
        let sample = r#"
Interface name : Wi-Fi
There are 2 networks currently visible.

SSID 1 : CoffeeNet
    Network type            : Infrastructure
    Authentication          : WPA2-Personal
    Encryption              : CCMP
    BSSID 1                 : 5c:ce:25:c7:cf:58
         Signal             : 100%
         Channel            : 36

SSID 2 : Pub
    Network type            : Infrastructure
    Authentication          : Open
    Encryption              : None
    BSSID 1                 : aa:bb:cc:dd:ee:ff
         Signal             : 68%
         Channel            : 11
"#;
        let nets = parse_netsh(sample);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].ssid, "CoffeeNet");
        assert_eq!(nets[0].rssi, -50);
        assert!(matches!(nets[0].security, Security::Wpa2));
        assert_eq!(nets[0].channel, Some(36));
        assert!(matches!(nets[1].security, Security::Open));
        assert_eq!(nets[1].rssi, -66);
    }

    #[test]
    fn airport_sample() {
        let sample = r#"
                            SSID BSSID             RSSI CHANNEL HT CC SECURITY (auth/unicast/group)
                     CoffeeNet 5c:ce:25:c7:cf:58 -50  36      Y  -- WPA2(PSK)/AES/AES
                       OpenPub aa:bb:cc:dd:ee:ff -71  11      N  US NONE
"#;
        let nets = parse_airport(sample);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].ssid, "CoffeeNet");
        assert_eq!(nets[0].rssi, -50);
        assert_eq!(nets[0].channel, Some(36));
        assert!(matches!(nets[0].security, Security::Wpa2));
        assert!(matches!(nets[1].security, Security::Open));
    }
}
