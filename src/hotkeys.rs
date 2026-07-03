use std::collections::HashMap;
use std::str::FromStr;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;
use uuid::Uuid;

use crate::config::{HotkeyBinding, ModifierKey, Profile};

/// Wraps `GlobalHotKeyManager` and keeps a `profile id -> HotKey` map so `sync`
/// can diff against the previous registration set instead of blindly
/// re-registering everything on every save (which would spuriously trip
/// "already registered" errors for hotkeys that didn't actually change).
pub struct HotkeyRegistry {
    manager: GlobalHotKeyManager,
    registered: HashMap<Uuid, HotKey>,
}

impl HotkeyRegistry {
    pub fn new() -> anyhow::Result<Self> {
        let manager = GlobalHotKeyManager::new()?;
        Ok(HotkeyRegistry {
            manager,
            registered: HashMap::new(),
        })
    }

    /// A `Send`-safe snapshot of `hotkey id -> profile id`, to be stashed in a
    /// shared `Arc<Mutex<..>>` and consulted from the (Send + Sync-bounded)
    /// `GlobalHotKeyEvent` callback - the registry itself can't be shared that
    /// way since `GlobalHotKeyManager` on Windows is `!Send`.
    pub fn lookup_snapshot(&self) -> HashMap<u32, Uuid> {
        self.registered.iter().map(|(profile_id, hk)| (hk.id(), *profile_id)).collect()
    }

    /// Re-registers exactly the hotkeys that changed since the last sync.
    pub fn sync(&mut self, profiles: &[Profile]) {
        let desired: HashMap<Uuid, HotKey> = profiles
            .iter()
            .filter_map(|p| {
                let binding = p.hotkey.as_ref()?;
                to_hotkey(binding).map(|hk| (p.id, hk))
            })
            .collect();

        // Unregister anything removed or changed to a different combo.
        let stale: Vec<Uuid> = self
            .registered
            .iter()
            .filter(|(id, hk)| desired.get(id).map(|d| d.id()) != Some(hk.id()))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(hk) = self.registered.remove(&id) {
                if let Err(e) = self.manager.unregister(hk) {
                    tracing::warn!("failed to unregister hotkey for profile {id}: {e}");
                }
            }
        }

        // Register anything new or changed.
        for (profile_id, hotkey) in &desired {
            if !self.registered.contains_key(profile_id) {
                match self.manager.register(*hotkey) {
                    Ok(()) => {
                        self.registered.insert(*profile_id, *hotkey);
                    }
                    Err(e) => {
                        tracing::warn!("failed to register hotkey for profile {profile_id}: {e}");
                    }
                }
            }
        }
    }
}

fn to_hotkey(binding: &HotkeyBinding) -> Option<HotKey> {
    let code = Code::from_str(&binding.code).ok()?;
    let mut mods = Modifiers::empty();
    for m in &binding.modifiers {
        mods |= match m {
            ModifierKey::Alt => Modifiers::ALT,
            ModifierKey::Control => Modifiers::CONTROL,
            ModifierKey::Shift => Modifiers::SHIFT,
            ModifierKey::Super => Modifiers::SUPER,
        };
    }
    Some(HotKey::new(Some(mods), code))
}
