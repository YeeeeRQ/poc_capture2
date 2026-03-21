use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
extern "system" {
    fn InvalidateRect(hwnd: isize, lpRect: *const RECT, bErase: i32) -> i32;
}

#[cfg(windows)]
static TIMER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
pub fn spawn_timer_wakeup_thread(hwnd: Arc<AtomicIsize>) {
    if TIMER_RUNNING.swap(true, Ordering::SeqCst) {
        log::warn!("[TimerWakeup] Timer thread already running");
        return;
    }

    thread::spawn(move || {
        log::info!("[TimerWakeup] Timer thread started");
        let check_interval = Duration::from_millis(100);

        while TIMER_RUNNING.load(Ordering::SeqCst) {
            let hwnd_value = hwnd.load(Ordering::SeqCst);
            if hwnd_value != 0 {
                unsafe {
                    InvalidateRect(hwnd_value as isize, std::ptr::null(), 0);
                }
            }
            thread::sleep(check_interval);
        }

        log::info!("[TimerWakeup] Timer thread exiting");
    });
}

#[cfg(windows)]
pub fn stop_timer_wakeup_thread() {
    TIMER_RUNNING.store(false, Ordering::SeqCst);
}

#[cfg(not(windows))]
pub fn spawn_timer_wakeup_thread(_hwnd: Arc<AtomicIsize>) {
    log::warn!("[TimerWakeup] Timer wakeup only supported on Windows");
}

#[cfg(not(windows))]
pub fn stop_timer_wakeup_thread() {}
