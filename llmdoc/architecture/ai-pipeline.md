# AI 管道设计

## 管道概览 (Phase 3.2 - 完整 AI 管道：VLM 分析 + Summary + Chat)

```
┌──────────────────────────────────────────────────────────────────────────┐
│       Engram AI Pipeline - VLM 后台自动处理与内存合成（M3.2 完整）      │
└──────────────────────────────────────────────────────────────────────────┘

[前台捕获]              [后台分析]           [周期合成]        [交互查询]
  |                       |                    |                  |
  v                       v                    v                  v
  捕获截图          VlmTask 定时扫描    SummarizerTask      Chat 页面
  保存图片文件    (ocr_text IS NULL)     (15分钟一次)      (用户交互)
  (ocr_text=NULL)    |                      |                  |
  |                  v                      v                  v
  |            批量加载截图              查询最近 traces    获取时间范围
  |                |                       |                 应用列表
  |                v                       v                  |
  |        ┌─────────────────┐     ┌──────────────┐         |
  |        │   OpenAI        │     │  LLM (摘要)   │         |
  |        │   兼容 API      │     │              │         |
  |        │ (Ollama/vLLM)   │     └──────┬───────┘         |
  |        └────────┬────────┘            |                 |
  |                 |                     v                 |
  |        ScreenDescription          Summaries            |
  |        {summary, text,            表更新              |
  |         detected_app,             (content+vector)    |
  |         activity_type,                                |
  |         entities,                                     |
  |         confidence}                                   |
  |                 |                                     |
  |                 v                                     |
  |        ┌─────────────────┐                           |
  |        │   MiniLM-L6     │                           |
  |        │   (嵌入模型)     │                           |
  |        └────────┬────────┘                           |
  |                 |                                     |
  |                 v                                     v
  |        文本向量 (384d)              ┌─────────────────────┐
  |                 |                   │ VlmEngine::chat()   │
  |                 |                   │ (纯文本对话)        │
  |                 |                   └────────┬────────────┘
  |                 |                            |
  └─────────┬───────┘                            |
            |                                    |
            v                                    v
    [数据库更新]                        ChatResponse
    - traces 表                       {message, sources,
    - traces_vec 虚拟表                references}
    (KNN 向量搜索)
```

## 三大核心任务

### 1. VlmTask - 屏幕分析 (M3.1 实现)
- **周期**: 每 10 秒扫描一次
- **输入**: 待分析的 traces (ocr_text IS NULL)
- **处理**: VLM 视觉理解 → 文本提取 → 向量生成
- **输出**: OCR 数据 + 嵌入向量 → 数据库

### 2. SummarizerTask - 周期摘要 (M3.2 新增)
- **周期**: 每 15 分钟生成一次
- **输入**: 最近的 traces 集合
- **处理**: LLM 摘要 → 结构化提取 → 向量化
- **输出**: Summaries 表 (content + embedding)

### 3. Chat - 交互查询 (M3.2 新增)
- **触发**: 用户输入查询
- **输入**: 时间范围、应用过滤、查询文本
- **处理**: 向量检索上下文 → VLM 文本对话
- **输出**: AI 回复 + 引用来源

---

**阶段 1 (M2.1)**: OCR → VLM 替换
- 移除 PaddleOCR 多步骤流程
- 引入 VLM OpenAI 兼容 API 支持

**阶段 2 (M2.5)**: 用户可配置
- 添加 AI 配置界面
- 支持多个后端选择

**阶段 3 (M3.1)**: 后台自动处理 (新增)
- 创建 VlmTask 后台任务
- 自动处理待分析的 traces
- 无需前端干预

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
- **VlmTask 后台任务** (M3.1 新增): 自动处理待分析的 traces
  - 文件: `src-tauri/src/daemon/vlm_task.rs` (~290 行)
  - 核心结构: `VlmTask`, `VlmTaskConfig`, `VlmTaskStatus`
  - 特性: 定时扫描、批处理、异步处理、可配置

