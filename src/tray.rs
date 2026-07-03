use std::collections::HashMap;

use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use uuid::Uuid;

use crate::config::Profile;

pub const OPEN_SETTINGS_ID: &str = "open-settings";
pub const QUIT_ID: &str = "quit";

pub struct AppTray {
    pub tray_icon: TrayIcon,
    /// Menu item id -> profile id, for the quick-switch entries.
    pub profile_menu_ids: HashMap<MenuId, Uuid>,
}

pub fn build_tray(profiles: &[Profile]) -> anyhow::Result<AppTray> {
    let (menu, profile_menu_ids) = build_menu(profiles);
    let icon = load_icon();

    let tray_icon = TrayIconBuilder::new()
        .with_tooltip("Monitor Switcher")
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .build()?;

    Ok(AppTray {
        tray_icon,
        profile_menu_ids,
    })
}

/// Rebuilds and swaps the tray context menu - call after profiles are added,
/// renamed, or removed so the quick-switch entries stay in sync.
pub fn rebuild_menu(tray: &mut AppTray, profiles: &[Profile]) {
    let (menu, profile_menu_ids) = build_menu(profiles);
    tray.tray_icon.set_menu(Some(Box::new(menu)));
    tray.profile_menu_ids = profile_menu_ids;
}

fn build_menu(profiles: &[Profile]) -> (Menu, HashMap<MenuId, Uuid>) {
    let menu = Menu::new();
    let mut profile_menu_ids = HashMap::new();

    let open_settings = MenuItem::with_id(OPEN_SETTINGS_ID, "Open Settings", true, None);
    let _ = menu.append(&open_settings);
    let _ = menu.append(&PredefinedMenuItem::separator());

    for profile in profiles {
        let item = MenuItem::new(&profile.name, true, None);
        profile_menu_ids.insert(item.id().clone(), profile.id);
        let _ = menu.append(&item);
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
    let _ = menu.append(&quit);

    (menu, profile_menu_ids)
}

fn load_icon() -> Icon {
    // A minimal embedded 16x16 solid-color RGBA icon - avoids a runtime PNG
    // decode dependency; swap for a real asset via include_bytes! + image
    // decode later if a nicer icon is wanted.
    const SIZE: u32 = 16;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for _ in 0..(SIZE * SIZE) {
        rgba.extend_from_slice(&[0x2b, 0x6c, 0xb0, 0xff]);
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("valid icon dimensions")
}
