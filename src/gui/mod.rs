mod hotkey_capture;
pub mod identify_overlay;
mod log_panel;
mod monitors_panel;
mod profiles_panel;
mod settings_panel;
mod shared;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use uuid::Uuid;
use winsafe::gui;
use winsafe::prelude::*;

use crate::controller::Controller;
use crate::hotkeys::HotkeyRegistry;
use crate::tray::AppTray;

pub use shared::Shared;

/// winsafe's high-level `ListBoxItems`/`ComboBox` wrappers don't expose
/// get/set-selection (only `iter_selected`, which is awkward for "what's the
/// one selected index" and has no setter at all) - these send the underlying
/// `LB_GETCURSEL`/`LB_SETCURSEL`/`CB_GETCURSEL`/`CB_SETCURSEL` messages
/// directly, same as the higher-level methods do internally for other ops.
pub(crate) fn listbox_selected_index(list: &gui::ListBox) -> Option<u32> {
    unsafe { list.hwnd().SendMessage(winsafe::msg::lb::GetCurSel {}) }
}
pub(crate) fn listbox_select(list: &gui::ListBox, index: Option<u32>) {
    let _ = unsafe { list.hwnd().SendMessage(winsafe::msg::lb::SetCurSel { index }) };
}
pub(crate) fn combo_selected_index(combo: &gui::ComboBox) -> Option<u32> {
    unsafe { combo.hwnd().SendMessage(winsafe::msg::cb::GetCurSel {}) }
}
pub(crate) fn combo_select(combo: &gui::ComboBox, index: Option<u32>) {
    unsafe { combo.hwnd().SendMessage(winsafe::msg::cb::SetCurSel { index }) };
}

/// Fixed pool size for per-monitor profile-assignment rows - winsafe requires
/// all controls to exist before the window runs (no creating them later at
/// runtime), so instead of a truly dynamic list, a generous fixed number of
/// rows are pre-created and only as many as are actually detected are shown.
pub const MAX_MONITORS: usize = 6;

const WINDOW_W: i32 = 780;
const WINDOW_H: i32 = 600;
/// Client area available to each tab page's own `WindowControl`, i.e. the
/// main window's tab area minus a little breathing room around the edges.
const PANEL_W: i32 = WINDOW_W - 40;
const PANEL_H: i32 = WINDOW_H - 90;

const HOTKEY_CAPTURE_TIMER_ID: usize = 100;

// Custom WM_APP-range messages, used to get from tray-icon/global-hotkey's
// Send+Sync-bounded callbacks (which can't capture the non-Send Rc<MainWindow>
// directly) back onto the UI thread - those callbacks instead capture a
// cloned `WindowMain` (explicitly marked Send by winsafe, since it's just a
// thread-safe HWND wrapper under the hood) and PostMessage one of these,
// which MainWindow's own `.on().wm(...)` handler (registered on the UI
// thread, so capturing Rc freely is fine there) picks up.
const WM_APP_SHOW_WINDOW: u32 = 0x8000 + 1;
const WM_APP_QUIT: u32 = 0x8000 + 2;

/// Per-monitor row of controls in the Profiles tab's assignment editor - one
/// pre-created set per slot in the `MAX_MONITORS` pool.
pub struct ProfileMonitorRow {
    pub label: gui::Label,
    pub combo: gui::ComboBox,
    pub manual_chk: gui::CheckBox,
    pub hex_edit: gui::Edit,
}

/// `winsafe::gui::Tab`'s `items` field wants `Box<dyn AsRef<WindowControl>>`,
/// but `WindowControl` has no `AsRef<Self>` impl of its own - this trivial
/// wrapper supplies it.
struct PanelRef(gui::WindowControl);
impl AsRef<gui::WindowControl> for PanelRef {
    fn as_ref(&self) -> &gui::WindowControl {
        &self.0
    }
}

/// A `HWND`'s address, as a plain integer - `Send + Sync` unlike `HWND`
/// itself (which winsafe marks `Send` but not `Sync`) or `WindowMain` (whose
/// internal fields are neither). Reconstructed with `HWND::from_ptr` just
/// before use. Safe here because the only thing ever done with it is
/// `PostMessage`, which is documented thread-safe regardless of which thread
/// created the window.
#[derive(Clone, Copy)]
struct SendableHwnd(usize);

impl SendableHwnd {
    fn from_window(wnd: &gui::WindowMain) -> Self {
        Self(wnd.hwnd().ptr() as usize)
    }

    fn post(self, msg_id: u32) {
        unsafe {
            let hwnd = winsafe::HWND::from_ptr(self.0 as *mut std::ffi::c_void);
            let _ = hwnd.PostMessage(winsafe::msg::WndMsg {
                msg_id: winsafe::co::WM::from_raw(msg_id),
                wparam: 0,
                lparam: 0,
            });
        }
    }
}

