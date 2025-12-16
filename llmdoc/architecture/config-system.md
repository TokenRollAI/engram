# 配置系统架构

## 1. Identity

- **What it is**: 基于 TOML 文件的集中式配置管理系统，完全替代了 SQLite 设置表的方案。
- **Purpose**: 在应用启动时从配置文件加载所有参数，为 daemon、AI 引擎、后台任务等模块提供统一的配置来源。

## 2. 核心组件

### 配置文件存储（XDG 规范）

配置文件遵循操作系统标准位置：

- **Linux**: `~/.config/engram/Engram/config.toml`
- **macOS**: `~/Library/Application Support/com.engram.Engram/config.toml`
- **Windows**: `%APPDATA%\engram\Engram\config.toml`

### 核心文件清单

- `src-tauri/src/config/mod.rs` (AppConfig, CaptureConfig, StorageConfig 等): 配置结构体定义与 TOML 序列化
- `src-tauri/src/lib.rs` (AppState): 应用启动时调用 `AppConfig::load()` 加载配置
- `src-tauri/src/commands/mod.rs` (get_settings, update_settings, get_ai_config, update_ai_config): 前端 API 命令

## 3. 执行流程（LLM 检索图）

### 应用启动时的配置加载

**文件**: `src-tauri/src/lib.rs:38-87` (AppState::new)

```
1. AppState 初始化
   └─► AppConfig::load()
       ├─► 检查配置文件是否存在
       │   ├─ 存在 → 读取并解析 TOML
       │   └─ 不存在 → 创建默认配置文件
       │
       └─► 返回配置对象 (AppConfig)

2. 使用加载的配置初始化各模块
   ├─► Daemon: capture.interval_ms, idle_threshold_ms, similarity_threshold
   ├─► VlmTask: vlm_task.concurrency, interval_ms 等
   ├─► SummarizerTask: 默认配置
   └─► 尝试自动初始化 AI（基于 VlmConfig 和 EmbeddingConfig）
```

### 运行时配置更新

**前端发起更新** → **后端API** → **更新内存配置** → **保存到文件** → **重新初始化模块**

**文件**: `src-tauri/src/commands/mod.rs:386-500`

```
前端调用 update_settings()
   │
   ├─► 更新内存中的 AppConfig
   │   └─ state.config.write().await.capture.interval_ms = new_value
   │
   ├─► 保存到 TOML 文件
   │   └─ config.save() → TOML 序列化 → 写入磁盘
   │
   └─► 完成（无需模块重新初始化，参数修改即时生效）
```

**前端调用 update_ai_config()**

```
更新 VlmConfig/EmbeddingConfig
   │
   ├─► 更新内存配置
   ├─► 保存到 TOML 文件
   │
   └─► reinitialize_ai()
       ├─► 重新初始化 VLM 引擎
       ├─► 重新初始化 TextEmbedder
       └─► 重启后台任务（如需要）
```

## 4. 配置结构体

### AppConfig（顶层）

```rust
pub struct AppConfig {
    pub capture: CaptureConfig,        // 截图设置
    pub storage: StorageConfig,        // 数据存储
    pub session: SessionConfig,        // 会话管理
    pub summary: SummaryConfig,        // 摘要生成
    pub vlm: VlmConfig,               // VLM 视觉模型（AI 相关）
    pub embedding: EmbeddingConfig,    // 文本嵌入模型（AI 相关）
    pub vlm_task: VlmTaskConfig,      // VLM 后台任务（AI 相关）
}
```

### CaptureConfig（截图配置）

- `interval_ms` (u64): 截图间隔，单位毫秒（默认 2000）
- `idle_threshold_ms` (u64): 闲置检测阈值，单位毫秒（默认 30000）
- `similarity_threshold` (u32): 帧去重相似度阈值（pHash 汉明距离，默认 5）
- `mode` (CaptureMode): 截图捕获模式，可选值：
  - `primary_monitor`: 捕获主显示器（默认）
  - `focused_monitor`: 捕获活动窗口所在的显示器
  - `active_window`: 只捕获活动窗口

**来源**: `src-tauri/src/config/mod.rs:20-65`


### StorageConfig（存储配置）

- `hot_data_days` (u32): 热数据保留天数（默认 7）
- `warm_data_days` (u32): 温数据保留天数（默认 30）

**来源**: `src-tauri/src/config/mod.rs:54-79`

### SessionConfig（会话配置）

- `active_window_ms` (u64): 活跃线程窗口，用于多线程 Session 路由/聚类（默认 20 分钟）
- `max_active_sessions` (u32): 最大活跃 Session 数（用于构建上下文与路由候选，默认 8）
- `similarity_threshold` (f32): embedding 相似度阈值（0-1），用于把 trace 归入既有 Session（默认 0.78）
- `gap_threshold_ms` (u64): Session 冷却阈值（超过该时间未继续推进的线程更倾向被视为结束，默认 300000 = 5 分钟）

**来源**: `src-tauri/src/config/mod.rs:81-99`

### SummaryConfig（摘要配置）

- `interval_min` (u32): 自动摘要生成间隔，单位分钟（默认 15）

**来源**: `src-tauri/src/config/mod.rs:101-119`

### VlmConfig（VLM 模型配置）

由 `src-tauri/src/ai/vlm.rs` 定义，重新导出到 config 模块。

