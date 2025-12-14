# API 规范

## Tauri Commands (前后端通信)

### 截图控制

```typescript
// 暂停/恢复录制
invoke('toggle_capture', { paused: boolean }): Promise<void>

// 获取当前状态
invoke('get_capture_status'): Promise<{
  is_running: boolean
  is_paused: boolean
  is_idle: boolean
  last_capture_time: number | null
  total_captures_today: number
}>

// 强制立即截图
invoke('capture_now'): Promise<void>
```

### 数据查询

```typescript
// 按时间范围查询痕迹
invoke('get_traces', {
  start_time: number,  // Unix ms
  end_time: number,
  limit?: number,
  offset?: number,
}): Promise<Trace[]>

// 按时间范围查询活动 Session（对外主入口）
invoke('get_activity_sessions', {
  start_time: number,  // Unix ms
  end_time: number,
  limit?: number,
  offset?: number,
  app_filter?: string[] | null,
}): Promise<ActivitySession[]>

// 获取某个 Session 下的 traces（用于展开细节）
invoke('get_activity_session_traces', {
  session_id: number,
  limit?: number,
  offset?: number,
}): Promise<Trace[]>

// 获取某个 Session 的事件（每条 trace 的 VLM 结论）
// 获取截图图片数据（用于 UI 展示）
// 前端推荐用 Blob URL 渲染：URL.createObjectURL(new Blob([bytes], { type: mime }))
invoke('get_image_data', {
  relative_path: string,
}): Promise<{ mime: string, bytes: number[] }>

// 搜索痕迹
invoke('search_traces', {
  query: string,
  mode: 'keyword' | 'semantic' | 'hybrid',
  start_time?: number,
  end_time?: number,
  app_filter?: string[],
  limit?: number,
}): Promise<SearchResult[]>
```

### 摘要查询 (Phase 3)

```typescript
// 获取摘要列表
invoke('get_summaries', {
  start_time: number,
  end_time: number,
  type?: '15min' | '1hour' | '1day',
}): Promise<Summary[]>

// 手动触发摘要生成
invoke('generate_summary', {
  start_time: number,
  end_time: number,
}): Promise<Summary>
```

### 设置管理

```typescript
// 获取所有设置
invoke('get_settings'): Promise<Settings>

// 更新设置
invoke('update_settings', { settings: Partial<Settings> }): Promise<void>

// 获取黑名单
invoke('get_blacklist'): Promise<BlacklistRule[]>

// 添加黑名单规则
invoke('add_blacklist_rule', {
  rule_type: 'app' | 'title' | 'semantic',
  pattern: string,
}): Promise<number>  // 返回新规则 ID

// 删除黑名单规则
invoke('delete_blacklist_rule', { id: number }): Promise<void>
```

### 统计信息

```typescript
// 获取存储统计
invoke('get_storage_stats'): Promise<{
  total_traces: number
  total_summaries: number
  total_entities: number
  database_size_bytes: number
  screenshots_size_bytes: number
  oldest_trace_time: number | null
}>
```

---

## 数据类型定义

```typescript
interface Trace {
  id: number
  timestamp: number
  image_path: string | null
  app_name: string | null
  window_title: string | null
  is_fullscreen: boolean
  is_idle: boolean
  ocr_text: string | null
  activity_session_id: number | null
  is_key_action: boolean
  vlm_summary: string | null
  vlm_action_description: string | null
  vlm_activity_type: string | null
  vlm_confidence: number | null
  vlm_entities_json: string | null
  vlm_raw_json: string | null
  created_at: number
}

interface ActivitySession {
  id: number
  app_name: string
  title: string | null
  description: string | null
  start_time: number
  end_time: number
  start_trace_id: number | null
  end_trace_id: number | null
  trace_count: number
  context_text: string | null
  entities_json: string | null
  key_actions_json: string | null
  created_at: number
  updated_at: number
}

interface SearchResult {
  trace: Trace
  score: number
  highlights: TextHighlight[]
}

interface TextHighlight {
  text: string
  start: number
  end: number
}

interface Summary {
  id: number
  start_time: number
  end_time: number
  summary_type: '15min' | '1hour' | '1day'
  content: string
  topics: string[]
  entities: Entity[]
  links: string[]
  trace_count: number
  created_at: number
}

interface Entity {
  name: string
  type: 'Person' | 'Project' | 'Technology' | 'URL' | 'File'
}

interface Settings {
  capture_interval_ms: number
  idle_threshold_ms: number
  similarity_threshold: number
  hot_data_days: number
  warm_data_days: number
  summary_interval_min: number
  session_active_window_ms: number
  session_max_active_sessions: number
  session_similarity_threshold: number
  session_gap_threshold_ms: number
}

interface ChatRequest {
  message: string
  start_time: number | null
  end_time: number | null
  app_filter: string[] | null
  thread_id: number | null
}

interface ChatResponse {
  content: string
  context_count: number
  time_range: string | null
  thread_id: number
}

interface ChatMessage {
  id: number
  thread_id: number
  role: 'user' | 'assistant' | 'system'
  content: string
  context_json: string | null
  created_at: number
}

// 获取对话历史
invoke('get_chat_messages', {
  thread_id: number,
  limit?: number,
  offset?: number,
}): Promise<ChatMessage[]>

interface BlacklistRule {
  id: number
  rule_type: 'app' | 'title' | 'semantic'
  pattern: string
  enabled: boolean
  created_at: number
}

interface AppStat {
  app_name: string
  frame_count: number
  first_seen: number
  last_seen: number
  duration_seconds: number
}
```

