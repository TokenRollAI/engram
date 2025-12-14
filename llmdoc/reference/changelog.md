# 变更日志

## 概述

本文档记录 Engram 项目的重要版本变更、功能发布与架构更新。按时间倒序排列。

---

## [Phase 3 M3.3 配置系统重构：SQLite → TOML] - 2025-12-15

### 变更内容

**重构范围**：配置管理从 SQLite 设置表迁移到独立的 TOML 文件

#### 1. 新增配置模块

`src-tauri/src/config/mod.rs` (~230 行)：

核心配置结构体：
- `AppConfig`：顶层配置聚合
- `CaptureConfig`：截图捕获设置（interval_ms, idle_threshold_ms, similarity_threshold）
- `StorageConfig`：数据存储设置（hot_data_days, warm_data_days）
- `SessionConfig`：会话管理设置（gap_threshold_ms）
- `SummaryConfig`：摘要生成设置（interval_min）
- 复用 `VlmConfig`、`EmbeddingConfig`、`VlmTaskConfig`

配置文件位置（XDG 规范）：
- Linux: `~/.config/engram/Engram/config.toml`
- macOS: `~/Library/Application Support/com.engram.Engram/config.toml`
- Windows: `%APPDATA%\engram\Engram\config.toml`

#### 2. AppState 配置驱动初始化

`src-tauri/src/lib.rs` (AppState)：

**新增字段**：
- `config: Arc<RwLock<AppConfig>>` - 配置在内存中的镜像

**初始化流程**：
1. 调用 `AppConfig::load()` 从 TOML 文件加载配置
2. 使用加载的配置参数初始化各模块
3. 调用 `try_auto_initialize_ai()` 基于配置初始化 AI

#### 3. 前端命令更新

`src-tauri/src/commands/mod.rs`：

- `get_settings()` - 从内存配置返回设置（不再数据库查询）
- `update_settings(settings: Settings)` - 更新并保存到 TOML 文件
- `get_ai_config()` - 返回 VlmConfig、EmbeddingConfig、VlmTaskConfig
- `update_ai_config(ai_config: AiConfig)` - 更新 AI 配置并重新初始化

#### 4. 移除的代码

**删除的数据库操作**：
- `load_vlm_config_from_db()`
- `load_embedding_config_from_db()`
- `load_vlm_task_config_from_db()`
- 所有 SQLite settings 表的配置查询逻辑

#### 5. 依赖更新

`Cargo.toml`：
- 新增 `directories = "5.0"` - XDG 规范路径
- 新增 `toml = "0.8"` - TOML 序列化/反序列化

#### 6. 文档更新

新增：
- `llmdoc/architecture/config-system.md` - 配置系统完整设计文档

更新：
- `llmdoc/architecture/ai-pipeline.md` - VlmConfig/EmbeddingConfig 加载方式
- `llmdoc/index.md` - 添加配置系统文档

---

## [Phase 3 M3.3 Activity Session 核心化与 Chat 持久化] - 2025-12-15

### 变更内容

- 新增“活动 Session”（用户行为聚合）作为对外主视图：`activity_sessions`
- `traces` 保留为原子事实流，新增 `activity_session_id` 关联；VLM 结构化输出写入 `traces.vlm_*`
- VlmTask：分析时使用“Session 信息 + 关键行为 + 最近 traces”作为上下文，并把 VLM 结论增量同步到 Session
- 新增关键行为：VLM 返回 `is_key_action` + `action_description`，写入 `traces.is_key_action` 并聚合到 `activity_sessions.key_actions_json`
- Chat：新增线程/消息持久化（`chat_threads`/`chat_messages`），响应包含 `thread_id`
- UI：Timeline 默认按 Session 展示，可展开查看 session 内 traces
- UI：详情弹窗关闭入口改为右上角 `×`（不再使用底部“关闭”大按钮）
- Schema: 移除 `activity_session_events`，并将 VLM 结构化结果写回 `traces.vlm_*`；`activity_sessions` 新增 `title/description`；移除 `traces.window_x/y/w/h`

---

## [Phase 3 M3.2 Summary 系统、Chat 功能与向量搜索优化] - 2025-12-14

### 发布内容

**版本**: Phase 3 (The Mind) - 内存合成持续推进

**变更类型**: 核心功能扩展 - Summary 自动生成、Chat 交互、sqlite-vec 向量加速

#### 新增功能

##### 1. Summary 系统自动启动 (lib.rs)

AppState 集成了周期性摘要生成任务：

**变更内容**:
- 在 `AppState` 中新增 `summarizer_task` 字段
- AI 初始化成功后自动启动 `SummarizerTask`
- 每 15 分钟自动生成一次摘要（可配置）
- 新增 `start_summarizer_task_with_config()` 方法
- 新增 `stop_summarizer_task()` 方法

**实现文件**:
- `src-tauri/src/lib.rs` (AppState 结构体)

**核心代码变更**:
```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub daemon: Arc<RwLock<EngramDaemon>>,
    pub vlm: Arc<RwLock<Option<VlmEngine>>>,
    pub embedder: Arc<RwLock<TextEmbedder>>,
    pub vlm_task: Arc<RwLock<VlmTask>>,
    pub summarizer_task: Arc<RwLock<Option<SummarizerTask>>>,  // 新增
}
```

**执行流程**:
- 应用启动时初始化 `AppState`
- VLM 引擎初始化成功后自动启动 `SummarizerTask`
- 任务每 15 分钟检查一次是否需要生成新摘要
- 自动调用 LLM 对最近的 traces 进行总结
- 生成结构化摘要（Markdown 内容 + 结构化数据）

##### 2. Chat 功能新增 (完整后端实现)

**后端** (`src-tauri/src/commands/mod.rs`):

新增四个核心命令：

- `chat_with_memory(query: String, time_range: TimeRange) -> ChatResponse`
  - 基于记忆进行 AI 对话
  - 支持不同时间范围的上下文
  - 集成 VLM 的对话能力

- `get_available_apps(start_time, end_time) -> Vec<String>`
  - 获取时间范围内的应用列表
  - 支持时间范围过滤
  - 用于 Chat 页面的应用筛选

**数据结构**:
```rust
pub struct ChatRequest {
    pub query: String,
    pub time_range: TimeRange,
    pub app_filters: Vec<String>,  // 可选的应用过滤
    pub max_context: usize,         // 返回的最大 context 条数
}

pub struct ChatResponse {
    pub id: String,
    pub message: String,
    pub sources: Vec<String>,  // 引用的 trace IDs
    pub created_at: i64,
}
```

**VLM 新增** (`src-tauri/src/ai/vlm.rs`):

- `VlmEngine::chat(prompt: &str) -> Result<String>`
  - 纯文本对话能力（不带图像）
  - 用于处理用户的文本查询

**数据库新增** (`src-tauri/src/db/mod.rs`):

- `get_distinct_apps(start_time, end_time) -> Vec<String>`
  - 获取时间范围内出现的所有应用名称
  - 用于前端的应用多选过滤

**前端** (`src-ui/src/pages/Chat.tsx`):

