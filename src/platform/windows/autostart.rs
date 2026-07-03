//! Hand-rolled `HKCU\...\Run` autostart entry - deliberately not using the
//! `auto-launch` crate (see plan Section 7): this app only ever needs the
//! per-user HKCU path, and hand-rolling keeps full control over the exact
//! command-line quoting/flag used, with the registry as the single source of
//! truth (read it back rather than trusting a cached config bool, in case the
//! user removed the entry via Task Manager's Startup tab).

use windows::core::{Result, HSTRING};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE,
};

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "MonitorSwitcher";

fn open_run_key(access: u32) -> Result<HKEY> {
    let mut hkey = HKEY::default();
    let path = HSTRING::from(RUN_KEY_PATH);
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &path,
            Some(0),
            windows::Win32::System::Registry::REG_SAM_FLAGS(access),
            &mut hkey,
        )
    }
    .ok()?;
    Ok(hkey)
}

/// Registers `exe_path --tray` to launch at login. `exe_path` should already be
/// the absolute path to the running executable (quoted internally here so paths
/// containing spaces, e.g. under `Program Files`, work correctly).
pub fn enable(exe_path: &str) -> Result<()> {
    let hkey = open_run_key(KEY_SET_VALUE.0)?;
    let command = format!("\"{exe_path}\" --tray");
    let value = HSTRING::from(command.as_str());
    let name = HSTRING::from(VALUE_NAME);

    // REG_SZ data must be a null-terminated wide string; HSTRING's Deref<[u16]>
    // excludes the implicit trailing null the buffer already holds, so it's
    // appended explicitly here.
    let mut wide: Vec<u16> = value.iter().copied().collect();
    wide.push(0);
    let bytes: Vec<u8> = wide.iter().flat_map(|w| w.to_le_bytes()).collect();

    let result = unsafe { RegSetValueExW(hkey, &name, Some(0), REG_SZ, Some(&bytes)) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    result.ok()
}

pub fn disable() -> Result<()> {
    let hkey = open_run_key(KEY_SET_VALUE.0)?;
    let name = HSTRING::from(VALUE_NAME);
    let result = unsafe { RegDeleteValueW(hkey, &name) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    // Missing value when disabling is not an error - already disabled.
    match result.ok() {
        Ok(()) => Ok(()),
        Err(e) if e.code().0 as u32 == 0x8007_0002 => Ok(()), // ERROR_FILE_NOT_FOUND
        Err(e) => Err(e),
    }
}

/// Reads back whether the autostart entry currently exists - this is the source
/// of truth the GUI checkbox should sync from on startup, not the config file.
pub fn is_enabled() -> bool {
    let Ok(hkey) = open_run_key(KEY_READ.0) else {
        return false;
    };
    let mut value_type = REG_VALUE_TYPE::default();
    let mut data_len: u32 = 0;
    let name = HSTRING::from(VALUE_NAME);
    let result = unsafe {
        RegQueryValueExW(
            hkey,
            &name,
            None,
            Some(&mut value_type),
            None,
            Some(&mut data_len),
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    result.is_ok()
}
