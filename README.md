# Engram

[![GitHub](https://img.shields.io/github/license/TokenRollAI/engram)](https://github.com/TokenRollAI/engram/blob/main/LICENSE)
[![GitHub stars](https://img.shields.io/github/stars/TokenRollAI/engram)](https://github.com/TokenRollAI/engram/stargazers)

> 本地优先的语义记忆增强系统 - Local-first Semantic Memory Augmentation System

基于"记忆保留痕迹，痕迹提供价值"的核心理念，构建全天候、低延迟、高隐私的屏幕记忆系统。

## 功能特性

- **高频屏幕捕获**: 每 2 秒自动截图，感知哈希去重
- **OCR 文本提取**: PaddleOCR 本地推理 (Phase 2)
- **语义搜索**: 向量化检索 + 全文搜索 (Phase 2)
- **智能摘要**: LLM 自动生成工作日志 (Phase 3)
- **MCP 协议**: 与 Claude/Cursor 集成 (Phase 4)
- **隐私至上**: 所有数据本地处理，支持语义黑名单

## 技术栈

- **后端**: Rust + Tauri v2
- **前端**: SolidJS + TailwindCSS
- **数据库**: SQLite + sqlite-vec + FTS5
- **AI 推理**: ONNX Runtime + llama.cpp

## 开发

### 环境要求

- Rust 1.75+
- Node.js 20+
- 系统依赖见 [开发环境搭建](./llmdoc/guides/dev-setup.md)

### 快速开始

```bash
# 克隆仓库
git clone https://github.com/TokenRollAI/engram.git
cd engram

# 安装前端依赖
cd src-ui && npm install && cd ..

# 开发模式
cargo tauri dev

# 构建发布版
cargo tauri build
```

## 项目结构

```
engram/
├── src-tauri/           # Rust 后端
│   ├── src/
│   │   ├── daemon/      # 后台截图服务
│   │   ├── db/          # 数据库层
│   │   └── commands/    # Tauri API
│   └── Cargo.toml
├── src-ui/              # SolidJS 前端
│   ├── src/
│   │   ├── pages/       # 页面组件
│   │   └── components/  # UI 组件
│   └── package.json
└── llmdoc/              # 项目文档
    ├── overview/        # 项目概览
    ├── architecture/    # 系统架构
    ├── guides/          # 开发指南
    └── reference/       # 参考规范
```

## 文档

- [项目愿景](./llmdoc/overview/vision.md)
- [技术选型](./llmdoc/overview/tech-decisions.md)
- [系统架构](./llmdoc/architecture/system-overview.md)
- [开发路线图](./llmdoc/guides/roadmap.md)
- [任务分解](./llmdoc/guides/tasks.md)
- [GUI 设计规范](./llmdoc/reference/gui-spec.md)
- [变更日志](./llmdoc/reference/changelog.md)

## 开发阶段

| 阶段 | 名称 | 状态 |
|------|-----|------|
| Phase 1 | 全知之眼 (The Eye) | 🚧 进行中 (83%) |
| Phase 2 | 深度认知 (The Brain) | 📋 计划中 |
| Phase 3 | 记忆合成 (The Mind) | 📋 计划中 |
| Phase 4 | 生态扩展 (Ecosystem) | 📋 计划中 |

## 贡献

欢迎贡献代码！请查看 [开发指南](./llmdoc/guides/dev-setup.md) 了解如何开始。

## 许可证

[MIT License](./LICENSE)
