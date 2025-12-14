# 数据库设计

## 数据库选型

**引擎**: SQLite 3.45+
**向量扩展**: sqlite-vec (M3.2 新增)
**全文检索**: FTS5 (内置)
**加密**: SQLCipher (可选)

## Schema 定义

```sql
-- ============================================
-- Engram Database Schema v2.0
-- ============================================

-- 启用外键约束
PRAGMA foreign_keys = ON;

-- 启用 WAL 模式提升并发性能
PRAGMA journal_mode = WAL;

-- ============================================
-- 活动会话表: activity_sessions (对外主视图)
-- ============================================
CREATE TABLE activity_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    app_name TEXT NOT NULL,
    title TEXT,
    description TEXT,
    start_time INTEGER NOT NULL,
    end_time INTEGER NOT NULL,

    start_trace_id INTEGER,
    end_trace_id INTEGER,
    trace_count INTEGER NOT NULL DEFAULT 0,

    -- 给 UI/Chat 用的“浓缩上下文”，由 VLM 分析结果增量追加、裁剪
    context_text TEXT,
    -- 聚合后的实体计数（JSON: { "entity": count, ... }）
    entities_json TEXT,
    -- 关键行为列表（JSON 数组）
    key_actions_json TEXT,

    created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
);

CREATE INDEX idx_activity_sessions_time ON activity_sessions(start_time, end_time);
CREATE INDEX idx_activity_sessions_app ON activity_sessions(app_name);

-- ============================================
-- 原子事实流: traces (痕迹记录)
-- ============================================
CREATE TABLE traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- 时间戳 (Unix 毫秒)
    timestamp INTEGER NOT NULL,

    -- 截图文件路径 (相对于数据目录)
    image_path TEXT NOT NULL,

    -- 窗口上下文
    app_name TEXT,
    window_title TEXT,
    is_fullscreen INTEGER DEFAULT 0,  -- 0/1 布尔

    -- 系统状态
    is_idle INTEGER DEFAULT 0,

    -- 轻量 OCR/文本（用于 FTS/Embedding/Search）
    ocr_text TEXT,

    -- 活动会话关联（对外主要暴露 session）
    activity_session_id INTEGER,

    -- 是否关键行为
    is_key_action INTEGER DEFAULT 0,

    -- VLM 结构化输出（用于回放/上下文构建）
    vlm_summary TEXT,
    vlm_action_description TEXT,
    vlm_activity_type TEXT,
    vlm_confidence REAL,
    vlm_entities_json TEXT,
    vlm_raw_json TEXT,

    -- 语义向量 (384 维 float32，以 BLOB 存储)
    embedding BLOB,

    -- 感知哈希 (用于去重)
    phash BLOB,

    -- 元数据
    created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),

    FOREIGN KEY (activity_session_id) REFERENCES activity_sessions(id) ON DELETE SET NULL
);

-- 时间索引 (最常用的查询条件)
CREATE INDEX idx_traces_timestamp ON traces(timestamp);

-- 应用名索引
CREATE INDEX idx_traces_app ON traces(app_name);

-- Session 索引
CREATE INDEX idx_traces_session ON traces(activity_session_id);

-- ============================================
-- 全文检索虚拟表
-- ============================================
CREATE VIRTUAL TABLE traces_fts USING fts5(
    ocr_text,
    window_title,
    content='traces',
    content_rowid='id',
    tokenize='unicode61'  -- 支持中文分词
);

-- 同步触发器: INSERT
CREATE TRIGGER traces_ai AFTER INSERT ON traces BEGIN
    INSERT INTO traces_fts(rowid, ocr_text, window_title)
    VALUES (new.id, new.ocr_text, new.window_title);
END;

-- 同步触发器: DELETE
CREATE TRIGGER traces_ad AFTER DELETE ON traces BEGIN
    INSERT INTO traces_fts(traces_fts, rowid, ocr_text, window_title)
    VALUES ('delete', old.id, old.ocr_text, old.window_title);
END;

-- 同步触发器: UPDATE
CREATE TRIGGER traces_au AFTER UPDATE ON traces BEGIN
    INSERT INTO traces_fts(traces_fts, rowid, ocr_text, window_title)
    VALUES ('delete', old.id, old.ocr_text, old.window_title);
    INSERT INTO traces_fts(rowid, ocr_text, window_title)
    VALUES (new.id, new.ocr_text, new.window_title);
END;

## 向量索引虚拟表 (sqlite-vec) - M3.2 新增
-- ============================================
CREATE VIRTUAL TABLE traces_vec USING vec0(
    trace_id INTEGER PRIMARY KEY,
    embedding float[384]
);

-- ============================================
-- 摘要表: summaries
-- ============================================
CREATE TABLE summaries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- 时间范围
    start_time INTEGER NOT NULL,
    end_time INTEGER NOT NULL,

    -- 摘要类型: '15min', '1hour', '1day'
    summary_type TEXT NOT NULL,

    -- 内容
    content TEXT NOT NULL,  -- Markdown 格式的摘要
    structured_data TEXT,   -- JSON: {topics, entities, links}

    -- 向量
    embedding BLOB,

    -- 元数据
    trace_count INTEGER,    -- 基于多少条 trace 生成
    created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
);

CREATE INDEX idx_summaries_time ON summaries(start_time, end_time);
CREATE INDEX idx_summaries_type ON summaries(summary_type);

-- ============================================
-- 实体表: entities (知识图谱节点)
-- ============================================
CREATE TABLE entities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- 实体信息
    name TEXT NOT NULL UNIQUE,
    type TEXT NOT NULL,  -- 'Person', 'Project', 'Technology', 'URL', 'File'

    -- 统计
    mention_count INTEGER DEFAULT 1,
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,

    -- 元数据 (JSON)
    metadata TEXT
);

CREATE INDEX idx_entities_type ON entities(type);
CREATE INDEX idx_entities_name ON entities(name);

-- ============================================
-- 实体关联表: entity_traces
-- ============================================
CREATE TABLE entity_traces (
    entity_id INTEGER NOT NULL,
    trace_id INTEGER NOT NULL,
    PRIMARY KEY (entity_id, trace_id),
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (trace_id) REFERENCES traces(id) ON DELETE CASCADE
);

-- ============================================
-- 配置表: settings
-- ============================================
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
);

-- 默认配置
INSERT INTO settings (key, value) VALUES
    ('capture_interval_ms', '2000'),
    ('idle_threshold_ms', '30000'),
    ('similarity_threshold', '5'),
    ('hot_data_days', '7'),
    ('warm_data_days', '30'),
    ('summary_interval_min', '15');

-- ============================================
-- 黑名单表: blacklist
-- ============================================
CREATE TABLE blacklist (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- 规则类型: 'app', 'title', 'semantic'
    rule_type TEXT NOT NULL,

    -- 规则内容
    pattern TEXT NOT NULL,  -- 正则表达式 或 语义描述

    -- 是否启用
    enabled INTEGER DEFAULT 1,

    created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
);

-- 默认黑名单
INSERT INTO blacklist (rule_type, pattern) VALUES
    ('app', '1Password'),
    ('app', 'Bitwarden'),
    ('app', 'KeePass'),
    ('title', 'Incognito'),
    ('title', 'Private Browsing'),
    ('title', 'InPrivate');
```