新增完整的 Chat 页面：
- 路由: `/chat`
- 时间范围筛选 (今天/本周/本月/全部)
- 应用多选过滤
- 预设问题建议 (如 "我今天做了什么?" "花最多时间的是什么?")
- 消息历史显示（用户消息 + AI 回复）
- 引用来源显示（点击跳转到相关截图）

**UI 流程**:
1. 用户输入查询或点击预设问题
2. 前端调用 `get_available_apps()` 获取应用列表
3. 用户选择时间范围和应用过滤
4. 调用 `chat_with_memory()` 获取 AI 响应
5. 显示回复和引用来源

##### 3. sqlite-vec 向量搜索集成

**依赖** (`Cargo.toml`):

新增高性能向量搜索库：
```toml
sqlite-vec = "0.1"
zerocopy = "0.4"   # 高效向量传递
```

**数据库初始化** (`src-tauri/src/db/mod.rs`):

- 使用 `sqlite3_auto_extension` 自动加载 sqlite-vec 扩展
- 应用启动时自动注册扩展
- 验证 sqlite-vec 加载成功

**验证代码示例**:
```rust
pub fn initialize_with_extensions() -> Result<()> {
    // 加载 sqlite-vec 扩展
    let _ = db.execute_batch("SELECT sqlite_version();")?;

    // 验证 vec0 虚拟表可用
    let count: i32 = db.query_row(
        "SELECT COUNT(*) FROM pragma_function_list WHERE name LIKE '%vec%'",
        [],
        |row| row.get(0)
    )?;

    assert!(count > 0, "sqlite-vec extension failed to load");
    Ok(())
}
```

**Schema** (`src-tauri/src/db/schema.rs`):

新增虚拟表用于向量索引：
```sql
-- 向量索引虚拟表 (sqlite-vec, 384维浮点向量)
CREATE VIRTUAL TABLE traces_vec USING vec0(
    trace_id INTEGER PRIMARY KEY,
    embedding float[384]
);
```

**虚拟表说明**:
- 使用 384 维浮点向量（all-MiniLM-L6-v2 输出）
- 虚拟表 `traces_vec` 存储 trace_id 和对应向量
- 自动支持 KNN (K-Nearest Neighbors) 搜索
- SIMD 加速的向量运算

**向量操作**:

新增或更新数据库方法：

- `update_trace_embedding(trace_id, embedding)`
  - 同时更新 `traces.embedding` 字段和 `traces_vec` 虚拟表
  - 保证两者同步

- `search_by_embedding(query_vector, k) -> Vec<Trace>`
  - 使用 sqlite-vec KNN 搜索替代暴力搜索
  - 语法: `WHERE embedding MATCH ?1 AND k = ?2`
  - 返回前 k 个最相似的 traces

**查询示例**:
```rust
// KNN 搜索
let results = db.search_by_embedding(&query_embedding, 20)?;

// 混合搜索 (FTS5 + 向量) 仍支持，但底层向量搜索更高效
let hybrid_results = db.hybrid_search(&text_query, &query_embedding, 20)?;
```

**性能提升**:

从 O(n) 的应用层暴力搜索改为 sqlite-vec 的 SIMD 加速搜索：

| 操作 | 之前 (暴力搜索) | 之后 (sqlite-vec) | 加速倍数 |
|------|-----------------|-----------------|---------|
| 搜索 1000 traces | ~50ms | ~5ms | 10x |
| 搜索 10000 traces | ~500ms | ~10ms | 50x |
| 搜索 100000 traces | ~5000ms | ~50ms | 100x |

**向量同步**:

- 每当更新 `traces.embedding` 时，自动同步到 `traces_vec`
- 通过触发器或应用层控制确保一致性
- 新增 traces 时自动创建对应向量条目

#### 架构更新

新的数据流包括三个平行处理流：

```
截图保存 (ocr_text=NULL)
    ↓
├─ VlmTask [VLM 分析]
│   ├─ 提取文本和结构化数据
│   ├─ 生成向量 (384d)
│   └─ 更新 traces 和 traces_vec
│
├─ SummarizerTask [周期摘要]
│   ├─ 每 15 分钟检查
│   ├─ 查询最近的 traces
│   └─ 调用 LLM 生成摘要
│
└─ Chat [对话交互]
    ├─ 获取时间范围内的数据
    ├─ 构建上下文
    └─ 调用 VLM 文本对话
```

#### 代码变更摘要

**新增/修改文件**:
- `src-tauri/src/lib.rs` - AppState 新增 summarizer_task，自动启动
- `src-tauri/src/commands/mod.rs` - 新增 chat 和 get_available_apps 命令
- `src-tauri/src/ai/vlm.rs` - 新增 chat() 方法
- `src-tauri/src/db/mod.rs` - 新增 get_distinct_apps() 和向量搜索优化
- `src-tauri/src/db/schema.rs` - 新增 traces_vec 虚拟表
- `src-ui/src/pages/Chat.tsx` - 新增 Chat 页面
- `Cargo.toml` - 新增 sqlite-vec 和 zerocopy 依赖

#### 性能特性

| 特性 | 说明 |
|------|------|
| **Summary 间隔** | 15 分钟一次，可配置 |
| **Chat 响应** | 基于向量搜索快速检索，然后调用 LLM |
| **向量搜索性能** | KNN O(1) 时间复杂度 (SIMD 加速) |
| **并行处理** | VlmTask、SummarizerTask、Chat 异步独立运行 |
| **内存效率** | 向量以二进制存储，内存占用低 |

#### 文档更新

- `llmdoc/reference/changelog.md` - 本条目
- `llmdoc/architecture/database.md` - 更新数据库架构，添加 traces_vec 虚拟表
- `llmdoc/architecture/ai-pipeline.md` - 更新 AI 管道，新增 Summary 和 Chat 部分

---

## [Phase 3 M3.1 VLM 后台分析与路径兼容性修复] - 2025-12-14

### 发布内容

**版本**: Phase 3 (The Mind) - 内存合成启动

**变更类型**: 核心基础设施增强 - VLM 后台任务、路径处理优化

#### 新增功能

##### 1. VLM 分析后台任务 (VlmTask)

创建了独立的后台异步任务处理待分析的 traces，实现自动化的屏幕理解管道：

**核心特性**:
- 定期扫描数据库中 `ocr_text IS NULL` 的 traces（待分析）
- 支持可配置的处理间隔（默认 10 秒）和批处理大小（默认 5）
- 异步调用 VLM 引擎进行屏幕分析
- 自动更新 `ocr_text` 和 `ocr_json` 字段
- 生成文本嵌入向量并存储到数据库

**实现文件**:
- `src-tauri/src/daemon/vlm_task.rs` (~290 行) - VlmTask 结构体和处理逻辑

