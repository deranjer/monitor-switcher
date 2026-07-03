use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_CONFIG_VERSION: u32 = 1;

/// Stable-ish identifier for a physical monitor.
///
/// Phase 1: `(adapter_enum_index, description_string)` from `ddc_hi::Display::enumerate()`.
/// `ddc-winapi` exposes no EDID/serial, so this collides for two identical monitor models -
/// the GUI's "Identify Monitors" overlay is the user-facing safety net for that collision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorKey(pub String);

impl MonitorKey {
    pub fn from_parts(adapter_index: usize, description: &str) -> Self {
        MonitorKey(format!("{adapter_index}:{description}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSourceValue {
    /// A value taken from the monitor's own VCP 0x60 capability list, e.g. "HDMI-2".
    Named(String, u8),
    /// Manual fallback for monitors that don't report a capability list.
    RawVcp(u8),
}

impl InputSourceValue {
    pub fn vcp_code(&self) -> u8 {
        match self {
            InputSourceValue::Named(_, code) => *code,
            InputSourceValue::RawVcp(code) => *code,
        }
    }

    pub fn display(&self) -> String {
        match self {
            InputSourceValue::Named(name, code) => format!("{name} (0x{code:02X})"),
            InputSourceValue::RawVcp(code) => format!("0x{code:02X} (manual)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierKey {
    Alt,
    Control,
    Shift,
    Super,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyBinding {
    pub modifiers: Vec<ModifierKey>,
    /// Serialized form of `global_hotkey::hotkey::Code` (its `Display`/`FromStr` string, e.g. "Digit1").
    pub code: String,
    /// Human-readable form for GUI display, e.g. "Ctrl+Alt+1".
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub hotkey: Option<HotkeyBinding>,
    pub assignments: HashMap<MonitorKey, InputSourceValue>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Profile {
            id: Uuid::new_v4(),
            name: name.into(),
            hotkey: None,
            assignments: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub profiles: Vec<Profile>,
    pub monitor_labels: HashMap<MonitorKey, String>,
    /// Per-monitor opt-out of post-switch read-back verification. Some
    /// monitors' `get_vcp_feature(0x60)` response doesn't reliably reflect
    /// their true active input (seen in practice: a "Dark Matter" brand
    /// monitor whose read-back matched neither the requested value nor what
    /// was actually on screen) - for those, verification produces false
    /// FAILED/OK reports that are worse than just trusting the DDC bus's
    /// write acknowledgment, so it needs to be disable-able per monitor.
    /// `#[serde(default)]` so older config files without this field still
    /// load instead of getting backed up/reset (see `config::load`).
    #[serde(default)]
    pub monitor_verify: HashMap<MonitorKey, bool>,
    pub launch_minimized: bool,
    pub autostart_enabled: bool,
}

impl Config {
    /// Defaults to `true` (verify) for any monitor without an explicit entry.
    pub fn verify_enabled(&self, key: &MonitorKey) -> bool {
        self.monitor_verify.get(key).copied().unwrap_or(true)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CURRENT_CONFIG_VERSION,
            profiles: Vec::new(),
            monitor_labels: HashMap::new(),
            monitor_verify: HashMap::new(),
            launch_minimized: true,
            autostart_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_key_from_parts_format() {
        assert_eq!(MonitorKey::from_parts(1, "Generic PnP Monitor").0, "1:Generic PnP Monitor");
    }

    #[test]
    fn input_source_value_vcp_code_and_display() {
        let named = InputSourceValue::Named("HDMI-2".to_owned(), 0x11);
        assert_eq!(named.vcp_code(), 0x11);
        assert_eq!(named.display(), "HDMI-2 (0x11)");

        let raw = InputSourceValue::RawVcp(0x0F);
        assert_eq!(raw.vcp_code(), 0x0F);
        assert_eq!(raw.display(), "0x0F (manual)");
    }

    #[test]
    fn verify_enabled_defaults_true_unless_overridden() {
        let mut cfg = Config::default();
        let key = MonitorKey::from_parts(0, "Dark Matter");
        assert!(cfg.verify_enabled(&key));

        cfg.monitor_verify.insert(key.clone(), false);
        assert!(!cfg.verify_enabled(&key));
    }

    #[test]
    fn config_roundtrips_through_json() {
        let mut cfg = Config::default();
        let key = MonitorKey::from_parts(0, "Generic PnP Monitor");
        let mut profile = Profile::new("Main PC");
        profile.hotkey = Some(HotkeyBinding {
            modifiers: vec![ModifierKey::Control, ModifierKey::Alt],
            code: "Digit1".to_owned(),
            display: "Ctrl+Alt+1".to_owned(),
        });
        profile.assignments.insert(key.clone(), InputSourceValue::Named("DP-1".to_owned(), 0x0F));
        cfg.profiles.push(profile);
        cfg.monitor_verify.insert(key, false);

        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.profiles.len(), 1);
        assert_eq!(restored.profiles[0].name, "Main PC");
        assert_eq!(restored.profiles[0].hotkey.as_ref().unwrap().display, "Ctrl+Alt+1");
        assert_eq!(restored.monitor_verify.len(), 1);
    }

    #[test]
    fn config_without_monitor_verify_field_still_deserializes() {
        // Simulates a config.json saved before `monitor_verify` existed -
        // `#[serde(default)]` must keep old files loading instead of tripping
        // the "failed to parse, back up and reset" path in `config::load`.
        let json = r#"{
            "version": 1,
            "profiles": [],
            "monitor_labels": {},
            "launch_minimized": true,
            "autostart_enabled": false
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert!(cfg.monitor_verify.is_empty());
    }
}
