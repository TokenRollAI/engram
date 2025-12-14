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
scap = "0.1"

# 窗口信息
active-win-pos-rs = "0.8"

# 闲置检测
user-idle-time = "0.1"
```

### 图像处理

```toml
# 图像操作
image = { version = "0.25", features = ["webp"] }

# WebP 编码
webp = "0.3"

# 感知哈希
image_hasher = "1.2"  # 或 blockhash
```

### 数据库

```toml
# SQLite
rusqlite = { version = "0.31", features = [
    "bundled",       # 内置 SQLite
    "vtab",          # 虚拟表支持 (FTS5)
    "functions",     # 自定义函数
] }

# 向量搜索扩展
sqlite-vec = "0.1"  # 需确认实际 crate 名
```

### AI 推理

```toml
# ONNX Runtime (AI 模型推理)
ort = { version = "2", features = [
    "load-dynamic",  # 动态加载运行时
] }

# 文本嵌入
fastembed = "4"

# Tokenizer (OCR 后处理)
tokenizers = "0.19"
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

| 模型 | 用途 | 格式 | 大小 | 下载链接 |
|------|-----|------|------|---------|
| PP-OCRv4 Det | 文本检测 | ONNX INT8 | 4MB | [HuggingFace](https://huggingface.co/ppocrv4) |
| PP-OCRv4 Rec | 文本识别 | ONNX INT8 | 10MB | [HuggingFace](https://huggingface.co/ppocrv4) |
| all-MiniLM-L6-v2 | 文本嵌入 | ONNX | 80MB | fastembed 自动下载 |

### 可选模型

| 模型 | 用途 | 格式 | 大小 | 下载链接 |
|------|-----|------|------|---------|
| CLIP-ViT-B-32 | 视觉嵌入 | ONNX | 350MB | [HuggingFace](https://huggingface.co/openai/clip-vit-base-patch32) |
| DeBERTa-v3-xsmall-NLI | 语义黑名单 | ONNX | 70MB | [HuggingFace](https://huggingface.co/cross-encoder) |
| Qwen-2.5-7B-Instruct | 摘要生成 | GGUF Q4_K_M | 4.5GB | [HuggingFace](https://huggingface.co/Qwen) |
| Llama-3.2-3B-Instruct | 摘要 (低配) | GGUF Q4_K_M | 2GB | [HuggingFace](https://huggingface.co/meta-llama) |

### 模型目录结构

```
~/.engram/models/
├── ocr/
│   ├── ppocr_det_v4.onnx
│   ├── ppocr_rec_v4.onnx
│   └── ppocr_keys_v1.txt  # 字符字典
├── embedding/
│   └── (fastembed 自动管理)
├── clip/
│   └── clip-vit-b-32.onnx
├── nli/
│   └── deberta-v3-xsmall-nli.onnx
└── llm/
    └── qwen2.5-7b-instruct-q4_k_m.gguf
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