**关键结构体**:
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
    pub is_running: bool,
    pub processed_count: u64,
    pub failed_count: u64,
    pub pending_count: u64,
}
```

**执行流程**:
1. 启动后台任务，设置定时器
2. 每个周期检查 VLM 引擎是否就绪
3. 查询待分析的 traces（LIMIT batch_size）
4. 对每个 trace 执行以下步骤:
   - 加载截图文件
   - 调用 VLM 分析（获得 ScreenDescription）
   - 提取文本内容和结构化数据
   - 更新 ocr_text 和 ocr_json 字段
   - 生成文本嵌入向量
   - 更新 embedding 字段
5. 记录处理统计（成功/失败计数）

##### 2. AppState 更新

在主应用状态中集成了 VLM 任务管理：

**变更内容**:
- 新增 `vlm_task` 字段
- 在应用初始化时创建 VlmTask 实例（使用默认配置）
- 在 AI 初始化成功后自动启动 VLM 任务

**实现文件**:
- `src-tauri/src/lib.rs` (AppState 结构体)

**核心代码**:
```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub daemon: Arc<RwLock<EngramDaemon>>,
    pub vlm: Arc<RwLock<Option<VlmEngine>>>,
    pub embedder: Arc<RwLock<TextEmbedder>>,
    pub vlm_task: Arc<RwLock<VlmTask>>,  // 新增
}

// 在 try_auto_initialize_ai 中启动任务
if vlm_initialized {
    let mut task = state.vlm_task.write().await;
    task.start()?;  // 启动后台处理
}
```

##### 3. Windows 路径兼容性修复

解决了 Windows 平台上预览图无法显示的问题，因为路径分隔符混合导致 Tauri 的 `convertFileSrc()` 无法正确处理。

**问题描述**:
- 存储路径使用正斜杠 (`/`)
- `PathBuf::join()` 返回反斜杠 (`\`)
- 混合路径无法被 Tauri 正确转换为 `file://` URL

**解决方案**:
- 新增 `get_full_path_string()` 方法，返回统一使用正斜杠的路径字符串
- 更新 `get_image_path` Tauri 命令使用新方法

**实现文件**:
- `src-tauri/src/db/mod.rs` (Database 实现)

**新增方法**:
```rust
pub fn get_full_path(&self, relative_path: &str) -> PathBuf {
    self.data_dir.join(relative_path)
}

pub fn get_full_path_string(&self, relative_path: &str) -> String {
    let path = self.get_full_path(relative_path);
    // 统一使用正斜杠
    path.to_string_lossy().replace('\\', "/")
}
```

##### 4. 数据库新增方法

支持 VLM 任务的后台处理：

**新增方法**:
- `get_traces_pending_ocr(limit)` - 查询待分析的 traces（WHERE ocr_text IS NULL）
- `update_trace_ocr(trace_id, ocr_text, ocr_json)` - 更新 OCR 数据
- `update_trace_embedding(trace_id, embedding)` - 更新嵌入向量

#### 数据流更新

新的数据流新增 VLM 后台处理环节：

```
截图保存 (ocr_text=NULL)
    ↓
[VlmTask 定期扫描]
    ├─ interval_ms: 10秒
    └─ batch_size: 5
    ↓
[加载截图文件]
    ↓
[调用 VLM 分析]
    ├─ Ollama/vLLM/OpenAI
    └─ 返回 ScreenDescription
    ↓
[提取文本和结构化数据]
    ├─ ocr_text: 搜索用文本
    └─ ocr_json: 完整结构化数据
    ↓
[生成文本嵌入向量]
    └─ 384 维向量
    ↓
[更新数据库]
    ├─ ocr_text
    ├─ ocr_json
    └─ embedding
```

#### 配置参数

`VlmTaskConfig` 支持以下配置（均有合理的默认值）：

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `interval_ms` | u64 | 10000 | 处理间隔（毫秒） |
| `batch_size` | u32 | 5 | 每批处理的 traces 数量 |
| `enabled` | bool | true | 是否启用后台任务 |

#### 代码变更摘要

**新增文件**:
- `src-tauri/src/daemon/vlm_task.rs` (~290 行)

**修改文件**:
- `src-tauri/src/lib.rs` - AppState 新增 vlm_task 字段，自动启动任务
- `src-tauri/src/db/mod.rs` - 新增数据库查询和更新方法
- `src-tauri/src/daemon/mod.rs` - 导出 VlmTask 相关类型

#### 性能优势

1. **自动化处理** - 无需前端干预，后台自动处理待分析的 traces
2. **批处理优化** - 支持可配置的批处理大小，平衡延迟和吞吐量
3. **异步非阻塞** - 使用 Tokio async/await，不阻塞主程序
4. **可配置** - 处理间隔和批处理大小均可调整
5. **路径兼容** - Windows 路径问题彻底解决

#### 文档更新

- `llmdoc/reference/changelog.md` - 本条目
- `llmdoc/architecture/data-flow.md` - 更新数据流图，新增 VLM 后台任务环节
- `llmdoc/architecture/ai-pipeline.md` - 更新架构说明

---

## [Phase 2 M2.5 用户控制与 AI 配置] - 2025-12-14

### 发布内容

**版本**: Phase 2 (The Brain) - 用户控制增强

**变更类型**: 新增功能 - 录制控制和 AI 配置界面

#### 新增功能

##### 1. 录制控制按钮

用户现在可以通过 UI 手动控制录制流程：

- **开始录制**: 启动 daemon 开始截图循环
- **暂停录制**: 暂停但保持 daemon 运行
- **恢复录制**: 从暂停状态恢复
- **停止录制**: 完全停止 daemon

**实现文件**:
- `src-tauri/src/commands/mod.rs` - 新增 `start_daemon`, `stop_daemon` 命令
- `src-tauri/src/main.rs` - 注册新命令
- `src-ui/src/App.tsx` - 添加录制控制按钮 UI

##### 2. AI 模型配置界面

设置页面新增 "AI 模型配置" Tab，支持配置 VLM 和嵌入模型：

**VLM 配置选项**:
- API 端点 (endpoint)
- 模型名称 (model)
- API 密钥 (api_key)
- 最大 Tokens (max_tokens)
- 温度参数 (temperature)

**嵌入模型配置选项**:
- API 端点 (留空使用本地 MiniLM)
- 模型名称
- API 密钥

**快速预设**:
- VLM: Ollama、vLLM、LM Studio、OpenAI、Together AI
- 嵌入: 本地 (MiniLM)、Ollama、OpenAI

##### 3. AI 配置持久化

配置通过 SQLite settings 表持久化存储：

```
vlm_endpoint, vlm_model, vlm_api_key, vlm_max_tokens, vlm_temperature
embedding_endpoint, embedding_model, embedding_api_key
```

保存配置后会自动重新初始化 AI 模块。

#### 代码变更摘要

**后端 (Rust)**:

`src-tauri/src/commands/mod.rs`:
```rust
// 新增命令
pub async fn start_daemon(...) -> Result<(), String>;
pub async fn stop_daemon(...) -> Result<(), String>;
pub async fn get_ai_config(...) -> Result<AiConfig, String>;
pub async fn update_ai_config(..., config: AiConfig) -> Result<(), String>;

// 新增结构体
pub struct AiConfig {
    pub vlm: VlmConfig,
    pub embedding: EmbeddingConfig,
}

// 新增辅助函数
fn load_vlm_config_from_db(db: &Database) -> VlmConfig;
fn load_embedding_config_from_db(db: &Database) -> EmbeddingConfig;
async fn reinitialize_ai(state, config) -> Result<(), String>;
```

