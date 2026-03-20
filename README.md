# Capture

A Windows 10/11 screen capture and recording tool with both CLI and GUI interfaces.

**Recording and screenshots both use [windows-capture](https://crates.io/crates/windows-capture) (DXGI Desktop Duplication) for high-performance, high-framerate capture.** No GDI or third-party capture library dependencies.

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
- **Screenshot button** — capture the selected monitor
- **Record / Stop button** — toggle recording
- **Pin button** — toggle always-on-top
- **Settings button** — open settings panel
- **Close button** — hides the window (process keeps running in system tray)

### System Tray

When the window is closed, the app continues running in the system tray:

**Tray Menu:**
- **显示窗口 (Show Window)** — restore the floating toolbar
- **截图 (Screenshot)** — capture current screen
- **开始录制 (Start Recording)** — begin recording
- **退出 (Quit)** — exit the application

**Left-click** on tray icon: Show window

**Session Folder Cleanup:** If you quit without taking any screenshots or recordings, the auto-generated session folder is automatically deleted.

---

## Architecture

### GUI Architecture

The GUI uses a state-driven architecture with a background worker thread:

```
[Tray Events] ──► [TrayState flags] ──► [Worker Thread]
                                               │
                                    ┌──────────┼──────────┐
                                    ▼          ▼          ▼
                              [Screenshot] [Recording] [Cleanup]
                                    │          │          │
                                    ▼          ▼          ▼
                              [Session Folder]      [Delete if empty]
```

**Key Components:**

| Component | Responsibility |
|-----------|----------------|
| `app.rs` | UI rendering only (reads state, sends viewport commands) |
| `tray.rs` | System tray + event handlers (set flags only) |
| `worker.rs` | Background worker thread (executes screenshot/recording/cleanup) |

**Why This Architecture?**

- **Window hidden = UI thread paused**: egui's `update()` stops when window is hidden
- **Worker thread runs independently**: Even when the window is hidden, screenshot/recording/quit still work
- **No blocking**: Tray handlers only set atomic flags, never block the event loop

### Recording Thread Model

```
┌──────────────────────────────────────────────────────────────┐
│  GUI thread (egui event loop)                                │
│  └── RecorderHandle (start / stop / take_screenshot)         │
│                    │                                         │
└────────────────────┼─────────────────────────────────────────┘
                     │
     ┌───────────────┼───────────────┐
     │               │               │
     ▼               ▼               ▼
┌──────────┐  ┌──────────┐  ┌──────────────────┐
│ Capture  │  │ Encoder  │  │ Screenshot       │
│ thread   │  │ thread   │  │ thread (in-rec)  │
│          │  │          │  │                  │
│ DXGI     │  │ FFmpeg   │  │ Poll request →  │
│ callback │──│ stdin    │  │ read snapshot →  │
│          │  │          │  │ encode → write   │
└──────────┘  └──────────┘  └──────────────────┘
     │               │
     ▼               ▼
  channel        .mp4 file
(bounded=2)
```

### Capture Thread (DXGI)

```
on_frame_arrived()
  ├── Skip if stop_requested or frame_tx is None
  ├── FPS throttle: skip if < interval since last sent
  ├── BGRA → RGBA conversion (in-place)
  ├── Send RGBA frame to encoder channel (non-blocking)
  ├── Update snapshot_buffer (atomic swap)     ← screenshot reads here
  └── Check duration limit
```

### Screenshot Architecture

#### In-Recording Screenshot

When a recording is active, the screenshot uses the **active capture session** via a dedicated background thread:

```
User clicks screenshot
  └── Worker reads tray_state.screenshot_flag
        └── set screenshot_request + notify_one()

Screenshot thread (independent polling)
  └── loop {
        if stopped: break
        req = request.lock().take()
        if req.is_some():
            pixels = snapshot_buffer.lock().clone()
            encode_screenshot_to_file(req, pixels)
        else:
            sleep(10ms) and retry
     }

Result: GUI never blocks, screenshot completes asynchronously
```

**Key design:**
- Screenshot thread polls `screenshot_request` with 10ms interval — no condvar deadlock risk
- `take_screenshot()` is fully non-blocking — GUI event loop never stalls
- `snapshot_buffer` provides atomic frame snapshot — capture thread writes, screenshot thread reads, zero contention on `&mut self`

#### Standalone Screenshot (non-recording)

When not recording, a separate `windows-capture` session captures a single frame:

```
take_screenshot(ScreenshotSettings)
  ├── Monitor::from_index(index + 1)
  ├── start_free_threaded(ScreenshotSettings)
  │     └── Spawns dedicated capture thread
  └── wait for frame on condvar (< 500ms typically)
        └── BGRA → RGBA → encode → write file → stop capture
```

### Encoder Thread

```
frame_rx.recv_timeout(100ms)
  ├── Ok(frame) → enc.write_frame(data)
  ├── Timeout   → check stop_flag / done_rx, then retry
  └── Disconnected → enc.finish()
        └── child.wait() ensures FFmpeg fully completes
```

---

## Key Design Decisions

- **Capture and encoding on separate threads** — frame acquisition is decoupled from FFmpeg write speed; a bounded channel (capacity=2) absorbs temporary throughput spikes
- **MinimumUpdateIntervalSettings** limits DXGI callback rate to target FPS, avoiding unnecessary CPU/GPU work
- **BGRA→RGBA conversion on capture thread** — encoder receives pre-converted data, no conversion overhead in the hot path
- **Non-blocking screenshot** — `take_screenshot()` returns immediately; encoding runs on an independent thread; GUI is never blocked
- **Double-buffered snapshot** — capture thread atomically updates `snapshot_buffer`; screenshot thread reads without locking `&mut self`, eliminating deadlocks between GUI and capture threads
- **Encoder drains channel on exit** — no frame loss at end of recording
- **State-driven UI** — UI reads from shared `TrayState`, never owns recording/cleanup logic
- **Worker thread runs always** — independent of window visibility, ensures tray commands work even when hidden

---

## Tech Stack

| Module | Technology |
|--------|------------|
| Screen capture | [windows-capture](https://github.com/YeeeeRQ/windows-capture) (DXGI Desktop Duplication, fork with graceful fallback) |
| Screenshot | [windows-capture](https://github.com/YeeeeRQ/windows-capture) (DXGI, same fork) |
| Image encoding | [image](https://crates.io/crates/image) crate (PNG, BMP, JPEG) |
| Video encoding | FFmpeg libx264 (H.264) |
| GUI framework | [eframe/egui](https://crates.io/crates/egui) |
| System tray | [tray-icon](https://crates.io/crates/tray-icon) |
| CLI | [clap v4](https://crates.io/crates/clap) |
| Threading | [crossbeam](https://crates.io/crates/crossbeam) + parking_lot |

### About the windows-capture Fork

This project uses [YeeeeRQ/windows-capture](https://github.com/YeeeeRQ/windows-capture) fork based on v1.5.0 with Graceful Fallback mechanism:

- When Graphics Capture API features (like `IsBorderRequired`) are not supported on the current system, outputs warnings instead of errors
- Fixes "Toggling the capture border is not supported" error on certain Windows 10/11 systems
- In testing, when detection returns false, screenshot and recording still work normally

---

## Binary Distribution

```
capture-gui.exe    # GUI (~42MB, embedded fonts)
capture-cli.exe   # CLI (~1.2MB)
ffmpeg.exe        # FFmpeg static build
ffprobe.exe       # FFprobe
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
- 截图
- 录制/停止
- 置顶切换
- 设置面板
- 关闭（隐藏窗口，进程继续在系统托盘运行）

### 系统托盘

关闭窗口后，应用会在系统托盘继续运行：

**托盘菜单：**
- **显示窗口** — 显示浮动工具条
- **截图** — 捕获当前屏幕
- **开始录制** — 开始录屏
- **退出** — 退出应用

**左键点击**托盘图标：显示窗口

**Session 文件夹清理：** 如果退出前没有进行任何截图或录屏操作，自动生成的 session 文件夹会被自动删除。

### 架构设计

#### GUI 架构

GUI 采用状态驱动的架构，包含后台 worker 线程：

```
[托盘事件] ──► [TrayState 标志位] ──► [Worker 线程]
                                              │
                                   ┌──────────┼──────────┐
                                   ▼          ▼          ▼
                             [截图]    [录屏]    [清理]
                                   │          │          │
                                   ▼          ▼          ▼
                             [Session 文件夹]    [空则删除]
```

**核心组件：**

| 组件 | 职责 |
|------|------|
| `app.rs` | 仅负责 UI 渲染（读取状态，发送 viewport 命令） |
| `tray.rs` | 系统托盘 + 事件处理（仅设置标志位） |
| `worker.rs` | 后台 worker 线程（执行截图/录屏/清理） |

**为什么这样设计？**

- **窗口隐藏 = UI 线程暂停**：egui 的 `update()` 在窗口隐藏时停止
- **Worker 线程独立运行**：即使窗口隐藏，截图/录屏/退出仍然有效
- **无阻塞**：托盘处理器只设置原子标志位，不阻塞事件循环

#### 录屏线程模型

```
┌──────────────────────────────────────────────────────────────┐
│  GUI 线程 (egui 事件循环)                                    │
│  └── RecorderHandle (start / stop / take_screenshot)        │
│                    │                                         │
└────────────────────┼─────────────────────────────────────────┘
                     │
     ┌───────────────┼───────────────┐
     │               │               │
     ▼               ▼               ▼
┌──────────┐  ┌──────────┐  ┌──────────────────┐
│ Capture  │  │ Encoder  │  │ Screenshot       │
│ 线程     │  │ 线程     │  │ 线程 (录制中)    │
│          │  │          │  │                  │
│ DXGI     │  │ FFmpeg   │  │ 轮询请求 →       │
│ 回调     │──│ stdin    │  │ 读取快照 →       │
│          │  │          │  │ 编码 → 写入      │
└──────────┘  └──────────┘  └──────────────────┘
     │               │
     ▼               ▼
  channel        .mp4 文件
(bounded=2)
```

#### 截图架构

**录制中截图** — 使用活动录制会话的后台线程：

```
用户点击截图
  └── Worker 读取 tray_state.screenshot_flag
        └── 设置 screenshot_request + notify_one()

独立轮询的截图线程
  └── 循环 {
        if stopped: 退出
        req = request.lock().take()
        if req 存在:
            pixels = snapshot_buffer.lock().clone()
            encode_screenshot_to_file(req, pixels)
        else:
            sleep(10ms) 重试
     }

结果：GUI 从不阻塞，截图异步完成
```

**关键设计：**
- 截图线程通过 10ms 间隔轮询 `screenshot_request` — 无 condvar 死锁风险
- `take_screenshot()` 完全非阻塞 — GUI 事件循环从不卡顿
- `snapshot_buffer` 提供原子帧快照 — capture 线程写入，截图线程读取，零竞争

**非录制独立截图** — 创建单独的 windows-capture 会话捕获单帧：

```
take_screenshot(ScreenshotSettings)
  ├── Monitor::from_index(index + 1)
  ├── start_free_threaded(ScreenshotSettings)
  │     └── 启动专用 capture 线程
  └── 等待帧到达（通常 < 500ms）
        └── BGRA → RGBA → 编码 → 写文件 → 停止 capture
```

#### 编码器线程

```
frame_rx.recv_timeout(100ms)
  ├── Ok(frame) → enc.write_frame(data)
  ├── Timeout   → 检查 stop_flag / done_rx，重试
  └── Disconnected → enc.finish()
        └── child.wait() 确保 FFmpeg 完全结束
```

### 关键设计决策

- **Capture 和 encoding 在独立线程** — 帧获取与 FFmpeg 写入速度解耦；有界 channel（容量=2）吸收临时吞吐峰值
- **MinimumUpdateIntervalSettings** 限制 DXGI 回调率为目标 FPS，避免不必要的 CPU/GPU 开销
- **BGRA→RGBA 转换在 capture 线程** — 编码器收到预转换数据，热路径无转换开销
- **非阻塞截图** — `take_screenshot()` 立即返回；编码在独立线程运行；GUI 永不阻塞
- **双缓冲快照** — capture 线程原子更新 `snapshot_buffer`；截图线程读取时无需锁定 `&mut self`，消除 GUI 与 capture 线程之间的死锁
- **编码器退出时排空 channel** — 录制结束时无帧丢失
- **状态驱动 UI** — UI 从共享 `TrayState` 读取状态，不拥有录屏/清理逻辑
- **Worker 线程常驻运行** — 独立于窗口可见性，确保托盘命令在窗口隐藏时仍然有效

### 技术栈

| 模块 | 技术 |
|------|------|
| 屏幕捕获 | [windows-capture](https://github.com/YeeeeRQ/windows-capture) (DXGI Desktop Duplication，含 graceful fallback) |
| 截图 | [windows-capture](https://github.com/YeeeeRQ/windows-capture) (DXGI，同一 fork) |
| 图片编码 | [image](https://crates.io/crates/image) crate (PNG, BMP, JPEG) |
| 视频编码 | FFmpeg libx264 (H.264) |
| GUI 框架 | [eframe/egui](https://crates.io/crates/egui) |
| 系统托盘 | [tray-icon](https://crates.io/crates/tray-icon) |
| CLI | [clap v4](https://crates.io/crates/clap) |
| 线程 | [crossbeam](https://crates.io/crates/crossbeam) + parking_lot |

### 关于 windows-capture Fork

本项目使用 [YeeeeRQ/windows-capture](https://github.com/YeeeeRQ/windows-capture) fork，基于 v1.5.0 添加了 Graceful Fallback 机制：

- 当 Graphics Capture API 某些功能（如 `IsBorderRequired`）在当前系统不支持时，输出警告而非报错
- 解决了在某些 Windows 10/11 系统上出现的 "Toggling the capture border is not supported" 错误
- 实际测试中，检测返回 false 时截图和录屏功能仍然正常工作

### 二进制分发

```
capture-gui.exe    # GUI 版本
capture-cli.exe    # CLI 版本
ffmpeg.exe         # FFmpeg 静态编译版
ffprobe.exe        # FFprobe
```

仅需以上四个文件，无需安装任何运行时。
