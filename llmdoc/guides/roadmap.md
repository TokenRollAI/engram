# 实现路线图

## 当前状态

**最后更新**: 2025-12-14

```
Phase 1: 全知之眼        Phase 2: 深度认知        Phase 3: 记忆合成        Phase 4: 生态扩展
   (Eye)                    (Brain)                  (Mind)                 (Ecosystem)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

┌─────────────┐         ┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│ Tauri 骨架   │         │ ONNX OCR    │         │ llama.cpp   │         │ WASM 插件   │
│ 屏幕截图     │   →     │ 文本向量化   │    →    │ 周期摘要    │    →    │ MCP 服务    │
│ SQLite 存储  │         │ 语义搜索    │         │ 实体提取    │         │ 高级分析    │
└─────────────┘         └─────────────┘         └─────────────┘         └─────────────┘
   ███████████             ███████░░░░              ░░░░░░░░░░              ░░░░░░░░░░
    100%                      35%                      0%                      0%
```

---

## Phase 1: 全知之眼 (The Eye) - 100% 完成

**目标**: 实现基础的屏幕捕获和存储功能

**状态**: ✅ 已完成

### 已完成 ✅

| 里程碑 | 任务 | 描述 | 实现文件 |
|--------|------|-----|---------|
| M1.1 | T1.1.1 | Tauri v2 项目初始化 | `src-tauri/tauri.conf.json` |
| M1.1 | T1.1.2 | Cargo.toml 依赖配置 | `src-tauri/Cargo.toml` |
| M1.1 | T1.1.3 | SolidJS 前端框架 | `src-ui/` |
| M1.1 | T1.1.4 | Tauri 权限配置 | `src-tauri/capabilities/` |
| M1.2 | T1.2.1 | xcap 截图集成 | `src-tauri/src/daemon/capture.rs` |
| M1.2 | T1.2.2 | 定时截图循环 | `src-tauri/src/daemon/mod.rs` |
| M1.2 | T1.2.4 | 感知哈希去重 | `src-tauri/src/daemon/hasher.rs` |
| M1.3 | T1.3.3 | 规则黑名单 | `src-tauri/src/db/schema.rs` |
| M1.4 | T1.4.1 | SQLite 初始化 | `src-tauri/src/db/schema.rs` |
| M1.4 | T1.4.2 | traces 表 CRUD | `src-tauri/src/db/mod.rs` |
| M1.4 | T1.4.3 | FTS5 全文索引 | `src-tauri/src/db/schema.rs` |
| M1.5 | T1.5.1 | 系统托盘 | `src-tauri/src/main.rs` |
| M1.5 | T1.5.2 | 时间线页面 | `src-ui/src/pages/Timeline.tsx` |
| M1.5 | T1.5.3 | 设置页面 | `src-ui/src/pages/Settings.tsx` |
| M1.5 | T1.5.4 | 搜索页面 | `src-ui/src/pages/Search.tsx` |

### 进行中 🚧

（无，Phase 1 已全部完成）

### 待开始 📋

| 任务 | 描述 | 依赖 |
|------|-----|------|
| Phase 2 | 开启 OCR 和语义搜索 | Phase 1 完成 ✅ |

### Phase 1 交付物

- [x] 能够后台运行并定时截图的应用
- [x] 可以按时间浏览历史截图
- [x] 支持简单的关键词搜索 (基于窗口标题)
- [x] WebP 压缩优化存储
- [x] 完整的窗口上下文获取
- [x] 闲置检测自动暂停

---

## Phase 2: 深度认知 (The Brain) - 35% 完成

**目标**: 实现 AI 驱动的内容理解与语义搜索

**状态**: ✅ M2.1 & M2.2 已完成，进行中

### M2.1: VLM 引擎集成 (架构升级)

| 任务 | 描述 | 产出 | 状态 |
|------|-----|------|------|
| T2.1.1 (重构) | 移除 ONNX Runtime & PaddleOCR | 删除 `src-tauri/src/ai/ocr.rs` | ✅ |
| T2.1.2 (新) | OpenAI 兼容 API 支持 | `src-tauri/src/ai/vlm.rs` (~400 行) | ✅ |
| T2.1.3 (新) | 灵活的配置系统 `VlmConfig` | ollama()、openai()、custom() 预设 | ✅ |
| T2.1.4 (新) | 自动检测 + 初始化 | VlmEngine::auto_detect()、initialize() | ✅ |
| T2.1.5 (新) | 结构化输出 `ScreenDescription` | summary, text_content, detected_app, activity_type, entities, confidence | ✅ |

### M2.2: 向量嵌入

| 任务 | 描述 | 产出 | 状态 |
|------|-----|------|------|
| T2.2.1 | 集成 fastembed-rs | 文本嵌入能力 | ✅ |
| T2.2.2 | 向量存储与检索 | 向量搜索基础设施 | ✅ |
| T2.2.3 | 实现混合搜索 (FTS + 向量) | 语义搜索 API | ✅ |
| T2.2.4 | (可选) 集成 CLIP 视觉嵌入 | 以图搜图能力 | 📋

### M2.3: 搜索 UI 增强