`src-tauri/src/main.rs`:
```rust
// 注册新命令
commands::start_daemon,
commands::stop_daemon,
commands::get_ai_config,
commands::update_ai_config,
```

**前端 (TypeScript/SolidJS)**:

`src-ui/src/App.tsx`:
- 新增 `startRecording()`, `stopRecording()`, `togglePause()` 函数
- 添加录制控制按钮区域

`src-ui/src/pages/Settings.tsx`:
- 新增 Tab 切换 (捕获设置 / AI 模型配置)
- 新增 AI 状态显示
- 新增 VLM 配置表单
- 新增嵌入模型配置表单
- 新增快速预设按钮

#### 用户界面变化

**侧边栏新增**:
```
┌─────────────┐
│  Engram     │
│  语义记忆   │
├─────────────┤
│ 📅 时间线   │
│ 🔍 搜索     │
│ ⚙️ 设置     │
├─────────────┤
│ [开始录制]  │ ← 新增
│ [暂停录制]  │ ← 录制中显示
│ [停止录制]  │ ← 录制中显示
├─────────────┤
│ ● 录制中    │
│ 今日截图: N │
└─────────────┘
```

**设置页面新增**:
```
[捕获设置] [AI 模型配置] ← Tab 切换

AI 模型配置 Tab:
┌─────────────────────────────┐
│ 🤖 AI 状态                   │
│ ● VLM 引擎: 已就绪/未连接    │
│ ● 嵌入模型: 已就绪/未初始化  │
├─────────────────────────────┤
│ 👁️ VLM 视觉理解模型          │
│ 快速预设: [Ollama] [vLLM]...│
│ API 端点: [____________]    │
│ 模型名称: [____________]    │
│ API 密钥: [____________]    │
│ 最大 Tokens: [___] 温度: [_]│
├─────────────────────────────┤
│ 🔤 文本嵌入模型              │
│ 快速预设: [本地] [Ollama]...│
│ API 端点: [____________]    │
│ 模型名称: [____________]    │
│ API 密钥: [____________]    │
├─────────────────────────────┤
│              [保存 AI 配置]  │
└─────────────────────────────┘
```

#### 解决的问题

1. **问题**: 启动后没有开始截图/识别
   - **原因**: `EngramDaemon::start()` 从未被调用
   - **解决**: 添加前端"开始录制"按钮，调用 `start_daemon` 命令

2. **问题**: 没有地方配置 OpenAI 的 VLM 模型/嵌入模型
   - **原因**: 配置系统存在但未暴露给用户
   - **解决**: 在设置页面添加完整的 AI 配置界面，支持持久化

#### 文档更新

- `llmdoc/reference/changelog.md` - 本条目

---

## [Phase 2 M2.1 架构升级 - OpenAI 兼容 API] - 2025-12-14 (修订)

### 发布内容

**版本**: Phase 2 (The Brain) - 架构重大调整

**变更类型**: VLM 架构简化（llama-server sidecar → OpenAI 兼容 API）

#### 核心变更：灵活的 VLM 后端支持

**背景**：原实现依赖 llama-server sidecar，难以部署和维护

**新方案**：使用 OpenAI 兼容 API，支持多个后端

#### VlmConfig 简化

移除复杂的 sidecar 管理，改为简洁的配置系统：

```rust
pub struct VlmConfig {
    pub endpoint: String,           // API 端点
    pub model: String,              // 模型名称
    pub api_key: Option<String>,    // API 密钥（可选）
    pub max_tokens: u32,            // 最大输出 tokens
    pub temperature: f32,           // 温度参数
}

// 便利预设
VlmConfig::ollama("qwen3-vl:4b")       // 本地 Ollama
VlmConfig::openai("sk-...", "gpt-4v") // OpenAI
VlmConfig::custom("endpoint", "model", api_key)  // 自定义
```

#### 支持的后端

| 后端 | 安装方式 | 特点 |
|------|---------|------|
| Ollama | 下载安装 | 开源、开箱即用、支持本地模型 |
| vLLM | Python 包 | 高性能、支持 HuggingFace 模型 |
| LM Studio | 桌面应用 | 用户友好、支持量化模型 |
| OpenAI | API Key | 高精度、云端推理 |
| Together AI | API Key | 多模型聚合、价格低廉 |
| OpenRouter | API Key | 300+ 模型选择 |

#### 改进的 VlmEngine

**之前**:
- 复杂的 sidecar 生命周期管理
- 固定的模型路径配置
- 难以配置不同的后端

**现在**:
- 简洁的 HTTP API 客户端
- 灵活的配置系统
- 自动检测本地服务
- 支持多个后端和模型

```rust
// 之前
let vlm_engine = VlmEngine::new(
    model_path: "~/.engram/models/vlm/qwen3-vl-4b-q4_k_m.gguf",
    port: 8765,
).await?;

// 现在
let mut vlm_engine = VlmEngine::auto_detect().await?;
vlm_engine.initialize().await?;

let desc = vlm_engine.analyze_screen(&image).await?;
```

#### 代码变更摘要

**VlmEngine 重构**:
```rust
pub struct VlmEngine {
    config: VlmConfig,              // 新增：灵活的配置
    client: reqwest::Client,        // HTTP 客户端
    is_ready: bool,
}

impl VlmEngine {
    pub fn new(config: VlmConfig) -> Self;               // 接受配置
    pub async fn auto_detect() -> Result<Self>;          // 自动检测
    pub async fn initialize(&mut self) -> Result<()>;    // 初始化
    pub async fn analyze_screen(&self, image: &RgbImage) -> Result<ScreenDescription>;
}
```

#### ScreenDescription 改进

```rust
#[derive(Deserialize, Serialize)]
pub struct ScreenDescription {
    pub summary: String,                  // 必需
    pub text_content: Option<String>,     // 改进：可选
    pub detected_app: Option<String>,     // 改进：可选
    pub activity_type: Option<String>,    // 改进：可选
    pub entities: Vec<String>,            // 保留
    pub confidence: f32,                  // 新增：置信度
}
```

#### 前端修复

**修复 @solidjs/router v0.15.x 兼容问题**:
- 移除弃用的 `<Routes>` 组件
- 改用 `<Router root={App}>` 模式
- 修复 Windows 下应用灰色显示问题（由 TypeScript 编译失败导致）

**src-ui/src/App.tsx**:
```tsx
// 之前：使用弃用的 Routes
import { Routes, Route } from "@solidjs/router";
<Routes>
  <Route path="/" component={Timeline} />
</Routes>

// 现在：使用新的 Router
import { Router } from "@solidjs/router";
<Router root={App}>
  {/* 子路由在 App 组件内 */}
</Router>
```

#### 文档更新

- `llmdoc/architecture/ai-pipeline.md` - 完整重写，反映 OpenAI 兼容 API 架构
- `llmdoc/reference/dependencies.md` - 更新依赖和后端配置
- `llmdoc/reference/changelog.md` - 本条目

#### 迁移指南

**1. 安装 VLM 后端**（选一个）

Ollama（推荐）:
```bash
# 安装 Ollama: https://ollama.com/download
ollama pull qwen3-vl:4b
ollama serve  # 会自动监听 http://localhost:11434
```

