pub mod windows;

use thiserror::Error;

use crate::config::MonitorKey;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("ddc backend error: {0}")]
    Ddc(String),
    #[error("monitor not currently connected")]
    NotConnected,
    /// The DDC/CI write was acknowledged over the bus, but a read-back
    /// afterwards showed the monitor never actually switched to the
    /// requested input - some monitor firmware ACKs a `SetVCPFeature` write
    /// for a value it can't actually honor (e.g. a port that's declared in
    /// its capability string but not physically wired on that unit), or
    /// silently drops it under bus contention. Distinguished from `Ddc` so
    /// callers/logs don't report a false "OK" for a switch that didn't
    /// happen.
    #[error("monitor acknowledged the write but did not switch to 0x{requested:02X} (reads back as 0x{actual:02X})")]
    NotConfirmed { requested: u8, actual: u8 },
}

/// A named input value discovered from a monitor's VCP 0x60 capability string,
/// e.g. ("HDMI-2", 0x11). Not all monitors report these - callers should fall
/// back to a manual raw-hex entry when this list is empty.
#[derive(Debug, Clone)]
pub struct CapabilityValue {
    pub name: String,
    pub code: u8,
}

/// EDID-derived identity info, when it could be found - backend-agnostic
/// shape (a future Linux backend could populate the same fields by reading
/// `/sys/class/drm/*/edid` instead of Windows' WMI `WmiMonitorID` class).
/// `ddc-winapi`'s own DDC/CI path exposes none of this, only a generic
/// OS-assigned description string, so this is sourced independently and
/// best-effort correlated to the DDC-enumerated monitor by hardware ID.
#[derive(Debug, Clone, Default)]
pub struct MonitorHardwareInfo {
    pub manufacturer: Option<String>,
    pub model_name: Option<String>,
    pub serial: Option<String>,
    pub manufacture_year: Option<u16>,
    pub manufacture_week: Option<u8>,
}

/// A monitor as currently detected on this run - the key may or may not match a
/// previously-saved `MonitorKey` if the enumeration order or description changed.
#[derive(Debug, Clone)]
pub struct MonitorSnapshot {
    pub key: MonitorKey,
    pub description: String,
    /// VCP 0x60 capability values, if the monitor reported any.
    pub input_capabilities: Vec<CapabilityValue>,
    /// EDID-derived info, best-effort correlated - `None` if correlation
    /// failed or the underlying query (WMI on Windows) didn't succeed.
    pub hardware_info: Option<MonitorHardwareInfo>,
}

/// Result of attempting to apply one input value to one monitor.
#[derive(Debug)]
pub struct ApplyResult {
    pub key: MonitorKey,
    /// Best-effort read of the input the monitor reported *before* the
    /// switch was attempted - `None` if the monitor didn't answer that read.
    /// Purely informational (for "from -> to" logging); not used to decide
    /// success/failure.
    pub previous: Option<u8>,
    pub result: Result<(), PlatformError>,
}

/// Abstraction over DDC/CI monitor control so only `platform::windows::ddc` needs
/// to know about `ddc-hi`/`ddc-winapi` - keeps a path open for a Linux/macOS
/// backend later without touching any caller of this trait.
pub trait DdcBackend {
    fn enumerate(&self) -> Vec<MonitorSnapshot>;
    /// Returns the best-effort "previous" reading alongside the outcome -
    /// see `ApplyResult::previous`. When `verify` is `false`, skips the
    /// read-back/retry loop entirely and just trusts `set_vcp_feature`'s own
    /// ack - some monitors' `get_vcp_feature(0x60)` response doesn't reliably
    /// reflect their true active input, making verification actively
    /// misleading rather than helpful on those units.
    fn apply(&self, key: &MonitorKey, vcp_code: u8, verify: bool) -> (Option<u8>, Result<(), PlatformError>);
}
