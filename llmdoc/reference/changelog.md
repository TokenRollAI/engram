# 变更日志

## 概述

本文档记录 Engram 项目的重要版本变更、功能发布与架构更新。按时间倒序排列。

---

## [Phase 1 完成] - 2025-12-14

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

