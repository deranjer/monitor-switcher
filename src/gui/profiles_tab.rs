use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::config::{InputSourceValue, MonitorKey};
use crate::controller::Controller;
use crate::gui::hotkey_capture::{self, CaptureState};
use crate::platform::MonitorSnapshot;

#[derive(Default)]
pub struct State {
    selected: Option<Uuid>,
    new_profile_name: String,
    hotkey_capture: CaptureState,
    /// Raw-hex text edit buffers, keyed by monitor - used both for monitors
    /// with no reported capability list, and as a manual override for
    /// monitors whose reported capability codes turn out not to match what
    /// actually switches the input (a real observed case: a monitor whose
    /// capability string claimed DisplayPort 1 = 0x0F, but the value that
    /// actually worked was 0x11).
    raw_hex_buffers: HashMap<MonitorKey, String>,
    /// Whether each monitor's assignment row is in manual-override mode
    /// (raw hex) vs. picking from the reported capability dropdown. Only
    /// meaningful for monitors that have a capability list at all - defaults
    /// to whichever mode matches the currently saved assignment.
    manual_mode: HashMap<MonitorKey, bool>,
}

/// Returns `true` if a change was made that needs saving + hotkey re-sync.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut State,
    controller: &Arc<Controller>,
    monitors: &[MonitorSnapshot],
) -> bool {
    let mut dirty = false;

    ui.columns(2, |columns| {
        dirty |= show_profile_list(&mut columns[0], state, controller);
        if let Some(selected) = state.selected {
            dirty |= show_profile_editor(&mut columns[1], state, controller, monitors, selected);
        } else {
            columns[1].label("Select or add a profile to edit it.");
        }
    });

    dirty
}

fn show_profile_list(ui: &mut egui::Ui, state: &mut State, controller: &Arc<Controller>) -> bool {
    let mut dirty = false;
    let profiles = controller.profiles_snapshot();

    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.new_profile_name);
        if ui.button("Add Profile").clicked() && !state.new_profile_name.trim().is_empty() {
            let id = controller.add_profile(state.new_profile_name.trim().to_string());
            state.new_profile_name.clear();
            state.selected = Some(id);
            dirty = true;
        }
    });

    ui.separator();

    egui::ScrollArea::vertical().id_salt("profile-list").show(ui, |ui| {
        for profile in &profiles {
            ui.horizontal(|ui| {
                let selected = state.selected == Some(profile.id);
                if ui.selectable_label(selected, &profile.name).clicked() {
                    state.selected = Some(profile.id);
                }
                if ui.small_button("Delete").clicked() {
                    controller.delete_profile(profile.id);
                    if state.selected == Some(profile.id) {
                        state.selected = None;
                    }
                    dirty = true;
                }
            });
        }
    });

    dirty
}

fn show_profile_editor(
    ui: &mut egui::Ui,
    state: &mut State,
    controller: &Arc<Controller>,
    monitors: &[MonitorSnapshot],
    profile_id: Uuid,
) -> bool {
    let mut dirty = false;

    let mut name = controller
        .with_config_mut(|cfg| cfg.profiles.iter().find(|p| p.id == profile_id).map(|p| p.name.clone()))
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.label("Name:");
        if ui.text_edit_singleline(&mut name).changed() {
            controller.with_config_mut(|cfg| {
                if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == profile_id) {
                    p.name = name.clone();
                }
            });
            dirty = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Hotkey:");
        let current = controller
            .with_config_mut(|cfg| cfg.profiles.iter().find(|p| p.id == profile_id).and_then(|p| p.hotkey.clone()));
        if let Some(binding) = hotkey_capture::show(ui, &mut state.hotkey_capture, &current) {
            controller.with_config_mut(|cfg| {
                if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == profile_id) {
                    p.hotkey = Some(binding);
                }
            });
            dirty = true;
        }
    });

    ui.separator();
    ui.label("Per-monitor input assignment:");

    egui::ScrollArea::vertical().id_salt("profile-monitors").show(ui, |ui| {
        for (i, monitor) in monitors.iter().enumerate() {
            ui.group(|ui| {
                let display_name = monitor
                    .hardware_info
                    .as_ref()
                    .and_then(|hw| hw.model_name.clone())
                    .unwrap_or_else(|| monitor.description.clone());
                ui.label(format!("Monitor {} - {display_name}", i + 1));

                let current = controller.with_config_mut(|cfg| {
                    cfg.profiles
                        .iter()
                        .find(|p| p.id == profile_id)
                        .and_then(|p| p.assignments.get(&monitor.key).cloned())
                });

                let has_capabilities = !monitor.input_capabilities.is_empty();
                let manual_mode = *state
                    .manual_mode
                    .entry(monitor.key.clone())
                    .or_insert_with(|| !has_capabilities || matches!(current, Some(InputSourceValue::RawVcp(_))));

                if has_capabilities {
                    let mut manual = manual_mode;
                    if ui
                        .checkbox(&mut manual, "Manual override")
                        .on_hover_text(
                            "Use this if the dropdown's reported codes don't actually match \
                             what switches this monitor's input in practice - some monitors \
                             misreport their own capability codes.",
                        )
                        .changed()
                    {
                        state.manual_mode.insert(monitor.key.clone(), manual);
                    }
                }

                if has_capabilities && !manual_mode {
                    let current_label = current.as_ref().map(|v| v.display()).unwrap_or_else(|| "(unset)".to_string());
                    egui::ComboBox::from_id_salt(("input-combo", &monitor.key.0))
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for cap in &monitor.input_capabilities {
                                let value = InputSourceValue::Named(cap.name.clone(), cap.code);
                                let selected = current.as_ref() == Some(&value);
                                if ui.selectable_label(selected, value.display()).clicked() {
                                    set_assignment(controller, profile_id, &monitor.key, value);
                                    dirty = true;
                                }
                            }
                        });
                } else {
                    let buf = state.raw_hex_buffers.entry(monitor.key.clone()).or_insert_with(|| {
                        current.as_ref().map(|v| format!("{:02X}", v.vcp_code())).unwrap_or_default()
                    });
                    ui.horizontal(|ui| {
                        ui.label("Raw VCP hex:");
                        if ui.text_edit_singleline(buf).lost_focus() {
                            if let Ok(code) = u8::from_str_radix(buf.trim_start_matches("0x").trim(), 16) {
                                set_assignment(controller, profile_id, &monitor.key, InputSourceValue::RawVcp(code));
                                dirty = true;
                            }
                        }
                    });
                }
            });
        }
        if monitors.is_empty() {
            ui.label("No monitors detected - visit the Monitors tab first.");
        }
    });

    dirty
}

fn set_assignment(controller: &Arc<Controller>, profile_id: Uuid, key: &MonitorKey, value: InputSourceValue) {
    controller.with_config_mut(|cfg| {
        if let Some(p) = cfg.profiles.iter_mut().find(|p| p.id == profile_id) {
            p.assignments.insert(key.clone(), value);
        }
    });
}