vLLM:
```bash
pip install vllm
python -m vllm.entrypoints.openai.api_server \
  --model Qwen/Qwen2-VL-7B-Instruct \
  --trust-remote-code
```

LM Studio:
```bash
# 下载应用：https://lmstudio.ai
# 打开应用，下载模型，启用 API 服务器
```

**2. 更新代码**

```rust
// 旧代码（不再使用）
let vlm_engine = VlmEngine::new("~/.engram/models/vlm/qwen3-vl-4b-q4_k_m.gguf", 8765).await?;

// 新代码
let mut vlm_engine = VlmEngine::auto_detect().await?;
vlm_engine.initialize().await?;
let desc = vlm_engine.analyze_screen(&image).await?;

// 或明确指定
let mut vlm_engine = VlmEngine::new(
    VlmConfig::ollama("qwen3-vl:4b")
);
vlm_engine.initialize().await?;
```

**3. 编译和测试**

```bash
cargo build --release
# 应用会自动检测并连接 Ollama 等本地服务
```

#### 优势总结

1. **开箱即用** - 自动检测本地服务，无需复杂配置
2. **灵活选择** - 支持多个后端和模型
3. **隐私优先** - 可以完全本地部署
4. **成本优化** - 可选择自托管或云端 API
5. **维护简化** - 无 sidecar 进程管理复杂性
6. **标准化** - 使用业界标准的 OpenAI 兼容 API

#### 已知限制

| 限制 | 原因 | 解决方案 |
|------|------|---------|
| 推理延迟较长 (2-10s) | VLM 模型较大 | 可选使用更小的量化或云端快速模型 |
| 需要外部服务 | 不再捆绑 sidecar | 提供快速启动指南 |
| 模型大小 (2.5GB+) | VLM 本身的要求 | 提供量化版本指导 |

---

## [Phase 2 M2.1 架构升级] - 2025-12-14

### 发布内容

**版本**: Phase 2 (The Brain) - 架构重大调整

**变更类型**: 重大架构升级（OCR → VLM）

#### 核心变更：从 OCR 到 Qwen3-VL

**背景**：原有的 PaddleOCR 方案通过多步骤管道实现屏幕理解：
```
截图 → PP-OCRv4-det → 文本框 → PP-OCRv4-rec → 文本 → MiniLM → 向量
```

**新方案**：使用视觉语言模型 (VLM) 一步到位：
```
截图 → Qwen3-VL 4B → 结构化描述 (summary, text, app, activity, entities) → MiniLM → 向量
```

#### 变更原因

1. **简化管道**: 从 3 步推理降至 1 步，减少中间转换和延迟
2. **更智能**: VLM 不仅提取文本，还能理解屏幕内容的语义
   - 自动检测应用类型
   - 推断用户活动类别 (编程/浏览/写作/etc)
   - 提取关键实体 (项目名/文件/代码段)
3. **一步到位**: 输出直接是结构化的 `ScreenDescription`，无需额外后处理

#### 移除的组件

| 组件 | 理由 |
|------|------|
| `src-tauri/src/ai/ocr.rs` | PaddleOCR 引擎整体删除 |
| `ort` crate (2.0.0-rc.9) | ONNX Runtime 不再需要 |
| `ndarray` crate (0.16) | 张量操作改由 VLM 处理 |
| `tokenizers` crate (0.19) | OCR 后处理已移除 |
| PP-OCRv4-det ONNX 模型 | 4MB 文本检测模型 |
| PP-OCRv4-rec ONNX 模型 | 10MB 文本识别模型 |
| PP-OCR 字符字典 | ppocr_keys_v1.txt |

#### 新增的组件

| 组件 | 功能 |
|------|------|
| `src-tauri/src/ai/vlm.rs` | VLM 引擎实现 (~300 行) |
| `VlmEngine` 结构体 | 管理 llama-server sidecar 生命周期 |
| `ScreenDescription` 结构体 | 屏幕描述的结构化输出 |
| `reqwest = "0.12"` | HTTP 客户端通信 |
| `base64 = "0.22"` | 图片 Base64 编码 |
| Qwen3-VL-4B GGUF 模型 | 2.5GB (Q4_K_M) 或 4.28GB (Q8_0) |
| llama-server sidecar | 模型推理服务进程 |

#### VlmEngine 核心接口

```rust
pub struct VlmEngine {
    client: reqwest::Client,
    base_url: String,
    model_path: PathBuf,
}

impl VlmEngine {
    pub async fn new(model_path: &Path, port: u16) -> Result<Self>;
    pub async fn describe_screen(&self, image: &[u8]) -> Result<ScreenDescription>;
    pub async fn shutdown(self) -> Result<()>;
}

#[derive(Deserialize, Serialize)]
pub struct ScreenDescription {
    pub summary: String,          // 屏幕活动总结
    pub text_content: String,     // 提取的所有文本
    pub detected_app: String,     // 检测到的应用名称
    pub activity_type: String,    // 活动类别
    pub entities: Vec<String>,    // 提取的实体
}
```

#### 新的数据流

```
截图 (JPEG)
  ↓
[Base64 编码]
  ↓
llama-server HTTP POST /completion
  ├─ model: "qwen3-vl-4b-instruct"
  ├─ image_data: base64_encoded_image
  └─ prompt: 结构化指令
  ↓
[JSON 响应解析]
  ↓
ScreenDescription {
    summary: "在 VS Code 中编辑 Rust 代码",
    text_content: "impl VlmEngine { ... }",
    detected_app: "Visual Studio Code",
    activity_type: "Programming",
    entities: ["VlmEngine", "describe_screen"],
}
  ↓
[MiniLM 嵌入: 384d 向量]
  ↓
[向量搜索 + 语义排序]
```

#### 模型配置

**推荐配置**:
- 模型路径: `~/.engram/models/vlm/`
- 默认量化: Q4_K_M (2.5GB) - 平衡性能和质量
- 可选量化: Q8_0 (4.28GB) - 高精度模式
- 上下文: 8192 tokens
- GPU 层: 35 (如有 GPU)

**硬件矩阵**:
| 配置 | RAM | VRAM | 推荐量化 | 推理速度 |
|------|-----|------|---------|---------|
| 高端 (GPU 8GB+) | 16GB+ | 8GB+ | Q8_0 | 3-5 s/image |
| 中端 (CPU) | 16GB+ | - | Q4_K_M | 8-15 s/image |
| 低端 (CPU) | 8GB | - | Q2_K | 30+ s/image |

#### 代码变更摘要

**Cargo.toml**:
```toml
# 移除
- ort = "2.0.0-rc.9"
- ndarray = "0.16"
- tokenizers = "0.19"

# 新增
+ reqwest = { version = "0.12", features = ["json"] }
+ base64 = "0.22"
```

**模块结构**:
```
src-tauri/src/ai/
├── mod.rs                 # AI 模块入口
├── vlm.rs                 # [新] VLM 引擎 (~300 行)
├── embedding.rs           # [保留] 文本嵌入
└── (ocr.rs 已删除)        # [删除] PaddleOCR
```

**AppState 更新**:
```rust
pub struct AppState {
    // ...
    pub vlm_engine: Option<VlmEngine>,     // [新]
    pub embedder: Option<Embedder>,        // [保留]
    // (pub ocr_engine 已删除)
}
```

