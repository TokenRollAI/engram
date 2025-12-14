# 任务分解

## 任务编号规则

```
T{Phase}.{Milestone}.{Task}
例: T1.2.3 = Phase 1, Milestone 2, Task 3
```

## 任务状态

- `[ ]` 未开始
- `[~]` 进行中
- `[x]` 已完成
- `[-]` 已取消

---

## Phase 1: 全知之眼

### M1.1: 项目骨架搭建

- [x] **T1.1.1** 初始化 Tauri v2 项目
  - 创建 `src-tauri/` 目录结构
  - 配置 `tauri.conf.json` (窗口、托盘、权限)
  - 创建 `build.rs`

- [x] **T1.1.2** 配置 Cargo.toml 依赖
  - Tauri v2 核心依赖
  - 异步运行时 (tokio)
  - 数据库 (rusqlite)
  - 图像处理 (image, xcap)
  - 工具库 (serde, chrono, anyhow, tracing)

- [x] **T1.1.3** 搭建前端 SolidJS 框架
  - 创建 `src-ui/` 目录
  - 初始化 Vite + SolidJS + TypeScript
  - 配置 Tailwind CSS
  - 创建基础路由结构 (Timeline, Search, Settings)

- [x] **T1.1.4** 配置 Tauri 权限系统
  - 创建 `capabilities/default.json`
  - 配置 core、tray、shell 权限

### M1.2: 屏幕捕获引擎

- [x] **T1.2.1** 集成 xcap crate
  - 创建 `src-tauri/src/daemon/capture.rs`
  - 实现 `ScreenCapture` 结构体
  - 支持多显示器环境

- [x] **T1.2.2** 实现定时截图循环
  - 创建后台 Tokio 任务
  - 实现 2 秒间隔定时器
  - 处理暂停/恢复控制
  - shutdown 信号处理

- [x] **T1.2.3** 实现 WebP 压缩存储
  - 使用 `image::codecs::webp::WebPEncoder` 实现无损 WebP 编码
  - 创建存储目录结构 `~/.engram/screenshots/YYYY/MM/DD/`
  - 生成文件名 `{timestamp_ms}.webp`

- [x] **T1.2.4** 实现感知哈希去重
  - 实现 dHash (差值哈希) 算法
  - 存储上一帧哈希值
  - 计算汉明距离并判断相似度
  - 可配置相似度阈值

### M1.3: 上下文感知

- [x] **T1.3.1** 集成窗口信息获取
  - 创建 `src-tauri/src/daemon/context.rs`
  - 定义 `FocusContext` 结构体
  - Linux 实现完成 (通过 X11 协议获取 _NET_WM_NAME, WM_CLASS, _NET_WM_PID, 几何信息)
  - 基础实现供 Windows/macOS 扩展

- [x] **T1.3.2** 集成 user-idle-time
  - 添加依赖: `user-idle = "0.6"`
  - 创建 `IdleDetector` 结构体 (在 `src-tauri/src/daemon/idle.rs`)
  - 检测闲置时间 > 30s
  - 闲置时自动暂停截图，活动时恢复

- [x] **T1.3.3** 实现规则黑名单过滤
  - 创建 `blacklist` 表
  - 默认黑名单 (1Password, Bitwarden, Incognito 等)
  - 支持应用名和标题匹配

### M1.4: 数据持久化

- [x] **T1.4.1** 初始化 SQLite 数据库
  - 创建 `src-tauri/src/db/schema.rs`
  - 实现数据库初始化函数
  - 执行完整 Schema 创建 SQL
  - WAL 模式优化

- [x] **T1.4.2** 实现 traces 表 CRUD
  - `insert_trace()` - 插入痕迹记录
  - `get_traces()` - 按时间范围查询
  - 存储统计功能

- [x] **T1.4.3** 实现 FTS5 全文索引
  - 创建 `traces_fts` 虚拟表
  - 设置同步触发器 (INSERT/UPDATE/DELETE)
  - 实现 `search_text()` 函数

### M1.5: 基础 UI

- [x] **T1.5.1** 实现系统托盘图标
  - 右键菜单 (暂停/恢复/打开/设置/退出)
  - 双击打开主窗口
  - 状态显示 (运行中/暂停)

- [x] **T1.5.2** 实现时间线页面
  - 日期导航 (前一天/后一天/今天)
  - 按小时分组显示
  - 截图缩略图网格
  - 点击查看大图弹窗

- [x] **T1.5.3** 实现设置页面
  - 截图频率设置
  - 闲置阈值设置
  - 相似度阈值设置
  - 数据保留天数设置
  - 存储统计显示

