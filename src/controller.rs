use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::config::{self, Config, MonitorKey, Profile};
use crate::platform::windows::ddc::apply_profile_assignments;
use crate::platform::windows::WindowsDdcBackend;
use crate::platform::{DdcBackend, MonitorSnapshot};

const MAX_LOG_ENTRIES: usize = 500;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub message: String,
}

/// The single place all three trigger paths (hotkey fired, tray menu clicked,
/// GUI edits) funnel through, so profile-apply logic never gets duplicated.
/// The log is pull-based (`log_snapshot`, read whenever the Log tab is shown
/// or refreshed) rather than push-notified - there's no per-frame loop in the
/// native GUI to drain a notification channel, so there's nothing for a push
/// channel to buy here.
/// Owns only `Send + Sync` state - notably *not* the platform hotkey manager,
/// since Windows' `GlobalHotKeyManager` wraps a raw `HWND` and is `!Send`
/// (registering/unregistering a hotkey is only valid from the thread that
/// created its message-only window anyway). `Controller` gets moved into
/// disposable worker threads for profile-apply, so it must stay free of any
/// thread-affine handles; the hotkey manager itself lives on the main thread
/// instead (see `gui::Shared`), with just a tiny `Send`-safe id->profile
/// lookup map shared into the hotkey-fired callback.
pub struct Controller {
    pub config: Mutex<Config>,
    log: Mutex<VecDeque<LogEntry>>,
    in_flight: Mutex<HashSet<Uuid>>,
    backend: Arc<WindowsDdcBackend>,
}

impl Controller {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Controller {
            config: Mutex::new(config),
            log: Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)),
            in_flight: Mutex::new(HashSet::new()),
            backend: Arc::new(WindowsDdcBackend::new()),
        })
    }

    pub fn push_log(&self, message: impl Into<String>) {
        let message = message.into();
        let entry = LogEntry {
            timestamp: chrono_like_timestamp(),
            message: message.clone(),
        };
        let mut log = self.log.lock().unwrap();
        if log.len() >= MAX_LOG_ENTRIES {
            log.pop_front();
        }
        log.push_back(entry);
    }

    pub fn log_snapshot(&self) -> Vec<LogEntry> {
        self.log.lock().unwrap().iter().cloned().collect()
    }

    pub fn detect_monitors(&self) -> Vec<MonitorSnapshot> {
        self.backend.enumerate()
    }

    pub fn save_config(&self) -> anyhow::Result<()> {
        let cfg = self.config.lock().unwrap();
        config::save(&cfg)?;
        Ok(())
    }

    /// Applies the given profile's monitor assignments on a disposable worker
    /// thread. Never blocks the caller (hotkey callback / UI thread) - a hung
    /// DDC/CI write on one monitor just hangs that thread forever, it never
    /// blocks the app or the other monitors in the profile.
    pub fn apply_profile(self: &Arc<Self>, profile_id: Uuid) {
        {
            let mut in_flight = self.in_flight.lock().unwrap();
            if !in_flight.insert(profile_id) {
                self.push_log("Profile apply already in progress, ignoring repeat trigger");
                return;
            }
        }

        let profile = {
            let cfg = self.config.lock().unwrap();
            cfg.profiles.iter().find(|p| p.id == profile_id).cloned()
        };

        let Some(profile) = profile else {
            self.in_flight.lock().unwrap().remove(&profile_id);
            self.push_log("Hotkey fired for a profile that no longer exists");
            return;
        };

        let this = Arc::clone(self);
        std::thread::spawn(move || {
            let assignments: Vec<(MonitorKey, u8, bool)> = {
                let cfg = this.config.lock().unwrap();
                profile
                    .assignments
                    .iter()
                    .map(|(k, v)| (k.clone(), v.vcp_code(), cfg.verify_enabled(k)))
                    .collect()
            };

            // Fetched fresh so "previous"/"actual" codes in the log can be
            // resolved back to human-readable names via the monitor's own
            // capability list, not just shown as raw hex.
            let monitors = this.backend.enumerate();
            let resolve_name = |key: &MonitorKey, code: u8| -> String {
                monitors
                    .iter()
                    .find(|m| m.key == *key)
                    .and_then(|m| m.input_capabilities.iter().find(|c| c.code == code))
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("0x{code:02X}"))
            };

            this.push_log(format!("Applying profile \"{}\"...", profile.name));
            let results = apply_profile_assignments(this.backend.as_ref(), &assignments);
            for r in &results {
                let target_label = profile
                    .assignments
                    .get(&r.key)
                    .map(|v| v.display())
                    .unwrap_or_else(|| "?".to_string());
                let from_label = r.previous.map(|p| resolve_name(&r.key, p)).unwrap_or_else(|| "?".to_string());

                match &r.result {
                    Ok(()) => this.push_log(format!("[{}] {from_label} -> {target_label}: OK", r.key.0)),
                    Err(crate::platform::PlatformError::NotConfirmed { actual, .. }) => {
                        let actual_label = resolve_name(&r.key, *actual);
                        this.push_log(format!(
                            "[{}] {from_label} -> {target_label}: FAILED (still reads as {actual_label})",
                            r.key.0
                        ));
                    }
                    Err(e) => this.push_log(format!("[{}] {from_label} -> {target_label}: FAILED: {e}", r.key.0)),
                }
            }

            this.in_flight.lock().unwrap().remove(&profile_id);
        });
    }

    pub fn add_profile(&self, name: impl Into<String>) -> Uuid {
        let profile = Profile::new(name);
        let id = profile.id;
        self.config.lock().unwrap().profiles.push(profile);
        id
    }

    pub fn delete_profile(&self, id: Uuid) {
        self.config.lock().unwrap().profiles.retain(|p| p.id != id);
    }

    pub fn profiles_snapshot(&self) -> Vec<Profile> {
        self.config.lock().unwrap().profiles.clone()
    }

    /// Flexible mutable access to the in-memory config for GUI edit flows
    /// (rename, hotkey capture, per-monitor assignment) - avoids a proliferating
    /// set of narrow setter methods on `Controller` for every editable field.
    pub fn with_config_mut<R>(&self, f: impl FnOnce(&mut Config) -> R) -> R {
        let mut cfg = self.config.lock().unwrap();
        f(&mut cfg)
    }
}

fn chrono_like_timestamp() -> String {
    // Avoids pulling in the `chrono` crate for one timestamp string - a plain
    // seconds-since-epoch is sufficient for the in-GUI log panel's ordering.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}