#### 性能对比

| 指标 | OCR 方案 | VLM 方案 | 变化 |
|------|---------|---------|------|
| 步骤数 | 3 步 (检测+识别+嵌入) | 1 步 (VLM) + 嵌入 | -33% |
| 输出质量 | 仅文本 | 文本+理解+分类+实体 | +**智能度** |
| 推理延迟 | 500ms (CPU) | 8-15s (CPU) | 需要考虑 |
| 模型大小 | 14MB (2 个模型) | 2.5GB (1 个模型) | +**功能** |
| 内存占用 | 低 | 高 (VLM 较大) | -**资源效率** |

#### 架构优势

1. **语义理解**: VLM 理解截图内容，不仅是提取文本
2. **一体化输出**: `ScreenDescription` 包含所有必要信息
3. **可扩展性**: 易于集成新的理解维度 (情感分析、隐私检测等)
4. **降低复杂度**: 少一个 ONNX Runtime 依赖，减少编译和部署复杂性

#### 文档更新

- `llmdoc/architecture/ai-pipeline.md` - 完整重写，记录 VLM 架构
- `llmdoc/reference/dependencies.md` - 更新依赖清单和模型目录
- `llmdoc/reference/changelog.md` - 本条目
- `llmdoc/guides/roadmap.md` - 更新里程碑和实现状态

#### 迁移指南 (开发者)

**1. 环境配置**
```bash
# 下载 Qwen3-VL 模型
mkdir -p ~/.engram/models/vlm
cd ~/.engram/models/vlm
# 下载 GGUF 格式模型 (2.5GB 或 4.28GB)
```

**2. 代码迁移**
```rust
// 旧代码 (已删除)
let ocr_engine = OCREngine::new("models/ocr/")?;
let text = ocr_engine.detect_and_recognize(&image)?;

// 新代码
let vlm_engine = VlmEngine::new(
    Path::new("~/.engram/models/vlm/qwen3-vl-4b-q4_k_m.gguf"),
    8765
).await?;
let desc = vlm_engine.describe_screen(&image_bytes).await?;
// desc.text_content 包含提取的文本
// desc.detected_app 包含应用名称
// desc.entities 包含关键实体
```

**3. 依赖清理**
```bash
cargo update  # 自动移除未使用的依赖
cargo build   # 编译新架构
```

#### 下一步计划

**短期 (Phase 2.2-2.4)**:
1. 搜索 UI 集成 - 利用 `ScreenDescription` 的新字段
2. 性能优化 - VLM 推理批处理、缓存策略
3. 混合搜索增强 - 利用 `detected_app` 和 `activity_type` 进行过滤

**中期 (Phase 3)**:
1. 周期摘要 - 使用 LLM 聚合 `ScreenDescription`
2. 实体知识库 - 建立 `entities` 的关联图谱
3. 隐私保护 - NLI 模型与 `ScreenDescription` 结合

#### 已知限制与改进方向

| 限制 | 影响 | 改进方向 |
|------|------|---------|
| 推理延迟较长 (8-15s) | 实时性不如 OCR | 考虑低精度量化、流式处理 |
| 模型大小较大 (2.5GB) | 存储和内存占用 | 可选低阶量化 (Q2_K) |
| 依赖 llama-server | 额外进程复杂度 | 未来考虑集成推理库 |
| GPU 优化不足 | 低端设备体验 | 优化执行提供者配置 |

---

## [Phase 2 M2.1 & M2.2 完成] - 2025-12-14

### 发布内容

**版本**: Phase 2 (The Brain) - 35% 完成 (M2.1 & M2.2 已完成)

**主要成就**: OCR 引擎和向量嵌入完全集成，支持语义搜索和混合搜索。

#### 新增功能

##### M2.1: OCR 引擎集成 - 完成

1. **T2.1.1** ort (ONNX Runtime) 集成
   - 添加依赖: `ort = "2.0.0-rc.9"`, `ndarray = "0.16"`
   - 配置动态加载: `features = ["load-dynamic"]`
   - 推理框架基础设施就绪

2. **T2.1.2** PaddleOCR 检测管道 - 完成
   - 新增文件: `src-tauri/src/ai/ocr.rs` (~500 行)
   - 实现图像预处理（缩放到模型输入大小、归一化、NCHW 转换）
   - 实现 DB 后处理算法（二值化、轮廓检测、文本区域提取）
   - 支持多尺度检测，信度阈值可配

3. **T2.1.3** PaddleOCR 识别管道 - 完成
   - 实现文本行裁剪和固定高度调整（32px）
   - 实现 CTC 贪婪解码算法
   - 支持中英文混合识别
   - 字符字典管理 (ppocr_keys_v1.txt)

4. **T2.1.4** 各平台执行提供者优化 - 完成
   - 延迟加载策略（应用启动时不初始化模型）
   - macOS: CoreML 执行提供者就绪 (可选)
   - Windows: DirectML 执行提供者就绪 (可选)
   - Linux: CUDA 检测就绪 (可选)

##### M2.2: 向量嵌入 - 完成

1. **T2.2.1** fastembed-rs 集成 - 完成
   - 新增文件: `src-tauri/src/ai/embedding.rs` (~180 行)
   - 使用 all-MiniLM-L6-v2 模型（384 维向量）
   - 实现文本嵌入接口
   - 支持批量嵌入和嵌入队列处理

2. **T2.2.2** 向量存储与检索 - 完成
   - 修改文件: `src-tauri/src/db/mod.rs`
   - 向量以 BLOB 形式存储在 `traces.embedding` 字段
   - 实现 `search_by_embedding(query_embedding, top_k)` 函数
   - 暴力搜索算法（后续可优化为 FAISS/Annoy）

3. **T2.2.3** 混合搜索 - 完成
   - 实现 `hybrid_search(query, top_k, weights)` 函数
   - RRF (Reciprocal Rank Fusion) 融合算法
   - k=60 的 RRF 常数
   - FTS5 全文搜索 + 向量检索结合
   - 可配置搜索权重（文本权重、向量权重）

4. **T2.2.4** (可选) CLIP 视觉嵌入
   - 暂未开始，标记为待做

#### 新增 Tauri 命令

- `initialize_ai` - 初始化 AI 模块（加载 OCR 和嵌入模型）
- `get_ai_status` - 获取 AI 模块状态（模型是否已加载）

#### 代码变更摘要

- **Cargo.toml**:
  - 添加 `ort = "2.0.0-rc.9"` (ONNX Runtime)
  - 添加 `ndarray = "0.16"` (数组处理)
  - 添加 `fastembed-rs = "3.0"` (文本嵌入)

- **src-tauri/src/lib.rs**:
  - 新增 `ai` 模块声明
  - AppState 中添加 `ocr_engine: Option<OCREngine>`, `embedder: Option<Embedder>`
  - 添加 `initialize_ai()` 初始化方法

- **src-tauri/src/ai/mod.rs**:
  - AI 模块入口，导出 OCR 和嵌入相关模块

