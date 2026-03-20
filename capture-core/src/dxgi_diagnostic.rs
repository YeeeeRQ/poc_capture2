use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DxgiDiagnosticOptions {
    pub verbose: bool,
    pub output_file: Option<PathBuf>,
    pub exit_after: bool,
}

impl Default for DxgiDiagnosticOptions {
    fn default() -> Self {
        Self {
            verbose: true,
            output_file: None,
            exit_after: false,
        }
    }
}

pub fn run_diagnostics(options: &DxgiDiagnosticOptions) {
    let mut output = String::new();
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    output.push_str(&format!(
        "=== DXGI Capture Diagnostic ({}) ===\n\n",
        timestamp
    ));

    output.push_str(&diagnose_system());
    output.push_str("\n");

    output.push_str(&diagnose_displays());
    output.push_str("\n");

    output.push_str(&diagnose_dxgi());
    output.push_str("\n");

    output.push_str("=== End Diagnostic ===\n");

    println!("{}", output);

    if let Some(ref path) = options.output_file {
        if let Err(e) = write_to_file(path, &output) {
            eprintln!("Failed to write diagnostic file: {}", e);
        }
    }

    if options.exit_after {
        std::process::exit(0);
    }
}

fn write_to_file(path: &PathBuf, content: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", content)?;
    Ok(())
}

fn diagnose_system() -> String {
    let mut output = String::from("[System]\n");

    output.push_str(&format!(
        "  Rust Version: {}\n",
        env!("CARGO_PKG_RUST_VERSION")
    ));
    output.push_str(&format!(
        "  capture-core Version: {}\n",
        env!("CARGO_PKG_VERSION")
    ));

    #[cfg(windows)]
    {
        output.push_str("  Platform: Windows\n");

        use windows::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};

        unsafe {
            let mut os_info: OSVERSIONINFOW = std::mem::zeroed();
            os_info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;

            if GetVersionExW(&mut os_info).is_ok() {
                output.push_str(&format!(
                    "  Windows Version: {}.{} (Build {})\n",
                    os_info.dwMajorVersion, os_info.dwMinorVersion, os_info.dwBuildNumber
                ));
            }
        }
    }

    #[cfg(not(windows))]
    {
        output.push_str("  Platform: Non-Windows\n");
    }

    output
}

fn diagnose_displays() -> String {
    let mut output = String::from("[Displays]\n");

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{LPARAM, RECT};
        use windows::Win32::Graphics::Gdi::MONITORINFOEXW;
        use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR};

        struct DisplayInfo {
            name: String,
            is_primary: bool,
        }

        let mut displays: Vec<DisplayInfo> = Vec::new();

        unsafe extern "system" fn enum_callback(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            lparam: LPARAM,
        ) -> windows::core::BOOL {
            let displays = &mut *(lparam.0 as *mut Vec<DisplayInfo>);

            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(
                hmonitor,
                &mut info as *mut _ as *mut windows::Win32::Graphics::Gdi::MONITORINFO,
            )
            .as_bool()
            {
                let name = String::from_utf16_lossy(&info.szDevice).trim().to_string();
                let is_primary = (info.monitorInfo.dwFlags & 1) != 0;
                displays.push(DisplayInfo { name, is_primary });
            }

            windows::core::BOOL(1)
        }

        unsafe {
            let lparam = &mut displays as *mut _ as isize;
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(enum_callback),
                windows::Win32::Foundation::LPARAM(lparam),
            );
        }

        output.push_str(&format!("  Display Count: {}\n", displays.len()));
        for (idx, display) in displays.iter().enumerate() {
            output.push_str(&format!("  Display {}: {}\n", idx, display.name));
            if display.is_primary {
                output.push_str("    Primary: Yes\n");
            }
        }
    }

    #[cfg(not(windows))]
    {
        output.push_str("  Not on Windows\n");
    }

    output
}

fn diagnose_dxgi() -> String {
    let mut output = String::from("[DXGI Desktop Duplication]\n");

    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

        unsafe {
            match CreateDXGIFactory1::<IDXGIFactory1>() {
                Ok(factory) => {
                    output.push_str("  DXGI Factory: Created Successfully\n");

                    match factory.EnumAdapters1(0) {
                        Ok(adapter) => {
                            let desc = adapter.GetDesc1().unwrap_or_default();
                            let name = String::from_utf16_lossy(&desc.Description);
                            output.push_str(&format!("  Graphics Adapter: {}\n", name));
                        }
                        Err(e) => {
                            output.push_str(&format!("  Adapter Enum: FAILED ({})\n", e));
                        }
                    }
                }
                Err(e) => {
                    output.push_str("  DXGI Factory: FAILED\n");
                    output.push_str(&format!("  Error: {}\n", e));
                    output.push_str(
                        "  Hint: Missing DirectX runtime or graphics driver not installed\n",
                    );
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        output.push_str("  DXGI only available on Windows\n");
    }

    output
}