---

## MCP 协议接口 (Phase 4)

### Tools (AI 主动调用)

#### search_memory

搜索用户的屏幕记忆。

**输入 Schema:**
```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "搜索查询词，支持自然语言"
    },
    "time_range": {
      "type": "string",
      "description": "时间范围，如 'today', 'yesterday', 'last_week', 'last_month' 或 ISO 日期范围"
    },
    "limit": {
      "type": "integer",
      "default": 10,
      "description": "返回结果数量上限"
    }
  },
  "required": ["query"]
}
```

**输出:**
```json
{
  "results": [
    {
      "timestamp": "2024-12-14T09:15:30Z",
      "app": "VS Code",
      "title": "main.rs - engram",
      "text_snippet": "impl ScreenCapture for Windows...",
      "relevance_score": 0.95
    }
  ],
  "total_count": 42
}
```

#### get_app_activity

获取特定应用的使用记录。

**输入 Schema:**
```json
{
  "type": "object",
  "properties": {
    "app_name": {
      "type": "string",
      "description": "应用名称 (支持模糊匹配)"
    },
    "time_range": {
      "type": "string",
      "description": "时间范围"
    }
  },
  "required": ["app_name"]
}
```

**输出:**
```json
{
  "app_name": "VS Code",
  "total_time_minutes": 180,
  "session_count": 5,
  "top_windows": [
    { "title": "main.rs - engram", "duration_minutes": 60 },
    { "title": "lib.rs - engram", "duration_minutes": 45 }
  ]
}
```

#### get_daily_summary

获取每日工作摘要。

**输入 Schema:**
```json
{
  "type": "object",
  "properties": {
    "date": {
      "type": "string",
      "description": "日期，格式 YYYY-MM-DD，默认今天"
    }
  }
}
```

**输出:**
```json
{
  "date": "2024-12-14",
  "summary": "今天主要在开发 Engram 项目的屏幕捕获模块...",
  "topics": ["Rust", "Engram", "屏幕捕获"],
  "entities": [
    { "name": "Alice", "type": "Person" },
    { "name": "scap", "type": "Technology" }
  ],
  "links": ["https://docs.rs/scap"],
  "app_breakdown": {
    "VS Code": 180,
    "Chrome": 60,
    "Terminal": 30
  }
}
```

### Resources (被动数据)

#### engram://screen/current

获取当前屏幕的 OCR 文本。

**URI:** `engram://screen/current`

**返回:**
```json
{
  "uri": "engram://screen/current",
  "mimeType": "text/plain",
  "text": "当前屏幕的 OCR 文本内容..."
}
```

#### engram://summary/daily

获取今日摘要。

**URI:** `engram://summary/daily`

**返回:**
```json
{
  "uri": "engram://summary/daily",
  "mimeType": "text/markdown",
  "text": "# 2024-12-14 工作日志\n\n## 上午\n..."
}
```

---

## 事件系统 (Tauri Events)

### 后端 → 前端

```typescript
// 截图完成
listen('capture-completed', (event: {
  trace_id: number
  timestamp: number
}) => void)

// 状态变化
listen('capture-status-changed', (event: {
  is_running: boolean
  is_paused: boolean
  is_idle: boolean
}) => void)

// 摘要生成完成
listen('summary-generated', (event: {
  summary_id: number
  summary_type: string
}) => void)

// 错误通知
listen('engram-error', (event: {
  code: string
  message: string
  details?: any
}) => void)
```

### 前端 → 后端

```typescript
// 请求立即截图
emit('request-capture')

// 请求暂停/恢复
emit('toggle-pause')
```

---

## 错误码

| 错误码 | 描述 | 处理建议 |
|-------|-----|---------|
| `E001` | 数据库连接失败 | 检查数据目录权限 |
| `E002` | 截图失败 | 检查屏幕录制权限 |
| `E003` | OCR 模型加载失败 | 检查模型文件完整性 |
| `E004` | 向量化失败 | 检查嵌入模型 |
| `E005` | LLM 服务不可用 | 检查 llama-server 进程 |
| `E006` | 存储空间不足 | 清理旧数据 |
| `E007` | MCP 连接失败 | 检查端口占用 |
