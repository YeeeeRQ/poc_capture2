use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::fs;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct FfmpegEncoder {
    width: u32,
    height: u32,
    output_path: PathBuf,
    stdin: Option<ChildStdin>,
    child: Option<std::process::Child>,
    raw_mode: bool,
    raw_file: Option<std::fs::File>,
    ffmpeg_path: Option<PathBuf>,
    frames_written: Arc<Mutex<u64>>,
    bytes_written: Arc<Mutex<u64>>,
    stderr_path: PathBuf,
}

impl FfmpegEncoder {
    pub fn new(
        width: u32,
        height: u32,
        output_path: &Path,
        preset: &str,
        target_fps: u32,
    ) -> Result<Self> {
        let ffmpeg_path = Self::find_ffmpeg();
        let ffmpeg_exe = ffmpeg_path.as_ref().map(|p| p.as_os_str());

        let stderr_path = std::env::temp_dir().join("capture_ffmpeg_stderr.log");

        if ffmpeg_exe.is_none() {
            log::warn!("FFmpeg not found. Using raw RGBA fallback mode.");
            let raw_path = std::env::temp_dir().join("capture_raw.rgba");
            let raw_file = std::fs::File::create(&raw_path)
                .with_context(|| format!("Failed to create raw file: {}", raw_path.display()))?;
            return Ok(Self {
                width,
                height,
                output_path: output_path.to_path_buf(),
                stdin: None,
                child: None,
                raw_mode: true,
                raw_file: Some(raw_file),
                ffmpeg_path: None,
                frames_written: Arc::new(Mutex::new(0)),
                bytes_written: Arc::new(Mutex::new(0)),
                stderr_path,
            });
        }

        let ffmpeg_exe = ffmpeg_exe.unwrap();

        let mut cmd = Command::new(ffmpeg_exe);
        cmd.args([
            "-y",
            "-f",
            "rawvideo",
            "-framerate",
            &target_fps.to_string(),
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", width, height),
            "-use_wallclock_as_timestamps",
            "1",
            "-i",
            "pipe:0",
            "-c:v",
            "libx264",
            "-preset",
            preset,
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "23",
            "-r",
            &target_fps.to_string(),
            output_path.to_str().unwrap_or("output.mp4"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().context("Failed to spawn FFmpeg")?;

        let stderr_path_clone = stderr_path.clone();
        if let Some(mut cs) = child.stderr.take() {
            std::thread::spawn(move || {
                if let Ok(mut sf) = fs::File::create(&stderr_path_clone) {
                    std::io::copy(&mut cs, &mut sf).ok();
                }
            });
        }

        let stdin = child.stdin.take().context("Failed to take FFmpeg stdin")?;

        log::info!(
            "FFmpeg started: {} ({}x{}, preset={})",
            output_path.display(),
            width,
            height,
            preset
        );

        Ok(Self {
            width,
            height,
            output_path: output_path.to_path_buf(),
            stdin: Some(stdin),
            child: Some(child),
            raw_mode: false,
            raw_file: None,
            ffmpeg_path,
            frames_written: Arc::new(Mutex::new(0)),
            bytes_written: Arc::new(Mutex::new(0)),
            stderr_path,
        })
    }

    fn find_ffmpeg() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("FFMPEG_PATH") {
            let p = PathBuf::from(&path);
            if p.exists() {
                return Some(p);
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let paths = [
                    exe_dir.join("ffmpeg.exe"),
                    exe_dir.join("bin").join("ffmpeg.exe"),
                ];
                for p in &paths {
                    if p.exists() {
                        return Some(p.clone());
                    }
                }

                if let Some(grandparent) = exe_dir.parent().and_then(|p| p.parent()) {
                    let p = grandparent.join("bin").join("ffmpeg.exe");
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }

        if Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return Some(PathBuf::from("ffmpeg"));
        }

        None
    }

    pub fn write_frame(&mut self, rgba_data: &[u8]) -> Result<()> {
        if self.raw_mode {
            if let Some(ref mut f) = self.raw_file {
                f.write_all(rgba_data)
                    .context("Failed to write raw frame")?;
                *self.frames_written.lock() += 1;
                *self.bytes_written.lock() += rgba_data.len() as u64;
            }
            return Ok(());
        }

        if let Some(ref mut stdin) = self.stdin {
            let write_start = Instant::now();
            stdin
                .write_all(rgba_data)
                .context("Failed to write frame to FFmpeg stdin")?;
            let write_elapsed = write_start.elapsed();
            *self.frames_written.lock() += 1;
            *self.bytes_written.lock() += rgba_data.len() as u64;

            if write_elapsed.as_millis() > 50 {
                let fw = *self.frames_written.lock();
                log::warn!(
                    "Slow stdin write: {:.1}ms for frame {} ({} bytes)",
                    write_elapsed.as_secs_f64() * 1000.0,
                    fw,
                    rgba_data.len()
                );
            }
        }
        Ok(())
    }

    pub fn frames_written(&self) -> u64 {
        *self.frames_written.lock()
    }

    pub fn bytes_written(&self) -> u64 {
        *self.bytes_written.lock()
    }

    pub fn frame_size(&self) -> u64 {
        self.width as u64 * self.height as u64 * 4
    }

    pub fn finish(&mut self) -> Result<()> {
        self.stdin = None;

        if let Some(ref mut child) = self.child {
            if let Ok(status) = child.wait() {
                log::info!("FFmpeg exited with status: {}", status);
            } else {
                log::warn!("FFmpeg did not exit cleanly");
            }
        }

        let fw = *self.frames_written.lock();
        let bw = *self.bytes_written.lock();
        let expected = self.frame_size() * fw;

        log::info!(
            "Encoder finish: frames_written={}, bytes_written={}, expected={}, match={}",
            fw,
            bw,
            expected,
            bw == expected
        );

        if self.stderr_path.exists() {
            if let Ok(content) = fs::read_to_string(&self.stderr_path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    log::info!("FFmpeg stderr:\n{}", trimmed);
                }
            }
            let _ = fs::remove_file(&self.stderr_path);
        }

        if self.raw_mode {
            if let Some(raw_path) = self.raw_file.take().and_then(|_| {
                let p = std::env::temp_dir().join("capture_raw.rgba");
                if p.exists() {
                    Some(p)
                } else {
                    None
                }
            }) {
                let ffmpeg = match self.ffmpeg_path.as_ref() {
                    Some(p) => p,
                    None => return Ok(()),
                };
                log::info!("Encoding raw file with FFmpeg...");
                let status = Command::new(ffmpeg)
                    .args([
                        "-y",
                        "-f",
                        "rawvideo",
                        "-pix_fmt",
                        "rgba",
                        "-s",
                        &format!("{}x{}", self.width, self.height),
                        "-i",
                        raw_path.to_str().unwrap_or(""),
                        "-c:v",
                        "libx264",
                        "-pix_fmt",
                        "yuv420p",
                        "-preset",
                        "medium",
                        self.output_path.to_str().unwrap_or("output.mp4"),
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW)
                    .status();

                if status.map(|s| s.success()).unwrap_or(false) {
                    let _ = fs::remove_file(&raw_path);
                    log::info!("Encoding complete: {}", self.output_path.display());
                }
            }
        }

        Ok(())
    }
}

impl Drop for FfmpegEncoder {
    fn drop(&mut self) {
        self.finish().ok();
    }
}
