//! macOS scanners.
//!
//! Modern macOS removed the legacy `airport` CLI, so the primary backend uses
//! the private-but-stable CoreWLAN framework via raw ObjC bindings. When an
//! `airport` binary exists (older macOS), it is preferred because it reports
//! full security-suite information.

use anyhow::{anyhow, Result};
use objc::{class, msg_send, sel, sel_impl};
use objc::runtime::Object;
use serde::Deserialize;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::process::Command;

use super::Scanner;
use crate::model::{Band, Security, WifiNetwork};

const AIRPORT_PATHS: &[&str] = &[
    "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport",
    "/System/Library/PrivateFrameworks/Apple80211.framework/Resources/airport",
    "/usr/local/sbin/airport",
    "/usr/local/bin/airport",
];

type Id = *mut Object;

pub fn create_scanner(iface: Option<&str>) -> Result<Box<dyn Scanner>> {
    if let Some(path) = AIRPORT_PATHS.iter().find(|p| std::path::Path::new(p).exists()) {
        return Ok(Box::new(AirportScanner::new(
            path.to_string(),
            iface.map(str::to_owned),
        )));
    }
    Ok(Box::new(CorewlanScanner::new(iface.map(str::to_owned))))
}

#[link(name = "CoreWLAN", kind = "framework")]
extern "C" {}

#[link(name = "CoreLocation", kind = "framework")]
extern "C" {}

// Retained for the lifetime of the process. Core Location requires the
// manager to stay alive while macOS presents and records the permission.
static mut LOCATION_MANAGER: Id = std::ptr::null_mut();

/// Trigger the system Location Services prompt. The embedded Info.plist
/// (added by build.rs) supplies the text shown by macOS and gives this command
/// line executable a stable identity in Privacy & Security.
pub fn request_location_permission() {
    unsafe {
        let enabled: u8 = msg_send![class!(CLLocationManager), locationServicesEnabled];
        if enabled == 0 {
            return;
        }
        if LOCATION_MANAGER.is_null() {
            let manager: Id = msg_send![class!(CLLocationManager), new];
            LOCATION_MANAGER = manager;
        }
        let status: i64 = msg_send![LOCATION_MANAGER, authorizationStatus];
        // kCLAuthorizationStatusNotDetermined
        if status == 0 {
            let _: () = msg_send![LOCATION_MANAGER, requestWhenInUseAuthorization];
        }
    }
}

pub fn poll_location_events() {
    unsafe {
        let run_loop: Id = msg_send![class!(NSRunLoop), currentRunLoop];
        let until: Id = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.001f64];
        let _: () = msg_send![run_loop, runUntilDate: until];
    }
}

