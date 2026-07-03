#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod controller;
mod gui;
mod hotkeys;
mod platform;
mod tray;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use platform::windows::WindowsDdcBackend;
use platform::DdcBackend;

fn main() -> anyhow::Result<()> {
    // Must happen before any window is created (winsafe's `gui::dpi()` only
    // scales layout numbers - it does not declare DPI awareness to Windows).
    // Without this, the process defaults to DPI-unaware and DWM
    // bitmap-stretches the whole window on any monitor above 100% scaling,
    // turning the native controls blurry.
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Diagnostic CLI flags (kept in the shipped binary permanently, hidden is
    // fine) - the fastest tool for "it doesn't detect my monitor" reports,
    // and how Milestones 0/1 get verified against real hardware before any
    // GUI/tray/hotkey code is trusted.
    if args.iter().any(|a| a == "--debug-monitors") {
        return debug_monitors();
    }
    if args.iter().any(|a| a == "--debug-monitors-sta") {
        // Simulates the GUI's main-thread COM state (winsafe/native controls
        // may initialize COM apartment-threaded for OLE drag-and-drop/shell
        // integration) before calling enumerate() - a regression test for the
        // bug where WMI hardware-info lookup silently returned nothing inside
        // the actual GUI despite working fine from this CLI's fresh thread.
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        return debug_monitors();
    }
    if let Some(pos) = args.iter().position(|a| a == "--debug-set-input") {
        let index: usize = args
            .get(pos + 1)
            .and_then(|s| s.parse().ok())
            .expect("usage: --debug-set-input <index> <hex>");
        let hex: String = args
            .get(pos + 2)
            .cloned()
            .expect("usage: --debug-set-input <index> <hex>");
        return debug_set_input(index, &hex);
    }

    // Claim single-instance ownership before creating anything (tray icon,
    // hotkey manager, window) - a second launch just exits quietly here
    // rather than spawning a duplicate tray icon and colliding with the
    // first instance's hotkey registrations and config.json writes. This
    // must stay alive for the rest of `main`, since dropping it releases the
    // mutex and would let a third instance start.
    let Some(_single_instance_guard) = platform::windows::single_instance::acquire() else {
        tracing::info!("another instance is already running; exiting");
        return Ok(());
    };

    let launch_tray_only = args.iter().any(|a| a == "--tray");

    let cfg = config::load()?;
    let start_visible = !launch_tray_only && !cfg.launch_minimized;

    let controller = controller::Controller::new(cfg);

    let mut hotkeys = hotkeys::HotkeyRegistry::new()?;
    hotkeys.sync(&controller.profiles_snapshot());
    let hotkey_lookup: Arc<Mutex<HashMap<u32, uuid::Uuid>>> = Arc::new(Mutex::new(hotkeys.lookup_snapshot()));

    let app_tray = tray::build_tray(&controller.profiles_snapshot())?;

    let main_window = gui::MainWindow::new(controller, hotkeys, hotkey_lookup, app_tray, start_visible);

    main_window
        .wnd
        .run_main(Some(if start_visible { winsafe::co::SW::SHOW } else { winsafe::co::SW::HIDE }))
        .map_err(|e| anyhow::anyhow!("window run_main error: {e}"))?;

    Ok(())
}

fn debug_monitors() -> anyhow::Result<()> {
    let backend = WindowsDdcBackend::new();
    for (i, m) in backend.enumerate().into_iter().enumerate() {
        println!("[{i}] key={} description=\"{}\"", m.key.0, m.description);
        match &m.hardware_info {
            Some(hw) => {
                println!(
                    "    hardware: manufacturer={:?} model={:?} serial={:?} mfg={:?}/{:?}",
                    hw.manufacturer, hw.model_name, hw.serial, hw.manufacture_week, hw.manufacture_year
                );
            }
            None => println!("    hardware: (no WMI EDID info correlated)"),
        }
        if m.input_capabilities.is_empty() {
            println!("    (no VCP 0x60 capability list reported)");
        } else {
            for cap in &m.input_capabilities {
                println!("    {} = 0x{:02X}", cap.name, cap.code);
            }
        }
    }
    Ok(())
}

fn debug_set_input(index: usize, hex: &str) -> anyhow::Result<()> {
    let code = u8::from_str_radix(hex.trim_start_matches("0x"), 16)?;
    let backend = WindowsDdcBackend::new();
    let monitors = backend.enumerate();
    let m = monitors
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("no monitor at index {index}"))?;
    println!("Setting {} to 0x{code:02X}...", m.description);
    let (previous, result) = backend.apply(&m.key, code, true);
    if let Some(previous) = previous {
        println!("(was reading 0x{previous:02X} beforehand)");
    }
    result.map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("Done.");
    Ok(())
}
