//! "Press a key combo" capture, implemented by polling `GetAsyncKeyState` on a
//! short timer while capture mode is active - the same technique
//! `global-hotkey`'s own Windows backend uses internally to detect key
//! release (see its `platform_impl/windows/mod.rs`). Deliberately not a
//! global low-level keyboard hook (`WH_KEYBOARD_LL`): a hook needs careful
//! install/uninstall lifetime management and runs system-wide, whereas this
//! only ever runs for a few seconds while the visible settings window has
//! focus, so simple polling is both simpler and lower-risk.

use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_SHIFT};

use crate::config::{HotkeyBinding, ModifierKey};

/// Virtual-key codes we recognize as the "final" (non-modifier) key of a
/// combo, paired with the `global_hotkey::hotkey::Code` string name and a
/// display label.
const CANDIDATES: &[(u16, &str, &str)] = &[
    (0x41, "KeyA", "A"), (0x42, "KeyB", "B"), (0x43, "KeyC", "C"), (0x44, "KeyD", "D"),
    (0x45, "KeyE", "E"), (0x46, "KeyF", "F"), (0x47, "KeyG", "G"), (0x48, "KeyH", "H"),
    (0x49, "KeyI", "I"), (0x4A, "KeyJ", "J"), (0x4B, "KeyK", "K"), (0x4C, "KeyL", "L"),
    (0x4D, "KeyM", "M"), (0x4E, "KeyN", "N"), (0x4F, "KeyO", "O"), (0x50, "KeyP", "P"),
    (0x51, "KeyQ", "Q"), (0x52, "KeyR", "R"), (0x53, "KeyS", "S"), (0x54, "KeyT", "T"),
    (0x55, "KeyU", "U"), (0x56, "KeyV", "V"), (0x57, "KeyW", "W"), (0x58, "KeyX", "X"),
    (0x59, "KeyY", "Y"), (0x5A, "KeyZ", "Z"),
    (0x30, "Digit0", "0"), (0x31, "Digit1", "1"), (0x32, "Digit2", "2"), (0x33, "Digit3", "3"),
    (0x34, "Digit4", "4"), (0x35, "Digit5", "5"), (0x36, "Digit6", "6"), (0x37, "Digit7", "7"),
    (0x38, "Digit8", "8"), (0x39, "Digit9", "9"),
    (0x70, "F1", "F1"), (0x71, "F2", "F2"), (0x72, "F3", "F3"), (0x73, "F4", "F4"),
    (0x74, "F5", "F5"), (0x75, "F6", "F6"), (0x76, "F7", "F7"), (0x77, "F8", "F8"),
    (0x78, "F9", "F9"), (0x79, "F10", "F10"), (0x7A, "F11", "F11"), (0x7B, "F12", "F12"),
];

fn is_down(vk: i32) -> bool {
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

/// Call on every capture-mode timer tick. Returns `Some(binding)` the moment
/// a non-modifier candidate key is currently down, alongside whatever
/// modifiers (Ctrl/Alt/Shift) are held at that instant. Arrow keys, Escape,
/// Space, and Enter are treated as cancel/plain keys here since they're more
/// useful left alone for normal navigation - only the candidates above (and
/// their listed modifiers) can be captured.
pub fn poll() -> Option<HotkeyBinding> {
    // Escape cancels the capture entirely rather than being bindable itself -
    // matches the convention of every other "press a shortcut" UI.
    if is_down(VK_ESCAPE.0 as i32) {
        return Some(HotkeyBinding {
            modifiers: Vec::new(),
            code: String::new(),
            display: String::new(),
        });
    }

    for &(vk, code, label) in CANDIDATES {
        if is_down(vk as i32) {
            let mut modifiers = Vec::new();
            let mut display = String::new();
            if is_down(VK_CONTROL.0 as i32) {
                modifiers.push(ModifierKey::Control);
                display.push_str("Ctrl+");
            }
            if is_down(VK_MENU.0 as i32) {
                modifiers.push(ModifierKey::Alt);
                display.push_str("Alt+");
            }
            if is_down(VK_SHIFT.0 as i32) {
                modifiers.push(ModifierKey::Shift);
                display.push_str("Shift+");
            }
            display.push_str(label);
            return Some(HotkeyBinding { modifiers, code: code.to_string(), display });
        }
    }

    None
}

/// A cancel sentinel: `poll()` returns this exact (empty) binding when Escape
/// was pressed, since capture should stop without committing a new binding.
pub fn is_cancel(binding: &HotkeyBinding) -> bool {
    binding.code.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_code_is_cancel() {
        let binding = HotkeyBinding { modifiers: Vec::new(), code: String::new(), display: String::new() };
        assert!(is_cancel(&binding));
    }

    #[test]
    fn non_empty_code_is_not_cancel() {
        let binding = HotkeyBinding {
            modifiers: vec![ModifierKey::Control],
            code: "Digit1".to_owned(),
            display: "Ctrl+1".to_owned(),
        };
        assert!(!is_cancel(&binding));
    }
}