## VlmTask 后台处理架构 (M3.1)

### 设计目标

1. **自动化** - 无需前端干预，自动处理待分析的 traces
2. **异步非阻塞** - 不影响前台捕获性能
3. **批处理** - 支持批量处理以优化吞吐量
4. **可配置** - 处理间隔和批处理大小可调整
5. **容错** - 单条失败不影响其他 traces

### VlmTask 结构体

```rust
pub struct VlmTask {
    db: Arc<Database>,
    vlm: Arc<RwLock<Option<VlmEngine>>>,
    embedder: Arc<RwLock<TextEmbedder>>,
    config: VlmTaskConfig,
    is_running: Arc<AtomicBool>,
    processed_count: Arc<AtomicU64>,
    failed_count: Arc<AtomicU64>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

pub struct VlmTaskConfig {
    pub interval_ms: u64,      // 处理间隔（默认 10000ms）
    pub batch_size: u32,       // 批处理大小（默认 5）
    pub enabled: bool,         // 是否启用（默认 true）
}

pub struct VlmTaskStatus {
    pub is_running: bool,           // 任务是否在运行
    pub processed_count: u64,       // 成功处理的 traces 数量
    pub failed_count: u64,          // 处理失败的 traces 数量
    pub pending_count: u64,         // 待处理的 traces 数量
}
```

### 执行流程

```
┌─────────────────────────────────┐
│  VlmTask::start()               │
│  (在 AppState 初始化时调用)      │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│  创建 Tokio 异步任务             │
│  设置定时器 (interval_ms)        │
└────────────┬────────────────────┘
             │
             ▼ (每个周期)
┌─────────────────────────────────┐
│  检查 VLM 是否就绪               │
│  (is_ready == true)             │
└────────────┬────────────────────┘
             │
      No ────┤──────► 跳过本周期
             │
             Yes
             ▼
┌─────────────────────────────────┐
│  查询待分析的 traces             │
│  WHERE ocr_text IS NULL         │
│  LIMIT batch_size               │
└────────────┬────────────────────┘
             │
             ├──(无待分析)──► 跳过本周期
             │
             Yes
             ▼
┌─────────────────────────────────┐
│  for each trace in pending:     │
│    process_single_trace()       │
└────────────┬────────────────────┘
             │
             ├─► 加载截图文件
             ├─► 调用 VLM 分析
             ├─► 提取 ocr_text/ocr_json
             ├─► 更新数据库 OCR 数据
             ├─► 生成嵌入向量
             └─► 更新数据库 embedding
```

### 单条 Trace 处理流程

```
Input: Trace { id, image_path, ocr_text=NULL, ... }
  │
  ├─► 1. 加载截图文件
  │      path = db.get_full_path(&trace.image_path)
  │      image = image::open(path).to_rgb8()
  │
  ├─► 2. 调用 VLM 分析
  │      desc = vlm_engine.analyze_screen(&image).await?
  │      返回 ScreenDescription {
  │        summary: String,
  │        text_content: Option<String>,
  │        detected_app: Option<String>,
  │        activity_type: Option<String>,
  │        entities: Vec<String>,
  │        confidence: f32,
  │      }
  │
  ├─► 3. 提取 OCR 数据
  │      ocr_text = VlmEngine::get_text_for_embedding(&desc)
  │      ocr_json = serde_json::to_string(&desc)
  │
  ├─► 4. 更新数据库（OCR 数据）
  │      db.update_trace_ocr(trace.id, &ocr_text, &ocr_json)?
  │
  ├─► 5. 生成嵌入向量
  │      embedding = embedder.embed(&ocr_text).await?
  │      返回 Vec<f32> (384 维)
  │
  ├─► 6. 序列化嵌入向量
  │      embedding_bytes = embedding.iter()
  │        .flat_map(|f| f.to_le_bytes())
  │        .collect::<Vec<u8>>()
  │
  ├─► 7. 更新数据库（嵌入向量）
  │      db.update_trace_embedding(trace.id, &embedding_bytes)?
  │
  └─► Output: Success (processed_count++)
              或 Error (failed_count++)
```

