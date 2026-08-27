use std::process::Command;

use anyhow::{anyhow, Context, Result};

use super::parsers;
use super::Scanner;
use crate::model::WifiNetwork;

/// Scanner based on `netsh wlan show networks mode=bssid`.
///
/// Caveat: `netsh` field labels are localized; the parser targets English
/// output. Replacing this with the Windows Native WiFi API (WlanGetNetworkBssList)
/// is a planned improvement.
pub struct NetshScanner;

impl NetshScanner {
    pub fn new() -> Self {
        NetshScanner
    }
}

impl Scanner for NetshScanner {
    fn scan(&self) -> Result<Vec<WifiNetwork>> {
        let out = Command::new("netsh")
            .args(["wlan", "show", "networks", "mode=bssid"])
            .output()
            .context("failed to run `netsh`")?;
        if !out.status.success() {
            return Err(anyhow!("`netsh wlan show networks` failed — is the WLAN service running?"));
        }
        // netsh writes codepage-dependent output; use lossy conversion.
        Ok(parsers::parse_netsh(&String::from_utf8_lossy(&out.stdout)))
    }

    fn backend_name(&self) -> &str {
        "netsh"
    }
}
