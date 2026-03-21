#[cfg(windows)]
use std::ffi::c_int;

#[cfg(windows)]
use std::mem;

#[cfg(windows)]
extern "system" {
    fn CreateRoundRectRgn(x1: c_int, y1: c_int, x2: c_int, y2: c_int, w: c_int, h: c_int) -> isize;
    fn SetWindowRgn(hwnd: isize, hrgn: isize, bRedraw: i32) -> i32;
    fn GetModuleHandleW(lpModuleName: *const u16) -> isize;
    fn GetProcAddress(hModule: isize, lpProcName: *const u8) -> isize;
    fn GetWindowPlacement(hwnd: isize, lpwndpl: *mut WINDOWPLACEMENT) -> i32;
    fn SetWindowPlacement(hwnd: isize, lpwndpl: *const WINDOWPLACEMENT) -> i32;
    fn ShowWindow(hwnd: isize, nCmdShow: c_int) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn SetActiveWindow(hwnd: isize) -> isize;
    fn SetFocus(hwnd: isize) -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn AttachThreadInput(idAttach: u32, idAttachTo: u32, fAttach: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, lpdwProcessId: *mut u32) -> u32;
    fn GetCurrentThreadId() -> u32;
    fn ShowOwnedPopups(hwnd: isize, fShow: i32) -> i32;
}

#[cfg(windows)]
#[repr(C)]
struct ACCENT_POLICY {
    accent_state: u32,
    accent_flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[cfg(windows)]
#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: u32,
    data: *mut std::ffi::c_void,
    size_of_data: u32,
}

#[cfg(windows)]
const WCA_ACCENT_POLICY: u32 = 19;
#[cfg(windows)]
const ACCENT_ENABLE_HOSTBACKDROP: u32 = 29;

#[cfg(windows)]
const WINDOWPLACEMENT_LENGTH: u32 = std::mem::size_of::<WINDOWPLACEMENT>() as u32;

#[cfg(windows)]
const SW_MAXIMIZE: c_int = 3;
#[cfg(windows)]
const SW_MINIMIZE: c_int = 6;
#[cfg(windows)]
const SW_RESTORE: c_int = 9;
#[cfg(windows)]
const SW_SHOW: c_int = 5;
#[cfg(windows)]
const SW_HIDE: c_int = 0;

#[cfg(windows)]
#[repr(C)]
#[allow(non_snake_case)]
struct POINT {
    x: c_int,
    y: c_int,
}

#[cfg(windows)]
#[repr(C)]
struct RECT {
    left: c_int,
    top: c_int,
    right: c_int,
    bottom: c_int,
}

#[cfg(windows)]
#[repr(C)]
#[allow(non_snake_case)]
struct WINDOWPLACEMENT {
    length: u32,
    flags: u32,
    showCmd: c_int,
    ptMinPosition: POINT,
    ptMaxPosition: POINT,
    rcNormalPosition: RECT,
}

type SetWindowCompositionAttributeFn =
    unsafe extern "system" fn(isize, *mut WindowCompositionAttributeData) -> i32;

#[cfg(windows)]
fn set_window_composition_attribute(hwnd: isize, state: u32) -> bool {
    unsafe {
        let user32 = GetModuleHandleW(std::ptr::null());
        if user32 == 0 {
            log::error!("Failed to get user32 module handle");
            return false;
        }

        let proc_name = b"SetWindowCompositionAttribute\0";
        let func = GetProcAddress(user32, proc_name.as_ptr() as *const u8);
        if func == 0 {
            log::error!("Failed to get SetWindowCompositionAttribute address");
            return false;
        }

        let func: SetWindowCompositionAttributeFn = std::mem::transmute(func);

        let mut accent = ACCENT_POLICY {
            accent_state: state,
            accent_flags: 0,
            gradient_color: 0,
            animation_id: 0,
        };

        let mut data = WindowCompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut accent as *mut _ as *mut std::ffi::c_void,
            size_of_data: mem::size_of::<ACCENT_POLICY>() as u32,
        };

        let result = func(hwnd, &mut data);
        if result == 0 {
            log::error!("SetWindowCompositionAttribute returned 0");
            return false;
        }

        log::info!(
            "SetWindowCompositionAttribute succeeded with state {}",
            state
        );
        true
    }
}

