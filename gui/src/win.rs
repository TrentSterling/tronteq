//! Raw Win32 window helpers. eframe's `ViewportCommand::Visible` can't restore a
//! hidden window, so the tray hide/show path drives `ShowWindow` on the HWND
//! directly. Pattern proven in trontclicker / powershellmanager.

/// Restore + show + focus the window.
pub fn show_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    unsafe {
        let h = HWND(hwnd as *mut _);
        let _ = ShowWindow(h, SW_RESTORE);
        let _ = ShowWindow(h, SW_SHOW);
        let _ = SetForegroundWindow(h);
    }
}

/// Hide the window entirely (no taskbar button — true tray).
pub fn hide_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut _), SW_HIDE);
    }
}
