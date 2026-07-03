//! Win32 monitor geometry lookup (backs the GUI's "Identify Monitors"
//! overlay, independent of DDC/CI) and EDID-derived hardware info via WMI's
//! `WmiMonitorID` class - `ddc-winapi`'s DDC path exposes neither of these,
//! only a generic OS-assigned description string and an opaque handle.
//!
//! Correlating WMI's per-monitor info back to a specific `HMONITOR` (and
//! therefore to a specific DDC-enumerated `MonitorKey`) is done via the PNP
//! hardware ID token shared by both `EnumDisplayDevicesW`'s `DeviceID`
//! (format `MONITOR\<HWID>\{gguid}\NNNN`) and WMI's `InstanceName`
//! (format `DISPLAY\<HWID>\4&...&UID...`) - the standard technique for this,
//! also used by tools like MultiMonitorTool/DisplayFusion.

use std::collections::HashMap;

use serde::Deserialize;
use windows::core::HSTRING;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR, MONITORINFO,
    MONITORINFOEXW,
};

use crate::platform::MonitorHardwareInfo;

#[derive(Debug, Clone, Copy)]
pub struct MonitorGeometry {
    #[allow(dead_code)] // part of the rectangle's meaning even where only the bottom-right corner is used today
    pub left: i32,
    #[allow(dead_code)]
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Enumerates monitors in the same OS-determined order `EnumDisplayMonitors`
/// always returns, matching the order `ddc_hi::Display::enumerate()` walks
/// internally on Windows - used to line up "Monitor N" in the identify overlay
/// with "Monitor N" in the DDC-derived `MonitorKey` adapter index.
pub fn enumerate_monitor_geometry() -> Vec<MonitorGeometry> {
    let mut monitors: Vec<MonitorGeometry> = Vec::new();

    unsafe extern "system" fn callback(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> windows::core::BOOL {
        let monitors = unsafe { &mut *(lparam.0 as *mut Vec<MonitorGeometry>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(hmonitor, &mut info as *mut MONITORINFO) }.as_bool() {
            monitors.push(MonitorGeometry {
                left: info.rcMonitor.left,
                top: info.rcMonitor.top,
                right: info.rcMonitor.right,
                bottom: info.rcMonitor.bottom,
            });
        }
        windows::core::BOOL(1)
    }

    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(callback), LPARAM(&mut monitors as *mut _ as isize));
    }

    monitors
}

/// Per-`HMONITOR` Win32 adapter device name (e.g. `\\.\DISPLAY1`), in the same
/// `EnumDisplayMonitors` order as `enumerate_monitor_geometry`/`Display::enumerate()`.
fn enumerate_adapter_device_names() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    unsafe extern "system" fn callback(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> windows::core::BOOL {
        let names = unsafe { &mut *(lparam.0 as *mut Vec<String>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(hmonitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
            names.push(wide_to_string(&info.szDevice));
        } else {
            names.push(String::new());
        }
        windows::core::BOOL(1)
    }

    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(callback), LPARAM(&mut names as *mut _ as isize));
    }

    names
}

/// The monitor's PNP hardware ID token (e.g. `ACI27A6`), extracted from
/// `EnumDisplayDevicesW`'s `DeviceID` for the monitor attached to
/// `adapter_device_name`. `None` if the adapter has no attached monitor
/// device or the name doesn't parse as expected.
fn monitor_hardware_id(adapter_device_name: &str) -> Option<String> {
    if adapter_device_name.is_empty() {
        return None;
    }
    let name = HSTRING::from(adapter_device_name);
    let mut dd = DISPLAY_DEVICEW {
        cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    // dwFlags = 0 (not EDD_GET_DEVICE_INTERFACE_NAME) so DeviceID comes back
    // as `MONITOR\<HWID>\{guid}\NNNN`, not a device interface path - the
    // former is what shares a token with WMI's InstanceName.
    if !unsafe { EnumDisplayDevicesW(&name, 0, &mut dd, 0) }.as_bool() {
        return None;
    }
    extract_hwid(&wide_to_string(&dd.DeviceID))
}

fn extract_hwid(path: &str) -> Option<String> {
    path.split('\\').nth(1).map(|s| s.to_uppercase())
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Decodes a WMI `uint16[]` string property (each element is one ASCII code
/// point, zero-padded) into a `String`, or `None` if empty/all-zero/whitespace.
/// EDID string fields are space-padded to a fixed length, hence the trim.
fn decode_wmi_string(codes: &[u16]) -> Option<String> {
    let s: String = codes
        .iter()
        .copied()
        .take_while(|&c| c != 0)
        .filter_map(|c| char::from_u32(c as u32))
        .collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct WmiMonitorID {
    InstanceName: String,
    ManufacturerName: Vec<u16>,
    UserFriendlyName: Vec<u16>,
    SerialNumberID: Vec<u16>,
    WeekOfManufacture: u8,
    YearOfManufacture: u16,
}

/// Queries `root\wmi`'s `WmiMonitorID` class, keyed by PNP hardware ID token
/// (see module docs) so it can be correlated against `HMONITOR`s. Best-effort:
/// returns an empty map (logging a warning) rather than erroring if WMI is
/// unavailable or the query fails, since DDC/CI switching itself doesn't
/// depend on this - it's supplementary display-only info.
///
/// Runs on a dedicated, throwaway thread every time, rather than on whatever
/// thread called `enumerate()` - `wmi`'s `COMLibrary::new()` always calls
/// `CoInitializeEx(COINIT_MULTITHREADED)`, which fails if the calling thread
/// already has COM initialized in a different apartment model. The app's
/// main/UI thread does (eframe/winit initializes COM in apartment-threaded
/// mode for OLE drag-and-drop and other shell integration), so calling this
/// inline from there silently returns nothing. A fresh thread has never had
/// COM touched, sidestepping the conflict entirely.
fn query_wmi_monitor_hardware_info() -> HashMap<String, MonitorHardwareInfo> {
    std::thread::spawn(query_wmi_monitor_hardware_info_on_this_thread)
        .join()
        .unwrap_or_else(|_| {
            tracing::warn!("WMI query thread panicked, skipping monitor hardware info");
            HashMap::new()
        })
}

fn query_wmi_monitor_hardware_info_on_this_thread() -> HashMap<String, MonitorHardwareInfo> {
    let mut result = HashMap::new();

    let com = match wmi::COMLibrary::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("WMI COM init failed, skipping monitor hardware info: {e}");
            return result;
        }
    };
    let wmi_con = match wmi::WMIConnection::with_namespace_path("ROOT\\WMI", com) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("WMI connection to ROOT\\WMI failed, skipping monitor hardware info: {e}");
            return result;
        }
    };
    let rows: Vec<WmiMonitorID> = match wmi_con.query() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("WmiMonitorID query failed, skipping monitor hardware info: {e}");
            return result;
        }
    };

    for row in rows {
        let Some(hwid) = extract_hwid(&row.InstanceName) else {
            continue;
        };
        result.insert(
            hwid,
            MonitorHardwareInfo {
                manufacturer: decode_wmi_string(&row.ManufacturerName),
                model_name: decode_wmi_string(&row.UserFriendlyName),
                serial: decode_wmi_string(&row.SerialNumberID),
                manufacture_year: (row.YearOfManufacture != 0).then_some(row.YearOfManufacture),
                manufacture_week: (row.WeekOfManufacture != 0).then_some(row.WeekOfManufacture),
            },
        );
    }

    result
}

/// One entry per `HMONITOR`, in the same order as `enumerate_monitor_geometry`
/// / `Display::enumerate()` - `None` for a monitor whose hardware ID couldn't
/// be determined or correlated to a WMI entry.
pub fn enumerate_monitor_hardware_info() -> Vec<Option<MonitorHardwareInfo>> {
    let wmi_by_hwid = query_wmi_monitor_hardware_info();
    enumerate_adapter_device_names()
        .iter()
        .map(|device_name| monitor_hardware_id(device_name).and_then(|hwid| wmi_by_hwid.get(&hwid).cloned()))
        .collect()
}
