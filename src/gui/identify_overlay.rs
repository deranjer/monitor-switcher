//! "Identify Monitors" numbered badges - a small always-on-top, click-through,
//! borderless popup window per physical monitor, positioned via
//! `MonitorGeometry`, auto-dismissing after a few seconds. Built directly on
//! raw Win32 (via the `windows` crate, same as `monitor_identity.rs`/
//! `autostart.rs`/`single_instance.rs`) rather than through winsafe's
//! higher-level `gui` module, since this is a transient custom-drawn overlay,
//! not a native control - and messages for these windows are pumped by the
//! same single message loop winsafe's `run_main` already owns, since they're
//! created on the same thread.

use std::sync::Once;

use windows::core::HSTRING;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DrawTextW, EndPaint, FillRect, SetBkMode, SetTextColor, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, HBRUSH, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, LoadCursorW, RegisterClassExW, SetTimer,
    SetWindowLongPtrW, ShowWindow, IDC_ARROW, GWLP_USERDATA, SW_SHOWNOACTIVATE, WM_DESTROY, WM_PAINT, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::platform::windows::monitor_identity::MonitorGeometry;

const BADGE_SIZE: i32 = 120;
const BADGE_MARGIN: i32 = 32;
const CLASS_NAME: &str = "MonitorSwitcherIdentifyOverlay";
const DISMISS_TIMER_ID: usize = 1;
const DISMISS_MS: u32 = 3000;

static REGISTER_ONCE: Once = Once::new();

fn register_class_once() {
    REGISTER_ONCE.call_once(|| {
        let class_name = HSTRING::from(CLASS_NAME);
        let hinstance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        unsafe {
            RegisterClassExW(&wc);
        }
    });
}

/// Shows one numbered badge per entry in `geometries`, in the bottom-right
/// corner of each monitor. Fire-and-forget: each window destroys itself via
/// its own timer, nothing needs to be kept alive by the caller.
pub fn show(geometries: &[MonitorGeometry]) {
    register_class_once();
    let class_name = HSTRING::from(CLASS_NAME);
    let hinstance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();

    for (i, geo) in geometries.iter().enumerate() {
        let x = geo.right - BADGE_SIZE - BADGE_MARGIN;
        let y = geo.bottom - BADGE_SIZE - BADGE_MARGIN;

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                windows::core::PCWSTR(class_name.as_ptr()),
                &HSTRING::from(""),
                WS_POPUP,
                x,
                y,
                BADGE_SIZE,
                BADGE_SIZE,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
        };

        let Ok(hwnd) = hwnd else { continue };

        unsafe {
            let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, (i + 1) as isize);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
                hwnd,
                COLORREF(0),
                230,
                windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetTimer(Some(hwnd), DISMISS_TIMER_ID, DISMISS_MS, None);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

            let mut rect = RECT::default();
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) };

            let brush: HBRUSH = unsafe { CreateSolidBrush(COLORREF(0x00202020)) };
            unsafe { FillRect(hdc, &rect, brush) };
            let _ = unsafe { windows::Win32::Graphics::Gdi::DeleteObject(brush.into()) };

            unsafe { SetBkMode(hdc, TRANSPARENT) };
            unsafe { SetTextColor(hdc, COLORREF(0x00FFFFFF)) };

            let number = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            let mut text: Vec<u16> = number.to_string().encode_utf16().collect();
            unsafe { DrawTextW(hdc, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE) };

            let _ = unsafe { EndPaint(hwnd, &ps) };
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
