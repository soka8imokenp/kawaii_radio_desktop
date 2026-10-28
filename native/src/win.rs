//! Windows: the borderless device window gets Windows 11's rounded corners (no 1 px accent frame), and keeps 16:9
//! while an edge is dragged (Snap and maximize still place it freely; the stage letterboxes into any shape).

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{WMSZ_BOTTOM, WMSZ_BOTTOMLEFT, WMSZ_BOTTOMRIGHT, WMSZ_LEFT, WMSZ_RIGHT, WMSZ_TOP, WMSZ_TOPLEFT, WMSZ_TOPRIGHT, WM_SIZING};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

unsafe extern "system" fn keep_aspect(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_SIZING && lp != 0 {
        let r = unsafe { &mut *(lp as *mut RECT) };
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        match wp as u32 {
            WMSZ_TOP | WMSZ_BOTTOM => r.right = r.left + h * 16 / 9,
            WMSZ_TOPLEFT | WMSZ_TOPRIGHT => r.top = r.bottom - w * 9 / 16,
            WMSZ_LEFT | WMSZ_RIGHT | WMSZ_BOTTOMLEFT | WMSZ_BOTTOMRIGHT => r.bottom = r.top + w * 9 / 16,
            _ => {}
        }
        return 1;
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

pub fn setup(w: &winit::window::Window) {
    let Ok(h) = w.window_handle() else { return };
    let RawWindowHandle::Win32(h) = h.as_raw() else { return };
    let hwnd = h.hwnd.get() as HWND;
    unsafe {
        SetWindowSubclass(hwnd, Some(keep_aspect), 0x4b52, 0);
        let pref: i32 = DWMWCP_ROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &pref as *const i32 as *const core::ffi::c_void, 4);
        // DWMWA_BORDER_COLOR = 34, DWMWA_COLOR_NONE: no 1 px accent / white frame around the device
        let none: u32 = 0xFFFF_FFFE;
        DwmSetWindowAttribute(hwnd, 34, &none as *const u32 as *const core::ffi::c_void, 4);
    }
}
