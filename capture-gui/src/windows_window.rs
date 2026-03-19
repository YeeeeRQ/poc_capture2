#[cfg(windows)]
use std::ffi::c_int;

#[cfg(windows)]
extern "system" {
    fn CreateRoundRectRgn(x1: c_int, y1: c_int, x2: c_int, y2: c_int, w: c_int, h: c_int) -> isize;
    fn SetWindowRgn(hwnd: isize, hrgn: isize, bRedraw: i32) -> i32;
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