- [x] **T1.5.4** 实现搜索页面
  - 搜索输入框
  - 关键词搜索
  - 搜索结果列表
  - 相关度显示

---

## Phase 2: 深度认知

### M2.1: VLM 引擎集成 (OpenAI 兼容 API)

- [x] **T2.1.1** 移除 PaddleOCR 和 ONNX Runtime
  - 删除 `src-tauri/src/ai/ocr.rs`
  - 移除 `ort`, `ndarray`, `tokenizers` 依赖
  - 完成日期: 2025-12-14

- [x] **T2.1.2** 实现 OpenAI 兼容 API 支持
  - 新增文件: `src-tauri/src/ai/vlm.rs` (~400 行)
  - 实现 HTTP API 客户端
  - 支持任意 OpenAI 兼容后端
  - 完成日期: 2025-12-14

- [x] **T2.1.3** 实现灵活的配置系统
  - 创建 `VlmConfig` 结构体
  - 实现 `ollama()`、`openai()`、`custom()` 预设
  - 支持 API 密钥管理
  - 完成日期: 2025-12-14

- [x] **T2.1.4** 实现自动检测和初始化
  - 实现 `VlmEngine::auto_detect()` 方法
  - 自动检测常见本地服务 (Ollama、vLLM、LM Studio)
  - 实现 `initialize()` 验证连接
  - 完成日期: 2025-12-14

- [x] **T2.1.5** 实现结构化输出
  - 创建 `ScreenDescription` 结构体
  - 新增 `confidence` 字段
  - 改进字段为 `Option<String>`（向后兼容）
  - 完成日期: 2025-12-14

### M2.2: 向量嵌入

- [x] **T2.2.1** 集成 fastembed-rs
  - 新增文件: `src-tauri/src/ai/embedding.rs`
  - 使用 all-MiniLM-L6-v2 模型（384 维向量）
  - 实现批量嵌入和嵌入队列
  - 完成日期: 2025-12-14

- [x] **T2.2.2** 向量存储与检索
  - 修改文件: `src-tauri/src/db/mod.rs`
  - 实现 `search_by_embedding()` 暴力搜索
  - 向量以 BLOB 形式存储在 traces.embedding 字段
  - 完成日期: 2025-12-14

- [x] **T2.2.3** 实现混合搜索
  - 实现 `hybrid_search()` RRF 融合算法
  - k=60 的 RRF 常数配置
  - FTS5 + 向量检索结合
  - 完成日期: 2025-12-14

- [ ] **T2.2.4** (可选) 集成 CLIP 视觉嵌入
  - 加载 CLIP 模型
  - 实现图像预处理
  - 创建 `visual_index` 表

### M2.3: 搜索 UI 增强

- [x] **T2.3.1** 实现搜索自动补全
  - 历史搜索记录
  - 快捷键支持 (Cmd/Ctrl+K)

- [x] **T2.3.2** 增强搜索结果
  - 语义搜索模式切换
  - 显示匹配文本片段

- [x] **T2.3.3** 实现结果高亮
  - 在截图上标记 OCR 区域
  - 匹配文本高亮显示

- [x] **T2.3.4** 实现高级过滤
  - 时间范围过滤器
  - 应用过滤器
  - 自定义日期范围

### M2.4: 性能优化

- [x] **T2.4.1** 实现 OCR 结果缓存
  - 基于图像哈希的缓存键
  - LRU 缓存策略

- [x] **T2.4.2** 实现嵌入批处理
  - 累积 10 条文本后批量处理
  - 定时强制刷新

- [x] **T2.4.3** 优化内存占用
  - 延迟加载模型
  - 空闲时释放模型

---

## Phase 3: 记忆合成

### M3.1: LLM 集成 (OpenAI 兼容 API)

- [x] **T3.1.1** 实现 LLM HTTP API 客户端
  - 创建 `src-tauri/src/ai/summarizer.rs`
  - 复用 OpenAI 兼容 API 架构 (参考 vlm.rs)
  - 支持 Ollama、vLLM、OpenAI 等后端
  - 完成日期: 2025-12-14

- [x] **T3.1.2** 实现摘要 Prompt 模板
  - 设计结构化摘要输出格式
  - 支持短周期 (15min) 和长周期 (每日) 摘要
  - JSON 格式输出
  - 完成日期: 2025-12-14

- [x] **T3.1.3** 实现实体提取 Prompt
  - 从摘要中提取 Person/Project/Technology/URL/File 实体
  - 支持置信度评分
  - 完成日期: 2025-12-14