pub struct MainWindow {
    pub wnd: gui::WindowMain,
    pub shared: Rc<Shared>,
    tab: gui::Tab,
    quitting: Cell<bool>,

    // Monitors panel
    pub mon_list: gui::ListBox,
    pub mon_detect_btn: gui::Button,
    pub mon_identify_btn: gui::Button,
    pub mon_detail: gui::Edit,
    pub mon_verify_chk: gui::CheckBox,

    // Profiles panel
    pub prof_list: gui::ListBox,
    pub prof_new_name: gui::Edit,
    pub prof_add_btn: gui::Button,
    pub prof_delete_btn: gui::Button,
    pub prof_name_edit: gui::Edit,
    pub prof_hotkey_btn: gui::Button,
    pub prof_rows: Vec<ProfileMonitorRow>,
    selected_profile: Cell<Option<Uuid>>,
    capturing_hotkey: Cell<bool>,

    // Log panel
    pub log_edit: gui::Edit,

    // Settings panel
    pub settings_autostart_chk: gui::CheckBox,
}

impl MainWindow {
    pub fn new(
        controller: Arc<Controller>,
        hotkeys: HotkeyRegistry,
        hotkey_lookup: Arc<Mutex<HashMap<u32, Uuid>>>,
        tray: AppTray,
        start_visible: bool,
    ) -> Rc<Self> {
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            title: "Monitor Switcher".to_owned(),
            // Resource ID 1 is where `build.rs` (via `winresource`) embeds
            // `assets/app_icon.ico` into the exe - this is also what makes
            // the icon show in the taskbar/alt-tab, since without a
            // class_icon winsafe registers the window class with no icon at
            // all (the tray icon is unrelated - `tray-icon` loads its own
            // RGBA buffer directly, it never touches the window class).
            class_icon: gui::Icon::Id(1),
            size: (WINDOW_W, WINDOW_H),
            style: winsafe::co::WS::CAPTION
                | winsafe::co::WS::SYSMENU
                | winsafe::co::WS::CLIPCHILDREN
                | winsafe::co::WS::BORDER
                | winsafe::co::WS::MINIMIZEBOX
                | if start_visible { winsafe::co::WS::VISIBLE } else { Default::default() },
            ..Default::default()
        });

        let mon = monitors_panel::build(&wnd);
        let prof = profiles_panel::build(&wnd);
        let log = log_panel::build(&wnd);
        let settings = settings_panel::build(&wnd);

        let tab = gui::Tab::new(
            &wnd,
            gui::TabOpts {
                position: (8, 8),
                size: (WINDOW_W - 32, WINDOW_H - 40),
                items: vec![
                    ("Monitors".to_owned(), Box::new(PanelRef(mon.panel.clone()))),
                    ("Profiles".to_owned(), Box::new(PanelRef(prof.panel.clone()))),
                    ("Log".to_owned(), Box::new(PanelRef(log.panel.clone()))),
                    ("Settings".to_owned(), Box::new(PanelRef(settings.panel.clone()))),
                ],
                ..Default::default()
            },
        );

        let shared = Rc::new(Shared {
            controller,
            monitors: RefCell::new(Vec::new()),
            hotkeys: RefCell::new(hotkeys),
            hotkey_lookup,
            tray: RefCell::new(tray),
        });

        let this = Rc::new(MainWindow {
            wnd,
            shared,
            tab,
            quitting: Cell::new(false),
            mon_list: mon.list,
            mon_detect_btn: mon.detect_btn,
            mon_identify_btn: mon.identify_btn,
            mon_detail: mon.detail,
            mon_verify_chk: mon.verify_chk,
            prof_list: prof.list,
            prof_new_name: prof.new_name,
            prof_add_btn: prof.add_btn,
            prof_delete_btn: prof.delete_btn,
            prof_name_edit: prof.name_edit,
            prof_hotkey_btn: prof.hotkey_btn,
            prof_rows: prof.rows,
            selected_profile: Cell::new(None),
            capturing_hotkey: Cell::new(false),
            log_edit: log.edit,
            settings_autostart_chk: settings.autostart_chk,
        });

        this.wire_events();
        this
    }

    fn wire_events(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.wnd.on().wm_create(move |_| {
            this.on_create();
            Ok(0)
        });

        // Hide, don't destroy, on the [x] button - keeps the tray/hotkey
        // message loop alive (this is the *only* window in the process, so
        // destroying it would end the whole app). Only the real WM_APP_QUIT
        // path (Quit menu item) sets `quitting` first and lets this actually
        // destroy the window.
        let this = Rc::clone(self);
        self.wnd.on().wm_close(move || {
            if this.quitting.get() {
                let _ = this.wnd.hwnd().DestroyWindow();
            } else {
                this.wnd.hwnd().ShowWindow(winsafe::co::SW::HIDE);
            }
            Ok(())
        });

        let this = Rc::clone(self);
        self.wnd.on().wm(unsafe { winsafe::co::WM::from_raw(WM_APP_SHOW_WINDOW) }, move |_| {
            this.show_window();
            Ok(0)
        });

        let this = Rc::clone(self);
        self.wnd.on().wm(unsafe { winsafe::co::WM::from_raw(WM_APP_QUIT) }, move |_| {
            this.quitting.set(true);
            let _ = unsafe { this.wnd.hwnd().PostMessage(winsafe::msg::wm::Close {}) };
            Ok(0)
        });

        let this = Rc::clone(self);
        self.tab.on().tcn_sel_change(move || {
            if this.tab.items().selected().map(|it| it.index()) == Some(2) {
                this.refresh_log();
            }
            Ok(())
        });

        self.wire_monitors_events();
        self.wire_profiles_events();
        self.wire_settings_events();
    }

    fn on_create(self: &Rc<Self>) {
        self.refresh_monitors();
        self.refresh_profiles_list();
        self.refresh_log();
        let autostart_on = crate::platform::windows::autostart::is_enabled();
        self.settings_autostart_chk.set_check(autostart_on);

        // Must happen here, not any earlier: `self.wnd.hwnd()` is null until
        // the real window is created (which happens inside `run_main()`, one
        // step before WM_CREATE fires) - installing these callbacks before
        // that point would capture a null HWND, silently breaking
        // "PostMessage" for the whole app (tray clicks would visibly fire the
        // callback but the window would never actually show).
        self.install_tray_and_hotkey_callbacks();
    }

    /// Shows (and focuses) the window - called from the tray click / "Open
    /// Settings" menu handlers, and from the `WM_APP_SHOW_WINDOW` message
    /// posted by those (Send-bounded) callbacks.
    pub fn show_window(&self) {
        self.wnd.hwnd().ShowWindow(winsafe::co::SW::SHOW);
        let _ = self.wnd.hwnd().SetForegroundWindow();
    }

    pub fn refresh_log(&self) {
        let entries = self.shared.controller.log_snapshot();
        let text: String = entries
            .iter()
            .map(|e| format!("[{}] {}\r\n", e.timestamp, e.message))
            .collect();
        let _ = self.log_edit.set_text(&text);
    }

    /// Registers the tray/menu/hotkey event handlers exactly once, after the
    /// window has been constructed (and hotkeys synced into `hotkey_lookup`).
    /// Uses `set_event_handler` (callback), not the polling `receiver()`
    /// pattern. These callbacks are `Send + Sync`-bounded by `tray-icon`/
    /// `global-hotkey`'s own APIs, so they can't capture the non-`Send`,
    /// non-`Sync` `Rc<MainWindow>` (or even `WindowMain`/`HWND` themselves,
    /// which winsafe marks `Send` but not `Sync`) - instead they capture a
    /// `SendableHwnd` (a plain integer address, trivially `Send + Sync`) to
    /// post the `WM_APP_*` messages above, and an `Arc<Controller>` to call
    /// `apply_profile` directly (`Controller` is itself `Send + Sync` and
    /// spawns its own worker thread per apply, so no UI-thread hop is needed
    /// for that part).
    pub fn install_tray_and_hotkey_callbacks(self: &Rc<Self>) {
        let hwnd = SendableHwnd::from_window(&self.wnd);
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event: tray_icon::TrayIconEvent| {
            if let tray_icon::TrayIconEvent::Click { button, button_state, .. } = event
                && button == tray_icon::MouseButton::Left && button_state == tray_icon::MouseButtonState::Up {
                    hwnd.post(WM_APP_SHOW_WINDOW);
                }
        }));

        let hwnd = SendableHwnd::from_window(&self.wnd);
        let controller = Arc::clone(&self.shared.controller);
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |event: tray_icon::menu::MenuEvent| {
            let id_str = event.id().0.as_str();
            if id_str == crate::tray::OPEN_SETTINGS_ID {
                hwnd.post(WM_APP_SHOW_WINDOW);
            } else if id_str == crate::tray::QUIT_ID {
                hwnd.post(WM_APP_QUIT);
            } else if let Ok(profile_id) = Uuid::parse_str(id_str) {
                controller.apply_profile(profile_id);
            }
        }));

        let controller = Arc::clone(&self.shared.controller);
        let hotkey_lookup = Arc::clone(&self.shared.hotkey_lookup);
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |event: global_hotkey::GlobalHotKeyEvent| {
            // global-hotkey's Windows backend emits both a Pressed event (on
            // the WM_HOTKEY message) and, from a background poller, a later
            // Released event once the key is lifted - acting on both would
            // apply every profile twice per physical press.
            if event.state != global_hotkey::HotKeyState::Pressed {
                return;
            }
            let profile_id = hotkey_lookup.lock().unwrap().get(&event.id).copied();
            if let Some(profile_id) = profile_id {
                controller.apply_profile(profile_id);
            }
        }));
    }
}
