//! Raw Win32 window helpers. eframe's `ViewportCommand::Visible` can't restore a
//! hidden window, so the tray hide/show path drives `ShowWindow` on the HWND
//! directly. Pattern proven in trontclicker / powershellmanager.
//!
//! THE TRAY STATE IS: minimized, taskbar button removed via `ITaskbarList`,
//! ex-styles untouched. Every clause there was paid for; see `hide_window` for
//! what each alternative leaks.

/// Add or remove this window's taskbar button through the shell's own API.
///
/// This is the only way to take a still-visible window out of the taskbar that
/// does not fight Explorer. Doing it with styles (set WS_EX_TOOLWINDOW, clear
/// WS_EX_APPWINDOW) loses twice over:
///
/// * Explorer caches its taskbar decision and re-reads the ex-style only across
///   show/hide transitions, so a window that has been activated and then
///   minimized keeps a stale, GLOWING phantom button. It looks exactly like the
///   bug you were trying to fix.
/// * WS_EX_TOOLWINDOW also takes the window out of shell management, and a
///   minimized window the shell does not manage never gets parked at
///   (-32000,-32000). Its legacy iconic stub stays in the work area instead: a
///   black 237x39 rectangle sitting above the taskbar, black because
///   `decorations(false)` means there is no caption to paint into it.
///
/// COM: the main thread has an apartment (winit calls OleInitialize on the event
/// loop thread) but `show_window` is also reachable from the show-acceptor
/// thread, which has none. Initialize defensively, uninitialize only if this call
/// is what initialized.
fn taskbar_tab(hwnd: isize, present: bool) {
    use windows::Win32::Foundation::{HWND, S_OK};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{ITaskbarList, TaskbarList};
    unsafe {
        let we_init = CoInitializeEx(None, COINIT_APARTMENTTHREADED) == S_OK;
        if let Ok(list) = CoCreateInstance::<_, ITaskbarList>(&TaskbarList, None, CLSCTX_ALL) {
            if list.HrInit().is_ok() {
                let h = HWND(hwnd as *mut _);
                let _ = if present { list.AddTab(h) } else { list.DeleteTab(h) };
            }
        }
        if we_init {
            CoUninitialize();
        }
    }
}

/// Put the taskbar button back, then restore + show + focus the window.
///
/// SW_RESTORE before SW_SHOW so this works from either parked state, including a
/// window that was hidden while already minimized.
pub fn show_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    taskbar_tab(hwnd, true);
    unsafe {
        let h = HWND(hwnd as *mut _);
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

/// Drop to the tray: minimize, then take the taskbar button away.
///
/// Four parked states were built and measured on 2026-07-29/30. Only this one
/// gets all three properties (idle, no taskbar button, nothing left on screen):
///
/// | parked state | idle? | taskbar button | on-screen artifact |
/// |---|---|---|---|
/// | `SW_HIDE` | **NO: 101% of one core, forever** | gone | none |
/// | `SW_MINIMIZE` | yes (~0%) | **stays** | none: the shell parks it at (-32000,-32000) |
/// | `SW_MINIMIZE` + TOOLWINDOW (+/- clearing APPWINDOW) | yes | **stale glowing phantom** | **black 237x39 stub above the taskbar** |
/// | **`SW_MINIMIZE` + `ITaskbarList::DeleteTab`** | **yes** | **gone** | **none** |
///
/// WHY NOT SW_HIDE, since it is the obvious answer: winit never delivers a
/// redraw to a hidden window, and an unretired immediate repaint request keeps
/// its event loop in Poll instead of Wait. So one queued repaint pins the main
/// thread at a full core for as long as the app sits in the tray. That is the
/// 0.12.2 bug (643 minutes of CPU over 14.7 hours tray'd). Latching the tray
/// state first, silencing the heartbeat, and hiding only after two
/// deadline-driven frames had retired everything queued was tried on 2026-07-30
/// and still measured 100% of a core: the hide ITSELF gets eframe to queue a
/// repaint, so no amount of pre-hide quiet can win. A minimized window still
/// gets `update()` called, which is what retires the request and lets the loop
/// reach Wait, so minimized genuinely idles.
///
/// Do not "improve" this by reaching for WS_EX_TOOLWINDOW. It is what produced
/// both the phantom button and the black stub; see `taskbar_tab`.
///
/// Known trade-off: a minimized window is still an Alt-Tab entry, and DeleteTab
/// does not change that (only TOOLWINDOW would, at the cost of the two bugs
/// above). Alt-tabbing to a tray'd TrontEQ restores it, which `presentable`
/// reconciles correctly, so the app recovers rather than showing blank.
pub fn hide_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_MINIMIZE};
    unsafe {
        // Minimize FIRST: this is what makes the shell park the window
        // off-screen, so there is no iconic stub to look at afterwards.
        let _ = ShowWindow(HWND(hwnd as *mut _), SW_MINIMIZE);
    }
    taskbar_tab(hwnd, false);
}
