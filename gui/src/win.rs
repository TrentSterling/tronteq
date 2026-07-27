//! Raw Win32 window helpers. eframe's `ViewportCommand::Visible` can't restore a
//! hidden window, so the tray hide/show path drives `ShowWindow` on the HWND
//! directly. Pattern proven in trontclicker / powershellmanager.

/// Add or remove WS_EX_TOOLWINDOW, which is what keeps a window out of the
/// taskbar and out of alt-tab.
unsafe fn set_toolwindow(hwnd: isize, on: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };
    let h = HWND(hwnd as *mut _);
    let cur = GetWindowLongPtrW(h, GWL_EXSTYLE);
    let bit = WS_EX_TOOLWINDOW.0 as isize;
    let next = if on { cur | bit } else { cur & !bit };
    if next != cur {
        SetWindowLongPtrW(h, GWL_EXSTYLE, next);
    }
}

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
        set_toolwindow(hwnd, false); // taskbar button comes back
        let _ = ShowWindow(h, SW_RESTORE);
        let _ = ShowWindow(h, SW_SHOW);
        let _ = SetForegroundWindow(h);
    }
}

/// True when the OS says the window is actually on screen: shown and not
/// minimized. Deliberately reads Win32 rather than the app's own flags, so ANY
/// route back on screen resumes rendering — including a second launch, which
/// restores the window from a thread that has no egui context to poke.
///
/// Fails OPEN (returns true) for a null handle: an unknown state must never be
/// the reason the app stops repainting.
pub fn is_on_screen(hwnd: isize) -> bool {
    if hwnd == 0 {
        return true;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};
    unsafe {
        let h = HWND(hwnd as *mut _);
        IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool()
    }
}

/// True when DWM has cloaked the window: parked on another virtual desktop, or
/// otherwise composited out of existence.
///
/// This is the one "the user cannot see this" state nothing else reports.
/// `IsWindowVisible` returns true for a cloaked window and eframe's viewport
/// info has no notion of it, so without this probe the app happily renders 60fps
/// for a desktop nobody is looking at.
pub fn is_cloaked(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut cloaked: u32 = 0;
    unsafe {
        DwmGetWindowAttribute(
            HWND(hwnd as *mut _),
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
    }
}

/// Drop to the tray: minimized, with no taskbar button and no alt-tab entry.
///
/// DELIBERATELY NOT SW_HIDE, which is what this used to do and what cost a full
/// CPU core around the clock (measured 2026-07-26: 643 minutes of CPU over 14.7
/// hours of uptime, 98.6% of it on the main thread, none of it on the audio
/// capture). winit never delivers a redraw to a hidden window, and an
/// outstanding repaint request keeps its event loop in Poll instead of Wait, so
/// the loop spun forever on a request that could never be serviced. Hiding via
/// winit's own `ViewportCommand::Visible(false)` behaves identically, and so
/// does SW_MINIMIZE followed by SW_HIDE — both were measured at ~98%.
///
/// Minimized is the one state where winit genuinely idles (~0%), so that is the
/// state we park in. WS_EX_TOOLWINDOW is what makes a minimized window read as
/// tray'd: it is excluded from both the taskbar and alt-tab. The style is set
/// while the window is still on screen so the shell notices the change.
///
/// `show_window` clears the style before restoring, so the round trip unwinds.
pub fn hide_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_MINIMIZE};
    unsafe {
        set_toolwindow(hwnd, true); // no taskbar button, no alt-tab entry
        let _ = ShowWindow(HWND(hwnd as *mut _), SW_MINIMIZE);
    }
}
