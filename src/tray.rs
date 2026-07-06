use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::config::Profile;

pub const OPEN_SETTINGS_ID: &str = "open-settings";
pub const QUIT_ID: &str = "quit";

pub struct AppTray {
    pub tray_icon: TrayIcon,
}

pub fn build_tray(profiles: &[Profile]) -> anyhow::Result<AppTray> {
    let menu = build_menu(profiles);
    let icon = load_icon();

    let tray_icon = TrayIconBuilder::new()
        .with_tooltip("Monitor Switcher")
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .build()?;

    Ok(AppTray { tray_icon })
}

/// Rebuilds and swaps the tray context menu - call after profiles are added,
/// renamed, or removed so the quick-switch entries stay in sync.
pub fn rebuild_menu(tray: &mut AppTray, profiles: &[Profile]) {
    let menu = build_menu(profiles);
    tray.tray_icon.set_menu(Some(Box::new(menu)));
}

fn build_menu(profiles: &[Profile]) -> Menu {
    let menu = Menu::new();

    let open_settings = MenuItem::with_id(OPEN_SETTINGS_ID, "Open Settings", true, None);
    let _ = menu.append(&open_settings);
    let _ = menu.append(&PredefinedMenuItem::separator());

    for profile in profiles {
        // Must use `with_id` (the profile's own UUID) rather than `new` -
        // `MenuItem::new` lets muda auto-assign an internal counter-based id
        // (e.g. "1000", "1001", ...), which the `MenuEvent` handler in
        // `gui/mod.rs` can never parse back into a profile `Uuid`, so the
        // quick-switch entry would silently do nothing when clicked.
        let item = MenuItem::with_id(profile.id.to_string(), &profile.name, true, None);
        let _ = menu.append(&item);
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
    let _ = menu.append(&quit);

    menu
}

fn load_icon() -> Icon {
    // Pre-rendered RGBA (no PNG decode dependency needed) - see assets/gen_icons.py.
    const SIZE: u32 = 32;
    const RGBA: &[u8] = include_bytes!("../assets/tray_icon_32.rgba");
    Icon::from_rgba(RGBA.to_vec(), SIZE, SIZE).expect("valid icon dimensions")
}
