//! Stops a second copy of the app from running at once - without this, a
//! double launch spawns duplicate tray icons and duplicate global hotkey
//! managers, where the second copy's hotkey registrations silently lose to
//! the first's (Windows only lets one process claim a given key combo) and
//! both copies write to the same `config.json`.

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

const MUTEX_NAME: &str = "MonitorSwitcher-SingleInstance-Mutex";

/// Holds the OS mutex that marks this process as *the* running instance.
/// Must be kept alive for the entire process lifetime (e.g. bound to a
/// variable in `main` that isn't dropped until `main` returns) - dropping it
/// early releases the mutex and lets a second instance start.
pub struct SingleInstanceGuard(HANDLE);

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Attempts to claim single-instance ownership.
///
/// Returns `None` if another instance already holds it - the caller should
/// exit immediately in that case, before creating the tray icon, hotkey
/// manager, or window. Returns `Some(guard)` if this is the first instance.
pub fn acquire() -> Option<SingleInstanceGuard> {
    let name = HSTRING::from(MUTEX_NAME);
    // bInitialOwner = false: this is used purely as an existence marker, not
    // for actual mutual-exclusion locking, so there's no "ownership" to take.
    let handle = unsafe { CreateMutexW(None, false, &name) }.ok()?;

    // CreateMutexW returns a valid handle whether it created a new mutex or
    // opened an existing one - GetLastError is the only way to tell which.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }

    Some(SingleInstanceGuard(handle))
}
