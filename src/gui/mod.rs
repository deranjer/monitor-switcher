mod hotkey_capture;
mod log_panel;
mod monitors_tab;
mod profiles_tab;

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::controller::{Controller, LogEntry};
use crate::platform::windows::autostart;
use crate::platform::windows::monitor_identity::MonitorGeometry;
use crate::platform::MonitorSnapshot;

/// How long each "Identify Monitors" overlay stays on screen before
/// auto-dismissing.
const IDENTIFY_DURATION: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Monitors,
    Profiles,
    Log,
    Settings,
}

pub struct GuiState {
    tab: Tab,
    monitors: monitors_tab::State,
    profiles: profiles_tab::State,
    /// Set whenever a profile/hotkey edit needs to be persisted and
    /// re-registered; drained by `app.rs` once per frame.
    profiles_dirty: bool,
    /// Active "Identify Monitors" overlays, if any - one numbered full-screen
    /// viewport per physical monitor, positioned via Win32 geometry
    /// (independent of DDC/CI), auto-dismissing after `IDENTIFY_DURATION`.
    identify: Option<(Vec<MonitorGeometry>, Instant)>,
}

impl Default for GuiState {
    fn default() -> Self {
        GuiState {
            tab: Tab::Monitors,
            monitors: monitors_tab::State::default(),
            profiles: profiles_tab::State::default(),
            profiles_dirty: false,
            identify: None,
        }
    }
}

impl GuiState {
    pub fn take_profiles_dirty(&mut self) -> bool {
        std::mem::take(&mut self.profiles_dirty)
    }

    pub fn start_identify(&mut self, geometries: Vec<MonitorGeometry>) {
        self.identify = Some((geometries, Instant::now()));
    }
}

pub fn render(
    ui: &mut egui::Ui,
    state: &mut GuiState,
    controller: &Arc<Controller>,
    monitors: &mut Vec<MonitorSnapshot>,
    log: &[LogEntry],
) {
    egui::Panel::top("tabs").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.tab, Tab::Monitors, "Monitors");
            ui.selectable_value(&mut state.tab, Tab::Profiles, "Profiles");
            ui.selectable_value(&mut state.tab, Tab::Log, "Log");
            ui.selectable_value(&mut state.tab, Tab::Settings, "Settings");
        });
    });

    let identify_requested = egui::CentralPanel::default()
        .show(ui, |ui| match state.tab {
            Tab::Monitors => monitors_tab::show(ui, &mut state.monitors, controller, monitors),
            Tab::Profiles => {
                let dirty = profiles_tab::show(ui, &mut state.profiles, controller, monitors);
                if dirty {
                    state.profiles_dirty = true;
                }
                None
            }
            Tab::Log => {
                log_panel::show(ui, log);
                None
            }
            Tab::Settings => {
                show_settings(ui, controller);
                None
            }
        })
        .inner;

    if let Some(geometries) = identify_requested {
        state.start_identify(geometries);
    }

    show_identify_overlays(ui, state);
}

/// Size (in points) of each identify badge, and its margin from the
/// monitor's bottom-right corner.
const IDENTIFY_BADGE_SIZE: f32 = 120.0;
const IDENTIFY_BADGE_MARGIN: f32 = 32.0;

/// Renders one numbered, click-through, always-on-top badge in the
/// bottom-right corner of each monitor while an "Identify Monitors" request
/// is active, positioned from each monitor's Win32 geometry. Must run every
/// frame the overlay should stay visible - `show_viewport_immediate` closes a
/// viewport automatically once a frame stops requesting it, which is also how
/// the auto-dismiss timeout works (once expired, this function simply stops
/// calling it).
fn show_identify_overlays(ui: &mut egui::Ui, state: &mut GuiState) {
    if let Some((_, started)) = &state.identify {
        if started.elapsed() > IDENTIFY_DURATION {
            state.identify = None;
        }
    }

    let Some((geometries, started)) = &state.identify else {
        return;
    };
    let remaining = IDENTIFY_DURATION.saturating_sub(started.elapsed());
    let ctx = ui.ctx().clone();

    for (i, geo) in geometries.iter().enumerate() {
        let viewport_id = egui::ViewportId::from_hash_of(("identify-overlay", i));
        let x = geo.right as f32 - IDENTIFY_BADGE_SIZE - IDENTIFY_BADGE_MARGIN;
        let y = geo.bottom as f32 - IDENTIFY_BADGE_SIZE - IDENTIFY_BADGE_MARGIN;
        let builder = egui::ViewportBuilder::default()
            .with_position([x, y])
            .with_inner_size([IDENTIFY_BADGE_SIZE, IDENTIFY_BADGE_SIZE])
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top()
            .with_transparent(true)
            .with_taskbar(false)
            .with_mouse_passthrough(true);

        ctx.show_viewport_immediate(viewport_id, builder, move |ui, _class| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::from_black_alpha(200)).corner_radius(12))
                .show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(format!("{}", i + 1)).size(64.0).color(egui::Color32::WHITE).strong());
                    });
                });
        });
    }

    // Keep repainting while the overlay is up so the auto-dismiss timeout
    // (checked at the top of this function) actually gets re-evaluated even
    // if nothing else is generating frames.
    ctx.request_repaint_after(remaining.min(Duration::from_millis(100)));
}

fn show_settings(ui: &mut egui::Ui, controller: &Arc<Controller>) {
    let mut enabled = autostart::is_enabled();
    if ui.checkbox(&mut enabled, "Start automatically at Windows login").changed() {
        let exe_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(str::to_string));
        let result = if enabled {
            exe_path.ok_or_else(|| anyhow::anyhow!("could not resolve exe path")).and_then(|p| {
                autostart::enable(&p).map_err(|e| anyhow::anyhow!("{e}"))
            })
        } else {
            autostart::disable().map_err(|e| anyhow::anyhow!("{e}"))
        };
        if let Err(e) = result {
            tracing::warn!("failed to update autostart registration: {e}");
        }
        controller.with_config_mut(|cfg| cfg.autostart_enabled = autostart::is_enabled());
        let _ = controller.save_config();
    }
    ui.label("When enabled, Monitor Switcher launches directly to the tray at login (no window flash).");
}
