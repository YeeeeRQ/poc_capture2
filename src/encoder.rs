use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};

pub struct FfmpegEncoder {
    width: u32,
    height: u32,
    fps: u32,
    output_path: PathBuf,
    stdin: Option<ChildStdin>,
    raw_mode: bool,
    raw_file: Option<std::fs::File>,
}

impl FfmpegEncoder {
    pub fn new(
        width: u32,
        height: u32,
        fps: u32,
        output_path: &Path,
        preset: &str,
    ) -> Result<Self> {
        let ffmpeg_available = Self::check_ffmpeg();

        if !ffmpeg_available {
            println!("[WARN] FFmpeg not found in PATH. Using raw RGBA mode.");
            println!("[WARN] After recording, encode manually with:");
            println!(
                "  ffmpeg -framerate {} -pix_fmt rgba -s {}x{} -i raw.rgba -c:v libx264 -pix_fmt yuv420p -preset {} \"{}\"",
                fps, width, height, preset, output_path.display()
            );
            let raw_path = std::env::temp_dir().join("capture_raw.rgba");
            let raw_file = std::fs::File::create(&raw_path)
                .with_context(|| format!("Failed to create raw file: {}", raw_path.display()))?;
            return Ok(Self {
                width,
                height,
                fps,
                output_path: output_path.to_path_buf(),
                stdin: None,
                raw_mode: true,
                raw_file: Some(raw_file),
            });
        }

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", width, height),
            "-framerate",
            &fps.to_string(),
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
            output_path.to_str().unwrap_or("output.mp4"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn FFmpeg")?;

        let stdin = child.stdin.take().context("Failed to take FFmpeg stdin")?;

        Ok(Self {
            width,
            height,
            fps,
            output_path: output_path.to_path_buf(),
            stdin: Some(stdin),
            raw_mode: false,
            raw_file: None,
        })
    }

    fn check_ffmpeg() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    pub fn write_frame(&mut self, rgba_data: &[u8]) -> Result<()> {
        if self.raw_mode {
            if let Some(ref mut f) = self.raw_file {
                f.write_all(rgba_data)
                    .context("Failed to write raw frame")?;
            }
            return Ok(());
        }

        if let Some(ref mut stdin) = self.stdin {
            stdin
                .write_all(rgba_data)
                .context("Failed to write frame to FFmpeg stdin")?;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.stdin = None;

        if self.raw_mode {
            if let Some(raw_path) = self.raw_file.take().and_then(|_| {
                let p = std::env::temp_dir().join("capture_raw.rgba");
                if p.exists() {
                    Some(p)
                } else {
                    None
                }
            }) {
                println!("\nEncoding raw file with FFmpeg...");
                let status = Command::new("ffmpeg")
                    .args([
                        "-y",
                        "-framerate",
                        &self.fps.to_string(),
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
                    .status();

                if status.map(|s| s.success()).unwrap_or(false) {
                    let _ = std::fs::remove_file(&raw_path);
                    println!("Encoding complete: {}", self.output_path.display());
                } else {
                    println!(
                        "[WARN] FFmpeg encoding failed. Raw file kept at: {}",
                        raw_path.display()
                    );
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
