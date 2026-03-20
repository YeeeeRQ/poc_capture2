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
