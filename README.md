[简体中文](README_zh-CN.md)

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
- **Show Window** — restore the floating toolbar
- **Screenshot** — capture current screen
- **Start Recording** — begin recording
- **Quit** — exit the application

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
| Screen capture / Screenshot | [windows-capture](https://github.com/YeeeeRQ/windows-capture) (DXGI Desktop Duplication, fork with graceful fallback) |
| Image encoding | [image](https://crates.io/crates/image) crate (PNG, BMP, JPEG) |
| Video encoding | FFmpeg libx264 (H.264) |
| GUI framework | [eframe/egui](https://crates.io/crates/egui) |
| System tray | [tray-icon](https://crates.io/crates/tray-icon) |
| CLI | [clap v4](https://crates.io/crates/clap) |
| Threading | [crossbeam](https://crates.io/crossbeam) + parking_lot |

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
