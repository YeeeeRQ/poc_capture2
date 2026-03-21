use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use capture_core::RecorderHandle;
use parking_lot::Mutex;

#[cfg(windows)]
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, TrayIconEvent,
};

static TRAY_STATE: std::sync::OnceLock<TrayStateInner> = std::sync::OnceLock::new();

#[derive(Clone)]
pub struct TrayState {
    pub quit_flag: Arc<AtomicBool>,
    pub screenshot_flag: Arc<AtomicBool>,
    pub recording_toggle_flag: Arc<AtomicBool>,
    pub show_window_flag: Arc<AtomicBool>,
    pub session_folder: PathBuf,
    pub is_recording: Arc<AtomicBool>,
    pub monitors: Arc<Mutex<Vec<capture_core::MonitorInfo>>>,
    pub selected_monitor: Arc<std::sync::atomic::AtomicUsize>,
    pub record_start: Arc<Mutex<Option<std::time::Instant>>>,
    pub handle: Arc<Mutex<Option<RecorderHandle>>>,
    pub hwnd: Arc<std::sync::atomic::AtomicIsize>,
    pub window_state: Arc<Mutex<WindowState>>,
}

#[derive(Clone, Default)]
pub struct WindowState {
    pub width: f32,
    pub height: f32,
    pub x: i32,
    pub y: i32,
    pub is_maximized: bool,
    pub is_minimized: bool,
}

struct TrayStateInner {
    quit_flag: Arc<AtomicBool>,
    screenshot_flag: Arc<AtomicBool>,
    recording_toggle_flag: Arc<AtomicBool>,
    show_window_flag: Arc<AtomicBool>,
    session_folder: PathBuf,
    is_recording: Arc<AtomicBool>,
    monitors: Arc<Mutex<Vec<capture_core::MonitorInfo>>>,
    selected_monitor: Arc<std::sync::atomic::AtomicUsize>,
    record_start: Arc<Mutex<Option<std::time::Instant>>>,
    handle: Arc<Mutex<Option<RecorderHandle>>>,
    hwnd: Arc<std::sync::atomic::AtomicIsize>,
    window_state: Arc<Mutex<WindowState>>,
}

pub fn get_tray_state() -> Option<TrayState> {
    TRAY_STATE.get().map(|state| TrayState {
        quit_flag: Arc::clone(&state.quit_flag),
        screenshot_flag: Arc::clone(&state.screenshot_flag),
        recording_toggle_flag: Arc::clone(&state.recording_toggle_flag),
        show_window_flag: Arc::clone(&state.show_window_flag),
        session_folder: state.session_folder.clone(),
        is_recording: Arc::clone(&state.is_recording),
        monitors: Arc::clone(&state.monitors),
        selected_monitor: Arc::clone(&state.selected_monitor),
        record_start: Arc::clone(&state.record_start),
        handle: Arc::clone(&state.handle),
        hwnd: Arc::clone(&state.hwnd),
        window_state: Arc::clone(&state.window_state),
    })
}

impl TrayState {
    pub fn new(
        quit_flag: Arc<AtomicBool>,
        session_folder: PathBuf,
        monitors: Vec<capture_core::MonitorInfo>,
    ) -> Self {
        Self {
            quit_flag,
            screenshot_flag: Arc::new(AtomicBool::new(false)),
            recording_toggle_flag: Arc::new(AtomicBool::new(false)),
            show_window_flag: Arc::new(AtomicBool::new(false)),
            session_folder,
            is_recording: Arc::new(AtomicBool::new(false)),
            monitors: Arc::new(Mutex::new(monitors)),
            selected_monitor: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            record_start: Arc::new(Mutex::new(None)),
            handle: Arc::new(Mutex::new(None)),
            hwnd: Arc::new(std::sync::atomic::AtomicIsize::new(0)),
            window_state: Arc::new(Mutex::new(WindowState::default())),
        }
    }

    pub fn take_screenshot(&self) -> bool {
        self.screenshot_flag.swap(false, Ordering::SeqCst)
    }

    pub fn take_recording_toggle(&self) -> bool {
        self.recording_toggle_flag.swap(false, Ordering::SeqCst)
    }