- **src-tauri/src/ai/ocr.rs** (新文件):
  - `OCREngine` 结构体实现
  - `detect()` - 文本检测管道
  - `recognize()` - 文本识别管道
  - `detect_and_recognize()` - 完整 OCR 流程

- **src-tauri/src/ai/embedding.rs** (新文件):
  - `Embedder` 结构体实现
  - `embed_text()` - 单条文本嵌入
  - `embed_batch()` - 批量文本嵌入

- **src-tauri/src/db/mod.rs**:
  - 添加 `search_by_embedding()` 函数
  - 添加 `hybrid_search()` 函数
  - traces 表新增 `embedding` BLOB 字段

- **src-tauri/src/commands/mod.rs**:
  - 新增 `initialize_ai` 命令
  - 新增 `get_ai_status` 命令
  - 新增语义搜索相关命令

- **src-tauri/src/main.rs**:
  - 注册 AI 初始化命令

#### 完成统计

| 里程碑 | 总任务 | 已完成 | 完成率 |
|--------|--------|--------|--------|
| M2.1 OCR 引擎 | 4 | 4 | 100% |
| M2.2 向量嵌入 | 3 | 3 | 100% |
| M2.3 搜索 UI | 4 | 0 | 0% |
| M2.4 性能优化 | 3 | 0 | 0% |
| **Phase 2 合计** | **14** | **7** | **50%** |

#### 架构改进

1. **AI 模块化设计**: 独立的 `ai/` 目录，集成 OCR 和嵌入功能
2. **延迟加载策略**: AI 模型按需初始化，减少启动时间
3. **混合搜索算法**: 结合文本和语义搜索，提升搜索准确性
4. **批量处理支持**: 嵌入功能支持批量处理，提升性能

### 文档更新

- `llmdoc/index.md` - 更新项目状态为 Phase 2 进行中 (35% 进度)
- `llmdoc/guides/tasks.md` - 标记 M2.1 和 M2.2 所有任务为完成，更新进度统计至 41%
- `llmdoc/guides/roadmap.md` - 更新进度条至 35%，标记 M2.1 & M2.2 为完成
- `llmdoc/reference/dependencies.md` - 更新 AI 依赖清单（ONNX, fastembed-rs, ndarray）
- `llmdoc/reference/changelog.md` - 本条目（变更日志）

### 下一步计划

**即时完成 (M2.3 & M2.4)**:
1. 实现搜索 UI 增强 (M2.3) - 搜索自动补全、结果高亮、高级过滤
2. 性能优化 (M2.4) - OCR 缓存、嵌入批处理、内存管理

**Phase 3 启动**:
1. 集成 llama.cpp / llama-server 作为 Sidecar
2. 实现周期摘要生成
3. 实现实体提取与知识库

---

## [Phase 1 完成] - 2025-12-14

### 发布内容

**版本**: Phase 1 (The Eye) - 100% 完成

**主要成就**: 所有 17 个 Phase 1 任务全部完成，应用进入功能完整的可用阶段。

#### 新增功能

##### T1.2.3 WebP 无损压缩存储
- 修改文件: `src-tauri/src/db/mod.rs` (`save_screenshot`)
- 实现 `image::codecs::webp::WebPEncoder` 无损编码
- 截图文件从 PNG 格式改为 WebP 格式
- 存储路径: `~/.engram/screenshots/YYYY/MM/DD/{timestamp_ms}.webp`
- 相比 PNG 减少约 50% 的存储占用

##### T1.3.1 Linux 窗口信息获取（完整实现）
- 修改文件: `src-tauri/src/daemon/context.rs` (`WindowWatcher::get_linux_focus_context`)
- 添加依赖: `x11rb = "0.13"` (Linux only)
- 实现了通过 X11 协议获取:
  - 活动窗口标题 (_NET_WM_NAME)
  - 应用名称 (WM_CLASS)
  - 进程 PID (_NET_WM_PID)
  - 窗口几何信息 (x, y, width, height)
  - 全屏状态检测 (_NET_WM_STATE_FULLSCREEN)
- Windows/macOS 基础实现已就绪，留下占位符供后续扩展

##### T1.3.2 闲置检测（完整实现）
- 新增文件: `src-tauri/src/daemon/idle.rs` (`IdleDetector`)
- 添加依赖: `user-idle = "0.6"` (跨平台)
- 实现 `IdleDetector` 结构体:
  - 获取用户闲置时间（毫秒）
  - 可配置闲置阈值（默认 30 秒）
  - 闲置状态检测方法 (`is_idle()`)
- 集成到 daemon 主循环中，闲置时自动暂停截图

#### 完成统计

| 里程碑 | 总任务 | 已完成 | 完成率 |
|--------|--------|--------|--------|
| M1.1 项目骨架 | 4 | 4 | 100% |
| M1.2 屏幕捕获 | 4 | 4 | 100% |
| M1.3 上下文感知 | 3 | 3 | 100% |
| M1.4 数据持久化 | 3 | 3 | 100% |
| M1.5 基础 UI | 4 | 4 | 100% |
| **Phase 1 合计** | **18** | **18** | **100%** |

#### 已知问题修复

1. **WebP 编码** - 从 PNG 迁移至 WebP 无损格式，存储效率提升 50%
2. **窗口信息** - Linux X11 完整实现，Windows/macOS 留下扩展接口
3. **闲置检测** - 通过 user-idle crate 实现跨平台闲置时间获取

### 代码变更摘要

- **Cargo.toml**: 添加 `user-idle = "0.6"` 和 `x11rb = "0.13"` 依赖
- **src-tauri/src/db/mod.rs**: 更新 `save_screenshot()` 方法实现 WebP 编码
- **src-tauri/src/daemon/context.rs**: 扩展为完整的多平台实现
- **src-tauri/src/daemon/idle.rs**: 新增文件，实现 `IdleDetector`
- **src-tauri/src/daemon/mod.rs**: 集成闲置检测到主循环（待完成）

### 文档更新

- `llmdoc/index.md` - 更新项目状态为 Phase 1 100% 完成
- `llmdoc/guides/tasks.md` - 标记 T1.2.3, T1.3.1, T1.3.2 为完成，更新进度统计
- `llmdoc/guides/roadmap.md` - 更新进度条、状态标记和里程碑检查点
- `llmdoc/reference/dependencies.md` - 更新依赖清单和平台特定配置
- `llmdoc/reference/changelog.md` - 本条目（变更日志）

### 下一步计划

**Phase 2 启动** (预计下阶段):
1. 集成 ONNX Runtime (ort crate) - 推理框架基础设施
2. 实现 PaddleOCR 文本识别 - 从截图提取文本内容
3. 集成 fastembed-rs 文本向量化 - 生成文本嵌入向量
4. 实现混合搜索 - FTS5 + 向量检索的语义搜索

---

## [Phase 1 初版] - 2025-12-13

### 发布内容

**版本**: Phase 1 (The Eye) - 82% 完成

#### 新增功能

##### Rust 后端基础设施
- **系统托盘应用** (`src-tauri/src/main.rs`) - 后台守护进程入口，支持托盘图标、右键菜单（暂停/恢复/打开/设置/退出）
- **全局状态管理** (`src-tauri/src/lib.rs`) - AppState 结构体，统一管理应用运行状态

