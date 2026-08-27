use std::process::Command;

use anyhow::{anyhow, Context, Result};

use super::parsers;
use super::Scanner;
use crate::model::WifiNetwork;

pub struct IwScanner {
    iface: Option<String>,
    prefer_nmcli: bool,
}

impl IwScanner {
    pub fn new(iface: Option<String>) -> Self {
        let prefer_nmcli = !command_exists("iw") && command_exists("nmcli");
        IwScanner { iface, prefer_nmcli }
    }

    fn scan_with_iw(&self) -> Result<Vec<WifiNetwork>> {
        let iface = match &self.iface {
            Some(i) => i.clone(),
            None => detect_iw_interface()?,
        };
        let out = Command::new("iw")
            .args(["dev", &iface, "scan"])
            .output()
            .context("failed to run `iw`")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!(
                "`iw dev {} scan` failed: {}. Scanning usually needs CAP_NET_ADMIN — try `sudo openwifidiag`.",
                iface,
                stderr.trim()
            ));
        }
        Ok(parsers::parse_iw(&String::from_utf8_lossy(&out.stdout)))
    }

    fn scan_with_nmcli(&self) -> Result<Vec<WifiNetwork>> {
        let out = Command::new("nmcli")
            .args(["-t", "-f", "SSID,BSSID,CHAN,SIGNAL,SECURITY", "dev", "wifi"])
            .output()
            .context("failed to run `nmcli`")?;
        if !out.status.success() {
            return Err(anyhow!("`nmcli dev wifi` failed: {}", String::from_utf8_lossy(&out.stderr)));
        }
        Ok(parsers::parse_nmcli(&String::from_utf8_lossy(&out.stdout)))
    }
}

impl Scanner for IwScanner {
    fn scan(&self) -> Result<Vec<WifiNetwork>> {
        if self.prefer_nmcli {
            self.scan_with_nmcli()
        } else {
            self.scan_with_iw()
        }
    }

    fn backend_name(&self) -> &str {
        if self.prefer_nmcli { "nmcli" } else { "iw" }
    }
}

fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn detect_iw_interface() -> Result<String> {
    let out = Command::new("iw").arg("dev").output().context("failed to run `iw dev`")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let iface = stdout
        .lines()
        .filter_map(|l| l.trim().strip_prefix("Interface ").map(str::to_owned))
        .next();
    iface.ok_or_else(|| anyhow!("no WiFi interface found via `iw dev`"))
}