/// Used when Launch Services starts the app bundle. Keeping the process alive
/// allows macOS to display the consent sheet and record the user's response.
pub fn wait_for_location_permission() {
    request_location_permission();
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(60) {
        poll_location_events();
        let status: i64 = unsafe { msg_send![LOCATION_MANAGER, authorizationStatus] };
        if status != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Copy an NSString into an owned Rust String. Returns "" for null.
unsafe fn ns_string(ns: Id) -> String {
    if ns.is_null() {
        return String::new();
    }
    let ptr: *const c_char = msg_send![ns, UTF8String];
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

/// CoreWLAN returns NSSet on modern macOS; materialise it as NSArray.
unsafe fn all_objects(set: Id) -> Id {
    if set.is_null() {
        return std::ptr::null_mut();
    }
    msg_send![set, allObjects]
}

/// Best-effort security classification from CoreWLAN's `description`, e.g.
/// "…security=WPA2 Personal…". Falls back to supportsSecurity: check.
unsafe fn security_of(net: Id) -> Security {
    let desc: Id = msg_send![net, description];
    let text = ns_string(desc).to_uppercase();
    if text.contains("WPA3") {
        return Security::Wpa3;
    }
    if text.contains("WPA2") {
        return Security::Wpa2;
    }
    if text.contains("WPA") {
        return Security::Wpa;
    }
    if text.contains("WEP") {
        return Security::Wep;
    }
    if text.contains("OPEN") {
        return Security::Open;
    }
    let open: u8 = msg_send![net, supportsSecurity: 0u64];
    if open != 0 {
        Security::Open
    } else {
        Security::Encrypted
    }
}

// ---------------------------------------------------------------------------
// CoreWLAN scanner (primary on modern macOS)
// ---------------------------------------------------------------------------

pub struct CorewlanScanner {
    iface: Option<String>,
}

impl CorewlanScanner {
    pub fn new(iface: Option<String>) -> Self {
        CorewlanScanner { iface }
    }

    unsafe fn run_scan(&self) -> Result<Vec<WifiNetwork>> {
        // interfaceNames is an NSSet on modern macOS — convert via allObjects.
        let names_set: Id = msg_send![class!(CWInterface), interfaceNames];
        let names = all_objects(names_set);
        let ifname = match &self.iface {
            Some(n) => n.clone(),
            None => {
                let count: u64 = msg_send![names, count];
                let first: Id = if count > 0 {
                    msg_send![names, objectAtIndex: 0usize]
                } else {
                    std::ptr::null_mut()
                };
                let s = ns_string(first);
                if s.is_empty() {
                    return Err(anyhow!("no WiFi interface found (CoreWLAN reported none)"));
                }
                s
            }
        };

        let nsname: Id = msg_send![class!(NSString), alloc];
        let nsname: Id = msg_send![
            nsname,
            initWithBytes: ifname.as_ptr() as *const u8
            length: ifname.len() as u64
            encoding: 4u64 // NSUTF8StringEncoding
        ];
        let iface: Id = msg_send![class!(CWInterface), alloc];
        let iface: Id = msg_send![iface, initWithInterfaceName: nsname];
        if iface.is_null() {
            return Err(anyhow!("CoreWLAN: could not open interface `{}`", ifname));
        }

        let mut err: Id = std::ptr::null_mut();
        let nets_set: Id = msg_send![
            iface,
            scanForNetworksWithName: std::ptr::null::<Object>() as Id
            error: &mut err
        ];
        if nets_set.is_null() {
            let msg = if err.is_null() {
                "unknown error".to_owned()
            } else {
                let desc: Id = msg_send![err, localizedDescription];
                ns_string(desc)
            };
            return Err(anyhow!("CoreWLAN scan failed: {}", msg));
        }
        let nets = all_objects(nets_set);

        let count: u64 = msg_send![nets, count];
        let mut results = Vec::with_capacity(count as usize);
        for i in 0..count {
            let net: Id = msg_send![nets, objectAtIndex: i as usize];
            let ssid: Id = msg_send![net, ssid];
            let bssid: Id = msg_send![net, bssid];
            let rssi: i64 = msg_send![net, rssiValue];
            let channel: Id = msg_send![net, wlanChannel];
            let channel_num: u64 = if channel.is_null() { 0 } else { msg_send![channel, channelNumber] };
            let ssid_s = ns_string(ssid);
            let bssid_s = ns_string(bssid);
            let ch = if channel_num > 0 { Some(channel_num as u32) } else { None };
            results.push(WifiNetwork {
                ssid: if ssid_s.is_empty() { "<hidden>".to_owned() } else { ssid_s },
                bssid: bssid_s,
                channel: ch,
                band: Band::from_freq(None, ch),
                rssi: rssi as i32,
                security: security_of(net),
            });
        }
        Ok(results)
    }
}

impl Scanner for CorewlanScanner {
    fn scan(&self) -> Result<Vec<WifiNetwork>> {
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let out = objc_exception::r#try(|| self.run_scan());
            let _: () = msg_send![pool, drain];
            match out {
                Ok(Ok(res)) => {
                    if !res.is_empty() && res.iter().all(|network| network.ssid == "<hidden>") {
                        if let Ok(fallback) = scan_via_apple_swift() {
                            if fallback.iter().any(|network| network.ssid != "<hidden>") {
                                return Ok(fallback);
                            }
                        }
                    }
                    Ok(res)
                }
                Ok(Err(e)) => Err(e),
                Err(exc) => {
                    let _ = exc; // don't message the exception object; it may re-throw
                    Err(anyhow!(
                        "CoreWLAN raised an ObjC exception. On modern macOS this often means missing entitlements/permissions; try running with sudo."
                    ))
                }
            }
        }
    }

    fn backend_name(&self) -> &str {
        "CoreWLAN"
    }
}

#[derive(Deserialize)]
struct SwiftNetwork {
    ssid: String,
    bssid: String,
    rssi: i32,
    channel: u32,
    description: String,
}

fn scan_via_apple_swift() -> Result<Vec<WifiNetwork>> {
    let output = Command::new("xcrun")
        .args(["swift", "-e", include_str!("../../resources/macos/scan.swift")])
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "Apple Swift CoreWLAN fallback failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let rows: Vec<SwiftNetwork> = serde_json::from_slice(&output.stdout)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let channel = (row.channel > 0).then_some(row.channel);
            WifiNetwork {
                ssid: row.ssid,
                bssid: row.bssid,
                channel,
                band: Band::from_freq(None, channel),
                rssi: row.rssi,
                security: security_from_text(&row.description),
            }
        })
        .collect())
}

fn security_from_text(text: &str) -> Security {
    let text = text.to_uppercase();
    if text.contains("WPA3") {
        Security::Wpa3
    } else if text.contains("WPA2") {
        Security::Wpa2
    } else if text.contains("WPA") {
        Security::Wpa
    } else if text.contains("WEP") {
        Security::Wep
    } else if text.contains("NONE") || text.contains("OPEN") {
        Security::Open
    } else {
        Security::Encrypted
    }
}

// ---------------------------------------------------------------------------
// airport fallback (legacy macOS)
// ---------------------------------------------------------------------------

pub struct AirportScanner {
    path: String,
    iface: Option<String>,
}

impl AirportScanner {
    pub fn new(path: String, iface: Option<String>) -> Self {
        AirportScanner { path, iface }
    }
}

impl Scanner for AirportScanner {
    fn scan(&self) -> Result<Vec<WifiNetwork>> {
        let mut cmd = std::process::Command::new(&self.path);
        if let Some(i) = &self.iface {
            cmd.arg("-i").arg(i);
        }
        cmd.arg("-s");
        let out = cmd.output()?;
        if !out.status.success() {
            return Err(anyhow!(
                "`airport -s` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(super::parsers::parse_airport(&String::from_utf8_lossy(&out.stdout)))
    }

    fn backend_name(&self) -> &str {
        "airport"
    }
}
