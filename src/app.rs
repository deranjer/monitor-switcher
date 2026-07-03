use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender};
use global_hotkey::GlobalHotKeyEvent;
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
use uuid::Uuid;

use crate::config::Config;
use crate::controller::{AppEvent, Controller, LogEntry};
use crate::gui;
use crate::hotkeys::HotkeyRegistry;
use crate::platform::MonitorSnapshot;
use crate::tray::{self, AppTray, OPEN_SETTINGS_ID, QUIT_ID};

/// Window-visibility/lifecycle commands produced by tray/menu callbacks and
/// consumed by `update()`, since they need to mutate `MonitorSwitcherApp`'s own
/// fields (e.g. `quitting`). Profile-apply requests, by contrast, are handled
/// directly inside the callbacks via `Controller::apply_profile` - no need to
/// round-trip through this channel for those.
enum UiCommand {
    ToggleWindow,
    OpenSettings,
    Quit,
}

pub struct MonitorSwitcherApp {
    controller: Arc<Controller>,
    tray: AppTray,
    /// Owned solely by the main thread - Windows' `GlobalHotKeyManager` wraps a
    /// raw `HWND` and is `!Send`, so this can never be wrapped in an `Arc` and
    /// shared into a callback closure (see `controller.rs` doc comment).
    hotkeys: HotkeyRegistry,
    /// The `Send`-safe id->profile lookup the hotkey-fired callback actually
    /// reads; kept in sync with `hotkeys` every time profiles change.
    hotkey_lookup: Arc<Mutex<HashMap<u32, Uuid>>>,
    events_rx: Receiver<AppEvent>,
    ui_cmd_rx: Receiver<UiCommand>,
    log_cache: Vec<LogEntry>,
    monitors_cache: Vec<MonitorSnapshot>,
    quitting: bool,
    gui_state: gui::GuiState,
}

impl MonitorSwitcherApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        let (controller, events_rx) = Controller::new(config);

        let mut hotkeys = HotkeyRegistry::new().expect("failed to init global hotkey manager");
        hotkeys.sync(&controller.profiles_snapshot());
        let hotkey_lookup = Arc::new(Mutex::new(hotkeys.lookup_snapshot()));

        let tray = tray::build_tray(&controller.profiles_snapshot()).expect("failed to build tray icon");

        let (ui_cmd_tx, ui_cmd_rx) = crossbeam_channel::unbounded();
        install_callbacks(ui_cmd_tx, &controller, &hotkey_lookup, cc.egui_ctx.clone());

        let monitors_cache = controller.detect_monitors();

        MonitorSwitcherApp {
            controller,
            tray,
            hotkeys,
            hotkey_lookup,
            events_rx,
            ui_cmd_rx,
            log_cache: Vec::new(),
            monitors_cache,
            quitting: false,
            gui_state: gui::GuiState::default(),
        }
    }
}

/// Registers the tray/menu/hotkey event handlers exactly once. Uses
/// `set_event_handler` (callback), not the polling `receiver()` pattern - the
/// callback's only job is to decide *what* happened; actual profile
/// application still happens on the controller's own worker thread, never
/// inside this callback.
fn install_callbacks(
    ui_cmd_tx: Sender<UiCommand>,
    controller: &Arc<Controller>,
    hotkey_lookup: &Arc<Mutex<HashMap<u32, Uuid>>>,
    ctx: egui::Context,
) {
    let tx = ui_cmd_tx.clone();
    let ctx1 = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click { button, button_state, .. } = event {
            if button == tray_icon::MouseButton::Left
                && button_state == tray_icon::MouseButtonState::Up
            {
                let _ = tx.send(UiCommand::ToggleWindow);
                ctx1.request_repaint();
            }
        }
    }));

    let tx = ui_cmd_tx;
    let ctx2 = ctx.clone();
    let controller_for_menu = Arc::clone(controller);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id_str = event.id().0.as_str();
        if id_str == OPEN_SETTINGS_ID {
            let _ = tx.send(UiCommand::OpenSettings);
        } else if id_str == QUIT_ID {
            let _ = tx.send(UiCommand::Quit);
        } else if let Ok(profile_id) = Uuid::parse_str(id_str) {
            controller_for_menu.apply_profile(profile_id);
        }
        ctx2.request_repaint();
    }));

    let ctx3 = ctx;
    let controller_for_hotkey = Arc::clone(controller);
    let hotkey_lookup = Arc::clone(hotkey_lookup);
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        // global-hotkey's Windows backend emits both a Pressed event (on the
        // WM_HOTKEY message) and, from a background poller, a later Released
        // event once the key is lifted - acting on both would apply every
        // profile twice per physical press.
        if event.state != global_hotkey::HotKeyState::Pressed {
            return;
        }
        let profile_id = hotkey_lookup.lock().unwrap().get(&event.id).copied();
        if let Some(profile_id) = profile_id {
            controller_for_hotkey.apply_profile(profile_id);
        }
        ctx3.request_repaint();
    }));
}

impl eframe::App for MonitorSwitcherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Keep ticking at a low rate even with no window/input events, so the
        // command/event channels never back up indefinitely while hidden.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));

        // Intercept the [x] close button: hide, don't exit, so the event loop
        // (and therefore tray/hotkey handling) keeps running.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        while let Ok(cmd) = self.ui_cmd_rx.try_recv() {
            match cmd {
                UiCommand::ToggleWindow | UiCommand::OpenSettings => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                UiCommand::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        if self.events_rx.try_recv().is_ok() {
            self.log_cache = self.controller.log_snapshot();
            // Drain any additional pending signals from this batch without
            // refreshing the log repeatedly.
            while self.events_rx.try_recv().is_ok() {}
        }

        gui::render(
            ui,
            &mut self.gui_state,
            &self.controller,
            &mut self.monitors_cache,
            &self.log_cache,
        );

        if self.gui_state.take_profiles_dirty() {
            let profiles = self.controller.profiles_snapshot();
            self.hotkeys.sync(&profiles);
            *self.hotkey_lookup.lock().unwrap() = self.hotkeys.lookup_snapshot();
            tray::rebuild_menu(&mut self.tray, &profiles);
            let _ = self.controller.save_config();
        }
    }
}
