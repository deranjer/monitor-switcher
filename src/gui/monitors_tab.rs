use std::sync::Arc;

use crate::controller::Controller;
use crate::platform::MonitorSnapshot;
use crate::platform::windows::monitor_identity::{enumerate_monitor_geometry, MonitorGeometry};

#[derive(Default)]
pub struct State;

/// Returns `Some(geometries)` the frame "Identify Monitors" is clicked, so the
/// caller (`gui::render`) can kick off the numbered overlay - rendering that
/// overlay has to happen at the top level since it must run every frame it's
/// visible, not just while this tab is selected.
pub fn show(
    ui: &mut egui::Ui,
    _state: &mut State,
    controller: &Arc<Controller>,
    monitors: &mut Vec<MonitorSnapshot>,
) -> Option<Vec<MonitorGeometry>> {
    let mut identify_requested = None;

    ui.horizontal(|ui| {
        if ui.button("Detect / Refresh").clicked() {
            *monitors = controller.detect_monitors();
        }
        if ui.button("Identify Monitors").clicked() {
            let geometry = enumerate_monitor_geometry();
            if geometry.is_empty() {
                controller.push_log("Identify Monitors: no monitors reported by Win32.");
            } else {
                identify_requested = Some(geometry);
            }
        }
    });

    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, m) in monitors.iter().enumerate() {
            ui.group(|ui| {
                // "Monitor N" here matches the number shown by the Identify
                // Monitors overlay (both count from the same enumeration
                // order), not the 0-based internal key shown below.
                let display_name = m
                    .hardware_info
                    .as_ref()
                    .and_then(|hw| hw.model_name.clone())
                    .unwrap_or_else(|| m.description.clone());
                ui.heading(format!("Monitor {} - {display_name}", i + 1));

                match &m.hardware_info {
                    Some(hw) => {
                        let mut details = Vec::new();
                        if let Some(mfr) = &hw.manufacturer {
                            details.push(format!("Manufacturer code: {mfr}"));
                        }
                        if let Some(serial) = &hw.serial {
                            details.push(format!("Serial: {serial}"));
                        }
                        match (hw.manufacture_week, hw.manufacture_year) {
                            (Some(week), Some(year)) => details.push(format!("Manufactured: week {week}, {year}")),
                            (None, Some(year)) => details.push(format!("Manufactured: {year}")),
                            _ => {}
                        }
                        if !details.is_empty() {
                            ui.label(details.join("   |   "));
                        }
                    }
                    None => {
                        ui.label("(no EDID info available for this monitor - Windows/WMI didn't report it)");
                    }
                }
                ui.small(format!("Internal key: {}   |   OS description: \"{}\"", m.key.0, m.description));

                ui.add_space(4.0);
                if m.input_capabilities.is_empty() {
                    ui.label("No VCP 0x60 capability list reported - use manual raw-hex input in Profiles.");
                } else {
                    ui.label("Reported input values (may not match reality - some monitors misreport this):");
                    for cap in &m.input_capabilities {
                        ui.label(format!("  {} = 0x{:02X}", cap.name, cap.code));
                    }
                }

                let mut verify = controller.with_config_mut(|cfg| cfg.verify_enabled(&m.key));
                if ui
                    .checkbox(&mut verify, "Verify switches on this monitor")
                    .on_hover_text(
                        "Reads the input back after switching to confirm it actually took, \
                         and retries a couple of times if not. Turn this off if this monitor's \
                         status reports don't match what you actually see on screen - some \
                         monitors' read-back doesn't reliably reflect their true active input.",
                    )
                    .changed()
                {
                    controller.with_config_mut(|cfg| {
                        cfg.monitor_verify.insert(m.key.clone(), verify);
                    });
                    let _ = controller.save_config();
                }
            });
        }
        if monitors.is_empty() {
            ui.label("No monitors detected yet - click Detect / Refresh.");
        }
    });

    identify_requested
}
