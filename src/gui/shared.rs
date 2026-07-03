use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::controller::Controller;
use crate::hotkeys::HotkeyRegistry;
use crate::platform::MonitorSnapshot;
use crate::tray::AppTray;

/// All state the main window's controls/event handlers need to reach into.
/// Owned by a single `Rc<Shared>` held by `MainWindow` and cloned into every
/// event closure - this app is single-threaded on the UI side (one winsafe
/// message loop, no winit), so `Rc`/`RefCell` is enough for everything except
/// `controller` (still `Arc`, since `Controller::apply_profile` spawns
/// disposable worker threads for DDC/CI writes) and `hotkey_lookup` (still
/// `Arc<Mutex<..>>`, since it's read from the `GlobalHotKeyEvent` callback,
/// which - like tray/menu callbacks - runs on this same thread but through an
/// API that requires `Send + Sync` closures at the type level regardless).
pub struct Shared {
    pub controller: Arc<Controller>,
    pub monitors: RefCell<Vec<MonitorSnapshot>>,
    pub hotkeys: RefCell<HotkeyRegistry>,
    pub hotkey_lookup: Arc<Mutex<HashMap<u32, Uuid>>>,
    pub tray: RefCell<AppTray>,
}
