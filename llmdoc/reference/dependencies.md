# 依赖清单

## Rust 依赖 (Cargo.toml)

### 核心框架

```toml
[dependencies]
# Tauri v2 核心
tauri = { version = "2", features = [
    "tray-icon",        # 系统托盘
    "shell-open",       # 打开外部链接
    "protocol-asset",   # 本地资源协议
] }
tauri-plugin-shell = "2"  # Shell 命令支持

# 异步运行时
tokio = { version = "1", features = ["full"] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"  # 配置文件
```

### 屏幕捕获

```toml
# 跨平台截图 (Windows Graphics Capture / ScreenCaptureKit / PipeWire)
xcap = "0.7.1"

# 用户闲置检测 (跨平台)
user-idle = "0.6"

# Windows 窗口信息
[target.'cfg(target_os = "windows")'.dependencies]
# TODO: 待集成 Windows API

# Linux X11 窗口信息
[target.'cfg(target_os = "linux")'.dependencies]
x11rb = { version = "0.13", features = ["allow-unsafe-code"] }

# macOS 窗口信息
[target.'cfg(target_os = "macos")'.dependencies]
# TODO: 待集成 AppKit
```

### 图像处理

```toml
# 图像操作 (已集成 WebP 编码)
image = { version = "0.25", default-features = false, features = [
    "png",
    "jpeg",
    "webp",  # WebP 支持
] }
```

### 数据库

```toml
# SQLite
rusqlite = { version = "0.31", features = [
    "bundled",       # 内置 SQLite
    "vtab",          # 虚拟表支持 (FTS5)
    "functions",     # 自定义函数
] }

# 向量搜索: 通过 BLOB 实现向量存储和 RRF 混合搜索
# (使用 bincode 序列化向量，存储在 traces.embedding)
```

### AI 推理

```toml
# VLM 引擎 (OpenAI 兼容 API)
reqwest = { version = "0.12", features = ["json"] }  # HTTP 客户端
base64 = "0.22"                                        # 图片编码

# 文本嵌入 (fastembed)
fastembed = "4"
```

### 工具库

```toml
# 错误处理
anyhow = "1"
thiserror = "1"

# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 时间处理
chrono = { version = "0.4", features = ["serde"] }

# 路径处理
directories = "5"  # 跨平台目录

# HTTP 客户端 (LLM API)
reqwest = { version = "0.12", features = ["json"] }

# UUID
uuid = { version = "1", features = ["v4", "serde"] }

# 正则表达式
regex = "1"
```

### 可选依赖

```toml
[dependencies]
# MCP 协议 (Phase 4)
mcp-sdk = { version = "0.1", optional = true }

# WASM 插件运行时 (Phase 4)
wasmtime = { version = "19", optional = true }

# 数据库加密 (Phase 4)
# 需要替换 rusqlite 为 rusqlite-sqlcipher

[features]
default = []
mcp = ["mcp-sdk"]
plugins = ["wasmtime"]
```

### 构建依赖

```toml
[build-dependencies]
tauri-build = "2"
```

---

## 前端依赖 (package.json)

### 核心框架

```json
{
  "dependencies": {
    "solid-js": "^1.8.0",
    "@solidjs/router": "^0.14.0",
    "@tauri-apps/api": "^2.0.0",
    "@tauri-apps/plugin-shell": "^2.0.0"
  }
}
```

### UI 组件

```json
{
  "dependencies": {
    "@kobalte/core": "^0.13.0",
    "solid-icons": "^1.1.0",
    "@solid-primitives/storage": "^3.0.0",
    "@solid-primitives/keyboard": "^1.2.0"
  }
}
```

### 样式

```json
{
  "dependencies": {
    "tailwindcss": "^3.4.0",
    "@tailwindcss/typography": "^0.5.0",
    "clsx": "^2.1.0"
  }
}
```

### 工具库

```json
{
  "dependencies": {
    "date-fns": "^3.0.0",
    "fuse.js": "^7.0.0"
  }
}
```

### 开发依赖

```json
{
  "devDependencies": {
    "vite": "^5.0.0",
    "vite-plugin-solid": "^2.10.0",
    "typescript": "^5.3.0",
    "postcss": "^8.4.0",
    "autoprefixer": "^10.4.0",
    "@types/node": "^20.0.0"
  }
}
```

---

## AI 模型清单

### 必需模型