### 与 AppState 集成

```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub daemon: Arc<RwLock<EngramDaemon>>,
    pub vlm: Arc<RwLock<Option<VlmEngine>>>,
    pub embedder: Arc<RwLock<TextEmbedder>>,
    pub vlm_task: Arc<RwLock<VlmTask>>,  // M3.1 新增
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        // ... 初始化其他部分 ...

        // 创建 VlmTask（默认启用）
        let vlm_task = Arc::new(RwLock::new(VlmTask::new(
            db.clone(),
            vlm.clone(),
            embedder.clone(),
            VlmTaskConfig::default(),
        )));

        // ... 尝试自动初始化 AI ...

        // 如果 VLM 初始化成功，启动后台任务
        if vlm_initialized {
            let mut task = state.vlm_task.write().await;
            task.start()?;
            info!("VLM background task started");
        }

        Ok(state)
    }
}
```

### 性能特性

| 特性 | 说明 |
|------|------|
| **处理间隔** | 默认 10 秒，可配置 |
| **批处理大小** | 默认 5，可配置 |
| **异步模式** | 使用 Tokio，不阻塞主线程 |
| **容错机制** | 单条失败记录，但继续处理其他 traces |
| **优雅关闭** | 支持 shutdown_tx 信号，安全停止任务 |
| **状态监控** | 通过 VlmTaskStatus 监控处理进度 |

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

---

## SummarizerTask - 周期摘要生成 (M3.2)

### 设计目标

1. **自动化摘要** - 无需用户手动触发，定期自动生成
2. **异步处理** - 不阻塞主程序和 VLM 分析
3. **内存聚合** - 将离散的 traces 摘要为连贯的记忆
4. **向量化** - 摘要本身也被向量化，支持语义搜索
5. **可配置** - 摘要生成间隔可调整

### SummarizerTask 结构体

```rust
pub struct SummarizerTask {
    db: Arc<Database>,
    embedder: Arc<RwLock<TextEmbedder>>,
    config: SummarizerConfig,
    is_running: Arc<AtomicBool>,
    generated_count: Arc<AtomicU64>,
}

pub struct SummarizerConfig {
    pub interval_ms: u64,      // 生成间隔（默认 900000ms = 15分钟）
    pub lookback_minutes: u64, // 回顾时间范围（默认 30 分钟）
    pub enabled: bool,
}

pub struct SummaryRecord {
    pub id: i32,
    pub start_time: i64,
    pub end_time: i64,
    pub summary_type: String,  // "15min", "1hour", "1day"
    pub content: String,       // Markdown 格式摘要
    pub structured_data: String, // JSON: {topics, entities, links}
    pub embedding: Vec<f32>,   // 384 维向量
    pub trace_count: i32,
}
```

### 执行流程

```
1. 启动后台任务
   └─ 创建 Tokio 异步任务，设置定时器

2. 每个周期 (15 分钟)
   ├─ 检查是否启用
   ├─ 查询最近 lookback_minutes 内的 traces
   ├─ 检查数量是否足够（至少 5 条）
   └─ 若足够，执行摘要生成

3. 摘要生成流程
   ├─ 构建上下文：提取 traces 的 ocr_text 和 app_name
   ├─ 调用 LLM 生成摘要（使用系统 prompt）
   ├─ 解析 LLM 响应（Markdown + JSON）
   ├─ 生成嵌入向量 (MiniLM)
   └─ 存储到 summaries 表

4. LLM Prompt 示例
   ┌───────────────────────────────────┐
   │ 请总结以下用户活动记录（30分钟）：  │
   │                                   │
   │ 时间轴：                          │
   │ - 14:00-14:10: VS Code 编程      │
   │ - 14:10-14:25: Chrome 浏览       │
   │ - 14:25-14:30: Slack 聊天        │
   │                                   │
   │ 请以以下 JSON 格式输出：          │
   │ {                                 │
   │   "summary": "用户花30分钟...",   │
   │   "topics": ["编程", "研究"],     │
   │   "entities": ["React", "API"],   │
   │   "sentiment": "专注"             │
   │ }                                 │
   └───────────────────────────────────┘
```

