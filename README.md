# Capture

A Windows 10/11 screen capture and recording tool with both CLI and GUI interfaces.

**Recording uses [windows-capture](https://crates.io/crates/windows-capture) (DXGI Desktop Duplication) for high-performance, high-framerate capture with configurable target FPS.** Screenshots use [xcap](https://crates.io/crates/xcap).

---

## Project Structure

```
capture-core/              # Shared core library (screenshot + recorder + encoder)
capture-gui/              # GUI binary (egui frameless floating toolbar)
capture-gui/fonts/        # Embedded fonts (Noto Sans SC + Noto Color Emoji)
capture-cli/              # CLI binary
bin/                      # FFmpeg static binaries (add manually, gitignored)
```

---

## Quick Start

### Build

```bash
# Build all
cargo build --release

# CLI only
cargo build -p capture-cli --release

# GUI only
cargo build -p capture-gui --release
```

### Prepare FFmpeg

Copy `ffmpeg.exe` and `ffprobe.exe` to the release directory:

```bash
cp bin/ffmpeg.exe target/release/
cp bin/ffprobe.exe target/release/
```

---

## CLI Usage

```bash
# List monitors
capture-cli list-monitors

# Screenshot
capture-cli screenshot -m 0 -o out.png

# Record (Ctrl+C to stop, --fps and --duration optional)
capture-cli record -m 0 -o video.mp4 --fps 60 --preset medium

# Record with duration limit
capture-cli record -m 0 --fps 30 --duration 10
```

### CLI Arguments

| Flag | Description | Default |
|------|-------------|---------|
| `-m, --monitor` | Monitor index | `0` |
| `-o, --output` | Output file path | Auto-generated |
| `--fps` | Target recording framerate | `60` |
| `--duration` | Recording duration in seconds | Unlimited |
| `--preset` | FFmpeg encoding preset | `medium` |

---

## GUI Usage

```bash
./capture-gui.exe
```

The GUI is a frameless floating toolbar:

- **Monitor selector** — choose which display to capture
- **FPS control** — drag to set target framerate (1–120 fps)
- **Screenshot button** — capture the selected monitor
- **Record / Stop button** — toggle recording
- **Pin button** — toggle always-on-top
- **Close button** — hides the window (process keeps running)

The FPS control is hidden during recording. Recording timer shows elapsed time.

---

## Architecture

```
Capture thread (DXGI Desktop Duplication)
    │
    ├── BGRA → RGBA conversion
    ├── Frame rate limiting (MinimumUpdateIntervalSettings)
    └── Non-blocking send (bounded channel, capacity=2)
              │
              ▼
    Encoder thread
    │
    └── write_frame() → FFmpeg stdin
              │
              ▼
    FFmpeg (libx264, H.264)
    └── output .mp4 file
```

**Key design decisions:**

- Capture and encoding run on **separate threads**, decoupling frame acquisition from FFmpeg write speed
- `MinimumUpdateIntervalSettings` limits DXGI callback rate to target FPS
- `bgra_to_rgba` conversion happens on the capture thread before sending
- Encoder thread drains the channel before exiting (no frame loss at end)
- `child.wait()` ensures FFmpeg fully completes before recording stops

---

## Tech Stack

| Module | Technology |
|--------|------------|
| Screen capture | [windows-capture](https://crates.io/crates/windows-capture) (DXGI) |
| Screenshot | [xcap](https://crates.io/crates/xcap) (GDI) |
| Image encoding | [image](https://crates.io/crates/image) crate |
| Video encoding | FFmpeg libx264 (H.264) |
| GUI framework | [eframe/egui](https://crates.io/crates/egui) |
| CLI | [clap v4](https://crates.io/crates/clap) |
| Threading | [crossbeam](https://crates.io/crates/crossbeam) + parking_lot |

---

## Binary Distribution

```
capture-gui.exe    # GUI (~42MB, embedded fonts)
capture-cli.exe    # CLI (~1.2MB)
ffmpeg.exe         # FFmpeg static build
ffprobe.exe        # FFprobe
```

No runtime installation required.

---

## Screenshots

See the [screenshots](screenshots/) directory for preview images.

---

## 简体中文

### 项目结构

```
capture-core/              # 共享核心库（截图 + 录屏 + 编码器）
capture-gui/              # GUI 二进制（egui 无边框浮动工具条）
capture-gui/fonts/        # 嵌入字体（Noto Sans SC + Noto Color Emoji）
capture-cli/              # CLI 二进制
bin/                      # FFmpeg 静态二进制（自行添加，gitignore）
```

### 快速开始

```bash
# 构建所有
cargo build --release

# 准备 FFmpeg
cp bin/ffmpeg.exe target/release/
cp bin/ffprobe.exe target/release/
```

### CLI 使用

```bash
# 列出显示器
capture-cli list-monitors

# 截图
capture-cli screenshot -m 0 -o out.png

# 录屏（Ctrl+C 停止）
capture-cli record -m 0 -o video.mp4 --fps 60

# 指定时长
capture-cli record -m 0 --fps 30 --duration 10
```

### GUI 使用

```bash
./capture-gui.exe
```

GUI 是一个无边框浮动工具条，支持：

- 显示器选择
- FPS 调节（录制时隐藏）
- 截图
- 录制/停止
- 置顶切换
- 关闭（隐藏窗口，进程继续运行）

### 技术栈

| 模块 | 技术 |
|------|------|
| 屏幕捕获 | [windows-capture](https://crates.io/crates/windows-capture) (DXGI Desktop Duplication) |
| 截图 | [xcap](https://crates.io/crates/xcap) (GDI) |
| 图片编码 | [image](https://crates.io/crates/image) crate |
| 视频编码 | FFmpeg libx264 (H.264) |
| GUI 框架 | [eframe/egui](https://crates.io/crates/egui) |
| CLI | [clap v4](https://crates.io/crates/clap) |
| 线程 | [crossbeam](https://crates.io/crates/crossbeam) + parking_lot |

### 二进制分发

```
capture-gui.exe    # GUI 版本
capture-cli.exe    # CLI 版本
ffmpeg.exe         # FFmpeg 静态编译版
ffprobe.exe        # FFprobe
```

仅需以上四个文件，无需安装任何运行时。