    pub fn take_show_window(&self) -> bool {
        self.show_window_flag.swap(false, Ordering::SeqCst)
    }
}

#[cfg(windows)]
pub fn setup_tray(
    quit_flag: Arc<AtomicBool>,
    session_folder: PathBuf,
    monitors: Vec<capture_core::MonitorInfo>,
) {
    let state = TrayState::new(Arc::clone(&quit_flag), session_folder, monitors);

    let quit_flag_clone = Arc::clone(&state.quit_flag);
    let screenshot_flag = Arc::clone(&state.screenshot_flag);
    let recording_toggle_flag = Arc::clone(&state.recording_toggle_flag);
    let show_window_flag = Arc::clone(&state.show_window_flag);
    let show_window_flag_for_menu = Arc::clone(&state.show_window_flag);

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = event
        {
            log::info!("[Tray] Left click");
            show_window_flag.store(true, Ordering::SeqCst);
        }
    }));

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id_str = event.id.as_ref();
        log::info!("[Tray] Menu event: {}", id_str);
        match id_str {
            "show" => {
                show_window_flag_for_menu.store(true, Ordering::SeqCst);
            }
            "screenshot" => {
                screenshot_flag.store(true, Ordering::SeqCst);
            }
            "record" => {
                recording_toggle_flag.store(true, Ordering::SeqCst);
            }
            "quit" => {
                log::info!("[Tray] Quit requested");
                quit_flag_clone.store(true, Ordering::SeqCst);
            }
            _ => {}
        }
    }));

    let icon = load_icon();

    let show_item = MenuItem::with_id("show", "显示窗口", true, None);
    let screenshot_item = MenuItem::with_id("screenshot", "截图", true, None);
    let record_item = MenuItem::with_id("record", "开始录制", true, None);
    let separator = PredefinedMenuItem::separator();
    let quit_item = MenuItem::with_id("quit", "退出", true, None);

    let menu = Menu::new();
    menu.append(&show_item).ok();
    menu.append(&screenshot_item).ok();
    menu.append(&record_item).ok();
    menu.append(&separator).ok();
    menu.append(&quit_item).ok();

    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("Capture")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .expect("Failed to create tray icon");

    Box::leak(Box::new(tray));

    let inner = TrayStateInner {
        quit_flag: Arc::clone(&state.quit_flag),
        screenshot_flag: Arc::clone(&state.screenshot_flag),
        recording_toggle_flag: Arc::clone(&state.recording_toggle_flag),
        show_window_flag: Arc::clone(&state.show_window_flag),
        session_folder: state.session_folder.clone(),
        is_recording: Arc::clone(&state.is_recording),
        monitors: Arc::clone(&state.monitors),
        selected_monitor: Arc::clone(&state.selected_monitor),
        record_start: Arc::clone(&state.record_start),
        handle: Arc::clone(&state.handle),
        hwnd: Arc::clone(&state.hwnd),
        window_state: Arc::clone(&state.window_state),
    };
    TRAY_STATE.set(inner).ok();
    log::info!("[Tray] Tray icon setup complete");
}

#[cfg(windows)]
fn load_icon() -> tray_icon::Icon {
    let icon_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icon.ico");

    if icon_path.exists() {
        match load_icon_from_file(&icon_path) {
            Ok(icon) => icon,
            Err(e) => {
                log::warn!("[Tray] Icon load failed: {}, using fallback", e);
                create_fallback_icon()
            }
        }
    } else {
        create_fallback_icon()
    }
}

#[cfg(windows)]
fn load_icon_from_file(
    path: &std::path::Path,
) -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
    let image = image::open(path)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Ok(tray_icon::Icon::from_rgba(rgba, width, height)?)
}

#[cfg(windows)]
fn create_fallback_icon() -> tray_icon::Icon {
    let size: u32 = 32;
    let rgba = vec![255u8; (size * size * 4) as usize];
    tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create fallback icon")
}

#[cfg(not(windows))]
pub fn setup_tray(_quit_flag: Arc<AtomicBool>, _session_folder: PathBuf) {
    log::warn!("[Tray] System tray is only supported on Windows");
}