#[cfg(windows)]
pub fn set_window_rounded_corner(hwnd: isize, width: i32, height: i32, corner_radius: i32) -> bool {
    unsafe {
        let region = CreateRoundRectRgn(0, 0, width, height, corner_radius, corner_radius);

        if region == 0 {
            log::error!("Failed to create rounded rect region");
            return false;
        }

        let result = SetWindowRgn(hwnd, region, 1);
        if result == 0 {
            log::error!("Failed to set window region");
            return false;
        }

        log::info!(
            "Window region set successfully: {}x{} corner_radius={}",
            width,
            height,
            corner_radius
        );

        set_window_composition_attribute(hwnd, ACCENT_ENABLE_HOSTBACKDROP);

        true
    }
}

#[cfg(not(windows))]
pub fn set_window_rounded_corner(
    _hwnd: isize,
    _width: i32,
    _height: i32,
    _corner_radius: i32,
) -> bool {
    log::warn!("Rounded corners only supported on Windows");
    false
}

#[cfg(windows)]
pub fn show_window(hwnd: isize, state: &crate::tray::WindowState) {
    if hwnd == 0 {
        log::warn!("[WindowsWindow] show_window called with invalid hwnd");
        return;
    }

    unsafe {
        let is_visible = IsWindowVisible(hwnd) != 0;

        if !is_visible {
            ShowWindow(hwnd, SW_SHOW);
        }

        let mut placement = WINDOWPLACEMENT {
            length: WINDOWPLACEMENT_LENGTH,
            flags: 0,
            showCmd: SW_RESTORE,
            ptMinPosition: POINT { x: 0, y: 0 },
            ptMaxPosition: POINT { x: 0, y: 0 },
            rcNormalPosition: RECT {
                left: state.x,
                top: state.y,
                right: state.x + state.width as c_int,
                bottom: state.y + state.height as c_int,
            },
        };

        if state.is_minimized {
            placement.showCmd = SW_MINIMIZE;
        } else if state.is_maximized {
            placement.showCmd = SW_MAXIMIZE;
        } else {
            placement.showCmd = SW_RESTORE;
        }

        SetWindowPlacement(hwnd, &placement);

        let mut flags: u32 = 0;
        let current_thread = GetCurrentThreadId();
        let window_thread = GetWindowThreadProcessId(hwnd, &mut flags);

        if current_thread != window_thread {
            AttachThreadInput(window_thread, current_thread, 1);
        }

        ShowOwnedPopups(hwnd, 1);
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);
        SetFocus(hwnd);

        if current_thread != window_thread {
            AttachThreadInput(window_thread, current_thread, 0);
        }

        log::info!(
            "[WindowsWindow] Window shown via Windows API: pos=({},{}), size=({}x{}), maximized={}, minimized={}, was_visible={}",
            state.x, state.y, state.width, state.height, state.is_maximized, state.is_minimized, is_visible
        );
    }
}

#[cfg(windows)]
pub fn hide_window(hwnd: isize) {
    if hwnd == 0 {
        log::warn!("[WindowsWindow] hide_window called with invalid hwnd");
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        log::info!("[WindowsWindow] Window hidden via Windows API");
    }
}

#[cfg(windows)]
pub fn save_window_state(hwnd: isize) -> Option<crate::tray::WindowState> {
    if hwnd == 0 {
        return None;
    }
    unsafe {
        let mut placement = WINDOWPLACEMENT {
            length: WINDOWPLACEMENT_LENGTH,
            flags: 0,
            showCmd: 0,
            ptMinPosition: POINT { x: 0, y: 0 },
            ptMaxPosition: POINT { x: 0, y: 0 },
            rcNormalPosition: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        };

        if GetWindowPlacement(hwnd, &mut placement) == 0 {
            log::error!("[WindowsWindow] Failed to get window placement");
            return None;
        }

        let state = crate::tray::WindowState {
            width: (placement.rcNormalPosition.right - placement.rcNormalPosition.left) as f32,
            height: (placement.rcNormalPosition.bottom - placement.rcNormalPosition.top) as f32,
            x: placement.rcNormalPosition.left,
            y: placement.rcNormalPosition.top,
            is_maximized: placement.showCmd == SW_MAXIMIZE,
            is_minimized: placement.showCmd == SW_MINIMIZE,
        };

        log::info!(
            "[WindowsWindow] Window state saved: pos=({},{}), size=({}x{}), maximized={}, minimized={}",
            state.x, state.y, state.width, state.height, state.is_maximized, state.is_minimized
        );

        Some(state)
    }
}
