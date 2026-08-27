use anyhow::Result;

use crate::model::WifiNetwork;

/// A source that can scan for near-by WiFi networks.
pub trait Scanner {
    /// Perform one scan. Returns the list of networks found.
    fn scan(&self) -> Result<Vec<WifiNetwork>>;
    /// Human-readable name of the backend (shown in the UI header).
    fn backend_name(&self) -> &str;
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;
mod parsers;

#[cfg(target_os = "macos")]
pub fn platform_scanner(iface: Option<&str>) -> Result<Box<dyn Scanner>> {
    macos::create_scanner(iface)
}

#[cfg(target_os = "linux")]
pub fn platform_scanner(iface: Option<&str>) -> Result<Box<dyn Scanner>> {
    Ok(Box::new(linux::IwScanner::new(iface.map(str::to_owned))))
}

#[cfg(target_os = "windows")]
pub fn platform_scanner(_iface: Option<&str>) -> Result<Box<dyn Scanner>> {
    Ok(Box::new(windows::NetshScanner::new()))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn platform_scanner(_iface: Option<&str>) -> Result<Box<dyn Scanner>> {
    anyhow::bail!("unsupported platform — only macOS, Linux and Windows are supported")
}