- `endpoint` (String): OpenAI 兼容 API 端点（如 `http://localhost:11434/v1`）
- `model` (String): 模型名称（如 `qwen3-vl:4b`）
- `api_key` (Option<String>): API 密钥（仅云端服务需要）
- `max_tokens` (u32): 最大输出 token 数（默认 512）
- `temperature` (f32): 温度参数（默认 0.3）

### EmbeddingConfig（嵌入模型配置）

由 `src-tauri/src/ai/embedding.rs` 定义，重新导出到 config 模块。

- `endpoint` (Option<String>): API 端点（None 使用本地 MiniLM）
- `model` (String): 模型名称（默认 `all-MiniLM-L6-v2`）
- `api_key` (Option<String>): API 密钥

### VlmTaskConfig（VLM 后台任务配置）

由 `src-tauri/src/daemon/vlm_task.rs` 定义。

- `interval_ms` (u64): 处理间隔（默认 10000）
- `batch_size` (u32): 批处理大小（默认 5）
- `enabled` (bool): 是否启用（默认 true）
- `concurrency` (u32): 并发数（新增）

## 5. TOML 文件示例

```toml
[capture]
interval_ms = 2000
idle_threshold_ms = 30000
similarity_threshold = 5
mode = "primary_monitor"  # 可选: focused_monitor, active_window


[storage]
hot_data_days = 7
warm_data_days = 30

[session]
active_window_ms = 1200000  # 20 分钟
max_active_sessions = 8
similarity_threshold = 0.78
gap_threshold_ms = 300000  # 5 分钟

[summary]
interval_min = 15

[vlm]
endpoint = "http://localhost:11434/v1"
model = "qwen3-vl:4b"
max_tokens = 512
temperature = 0.3
# api_key = "sk-..." # 仅云端服务需要，本地 Ollama 不需要

[embedding]
# endpoint = "http://localhost:11434/v1"  # 留空使用本地
model = "all-MiniLM-L6-v2"
# api_key 可选

[vlm_task]
interval_ms = 10000
batch_size = 5
enabled = true
concurrency = 1
```

## 6. 前端 API 接口

### 获取通用设置

**命令**: `get_settings()`

**返回**: Settings 结构体，包含所有配置参数

**源文件**: `src-tauri/src/commands/mod.rs:369-382`

### 更新通用设置

**命令**: `update_settings(settings: Settings)`

流程:
1. 更新内存中的 AppConfig
2. 调用 `config.save()` 保存到 TOML 文件
3. 返回成功

**源文件**: `src-tauri/src/commands/mod.rs:386-400`

### 获取 AI 配置

**命令**: `get_ai_config()`

**返回**: AiConfig 结构体

```rust
pub struct AiConfig {
    pub vlm: VlmConfig,
    pub embedding: EmbeddingConfig,
    pub vlm_task: VlmTaskConfig,
}
```

**源文件**: `src-tauri/src/commands/mod.rs:468-477`

### 更新 AI 配置

**命令**: `update_ai_config(ai_config: AiConfig)`

流程:
1. 更新内存中的 AppConfig（VLM、Embedding、VlmTask 配置）
2. 保存到 TOML 文件
3. 调用 `reinitialize_ai()` 重新初始化 AI 模块
   - 重新创建 VlmEngine 实例
   - 重新初始化 TextEmbedder
   - 重启后台任务

**源文件**: `src-tauri/src/commands/mod.rs:481-550`

## 7. 配置加载特性

### 自动创建默认配置

如果配置文件不存在，`AppConfig::load()` 会：
1. 创建默认的 AppConfig 实例
2. 创建所需的目录（使用 `fs::create_dir_all`）
3. 保存为 TOML 文件
4. 返回默认配置

**源文件**: `src-tauri/src/config/mod.rs:181-199`

### 文件权限设置

Unix 系统上，配置文件权限设置为 `0o600`（仅用户可读写），保护 API 密钥等敏感信息。

**源文件**: `src-tauri/src/config/mod.rs:219-225`

### 错误处理与日志

- 配置文件不存在 → 创建默认配置并保存
- 解析错误 → 记录警告日志，使用默认配置
- 保存错误 → 返回错误，前端显示失败提示

## 8. 关键改动对比

### 之前（SQLite 设置表）

```
settings 表
├─ vlm_endpoint
├─ vlm_model
├─ vlm_api_key
├─ embedding_endpoint
└─ embedding_model
（配置与数据混合存储在同一数据库）
```

### 现在（TOML 文件）

```
config.toml（独立文件）
├─ [vlm]
├─ [embedding]
├─ [vlm_task]
├─ [capture]
└─ [storage]
（配置与数据完全分离）
```

## 9. 设计优势

| 特性 | TOML 文件 | SQLite 表 |
|------|---------|---------|
| **可见性** | 易于查看和编辑 | 需要数据库工具 |
| **隐私** | 文件权限控制 (0o600) | 数据库级别 |
| **启动性能** | 直接从文件加载 | 数据库查询 |
| **版本管理** | 易于版本控制 | 数据库迁移复杂 |
| **配置分离** | 配置与数据分离 | 混合存储 |
| **标准化** | XDG 规范 | 非标准 |

## 10. 已知限制与未来改进

| 限制 | 影响 | 改进方向 |
|------|------|---------|
| 配置文件纯文本存储 | 敏感信息（API 密钥）可见 | 考虑加密存储或系统密钥管理 |
| 修改后需手动重新初始化 | AI 模块可能需要重启 | 自动检测并增量更新 |
| 无配置版本管理 | 无法快速回滚 | 添加配置备份机制 |