## 查询示例

### 1. 按时间范围查询

```sql
SELECT * FROM traces
WHERE timestamp BETWEEN :start AND :end
ORDER BY timestamp DESC
LIMIT 100;
```

### 2. 全文检索

```sql
SELECT t.* FROM traces t
JOIN traces_fts fts ON t.id = fts.rowid
WHERE traces_fts MATCH :query
ORDER BY rank
LIMIT 20;
```

### 3. 向量相似度搜索 (M3.2 优化)

**之前** (应用层暴力搜索):
```rust
// O(n) 复杂度，逐条计算余弦相似度
let results = traces
    .iter()
    .map(|t| (t, cosine_similarity(&t.embedding, &query)))
    .sorted_by_key(|&(_, sim)| -sim)
    .take(20)
    .collect()
```

**现在** (sqlite-vec KNN 搜索):
```sql
-- 使用 sqlite-vec SIMD 加速搜索
SELECT
    t.*,
    distance
FROM traces_vec vec
JOIN traces t ON vec.trace_id = t.id
WHERE vec.embedding MATCH :query_embedding
    AND k = 20
ORDER BY distance;
```

**性能对比**:
| 数据规模 | 应用层 (ms) | sqlite-vec (ms) | 加速 |
|---------|-----------|-----------------|------|
| 1,000 | 50 | 5 | 10x |
| 10,000 | 500 | 10 | 50x |
| 100,000 | 5000 | 50 | 100x |

