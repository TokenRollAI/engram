# AI 管道设计

## 管道概览 (Phase 2.1 架构 - OpenAI 兼容 API)

```
┌──────────────────────────────────────────────────────────────────────────┐
│              Engram AI Pipeline - VLM 架构（OpenAI 兼容 API）              │
└──────────────────────────────────────────────────────────────────────────┘

              输入                      处理                        输出
         ┌──────────┐            ┌──────────────┐        ┌──────────────────┐
   图像 ─►│  OpenAI  │───────────►│   Qwen3-VL   │───────►│ ScreenDescription│
         │  兼容    │            │   (或其他)    │        │ {summary,        │
         │  API     │            │   VLM 模型   │        │  text_content,   │
         │          │            │ (通过HTTP)   │        │  detected_app,   │
         │  后端:   │            └──────────────┘        │  activity_type,  │
         │ Ollama   │                   │                 │  entities,       │
         │ vLLM     │                   ▼                 │  confidence}     │
         │ LM       │            ┌──────────────┐        └──────────────────┘
         │ Studio   │───────────►│   MiniLM     │        ┌──────────────────┐
   文本 ─►│ OpenAI   │            │   L6-v2      │───────►│  文本向量        │
         │ Together │            │ (嵌入)       │        │  (384d)          │
         │ AI 等    │            └──────────────┘        └──────────────────┘
         │          │                   │
         └──────────┘                   ▼
                                   [向量搜索]
                                        ▼
                                ┌──────────────────┐
                                │ 语义相关性排序    │
                                └──────────────────┘
```

## 核心流程变更 (PaddleOCR → VLM)

### 移除的组件
- **PaddleOCR** (ONNX): 多步骤的文本检测和识别流程
- **ONNX Runtime** (`ort` crate): 不再需要本地推理框架
- **ndarray** crate: 张量操作库
- **llama-server sidecar**: 不再捆绑，改为配置外部 OpenAI 兼容 API

### 新增的组件
- **VLM 支持**: 通过 OpenAI 兼容 API 调用任何 VLM 模型
  - 支持的后端: Ollama、vLLM、LM Studio、OpenAI、Together AI、OpenRouter 等
  - 模型示例: Qwen3-VL-4B、GPT-4V、Claude Vision 等
- **VlmEngine**: Rust 模块，管理 OpenAI 兼容 API 通信
  - 文件: `src-tauri/src/ai/vlm.rs` (~400 行)
  - 核心结构: `VlmEngine`, `VlmConfig`, `ScreenDescription`
  - HTTP 客户端: reqwest 0.12
  - 配置预设: `VlmConfig::ollama()`, `VlmConfig::openai()`, `VlmConfig::custom()`

## 模型清单

| 模型 | 用途 | 支持 | 后端示例 | 状态 |
|------|-----|------|---------|------|
| Qwen3-VL-4B | 屏幕理解 + OCR | OpenAI 兼容 API | Ollama、vLLM、LM Studio | ✅ 已集成 |
| GPT-4V | 屏幕理解（高精度） | OpenAI API | OpenAI | ✅ 已集成 |
| Claude Vision | 屏幕理解 | 未来支持 | Anthropic | 📋 计划 |
| all-MiniLM-L6-v2 | 文本嵌入 | ONNX | 本地推理 | ✅ 已集成 |
| CLIP-ViT-B-32 | 视觉嵌入 (可选) | ONNX | 本地推理 | 📋 可选 |
| DeBERTa-v3-xsmall-NLI | 零样本分类 | ONNX | 本地推理 | 📋 待集成 |

## VLM 管道详细设计

### 数据流

```
截图 (JPEG)
    ↓
[Base64 编码]
    ↓
OpenAI 兼容 API POST /chat/completions
    ├─ model: 配置的模型名称
    ├─ messages: [{ role: "user", content: [{ type: "image_url", ... }] }]
    ├─ max_tokens: 512 (可配置)
    └─ temperature: 0.3 (可配置)
    ↓
[JSON 响应解析]
    ↓
ScreenDescription {
    summary: String,           // 屏幕活动总结
    text_content: Option<String>,  // 提取的所有文本
    detected_app: Option<String>,  // 检测到的应用名称
    activity_type: Option<String>, // 活动类别 (coding/browsing/etc)
    entities: Vec<String>,     // 提取的实体 (项目名/文件/URL)
    confidence: f32,           // 置信度 (0.0-1.0)
}
    ↓
[MiniLM 嵌入]
    ↓
text_embedding (384d)
    ↓
[存储到 SQLite]
```