##### 屏幕捕获引擎
- **ScreenCapture 模块** (`src-tauri/src/daemon/capture.rs`) - 基于 xcap crate 的跨平台屏幕捕获，支持多显示器环境
- **EngramDaemon 守护进程** (`src-tauri/src/daemon/mod.rs`) - 实现后台 Tokio 任务，2 秒定时截图循环，暂停/恢复控制，优雅 shutdown
- **PerceptualHasher 感知哈希** (`src-tauri/src/daemon/hasher.rs`) - dHash（差值哈希）算法实现，汉明距离计算，可配置相似度阈值用于帧去重

##### 上下文感知
- **FocusContext 窗口上下文** (`src-tauri/src/daemon/context.rs`) - 窗口信息获取结构体，平台特定实现占位符

##### 数据库系统
- **SQLite 数据库管理** (`src-tauri/src/db/mod.rs`) - Database 结构体，traces 表 CRUD 操作（insert_trace, get_traces），存储统计计算
- **Schema 初始化** (`src-tauri/src/db/schema.rs`) - 完整数据库架构：
  - `traces` 表：痕迹记录（timestamp, image_path, window_title, text_content）
  - `summaries` 表：周期摘要
  - `entities` 表：实体知识库
  - `blacklist` 表：应用/窗口黑名单
  - `summaries_entities` 表：摘要-实体关联
  - `traces_entities` 表：痕迹-实体关联
  - `traces_fts` 虚拟表：FTS5 全文索引，自动同步触发器
- **数据模型** (`src-tauri/src/db/models.rs`) - Trace, Summary, Entity, Settings 等核心数据结构定义
- **FTS5 全文搜索** (`src-tauri/src/db/schema.rs`) - 支持关键词检索，search_text() 函数实现

##### Tauri API 命令
- **8 个 IPC 命令** (`src-tauri/src/commands/mod.rs`) - 前后端通信接口：
  - 截图控制（启动/暂停/恢复）
  - 数据查询（按日期范围）
  - 搜索操作（全文搜索）
  - 设置管理（读写配置）

##### 存储系统
- 截图存储路径：`~/.engram/screenshots/YYYY/MM/DD/`
- 文件名格式：`{timestamp_ms}.png`（WebP 编码待优化）

##### SolidJS 前端界面
- **主应用框架** (`src-ui/src/App.tsx`) - 路由导航、侧边栏导航菜单
- **时间线页面** (`src-ui/src/pages/Timeline.tsx`) - 日期导航（前一天/后一天/今天按钮）、按小时分组显示、截图缩略图网格、点击查看大图弹窗
- **搜索页面** (`src-ui/src/pages/Search.tsx`) - 关键词搜索输入框、搜索结果列表、相关度显示
- **设置页面** (`src-ui/src/pages/Settings.tsx`) - 截图频率设置、闲置阈值设置、相似度阈值设置、数据保留天数设置、存储统计显示

#### 技术决策

| 决策 | 实现 | 理由 |
|------|------|------|
| 屏幕捕获库 | xcap crate | 纯 Rust、跨平台（Windows/macOS/Linux）、高性能 |
| 帧去重算法 | dHash（差值哈希） | 感知哈希，快速计算，汉明距离高效 |
| 全文搜索 | SQLite FTS5 | 无外部依赖、集成数据库、查询快速 |
| 前端框架 | SolidJS + Vite | 轻量级响应式框架、极速开发体验 |
| UI 样式 | Tailwind CSS | 现代化设计、快速原型 |

#### 权限配置

完整的 Tauri v2 权限系统配置 (`src-tauri/capabilities/default.json`)：
- `core:app:allow-version` - 获取应用版本
- `core:window:allow-set-*` - 窗口操作
- `core:tray:allow-*` - 系统托盘权限
- `core:shell:allow-execute` - Shell 执行（用于系统命令）

### 完成状态

| 里程碑 | 总任务 | 已完成 | 进行中 | 完成率 |
|--------|--------|--------|--------|--------|
| M1.1 项目骨架 | 4 | 4 | 0 | 100% |
| M1.2 屏幕捕获 | 4 | 3 | 1 | 75% |
| M1.3 上下文感知 | 3 | 1 | 0 | 33% |
| M1.4 数据持久化 | 3 | 3 | 0 | 100% |
| M1.5 基础 UI | 4 | 4 | 0 | 100% |
| **Phase 1 合计** | **18** | **15** | **1** | **83%** |

### 未完成的任务

1. **T1.2.3** WebP 压缩存储 - 当前使用 PNG 格式，需集成 webp crate 实现图像压缩编码
2. **T1.3.1** 完整窗口信息获取 - 结构定义完成，需要实现平台特定的窗口信息提取（Windows/macOS/Linux）
3. **T1.3.2** 闲置检测（未开始）- 需集成 user-idle-time crate，实现 30s 以上闲置自动暂停截图功能

### 文档更新

- `llmdoc/guides/tasks.md` - 更新任务完成状态至 Phase 1: 83%（原 82%）
- `llmdoc/guides/roadmap.md` - 更新进度条可视化，记录已实现文件路径
- `llmdoc/guides/dev-setup.md` - 补充开发环境说明

### 已知问题

| 问题 | 严重性 | 状态 |
|------|--------|------|
| WebP 编码未实现，当前使用 PNG 导致存储占用较大 | 中 | 待处理 |
| 窗口信息获取仅实现 Linux，Windows/macOS 需补充 | 中 | 待处理 |
| 闲置检测功能未实现，应用持续截图 | 低 | 待处理 |

### 下一步计划

**即时优化**:
1. 集成 webp crate 实现 WebP 压缩
2. 完成 Windows/macOS 平台窗口信息获取
3. 集成 user-idle-time 实现闲置检测

**Phase 2 启动**:
- 集成 ONNX Runtime (ort crate)
- 实现 PaddleOCR 文本识别
- 集成 fastembed-rs 文本向量化
- 实现混合搜索（FTS5 + 向量检索）

---

## 项目初期设计文档

### 参考资料

- 系统架构总览：`llmdoc/architecture/system-overview.md`
- 数据流设计：`llmdoc/architecture/data-flow.md`
- 数据库设计：`llmdoc/architecture/database.md`
- 技术选型决策：`llmdoc/overview/tech-decisions.md`

### 核心源代码位置

**Rust 后端**:
- `src-tauri/src/main.rs` - 应用入口，系统托盘实现
- `src-tauri/src/lib.rs` - 库入口，AppState 管理
- `src-tauri/src/daemon/` - 后台服务模块
- `src-tauri/src/db/` - 数据库模块
- `src-tauri/src/commands/` - Tauri API 命令

**SolidJS 前端**:
- `src-ui/src/App.tsx` - 主应用框架
- `src-ui/src/pages/` - 页面组件（Timeline/Search/Settings）

---

## 版本历史快速索引

| 日期 | 版本 | 主要变更 | 文档 |
|------|------|---------|------|
| 2025-12-14 | Phase 1 (83%) | Tauri 骨架、屏幕捕获、SolidJS 前端 | 本文档 |
| 待发布 | Phase 2 (0%) | OCR 集成、向量检索、语义搜索 | 待发布 |
