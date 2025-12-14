# 开发环境搭建

## 当前项目状态

**已完成**: 项目骨架已创建，前端依赖已安装

```bash
# 项目已有结构
engram/
├── src-tauri/     # Rust 后端 (已创建)
├── src-ui/        # SolidJS 前端 (已创建，依赖已安装)
├── llmdoc/        # 项目文档 (已创建)
└── README.md      # 项目说明 (已创建)
```

## 系统要求

### 最低配置
- **CPU**: 4 核心 (支持 AVX2)
- **内存**: 8GB RAM
- **存储**: 10GB 可用空间
- **操作系统**: Windows 10 (1903+) / macOS 12.3+ / Ubuntu 22.04+

### 推荐配置
- **CPU**: 8 核心 (支持 AVX512)
- **内存**: 16GB+ RAM
- **GPU**: 支持 CUDA 11+ 或 Apple Silicon
- **存储**: SSD 20GB+

## 快速开始

### 1. 安装 Rust 工具链

```bash
# 安装 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 设置默认工具链
rustup default stable

# 验证安装
rustc --version  # >= 1.75.0
cargo --version
```

### 2. 安装 Tauri CLI

```bash
cargo install tauri-cli

# 验证
cargo tauri --version  # >= 2.0.0
```

### 3. 安装平台依赖

#### Windows

```powershell
# 安装 Visual Studio Build Tools
# 下载: https://visualstudio.microsoft.com/visual-cpp-build-tools/
# 选择 "Desktop development with C++"

# 安装 WebView2 Runtime (Windows 10 需要)
# 下载: https://developer.microsoft.com/microsoft-edge/webview2/
```

#### macOS

```bash
# 安装 Xcode Command Line Tools
xcode-select --install
```

#### Linux (Ubuntu/Debian)

```bash
# 安装系统依赖
sudo apt update
sudo apt install -y \
    libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libpipewire-0.3-dev
```

### 4. 运行项目

```bash
cd /home/djj/code/engram

# 前端依赖已安装，直接运行开发模式
cargo tauri dev
```

## 完整依赖安装

### Node.js 环境

```bash
# 推荐使用 fnm 管理 Node 版本
curl -fsSL https://fnm.vercel.app/install | bash

# 安装 Node.js 20 LTS
fnm install 20
fnm use 20

# 验证
node --version  # >= 20.0.0
npm --version
```

### AI 模型下载 (Phase 2+)

```bash
# 创建模型目录
mkdir -p ~/.engram/models

# 下载 PaddleOCR 模型 (ONNX 格式) - Phase 2 需要
# 检测模型
wget -O ~/.engram/models/ppocr_det_v4.onnx \
    https://huggingface.co/ppocrv4/ppocr_det_v4/resolve/main/ppocr_det_v4.onnx

# 识别模型
wget -O ~/.engram/models/ppocr_rec_v4.onnx \
    https://huggingface.co/ppocrv4/ppocr_rec_v4/resolve/main/ppocr_rec_v4.onnx

# Qwen 2.5 7B (可选，Phase 3 摘要功能)
wget -O ~/.engram/models/qwen2.5-7b-instruct-q4_k_m.gguf \
    https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF/resolve/main/qwen2.5-7b-instruct-q4_k_m.gguf
```

## 项目结构

```
engram/
├── src-tauri/                      # Rust 后端
│   ├── Cargo.toml                  # Rust 依赖配置
│   ├── build.rs                    # 构建脚本
│   ├── tauri.conf.json             # Tauri 应用配置
│   ├── capabilities/
│   │   └── default.json            # 权限配置
│   └── src/
│       ├── main.rs                 # 入口 + 系统托盘
│       ├── lib.rs                  # 库入口 + AppState
│       ├── daemon/                 # 后台服务模块
│       │   ├── mod.rs              # EngramDaemon
│       │   ├── capture.rs          # 屏幕捕获
│       │   ├── context.rs          # 窗口上下文
│       │   └── hasher.rs           # 感知哈希
│       ├── db/                     # 数据库模块
│       │   ├── mod.rs              # Database
│       │   ├── schema.rs           # Schema 初始化
│       │   └── models.rs           # 数据模型
│       └── commands/
│           └── mod.rs              # Tauri API
├── src-ui/                         # SolidJS 前端
│   ├── package.json                # npm 配置
│   ├── vite.config.ts              # Vite 配置
│   ├── tsconfig.json               # TypeScript 配置
│   ├── tailwind.config.js          # Tailwind 配置
│   ├── postcss.config.js           # PostCSS 配置
│   ├── index.html                  # HTML 入口
│   └── src/
│       ├── index.tsx               # 应用入口
│       ├── index.css               # 全局样式
│       ├── App.tsx                 # 主应用 + 路由
│       └── pages/
│           ├── Timeline.tsx        # 时间线页面
│           ├── Search.tsx          # 搜索页面
│           └── Settings.tsx        # 设置页面
├── llmdoc/                         # 项目文档
│   ├── index.md                    # 文档索引
│   ├── overview/                   # 项目概览
│   ├── architecture/               # 系统架构
│   ├── guides/                     # 开发指南
│   └── reference/                  # 参考规范
└── README.md
```

## 开发命令

```bash
# 开发模式 (热重载)
cargo tauri dev

# 构建发布版本
cargo tauri build

# 仅运行 Rust 测试
cd src-tauri && cargo test

# 仅运行前端开发服务器
cd src-ui && npm run dev

# 检查代码风格
cd src-tauri && cargo fmt --check && cargo clippy

# 前端类型检查
cd src-ui && npm run typecheck
```

## 环境变量

```bash
# 开发环境
ENGRAM_LOG_LEVEL=debug
ENGRAM_DATA_DIR=~/.engram-dev
ENGRAM_MODEL_DIR=~/.engram/models

# 生产环境
ENGRAM_LOG_LEVEL=info
ENGRAM_DATA_DIR=~/.engram
```

## IDE 配置

### VS Code 推荐扩展

```json
// .vscode/extensions.json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "tauri-apps.tauri-vscode",
    "bradlc.vscode-tailwindcss",
    "esbenp.prettier-vscode"
  ]
}
```

### rust-analyzer 配置

```json
// .vscode/settings.json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.check.command": "clippy",
  "[rust]": {
    "editor.formatOnSave": true
  }
}
```

## 常见问题

### Q: cargo tauri dev 报错找不到前端

确保在项目根目录运行，且 `src-ui/` 目录存在。

```bash
cd /home/djj/code/engram
cargo tauri dev
```

### Q: Linux 下截图权限问题

Wayland 环境需要授权 PipeWire 权限：

```bash
# 安装 xdg-desktop-portal
sudo apt install xdg-desktop-portal xdg-desktop-portal-gtk
```

### Q: macOS 下截图权限问题

首次运行时系统会请求屏幕录制权限，在"系统偏好设置 > 隐私与安全 > 屏幕录制"中授权。

### Q: Windows 下编译失败

确保安装了 Visual Studio Build Tools 并选择了 "Desktop development with C++"。