### VlmConfig 配置

```rust
pub struct VlmConfig {
    /// API 端点 (如 http://localhost:11434/v1)
    pub endpoint: String,
    /// 模型名称 (如 qwen3-vl:4b)
    pub model: String,
    /// API 密钥 (远程服务需要)
    pub api_key: Option<String>,
    /// 最大输出 tokens (默认 512)
    pub max_tokens: u32,
    /// 温度参数 (默认 0.3)
    pub temperature: f32,
}

// 便利预设
let ollama_config = VlmConfig::ollama("qwen3-vl:4b");
let openai_config = VlmConfig::openai("sk-...", "gpt-4v");
let custom_config = VlmConfig::custom("http://...", "model", Some("key"));
```

### VlmEngine 核心接口

```rust
pub struct VlmEngine {
    config: VlmConfig,
    client: reqwest::Client,
    is_ready: bool,
}

impl VlmEngine {
    // 创建新引擎
    pub fn new(config: VlmConfig) -> Self;

    // 初始化（验证连接）
    pub async fn initialize(&mut self) -> Result<()>;

    // 自动检测可用的本地服务
    pub async fn auto_detect() -> Result<Self>;

    // 检查是否就绪
    pub fn is_running(&self) -> bool;

    // 分析屏幕截图
    pub async fn analyze_screen(&self, image: &RgbImage) -> Result<ScreenDescription>;

    // 获取用于嵌入的文本
    pub fn get_text_for_embedding(desc: &ScreenDescription) -> String;

    // 获取后端名称
    pub fn backend_name(&self) -> String;
}

#[derive(Deserialize, Serialize)]
pub struct ScreenDescription {
    pub summary: String,
    pub text_content: Option<String>,
    pub detected_app: Option<String>,
    pub activity_type: Option<String>,
    pub entities: Vec<String>,
    pub confidence: f32,
}
```

### 支持的后端

| 后端 | 端点示例 | 安装方式 | 模型支持 |
|------|---------|---------|---------|
| **Ollama** | http://localhost:11434/v1 | [ollama.com](https://ollama.com/download) | Qwen3-VL、Llama、Mistral 等 |
| **vLLM** | http://localhost:8000/v1 | `pip install vllm` | 所有 HuggingFace 模型 |
| **LM Studio** | http://localhost:1234/v1 | [lmstudio.ai](https://lmstudio.ai/) | 本地 GGUF 模型 |
| **OpenAI** | https://api.openai.com/v1 | API Key | GPT-4V、GPT-4o |
| **Together AI** | https://api.together.xyz/v1 | API Key | Qwen、Llama、Mistral 等 |
| **OpenRouter** | https://openrouter.ai/api/v1 | API Key | 300+ 模型聚合 |

### 快速开始示例

```rust
use engram_lib::ai::vlm::{VlmEngine, VlmConfig};

// 方式 1: 自动检测本地服务
let mut engine = VlmEngine::auto_detect().await?;
engine.initialize().await?;

// 方式 2: 指定 Ollama 配置
let mut engine = VlmEngine::new(VlmConfig::ollama("qwen3-vl:4b"));
engine.initialize().await?;

// 方式 3: 使用 OpenAI
let mut engine = VlmEngine::new(
    VlmConfig::openai("sk-...", "gpt-4v")
);
engine.initialize().await?;

// 分析截图
let desc = engine.analyze_screen(&image).await?;
println!("{}", desc.summary);
println!("App: {:?}", desc.detected_app);
println!("Confidence: {}", desc.confidence);
```

## 新增依赖

```toml
[dependencies]
# HTTP 客户端
reqwest = { version = "0.12", features = ["json"] }

# 图片编码 (Base64)
base64 = "0.22"
```

## 移除的依赖

```toml
# 已移除（不再需要）
# ort = "2.0.0-rc.9"        # ONNX Runtime
# ndarray = "0.16"          # 张量操作
# tokenizers = "0.19"       # OCR 后处理
```

