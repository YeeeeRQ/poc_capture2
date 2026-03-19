# Capture

Windows 10/11 屏幕截图 + 录屏 CLI 工具，使用 Rust 构建。

## 功能

- **截图** — 捕获单个显示器的全屏图像，输出 PNG 或 JPEG
- **录屏** — 实时录制屏幕为 H.264 MP4 视频，支持自定义帧率和时长
- **列出显示器** — 查看所有可用显示器及其分辨率

## 依赖

### FFmpeg（必需）

FFmpeg 用于视频编码。工具会自动按以下顺序查找：

1. `FFMPEG_PATH` 环境变量指定的路径
2. `capture.exe` 可执行文件同级目录
3. `bin/ffmpeg.exe`
4. 系统 PATH

推荐使用 **静态编译版本**（如 [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) 的 full_build-static），只需复制 `ffmpeg.exe` 和 `ffprobe.exe` 两个文件即可，无需 DLL。

## 构建

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release
```

## 使用方法

### 列出显示器

```bash
capture list-monitors
```

### 截图

```bash
# 默认截图到当前目录（PNG 格式）
capture screenshot

# 指定显示器、输出路径、JPEG 格式
capture screenshot -m 0 -o screenshot.jpg -f jpg -q 85

# 参数说明
# -m, --monitor   显示器索引（0 = 主屏）
# -o, --output    输出文件路径
# -f, --format    输出格式：png 或 jpg
# -q, --quality   JPEG 质量 0-100
```

### 录屏

```bash
# 录制（Ctrl+C 停止）
capture record

# 指定显示器、输出路径、帧率、时长
capture record -m 0 -o video.mp4 -f 30 --duration 60 --preset medium

# 参数说明
# -m, --monitor   显示器索引（0 = 主屏）
# -o, --output    输出文件路径
# -f, --fps       帧率（默认 30）
# --duration      最大录制时长（秒），到达后自动停止
# --preset        FFmpeg 编码速度预设
#                 ultrafast > superfast > veryfast > faster > fast > medium > slow > slower > veryslow
#                 越快文件越大，越慢文件越小质量越高（默认 medium）
```

## 技术细节

| 模块 | 技术 |
|------|------|
| 屏幕捕获 | [xcap](https://crates.io/crates/xcap)（DXGI Desktop Duplication API） |
| 图片编码 | [image](https://crates.io/crates/image) crate |
| 视频编码 | FFmpeg（libx264 + H.264） |
| CLI | [clap](https://crates.io/crates/clap) v4 |
| 进程保护 | [ctrlc](https://crates.io/crates/ctrlc)（优雅处理 Ctrl+C） |

## 项目结构

```
src/
├── main.rs              # CLI 入口
├── encoder.rs           # FFmpeg 封装（stdin pipe 实时编码）
├── capture/
│   ├── screenshot.rs   # 截图实现
│   └── recorder.rs     # 录屏实现
└── cli/
    └── mod.rs           # clap 命令定义
bin/                    # FFmpeg 二进制文件（自行添加，gitignore）
```

## 分发

复制 `target/release/capture.exe` 和 FFmpeg 二进制到同一目录即可分发，无需安装 Rust 或其他依赖：

```
capture.exe
ffmpeg.exe
ffprobe.exe
```