| 模型 | 用途 | 格式 | 后端 | 下载链接 |
|------|-----|------|------|---------|
| Qwen3-VL-4B | 屏幕理解 + OCR | OpenAI 兼容 API | Ollama、vLLM、LM Studio | [Ollama Hub](https://ollama.com/library/qwen3-vl) |
| all-MiniLM-L6-v2 | 文本嵌入 | ONNX | 本地推理 (fastembed 自动下载) | [HuggingFace](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) |

### 可选模型 (高级用户)

| 模型 | 用途 | 格式 | 后端 | 下载链接 |
|------|-----|------|------|---------|
| GPT-4V | 屏幕理解 (高精度) | OpenAI API | OpenAI | [OpenAI Docs](https://platform.openai.com/docs) |
| Qwen3-VL-8B | 屏幕理解 (高精度) | OpenAI 兼容 API | vLLM、LM Studio | [HuggingFace](https://huggingface.co/Qwen) |
| CLIP-ViT-B-32 | 视觉嵌入 | ONNX | 本地推理 | [HuggingFace](https://huggingface.co/openai/clip-vit-base-patch32) |
| DeBERTa-v3-xsmall-NLI | 语义分类 | ONNX | 本地推理 | [HuggingFace](https://huggingface.co/cross-encoder) |

### VLM 服务器配置

**Ollama (推荐 - 最简单)**

```bash
# 安装
brew install ollama  # macOS
# 或下载：https://ollama.com/download

# 运行
ollama serve

# 在另一个终端拉取模型
ollama pull qwen3-vl:4b

# 验证服务
curl http://localhost:11434/v1/models
```

端点: `http://127.0.0.1:11434/v1`

**vLLM (高性能)**

```bash
# 安装
pip install vllm

# 运行
python -m vllm.entrypoints.openai.api_server \
  --model Qwen/Qwen3-VL-4B-Instruct \
  --trust-remote-code \
  --host 0.0.0.0 \
  --port 8000
```

端点: `http://127.0.0.1:8000/v1`

**LM Studio (GUI)**

```
1. 下载：https://lmstudio.ai
2. 打开应用
3. 在 "Models" 中搜索并下载 Qwen3-VL
4. 点击 "Local Server" → "Start Server"
5. 保持服务器运行
```

端点: `http://127.0.0.1:1234/v1`

**OpenAI (云端)**

```rust
let config = VlmConfig::openai("sk-...", "gpt-4v");
```

端点: `https://api.openai.com/v1`

### 模型目录结构（可选，仅用于参考）

```
~/.engram/
├── models/
│   ├── vlm/
│   │   ├── qwen3-vl-4b-q4_k_m.gguf  # 2.5GB (低端设备)
│   │   └── qwen3-vl-4b-q8_0.gguf    # 4.28GB (高精度)
│   ├── embedding/
│   │   └── (fastembed 自动管理)
│   ├── clip/
│   │   └── clip-vit-b-32.onnx       # 可选
│   └── nli/
│       └── deberta-v3-xsmall-nli.onnx  # 可选
└── screenshots/
    └── 2024/12/14/  # 截图目录
```

### 快速诊断

```bash
# 检查 Ollama 是否运行
curl http://localhost:11434/v1/models

# 检查 vLLM 是否运行
curl http://localhost:8000/v1/models

# 检查 LM Studio 是否运行
curl http://localhost:1234/v1/models
```

---

## 系统依赖

### Windows

| 依赖 | 版本 | 安装方式 |
|------|-----|---------|
| Visual C++ Redistributable | 2019+ | [下载](https://aka.ms/vs/17/release/vc_redist.x64.exe) |
| WebView2 Runtime | 最新 | [下载](https://developer.microsoft.com/microsoft-edge/webview2/) |

### macOS

| 依赖 | 版本 | 安装方式 |
|------|-----|---------|
| Xcode Command Line Tools | 最新 | `xcode-select --install` |

### Linux (Ubuntu/Debian)

```bash
# Tauri 运行时依赖
sudo apt install -y \
    libwebkit2gtk-4.1-0 \
    libayatana-appindicator3-1 \
    librsvg2-common

# 屏幕捕获依赖
sudo apt install -y \
    libpipewire-0.3-0 \
    xdg-desktop-portal \
    xdg-desktop-portal-gtk  # 或 xdg-desktop-portal-wlr (Wayland)
```

---

## 版本兼容性矩阵

| 组件 | 最低版本 | 推荐版本 |
|------|---------|---------|
| Rust | 1.75.0 | 1.82.0 |
| Node.js | 18.0.0 | 20.x LTS |
| Tauri CLI | 2.0.0 | 2.x |
| ONNX Runtime | 1.16.0 | 1.19.0 |
| SQLite | 3.40.0 | 3.45.0 |

## 磁盘空间需求

| 组件 | 大小 |
|------|-----|
| 应用程序 (安装后) | ~50MB |
| 必需 AI 模型 | ~100MB |
| 可选 AI 模型 | ~5GB |
| 数据存储 (日均) | ~500MB |
| **总计 (推荐)** | **20GB+** |
