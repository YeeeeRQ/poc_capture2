use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
}

struct TrayStateInner {
    quit_flag: Arc<AtomicBool>,
    screenshot_flag: Arc<AtomicBool>,
    recording_toggle_flag: Arc<AtomicBool>,
    show_window_flag: Arc<AtomicBool>,
}

pub fn get_tray_state() -> Option<TrayState> {
    TRAY_STATE.get().map(|inner| TrayState {
        quit_flag: Arc::clone(&inner.quit_flag),
        screenshot_flag: Arc::clone(&inner.screenshot_flag),
        recording_toggle_flag: Arc::clone(&inner.recording_toggle_flag),
        show_window_flag: Arc::clone(&inner.show_window_flag),
    })
}

impl TrayState {
    pub fn new(quit_flag: Arc<AtomicBool>) -> Self {
        Self {
            quit_flag,
            screenshot_flag: Arc::new(AtomicBool::new(false)),
            recording_toggle_flag: Arc::new(AtomicBool::new(false)),
            show_window_flag: Arc::new(AtomicBool::new(false)),
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
pub fn setup_tray(quit_flag: Arc<AtomicBool>) {
    let state = TrayState::new(Arc::clone(&quit_flag));

    let inner = TrayStateInner {
        quit_flag: Arc::clone(&state.quit_flag),
        screenshot_flag: Arc::clone(&state.screenshot_flag),
        recording_toggle_flag: Arc::clone(&state.recording_toggle_flag),
        show_window_flag: Arc::clone(&state.show_window_flag),
    };
    TRAY_STATE.set(inner).ok();

    let show_window_flag = Arc::clone(&state.show_window_flag);
    let screenshot_flag = Arc::clone(&state.screenshot_flag);
    let recording_toggle_flag = Arc::clone(&state.recording_toggle_flag);
    let quit_flag_clone = Arc::clone(&state.quit_flag);

    let show_flag_for_tray = Arc::clone(&show_window_flag);
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = event
        {
            log::info!("[Tray] Left click");
            show_flag_for_tray.store(true, Ordering::SeqCst);
        }
    }));

    let show_flag_for_menu = Arc::clone(&show_window_flag);
    let screenshot_flag_for_menu = Arc::clone(&screenshot_flag);
    let record_flag_for_menu = Arc::clone(&recording_toggle_flag);
    let quit_flag_for_menu = Arc::clone(&quit_flag_clone);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        log::info!("[Tray] Menu: {}", event.id.as_ref());
        match event.id.as_ref() {
            "show" => {
                show_flag_for_menu.store(true, Ordering::SeqCst);
            }
            "screenshot" => {
                screenshot_flag_for_menu.store(true, Ordering::SeqCst);
            }
            "record" => {
                record_flag_for_menu.store(true, Ordering::SeqCst);
            }
            "quit" => {
                log::info!("[Tray] Quit");
                quit_flag_for_menu.store(true, Ordering::SeqCst);
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
        .build()
        .expect("Failed to create tray icon");

    Box::leak(Box::new(tray));
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
pub fn setup_tray(_quit_flag: Arc<AtomicBool>) {
    log::warn!("[Tray] System tray is only supported on Windows");
}