## 架构优势

### 1. 灵活性
- 支持任何 OpenAI 兼容 API
- 无需捆绑推理服务器
- 用户可选择本地或云端服务

### 2. 简化管道
**之前** (PaddleOCR → 嵌入 → 搜索):
```
截图 → PP-OCRv4-det (300ms) → 文本框 → PP-OCRv4-rec (200ms) → 文本 → MiniLM (100ms) → 向量
```

**现在** (VLM → 嵌入 → 搜索):
```
截图 → VLM (2-10s) → 结构化描述 + 文本 → MiniLM (100ms) → 向量
```

### 3. 更智能
- VLM 不仅提取文本，还能理解上下文
- 自动检测应用和活动类型
- 提取语义相关的实体和置信度

### 4. 开放生态
- 支持本地开源模型（成本低、隐私好）
- 支持云端模型（精度高、响应快）
- 自动检测本地服务，开箱即用

## 当前实现状态 (Phase 2.1 - 架构升级完成)

- **文件**: `src-tauri/src/ai/vlm.rs` (~400 行)
- **核心结构**:
  - `VlmEngine` - OpenAI 兼容 API 引擎
  - `VlmConfig` - 灵活的配置系统
  - `ScreenDescription` - 结构化屏幕描述

- **关键方法**:
  - `new(config)` - 初始化 VLM 引擎
  - `auto_detect()` - 自动检测可用服务
  - `initialize()` - 验证连接
  - `analyze_screen(image)` - 执行屏幕理解
  - `get_text_for_embedding()` - 获取嵌入文本

- **支持特性**:
  - 多后端支持（本地 + 云端）
  - API 密钥管理
  - 图片缩放优化
  - JSON 响应自动解析
  - 置信度评分

---

## 嵌入管道设计

### 双后端架构

嵌入模块支持两种后端，优先使用 OpenAI 兼容 API，无配置或连接失败时回退到本地模型：

```
配置检查
    ↓
┌─────────────────────────────────────────────┐
│ endpoint 已配置?                              │
│   ├─ Yes → 尝试 OpenAI 兼容 API              │
│   │         ├─ 成功 → 使用 API 嵌入          │
│   │         └─ 失败 → 回退到本地             │
│   └─ No  → 使用本地 fastembed               │
└─────────────────────────────────────────────┘
```

### EmbeddingConfig 配置

```rust
pub struct EmbeddingConfig {
    /// API 端点（None = 使用本地）
    pub endpoint: Option<String>,
    /// 模型名称
    pub model: String,
    /// API 密钥
    pub api_key: Option<String>,
}

// 预设配置
let local = EmbeddingConfig::local();                    // 本地 MiniLM
let openai = EmbeddingConfig::openai("sk-...");          // OpenAI API
let ollama = EmbeddingConfig::ollama("nomic-embed-text"); // Ollama
let custom = EmbeddingConfig::custom(endpoint, model, api_key);
```

### TextEmbedder 核心接口

```rust
pub struct TextEmbedder {
    config: EmbeddingConfig,
    backend: EmbeddingBackend,  // OpenAiCompatible | Local
    client: reqwest::Client,
    local_model: Option<fastembed::TextEmbedding>,
}

impl TextEmbedder {
    // 创建嵌入器
    pub fn new() -> Self;                              // 默认本地
    pub fn with_config(config: EmbeddingConfig) -> Self;

    // 初始化（API 失败自动回退到本地）
    pub async fn initialize(&mut self) -> Result<()>;

    // 嵌入文本
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    // 同步版本（仅本地模式）
    pub fn embed_sync(&self, text: &str) -> Result<Vec<f32>>;

    // 辅助方法
    pub fn backend_name(&self) -> String;
    pub fn embedding_dim(&self) -> usize;
}
```

### 支持的嵌入模型

| 后端 | 模型 | 维度 | 特点 |
|------|-----|------|------|
| **本地** | all-MiniLM-L6-v2 | 384 | 离线可用，快速 |
| **OpenAI** | text-embedding-3-small | 1536 | 高质量，需 API Key |
| **OpenAI** | text-embedding-3-large | 3072 | 最高质量 |
| **Ollama** | nomic-embed-text | 768 | 本地服务，免费 |
| **Ollama** | mxbai-embed-large | 1024 | 本地高质量 |

