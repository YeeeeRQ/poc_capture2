# Capture

Windows 10/11 屏幕截图 + 录屏工具，提供 CLI 和 GUI 两种界面。

## 项目结构

```
capture-core/              # 共享核心库（screenshot + recorder + encoder）
capture-gui/              # GUI 二进制（egui frameless 工具条，含嵌入字体）
capture-gui/fonts/        # 嵌入字体（Noto Sans SC + Noto Color Emoji）
capture-cli/              # CLI 二进制
bin/                     # FFmpeg 静态二进制（自行添加，gitignore）
src_old/                 # 废弃的旧实现（参考用）
```

## 快速开始

### 构建

```bash
# 构建所有
cargo build --release

# 仅构建 CLI
cargo build -p capture-cli --release

# 仅构建 GUI
cargo build -p capture-gui --release
```

### 准备 FFmpeg

将 `ffmpeg.exe` 和 `ffprobe.exe` 复制到 release 目录：

```bash
cp bin/ffmpeg.exe target/release/
cp bin/ffprobe.exe target/release/
```

### CLI 使用

```bash
# 列出显示器
capture-cli list-monitors

# 截图
capture-cli screenshot -m 0 -o out.png -f jpg -q 85

# 录屏（Ctrl+C 停止）
capture-cli record -m 0 -o video.mp4 -f 30 --preset medium
```

### GUI 使用

```bash
./capture-gui.exe
```

GUI 是一个无边框浮动工具条，包含：
- 显示器选择
- 截图按钮
- 录制/停止按钮
- FPS 调节
- 关闭按钮（最小化到后台）

窗口默认置顶，关闭按钮将窗口隐藏（进程继续运行）。

## 技术栈

| 模块 | 技术 |
|------|------|
| 屏幕捕获 | xcap (DXGI Desktop Duplication) |
| 图片编码 | image crate |
| 视频编码 | FFmpeg libx264 (H.264) |
| GUI 框架 | eframe/egui |
| CLI | clap v4 |
| 线程管理 | parking_lot |

## 二进制分发

```
capture-gui.exe    # GUI 版本（~42MB，含嵌入字体）
capture-cli.exe    # CLI 版本（~1.2MB）
ffmpeg.exe         # FFmpeg 静态编译版
ffprobe.exe
```

仅需以上四个文件，无需安装任何运行时。