| 任务 | 描述 | 产出 | 状态 |
|------|-----|------|------|
| T2.3.1 | 实现搜索自动补全 | 历史记录 + 快捷键 | 📋 |
| T2.3.2 | 增强搜索结果 | 语义模式 + 文本片段 | 📋 |
| T2.3.3 | 实现结果高亮 | 文本框标记 | 📋 |
| T2.3.4 | 实现高级过滤 | 时间/应用过滤器 | 📋 |

### M2.4: 性能优化

| 任务 | 描述 | 产出 | 状态 |
|------|-----|------|------|
| T2.4.1 | 实现 VLM 推理缓存 | LRU 缓存策略 | 📋 |
| T2.4.2 | 实现嵌入批处理 | 批量推理 | 📋 |
| T2.4.3 | 优化内存占用 | 模型按需加载 | 📋 |

**Phase 2 交付物**:
- [ ] 完整的屏幕理解与文本提取功能 (通过 VLM)
- [ ] 支持自然语言的语义搜索
- [ ] 搜索结果可视化与高级过滤

---

## Phase 3: 记忆合成 (The Mind) - 0% 完成

**目标**: 实现 LLM 驱动的智能摘要与知识提取

**状态**: 📋 计划中

### M3.1: LLM Sidecar

| 任务 | 描述 | 产出 |
|------|-----|------|
| T3.1.1 | 打包 llama-server | Sidecar 二进制 |
| T3.1.2 | 子进程生命周期管理 | 启动/停止/健康检查 |
| T3.1.3 | HTTP API 客户端 | 推理请求封装 |
| T3.1.4 | GBNF 语法约束 | JSON 输出保证 |

### M3.2: 周期摘要

| 任务 | 描述 | 产出 |
|------|-----|------|
| T3.2.1 | 摘要 Prompt 模板 | 结构化输出 |
| T3.2.2 | 15 分钟摘要任务 | 短周期摘要 |
| T3.2.3 | 每日摘要聚合 | 长周期摘要 |
| T3.2.4 | summaries 表存储 | 摘要持久化 |

### M3.3: 实体提取 & M3.4: 语义黑名单 & M3.5: 摘要 UI

详见 [任务分解](./tasks.md)

**Phase 3 交付物**:
- [ ] 自动生成的工作日志
- [ ] 提取的实体知识库
- [ ] 语义级别的隐私保护

---

## Phase 4: 生态扩展 (The Ecosystem) - 0% 完成

**目标**: 开放 API 与插件系统，接入 AI Agent 生态

**状态**: 📋 计划中

详见 [任务分解](./tasks.md)

**Phase 4 交付物**:
- [ ] 可被 Claude Desktop 调用的 MCP 服务
- [ ] 可扩展的 WASM 插件系统
- [ ] 生产就绪的应用程序

---

## 里程碑检查点

| 里程碑 | 验收标准 | 状态 |
|--------|---------|------|
| **M1** | 应用能后台运行，截取屏幕并存储，可浏览历史 | ✅ 100% |
| **M2** | 输入"昨天看的 Rust 文章"能找到相关截图 | 🚧 35% (M2.1 & M2.2 完成) |
| **M3** | 自动生成"今日工作摘要"并提取提到的人名/项目 | 📋 0% |
| **M4** | Claude 能通过 MCP 查询"我上周用了多少时间写代码" | 📋 0% |

## 已实现的代码结构

```
src-tauri/
├── Cargo.toml                 # Rust 依赖配置
├── tauri.conf.json            # Tauri 应用配置
├── capabilities/default.json  # 权限配置
└── src/
    ├── main.rs                # 入口 + 系统托盘
    ├── lib.rs                 # 库入口 + AppState
    ├── daemon/                # 后台服务模块
    │   ├── mod.rs             # EngramDaemon 守护进程
    │   ├── capture.rs         # ScreenCapture 截图
    │   ├── context.rs         # FocusContext 窗口上下文
    │   └── hasher.rs          # PerceptualHasher 感知哈希
    ├── db/                    # 数据库模块
    │   ├── mod.rs             # Database 管理
    │   ├── schema.rs          # Schema 初始化
    │   └── models.rs          # 数据模型
    └── commands/
        └── mod.rs             # Tauri API 命令

src-ui/
├── package.json               # npm 依赖
├── vite.config.ts             # Vite 配置
├── tailwind.config.js         # Tailwind 配置
└── src/
    ├── index.tsx              # 入口
    ├── App.tsx                # 主应用 + 路由
    └── pages/
        ├── Timeline.tsx       # 时间线页面
        ├── Search.tsx         # 搜索页面
        └── Settings.tsx       # 设置页面
```

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|-----|---------|
| OCR 在特定字体/分辨率下不准 | 搜索质量下降 | 提供手动 OCR 重试功能 |
| LLM 推理太慢 (<8GB 内存设备) | 摘要功能不可用 | 提供禁用选项，回退到规则摘要 |
| 跨平台截图 API 差异 | 开发周期延长 | 优先支持单一平台，逐步扩展 |
| 存储空间增长过快 | 用户磁盘告警 | 智能清理策略 + 用户提醒 |
