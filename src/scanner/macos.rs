//! macOS scanners.
//!
//! Modern macOS removed the legacy `airport` CLI, so the primary backend uses
//! the private-but-stable CoreWLAN framework via raw ObjC bindings. When an
//! `airport` binary exists (older macOS), it is preferred because it reports
//! full security-suite information.

use anyhow::{anyhow, Result};
use objc::{class, msg_send, sel, sel_impl};
use objc::runtime::Object;
use std::ffi::CStr;
use std::os::raw::c_char;

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
                Ok(Ok(res)) => Ok(res),
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