**SIMD 加速原理**:
- sqlite-vec 使用 CPU SIMD 指令集 (AVX-512, AVX2, NEON) 并行计算
- 向量点积运算从逐个计算改为批量计算
- 内存访问优化，缓存局部性更好

### 4. 混合搜索 (RRF 融合)

```sql
WITH
-- 全文检索结果
fts_results AS (
    SELECT rowid as id, rank() OVER () as fts_rank
    FROM traces_fts WHERE traces_fts MATCH :query
    LIMIT 50
),
-- 向量检索结果
vec_results AS (
    SELECT trace_id as id, row_number() OVER (ORDER BY distance) as vec_rank
    FROM traces_vec
    WHERE embedding MATCH :embedding AND k = 50
),
-- RRF 融合
rrf_scores AS (
    SELECT
        COALESCE(f.id, v.id) as id,
        COALESCE(1.0 / (60 + f.fts_rank), 0) +
        COALESCE(1.0 / (60 + v.vec_rank), 0) as score
    FROM fts_results f
    FULL OUTER JOIN vec_results v ON f.id = v.id
)
SELECT t.* FROM rrf_scores r
JOIN traces t ON r.id = t.id
ORDER BY r.score DESC
LIMIT 20;
```

### 5. 应用使用统计

```sql
SELECT
    app_name,
    COUNT(*) as frame_count,
    MIN(timestamp) as first_seen,
    MAX(timestamp) as last_seen,
    (MAX(timestamp) - MIN(timestamp)) / 1000 / 60 as duration_minutes
FROM traces
WHERE timestamp > :since
GROUP BY app_name
ORDER BY frame_count DESC;
```

## 数据迁移策略

### 热 → 温迁移 (7天后)

```sql
-- 删除截图文件，保留数据库记录
UPDATE traces
SET image_path = NULL
WHERE timestamp < (strftime('%s', 'now') - 7 * 86400) * 1000
  AND image_path IS NOT NULL;
```

### 温 → 冷迁移 (30天后)

```sql
-- 删除 OCR 详细数据和向量，仅保留元数据
DELETE FROM traces_vec
WHERE trace_id IN (
    SELECT id FROM traces
    WHERE timestamp < (strftime('%s', 'now') - 30 * 86400) * 1000
);

UPDATE traces
SET embedding = NULL
WHERE timestamp < (strftime('%s', 'now') - 30 * 86400) * 1000;

-- 可选：清理 traces 的 vlm_raw_json（保留聚合后的 session 结论即可）
UPDATE traces
SET vlm_raw_json = NULL
WHERE timestamp < (strftime('%s', 'now') - 30 * 86400) * 1000;
```

## 存储估算

| 组件 | 单条大小 | 日均条数 | 日均存储 |
|------|---------|---------|---------|
| traces 行 | ~2KB | 14,400 | ~28MB |
| WebP 截图 | ~80KB | 5,000 (去重后) | ~400MB |
| 向量索引 | 1.5KB | 14,400 | ~21MB |
| FTS 索引 | ~500B | 14,400 | ~7MB |
| **合计** | - | - | **~460MB/天** |

> 假设 8 小时工作，60% 去重率