### 快速开始示例

```rust
use engram_lib::ai::embedding::{TextEmbedder, EmbeddingConfig};

// 方式 1: 本地模式（默认）
let mut embedder = TextEmbedder::new();
embedder.initialize().await?;

// 方式 2: OpenAI API
let mut embedder = TextEmbedder::with_config(
    EmbeddingConfig::openai("sk-...")
);
embedder.initialize().await?;  // 失败会自动回退到本地

// 方式 3: Ollama
let mut embedder = TextEmbedder::with_config(
    EmbeddingConfig::ollama("nomic-embed-text")
);
embedder.initialize().await?;

// 嵌入文本
let vec = embedder.embed("hello world").await?;
let vecs = embedder.embed_batch(&texts).await?;

println!("Backend: {}", embedder.backend_name());
println!("Dimension: {}", embedder.embedding_dim());
```

### 当前实现状态 (Phase 2 M2.2 完成)

- **文件**: `src-tauri/src/ai/embedding.rs` (~500 行)
- **核心结构**:
  - `TextEmbedder` - 双后端文本嵌入器
  - `EmbeddingConfig` - 灵活配置系统
  - `EmbeddingQueue` - 批处理队列

- **关键特性**:
  - OpenAI 兼容 API 支持
  - 自动回退到本地模型
  - 异步和同步 API
  - 批量嵌入优化

---

## 向量搜索与混合搜索

### 向量存储设计

```sql
-- traces 表新增字段 (M2.2.2 完成)
ALTER TABLE traces ADD COLUMN embedding BLOB;  -- 向量以 BLOB 形式存储

-- 向量格式: 使用 bincode 序列化为二进制
-- Vec<f32> -> bincode 编码 -> BLOB
```

### 向量搜索实现 (M2.2.2)

参见 `llmdoc/architecture/data-flow.md` 中的向量搜索部分。

### 混合搜索 - RRF 融合 (M2.2.3 完成)

结合全文搜索 (FTS5) 和向量搜索，使用 RRF (Reciprocal Rank Fusion) 融合算法进行结果排序。

---

## 性能优化策略

### 1. 自动检测 + 默认配置

```rust
// 自动检测本地服务，开箱即用
let mut engine = VlmEngine::auto_detect().await.expect(
    "No local VLM service detected.\n\
     Please install Ollama: https://ollama.com/download"
);
engine.initialize().await?;
```

### 2. 批处理

```rust
// 每 10 帧进行一次批量嵌入
const EMBEDDING_BATCH_SIZE: usize = 10;

impl EmbeddingQueue {
    fn enqueue(&mut self, text: String, callback: Callback) {
        self.queue.push((text, callback));

        if self.queue.len() >= EMBEDDING_BATCH_SIZE {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let texts: Vec<_> = self.queue.iter().map(|(t, _)| t.clone()).collect();
        let embeddings = self.embedder.embed_batch(&texts).unwrap();

        for ((_, callback), emb) in self.queue.drain(..).zip(embeddings) {
            callback(emb);
        }
    }
}
```

### 3. 硬件适配

| 硬件配置 | VLM 选择 | 嵌入精度 | 推荐用途 |
|---------|---------|---------|---------|
| 高端 (16GB+, GPU) | GPT-4V 或 QwenVL-8B | FP32 | 高精度、实时处理 |
| 中端 (8-16GB) | Qwen3-VL-4B (Ollama) | FP32 | 平衡性能和质量 |
| 低端 (<8GB) | Qwen3-VL-4B Q2_K 量化 | FP16 | 有限资源下可用 |

### 4. 缓存策略

```rust
// 图片哈希缓存，避免重复分析
struct VlmCache {
    lru: LRUCache<ImageHash, ScreenDescription>,
    max_size: usize,
}

impl VlmCache {
    fn get_or_analyze(&mut self, image: &RgbImage, engine: &VlmEngine) -> Result<ScreenDescription> {
        let hash = hash_image(image);
        if let Some(cached) = self.lru.get(&hash) {
            return Ok(cached.clone());
        }

        let desc = engine.analyze_screen(image).await?;
        self.lru.insert(hash, desc.clone());
        Ok(desc)
    }
}
```