### M3.2: 周期摘要

- [x] **T3.2.1** 实现 15 分钟摘要任务
  - 创建 `SummarizerTask` 定时任务
  - 查询最近 15min 的 traces
  - 调用 LLM 生成摘要
  - 完成日期: 2025-12-14

- [x] **T3.2.2** 实现每日摘要聚合
  - 聚合当日所有短周期摘要
  - 生成每日工作报告
  - 完成日期: 2025-12-14

- [x] **T3.2.3** 实现 summaries 表 CRUD
  - `insert_summary()` - 插入摘要记录
  - `get_summaries()` - 按时间/类型查询
  - `get_summary_by_id()` - 获取单条摘要
  - 完成日期: 2025-12-14

### M3.3: 实体提取

- [x] **T3.3.1** 从摘要中提取实体
  - 使用 LLM 提取命名实体
  - 支持 Person/Project/Technology/URL/File 类型
  - 完成日期: 2025-12-14

- [x] **T3.3.2** 实现 entities 表管理
  - `insert_entity()` - 插入或更新实体
  - `get_entities()` - 查询实体列表
  - `increment_mention_count()` - 增加提及计数
  - 完成日期: 2025-12-14

- [x] **T3.3.3** 实现实体-痕迹关联
  - `link_entity_to_trace()` - 关联实体和痕迹
  - `get_traces_by_entity()` - 按实体查询痕迹
  - 完成日期: 2025-12-14

### M3.4: 摘要 UI

- [x] **T3.4.1** 实现摘要列表页
  - 按日期分组显示摘要
  - 短摘/日摘切换
  - 完成日期: 2025-12-14

- [x] **T3.4.2** 实现摘要详情页
  - 显示摘要内容
  - 关联的截图列表
  - 提取的实体标签
  - 完成日期: 2025-12-14

- [x] **T3.4.3** 实现实体浏览页
  - 实体列表展示
  - 按类型/提及次数排序
  - 点击查看关联痕迹
  - 完成日期: 2025-12-14

---

## Phase 4: 生态扩展

### M4.1: MCP 服务端

- [ ] **T4.1.1** 集成 mcp-sdk
- [ ] **T4.1.2** 实现 Stdio 传输
- [ ] **T4.1.3** 实现 SSE 传输
- [ ] **T4.1.4** 实现 search_memory 工具
- [ ] **T4.1.5** 实现 get_app_activity 工具

### M4.2: 插件系统

- [ ] **T4.2.1** 集成 wasmtime
- [ ] **T4.2.2** 定义 Host Functions
- [ ] **T4.2.3** 实现插件加载器
- [ ] **T4.2.4** 编写示例插件

### M4.3: 高级功能

- [ ] **T4.3.1** 实现数据生命周期管理
- [ ] **T4.3.2** 实现数据库加密
- [ ] **T4.3.3** 实现使用统计分析
- [ ] **T4.3.4** 实现数据导出

### M4.4: 打磨与发布

- [ ] **T4.4.1** 性能调优
- [ ] **T4.4.2** 跨平台测试
- [ ] **T4.4.3** 安装包构建
- [ ] **T4.4.4** 编写用户文档

---

## 完成进度统计

| 阶段 | 总任务 | 已完成 | 进行中 | 完成率 |
|------|--------|--------|--------|--------|
| Phase 1 | 17 | 17 | 0 | 100% |
| Phase 2 | 15 | 15 | 0 | 100% |
| Phase 3 | 12 | 12 | 0 | 100% |
| Phase 4 | 13 | 0 | 0 | 0% |
| **总计** | **57** | **44** | **0** | **77%** |

## 依赖关系图

```
T1.1.1 ──► T1.1.2 ──► T1.2.1
              │
              ▼
           T1.1.3 ──► T1.5.2
              │
              ▼
           T1.4.1 ──► T1.4.2 ──► T2.2.2
                         │
                         ▼
                      T1.4.3 ──► T2.2.3
                                    │
T1.2.1 ──► T1.2.2 ──► T2.1.1 ──► T2.1.2 ──► T2.2.1
                                    │
                                    ▼
                                 T2.2.3 ──► T3.1.1
                                              │
                                              ▼
T3.1.1 ──► T3.1.2 ──► T3.2.1 ──► T3.2.3 ──► T4.1.4
              │
              ▼
           T3.1.3 ──► T3.3.1 ──► T3.3.2
```

## Phase 1 剩余工作

已完成！所有 17 个 Phase 1 任务全部完成。