### AppState 集成

```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub daemon: Arc<RwLock<EngramDaemon>>,
    pub vlm: Arc<RwLock<Option<VlmEngine>>>,
    pub embedder: Arc<RwLock<TextEmbedder>>,
    pub vlm_task: Arc<RwLock<VlmTask>>,
    pub summarizer_task: Arc<RwLock<Option<SummarizerTask>>>,  // M3.2 新增
}

// 应用初始化时
impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        // ... 初始化其他部分 ...

        // 创建 SummarizerTask（默认启用）
        let summarizer_task = Arc::new(RwLock::new(
            SummarizerTask::new(
                db.clone(),
                embedder.clone(),
                SummarizerConfig::default(),
            )
        ));

        // AI 初始化成功后启动摘要任务
        if vlm_initialized && embedder_ready {
            let mut task = state.summarizer_task.write().await;
            task.start()?;
            info!("Summarizer task started");
        }

        Ok(state)
    }
}
```

### 性能特性

| 特性 | 说明 |
|------|------|
| **生成间隔** | 默认 15 分钟，可配置 |
| **回顾范围** | 默认 30 分钟 |
| **LLM 调用** | 支持本地 LLM 和云端 API |
| **向量化** | 自动使用 MiniLM 生成 384d 向量 |
| **数据持久化** | 全部存储到 SQLite，支持后续检索 |

---

## Chat - 基于记忆的交互对话 (M3.2)

### 设计目标

1. **上下文感知** - 基于用户活动历史进行对话
2. **灵活过滤** - 支持时间范围和应用类型过滤
3. **来源引用** - 显示回复的数据来源
4. **预设问题** - 提供常用查询模板
5. **向量加速** - 利用 sqlite-vec KNN 快速检索上下文

### Chat 数据流

```
用户输入查询 + 过滤条件
    ↓
[查询预处理]
  ├─ 解析时间范围 (今天/本周/本月/全部)
  ├─ 解析应用过滤 (多选)
  └─ 准备向量搜索
    ↓
[向量上下文检索] (M3.2 sqlite-vec 优化)
  ├─ 嵌入用户查询 (384d MiniLM 向量)
  ├─ 使用 sqlite-vec KNN 搜索相关 traces
  │  └─ WHERE embedding MATCH :query AND k = 20
  ├─ 过滤时间范围和应用
  └─ 返回前 5 条最相关的 traces
    ↓
[构建对话上下文]
  ├─ 汇总 traces 的 ocr_text
  ├─ 添加 summaries（如果存在）
  └─ 组织为 prompt 的上下文
    ↓
[VLM 文本对话]
  ├─ 调用 VlmEngine::chat(prompt)
  ├─ 返回 AI 回复文本
  └─ 记录引用的 trace IDs
    ↓
[前端展示]
  ├─ 显示 AI 回复
  ├─ 显示引用来源（点击跳转截图）
  └─ 消息历史保存
```

### 关键命令

**后端命令**:

```rust
// 获取可用应用列表
pub async fn get_available_apps(
    start_time: i64,
    end_time: i64,
) -> Result<Vec<String>, String>

// 基于记忆的对话
pub async fn chat_with_memory(
    query: String,
    time_range: TimeRange,      // Today/Week/Month/All
    app_filters: Vec<String>,   // 可选应用过滤
    max_context: usize,         // 最多返回 context 条数（默认 10）
) -> Result<ChatResponse, String>
```

**数据结构**:

