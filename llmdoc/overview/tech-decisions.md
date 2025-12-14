# 技术选型决策

## 决策总览

| 领域 | 选型 | 备选方案 | 决策理由 |
|-----|------|---------|---------|
| 应用框架 | **Tauri v2** | Electron | 内存占用低 10 倍，Rust 后端性能强 |
| 后端语言 | **Rust** | Go, C++ | 内存安全 + 高性能 + Tauri 原生支持 |
| 前端框架 | **SolidJS** | React, Vue | 更小的 bundle + 更好的响应式性能 |
| 屏幕捕获 | **scap** | xcap, screenshots | 支持 GPU 加速的现代 API |
| OCR 引擎 | **PaddleOCR v4** | Tesseract | 中英文混排精度高，速度快 |
| AI 推理 | **ONNX Runtime** | Candle | 成熟的多后端支持 (CPU/GPU/NPU) |
| 向量模型 | **all-MiniLM-L6-v2** | bge-m3 | 轻量 (80MB) + 质量平衡 |
| 数据库 | **SQLite + sqlite-vec** | Qdrant | 单文件部署，无需服务进程 |
| LLM 推理 | **llama.cpp** | candle-llm | 量化模型支持完善，硬件兼容性广 |
| 默认 LLM | **Qwen 2.5 7B** | Llama 3.2 3B | 中文能力最强的开源 7B 模型 |

## 详细决策记录

### ADR-001: 应用框架选择 Tauri v2

**背景**: 需要跨平台桌面应用框架，支持后台常驻运行。

**决策**: 使用 Tauri v2 而非 Electron。

**理由**:
- **内存效率**: Electron 空闲占用 150-300MB，Tauri 仅 30-50MB
- **IPC 开销**: 每 2 秒传输图像数据，Electron 的序列化开销不可接受
- **Rust 后端**: 图像处理、AI 推理等计算密集任务需要系统级语言
- **安全模型**: Tauri v2 的 Capability-based 权限系统更适合隐私敏感应用

**后果**:
- 需要 Rust 开发能力
- 前端需适配 WebView (非 Chromium)
- 部分 npm 包可能不兼容

---

### ADR-002: 屏幕捕获库选择 scap

**背景**: 需要高频 (0.5Hz) 屏幕截图，尽量使用 GPU 加速。

**决策**: 使用 `scap` crate。

**理由**:
- **现代 API**: 封装 Windows Graphics Capture、macOS ScreenCaptureKit、Linux PipeWire
- **GPU 加速**: 直接访问帧缓冲区，CPU 占用极低
- **统一抽象**: 单一 API 覆盖三大平台

**备选分析**:
- `xcap`: 不支持 Linux PipeWire
- `screenshots`: 基于老旧的 GDI/Quartz API

---

### ADR-003: OCR 引擎选择 PaddleOCR

**背景**: 需要本地实时 OCR，支持中英文混排。

**决策**: 使用 PaddleOCR v4 通过 ONNX Runtime 部署。

**理由**:
- **精度**: PP-OCRv4 在中文场景显著优于 Tesseract
- **速度**: 量化后单帧 OCR < 200ms
- **体积**: 检测模型 4MB + 识别模型 10MB

**部署方案**:
```
PaddleOCR (Python 训练)
    ↓ paddle2onnx
ONNX 模型
    ↓ ort (Rust binding)
Rust 推理
```

**执行提供者配置**:
- Windows: DirectML (GPU) / OpenVINO (Intel CPU)
- macOS: CoreML (Neural Engine)
- Linux: CUDA / CPU (AVX512)

---

### ADR-004: 数据库选择 SQLite

**背景**: 需要存储时序数据 + 向量索引 + 全文检索。

**决策**: SQLite + sqlite-vec + FTS5。

**理由**:
- **单文件**: 用户数据可完整备份/迁移
- **无服务**: 不需要 Docker 或独立进程
- **成熟稳定**: SQLite 是地球上测试最充分的软件之一

**向量检索方案**:
- `sqlite-vec`: 纯 Rust 实现，支持 SIMD 加速
- 百万级数据暴力搜索 < 50ms
- 可选 IVFFlat 索引应对更大规模

**全文检索方案**:
- FTS5: SQLite 内置，支持中文分词 (需配置 tokenizer)

---

### ADR-005: LLM 部署选择 llama.cpp Sidecar

**背景**: 需要本地运行 7B 参数 LLM 生成摘要。

**决策**: 使用 llama.cpp 作为独立进程 (Sidecar)。

**理由**:
- **内存隔离**: 避免 LLM 内存与主进程竞争
- **灵活性**: 用户可自行更换 GGUF 模型
- **稳定性**: llama.cpp 是量化推理的事实标准

**架构**:
```
Engram (Tauri)
    ↓ 启动子进程
llama-server (监听 127.0.0.1:随机端口)
    ↑ HTTP API 调用
Engram (摘要生成模块)
```

**默认模型**: Qwen 2.5 7B Instruct (Q4_K_M, ~5GB 内存)

---

## 依赖 Crate 清单

```toml
# Cargo.toml (核心依赖)

[dependencies]
# 应用框架
tauri = { version = "2", features = ["tray-icon", "shell-open"] }

# 屏幕捕获
scap = "0.1"

# 系统信息
active-win-pos-rs = "0.8"
user-idle-time = "0.1"

# 图像处理
image = "0.25"
webp = "0.3"

# AI 推理
ort = { version = "2", features = ["load-dynamic"] }
fastembed = "4"

# 数据库
rusqlite = { version = "0.31", features = ["bundled", "vtab"] }
sqlite-vec = "0.1"

# 异步运行时
tokio = { version = "1", features = ["full"] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# MCP 协议
mcp-sdk = "0.1"  # 待确认实际包名
```

## 前端技术栈

```json
{
  "dependencies": {
    "solid-js": "^1.8",
    "@solidjs/router": "^0.14",
    "@tauri-apps/api": "^2"
  },
  "devDependencies": {
    "vite": "^5",
    "vite-plugin-solid": "^2",
    "typescript": "^5",
    "tailwindcss": "^3"
  }
}
```
