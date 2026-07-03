use crate::config::{HotkeyBinding, ModifierKey};

#[derive(Default)]
pub struct CaptureState {
    pub capturing: bool,
}

/// Renders the "click to set hotkey" button. While capturing, reads egui's own
/// input events directly (the window is guaranteed focused during capture)
/// rather than going through `global-hotkey` - that lets a conflict with
/// another app's registration surface only once the user commits, via
/// `HotkeyRegistry::sync`'s `register()` error path, instead of failing silently
/// mid-capture.
///
/// Returns `Some(binding)` the moment a non-modifier key is pressed while
/// capturing (commits immediately - simpler than a press/hold/release flow).
pub fn show(ui: &mut egui::Ui, state: &mut CaptureState, current: &Option<HotkeyBinding>) -> Option<HotkeyBinding> {
    let label = if state.capturing {
        "Press a key combo...".to_string()
    } else {
        current.as_ref().map(|b| b.display.clone()).unwrap_or_else(|| "(none)".to_string())
    };

    if ui.button(label).clicked() {
        state.capturing = true;
    }

    if !state.capturing {
        return None;
    }

    let mut result = None;
    ui.ctx().input(|input| {
        let modifiers = input.modifiers;
        for event in &input.events {
            if let egui::Event::Key { key, pressed: true, repeat: false, .. } = event {
                if let Some(code) = egui_key_to_global_hotkey_code(*key) {
                    let mut mods = Vec::new();
                    if modifiers.ctrl {
                        mods.push(ModifierKey::Control);
                    }
                    if modifiers.alt {
                        mods.push(ModifierKey::Alt);
                    }
                    if modifiers.shift {
                        mods.push(ModifierKey::Shift);
                    }
                    let display = format!(
                        "{}{}{}{:?}",
                        if modifiers.ctrl { "Ctrl+" } else { "" },
                        if modifiers.alt { "Alt+" } else { "" },
                        if modifiers.shift { "Shift+" } else { "" },
                        key
                    );

                    result = Some(HotkeyBinding {
                        modifiers: mods,
                        code: code.to_string(),
                        display,
                    });
                }
            }
        }
    });

    if result.is_some() {
        state.capturing = false;
    }

    result
}

fn egui_key_to_global_hotkey_code(key: egui::Key) -> Option<global_hotkey::hotkey::Code> {
    use egui::Key as K;
    use global_hotkey::hotkey::Code as C;

    Some(match key {
        K::A => C::KeyA,
        K::B => C::KeyB,
        K::C => C::KeyC,
        K::D => C::KeyD,
        K::E => C::KeyE,
        K::F => C::KeyF,
        K::G => C::KeyG,
        K::H => C::KeyH,
        K::I => C::KeyI,
        K::J => C::KeyJ,
        K::K => C::KeyK,
        K::L => C::KeyL,
        K::M => C::KeyM,
        K::N => C::KeyN,
        K::O => C::KeyO,
        K::P => C::KeyP,
        K::Q => C::KeyQ,
        K::R => C::KeyR,
        K::S => C::KeyS,
        K::T => C::KeyT,
        K::U => C::KeyU,
        K::V => C::KeyV,
        K::W => C::KeyW,
        K::X => C::KeyX,
        K::Y => C::KeyY,
        K::Z => C::KeyZ,
        K::Num0 => C::Digit0,
        K::Num1 => C::Digit1,
        K::Num2 => C::Digit2,
        K::Num3 => C::Digit3,
        K::Num4 => C::Digit4,
        K::Num5 => C::Digit5,
        K::Num6 => C::Digit6,
        K::Num7 => C::Digit7,
        K::Num8 => C::Digit8,
        K::Num9 => C::Digit9,
        K::F1 => C::F1,
        K::F2 => C::F2,
        K::F3 => C::F3,
        K::F4 => C::F4,
        K::F5 => C::F5,
        K::F6 => C::F6,
        K::F7 => C::F7,
        K::F8 => C::F8,
        K::F9 => C::F9,
        K::F10 => C::F10,
        K::F11 => C::F11,
        K::F12 => C::F12,
        K::ArrowUp => C::ArrowUp,
        K::ArrowDown => C::ArrowDown,
        K::ArrowLeft => C::ArrowLeft,
        K::ArrowRight => C::ArrowRight,
        K::Escape => C::Escape,
        K::Space => C::Space,
        K::Enter => C::Enter,
        _ => return None,
    })
}
