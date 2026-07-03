//! The only module allowed to import `ddc_hi`/`ddc_winapi` directly - everything
//! else in the app talks to the `DdcBackend` trait in `platform::mod`.

use ddc_hi::{Ddc, Display, Handle};

use crate::config::MonitorKey;
use crate::platform::windows::monitor_identity::enumerate_monitor_hardware_info;
use crate::platform::{ApplyResult, CapabilityValue, DdcBackend, MonitorSnapshot, PlatformError};

pub const VCP_INPUT_SOURCE: u8 = 0x60;

/// How many times to re-send the VCP write if a read-back doesn't confirm
/// the switch actually took (1 initial attempt + this many retries).
const MAX_RETRIES: u32 = 2;
/// Some monitors take a noticeable moment to actually renegotiate the new
/// input signal before their VCP register reflects it - reading back
/// immediately after the write risks seeing stale state, not a real failure.
const SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

pub struct WindowsDdcBackend;

impl WindowsDdcBackend {
    pub fn new() -> Self {
        WindowsDdcBackend
    }
}

impl Default for WindowsDdcBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DdcBackend for WindowsDdcBackend {
    fn enumerate(&self) -> Vec<MonitorSnapshot> {
        // Best-effort, correlated by index against the same EnumDisplayMonitors
        // order ddc-winapi itself walks - see monitor_identity's module docs
        // for how the underlying WMI/EnumDisplayDevicesW correlation works.
        let hardware_info = enumerate_monitor_hardware_info();

        Display::enumerate()
            .into_iter()
            .enumerate()
            .map(|(adapter_index, mut display)| {
                let description = match &display.handle {
                    Handle::WinApi(monitor) => monitor.description().to_string(),
                    #[allow(unreachable_patterns)]
                    _ => display.info.id.clone(),
                };
                let key = MonitorKey::from_parts(adapter_index, &description);

                // Capability queries can fail/time out on monitors that don't support
                // them at all - treat that as "no named values", not a hard error.
                let input_capabilities = match display.update_capabilities() {
                    Ok(()) => extract_input_capabilities(&display),
                    Err(e) => {
                        tracing::warn!("capability query failed for {description}: {e}");
                        Vec::new()
                    }
                };

                let hardware_info = hardware_info.get(adapter_index).cloned().flatten();

                MonitorSnapshot {
                    key,
                    description,
                    input_capabilities,
                    hardware_info,
                }
            })
            .collect()
    }

    fn apply(&self, key: &MonitorKey, vcp_code: u8, verify: bool) -> (Option<u8>, Result<(), PlatformError>) {
        let target = Display::enumerate().into_iter().enumerate().find_map(|(adapter_index, display)| {
            let description = match &display.handle {
                Handle::WinApi(monitor) => monitor.description().to_string(),
                #[allow(unreachable_patterns)]
                _ => display.info.id.clone(),
            };
            let candidate_key = MonitorKey::from_parts(adapter_index, &description);
            (candidate_key == *key).then_some(display)
        });

        let mut display = match target {
            Some(d) => d,
            None => return (None, Err(PlatformError::NotConnected)),
        };

        if !verify {
            // Some monitors' get_vcp_feature(0x60) response doesn't reliably
            // reflect their true active input - for those, trust the DDC
            // bus's write acknowledgment alone rather than second-guessing it
            // with a readback that can be actively misleading.
            let result = display
                .handle
                .set_vcp_feature(VCP_INPUT_SOURCE, vcp_code as u16)
                .map_err(|e| PlatformError::Ddc(e.to_string()));
            return (None, result);
        }

        // Best-effort "what was it on before" read, purely for "from -> to"
        // logging - never affects the switch itself or its success/failure.
        let previous = display.handle.get_vcp_feature(VCP_INPUT_SOURCE).ok().map(|v| v.value() as u8);

        let mut confirmed_mismatch = None;
        let mut readback_ever_succeeded = false;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                std::thread::sleep(SETTLE_DELAY);
            }

            if let Err(e) = display.handle.set_vcp_feature(VCP_INPUT_SOURCE, vcp_code as u16) {
                return (previous, Err(PlatformError::Ddc(e.to_string())));
            }

            // Some monitors take a moment to actually renegotiate the new
            // input signal before their VCP register reflects it.
            std::thread::sleep(SETTLE_DELAY);

            match display.handle.get_vcp_feature(VCP_INPUT_SOURCE) {
                Ok(value) => {
                    readback_ever_succeeded = true;
                    let actual = value.value() as u8;
                    if actual == vcp_code {
                        return (previous, Ok(()));
                    }
                    confirmed_mismatch = Some(actual);
                }
                // Some monitors don't reliably answer a Get at all (Set-only
                // support is common) - an unreadable confirmation is
                // inconclusive, not proof of failure.
                Err(e) => {
                    tracing::warn!("read-back after set_vcp_feature failed: {e}");
                }
            }
        }

        let result = if readback_ever_succeeded {
            // We got real answers from the monitor and they never matched -
            // the write is being acknowledged but not actually honored. Note
            // this can also happen if the monitor has an always-on "Auto
            // Input Select" mode racing against the manual DDC command - the
            // read-back isn't a fully trustworthy ground truth on such units.
            Err(PlatformError::NotConfirmed {
                requested: vcp_code,
                actual: confirmed_mismatch.unwrap_or(0),
            })
        } else {
            // Could never verify either way (monitor doesn't answer Get for
            // this feature) - trust that the Set call itself didn't error,
            // rather than reporting a false failure.
            Ok(())
        };
        (previous, result)
    }
}

/// Applies one input value per monitor in a single profile, isolating failures so
/// one unresponsive monitor never blocks the rest. Meant to be called from a
/// disposable worker thread, never the UI thread.
pub fn apply_profile_assignments(
    backend: &dyn DdcBackend,
    assignments: &[(MonitorKey, u8, bool)],
) -> Vec<ApplyResult> {
    assignments
        .iter()
        .map(|(key, code, verify)| {
            let (previous, result) = backend.apply(key, *code, *verify);
            ApplyResult {
                key: key.clone(),
                previous,
                result,
            }
        })
        .collect()
}

fn extract_input_capabilities(display: &Display) -> Vec<CapabilityValue> {
    use mccs_db::ValueType;

    let Some(descriptor) = display.info.mccs_database.get(VCP_INPUT_SOURCE) else {
        return Vec::new();
    };

    match &descriptor.ty {
        ValueType::NonContinuous { values, .. } => values
            .iter()
            .filter_map(|(code, name)| {
                name.as_ref().map(|n| CapabilityValue {
                    name: n.clone(),
                    code: *code,
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}