```rust
pub struct ChatRequest {
    pub query: String,
    pub time_range: TimeRange,
    pub app_filters: Vec<String>,
    pub max_context: usize,
}

pub struct ChatResponse {
    pub id: String,                    // 对话 ID
    pub message: String,               // AI 回复文本
    pub sources: Vec<TraceReference>,  // 引用的 trace
    pub summaries_used: Vec<i32>,      // 使用的 summary IDs
    pub created_at: i64,
}

pub struct TraceReference {
    pub trace_id: i32,
    pub timestamp: i64,
    pub app_name: Option<String>,
    pub preview_text: String,  // ocr_text 的前 100 字
}
```

### 前端 Chat 页面

**路由**: `/chat`

**功能**:
1. **时间范围筛选** - 今天 / 本周 / 本月 / 全部时间
2. **应用多选过滤** - 从 `get_available_apps()` 动态获取
3. **预设问题** - 如：
   - "我今天做了什么?"
   - "花最多时间的是什么?"
   - "打开了哪些项目文件?"
   - "有哪些重要的会议?"

4. **消息历史** - 显示用户消息和 AI 回复
5. **来源引用** -
   - 显示引用的 trace 数量
   - 点击可跳转到对应时间的截图
   - 高亮引用文本

**UI 交互流**:
```
┌──────────────────────────────┐
│ 🔍 Chat - 与记忆对话        │
├──────────────────────────────┤
│ 时间范围: [今天 v]           │
│ 应用过滤: [+选择应用]        │
├──────────────────────────────┤
│ 快速问题:                    │
│ [我今天做了什么?]           │
│ [花最多时间的是什么?]       │
├──────────────────────────────┤
│ 消息历史:                    │
│ ┌────────────────────────┐  │
│ │ 你: 我今天的工作       │  │
│ │                        │  │
│ │ AI: 基于你的活动...   │  │
│ │ 📌 引用 5 条记录      │  │
│ │ [显示来源]             │  │
│ └────────────────────────┘  │
├──────────────────────────────┤
│ [输入框: 问我任何关于今天...] │
│ [发送]                       │
└──────────────────────────────┘
```

### sqlite-vec 集成

Chat 命令利用 sqlite-vec 的高效向量搜索：

```rust
// VLM 文本对话（不带图像）
impl VlmEngine {
    pub async fn chat(&self, prompt: &str) -> Result<String> {
        // 构建纯文本对话 prompt
        let messages = vec![
            {
                "role": "system",
                "content": "你是用户的个人助手，基于用户的活动记录回答问题。..."
            },
            {
                "role": "user",
                "content": prompt
            }
        ];

        // 调用 OpenAI 兼容 API
        let response = self.client
            .post(&format!("{}/v1/chat/completions", self.endpoint))
            .json(&request)
            .send()
            .await?;

        // 解析响应
        let result = response.json::<ChatCompletionResponse>().await?;
        Ok(result.choices[0].message.content.clone())
    }
}

// 数据库层：使用 sqlite-vec KNN 检索
impl Database {
    pub fn search_by_embedding(
        &self,
        query_vector: &[f32],
        k: i32,
        filters: SearchFilters,
    ) -> Result<Vec<Trace>> {
        // sqlite-vec KNN 查询
        let mut stmt = self.db.prepare(
            "SELECT t.* FROM traces_vec vec
             JOIN traces t ON vec.trace_id = t.id
             WHERE vec.embedding MATCH ?1 AND k = ?2
             ORDER BY distance LIMIT ?3"
        )?;

        let results = stmt.query_map(
            params![&query_vector, k, filters.limit],
            |row| Trace::from_row(row)
        )?;

        // 应用时间和应用过滤
        results
            .filter_map(|r| r.ok())
            .filter(|t| filters.matches(t))
            .collect()
    }
}
```

### 性能特性

| 特性 | 说明 |
|------|------|
| **检索速度** | sqlite-vec KNN 10-50ms（10000+ traces） |
| **上下文大小** | 默认 10 条 traces + summaries |
| **并发支持** | 多用户同时对话，异步处理 |
| **缓存策略** | 应用列表缓存，减少数据库查询 |
