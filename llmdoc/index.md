# Engram - 本地优先的语义记忆增强系统

> 基于"记忆保留痕迹，痕迹提供价值"的核心理念，构建全天候、低延迟、高隐私的语义记忆增强系统。

## 项目状态

**当前阶段**: Phase 3 进行中 (The Mind - 记忆合成)

## 文档索引

### 概览 (Overview)
- [项目愿景与目标](./overview/vision.md) - 项目的核心理念与长期目标
- [核心需求规格](./overview/requirements.md) - 功能需求与非功能需求
- [技术选型决策](./overview/tech-decisions.md) - 关键技术选型及其理由

### 架构 (Architecture)
- [系统架构总览](./architecture/system-overview.md) - 四层架构设计
- [数据流设计](./architecture/data-flow.md) - 数据采集、处理、存储流程
- [数据库设计](./architecture/database.md) - SQLite Schema 与向量索引
- [AI 管道设计](./architecture/ai-pipeline.md) - VLM 视觉理解、文本嵌入、向量搜索流水线

### 开发指南 (Guides)
- [开发环境搭建](./guides/dev-setup.md) - 环境准备与依赖安装
- [实现路线图](./guides/roadmap.md) - 分阶段开发计划与里程碑
- [任务分解](./guides/tasks.md) - 具体开发任务清单

### 参考 (Reference)
- [变更日志](./reference/changelog.md) - 版本发布记录与重要变更
- [GUI 设计规范](./reference/gui-spec.md) - 用户界面设计规范
- [API 规范](./reference/api-spec.md) - MCP 协议接口定义
- [依赖清单](./reference/dependencies.md) - Rust Crate 与前端依赖

## 快速导航

| 我想了解... | 阅读文档 |
|------------|---------|
| 项目是什么、为什么要做 | [项目愿景](./overview/vision.md) |
| 系统如何工作 | [系统架构](./architecture/system-overview.md) |
| 如何开始开发 | [开发环境搭建](./guides/dev-setup.md) |
| 下一步做什么 | [实现路线图](./guides/roadmap.md) |
| UI 长什么样 | [GUI 设计规范](./reference/gui-spec.md) |
