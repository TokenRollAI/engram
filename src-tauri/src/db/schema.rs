//! 数据库 Schema 初始化

use anyhow::Result;
use rusqlite::Connection;
use tracing::info;

const SCHEMA_VERSION: i32 = 4;

/// 初始化数据库 Schema
pub fn init_schema(conn: &Connection) -> Result<()> {
    info!("Initializing database schema...");

    // 启用 WAL 模式
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;

    let current_version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    // 当前阶段无用户数据，直接采用“版本不一致则重建”的策略，避免复杂迁移。
    if current_version != SCHEMA_VERSION {
        info!(
            "Schema version mismatch (current={}, expected={}), rebuilding schema...",
            current_version, SCHEMA_VERSION
        );

        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = OFF;

            DROP TRIGGER IF EXISTS traces_ai;
            DROP TRIGGER IF EXISTS traces_ad;
            DROP TRIGGER IF EXISTS traces_au;

            DROP TABLE IF EXISTS traces_fts;
            DROP TABLE IF EXISTS traces_vec;
            DROP TABLE IF EXISTS traces;

            DROP TABLE IF EXISTS activity_sessions;

            DROP TABLE IF EXISTS chat_messages;
            DROP TABLE IF EXISTS chat_threads;

            DROP TABLE IF EXISTS summaries;
            DROP TABLE IF EXISTS entity_traces;
            DROP TABLE IF EXISTS entities;

            DROP TABLE IF EXISTS settings;
            DROP TABLE IF EXISTS blacklist;

            PRAGMA foreign_keys = ON;
            "#,
        )?;
    }

    // 启用外键
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    // 活动会话表（用户行为 Session：按 app + 时间连续性聚合）
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS activity_sessions (
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
            -- 关键行为列表（JSON 数组，面向对外展示）
            key_actions_json TEXT,

            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),
            updated_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_activity_sessions_time ON activity_sessions(start_time, end_time);
        CREATE INDEX IF NOT EXISTS idx_activity_sessions_app ON activity_sessions(app_name);
        "#,
    )?;

    // 核心：traces（原子事实流）
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS traces (
            id INTEGER PRIMARY KEY AUTOINCREMENT,

            -- 时间戳 (Unix 毫秒)
            timestamp INTEGER NOT NULL,

            -- 截图文件路径 (相对于数据目录)
            image_path TEXT NOT NULL,

            -- 窗口上下文
            app_name TEXT,
            window_title TEXT,
            is_fullscreen INTEGER DEFAULT 0,

            -- 系统状态
            is_idle INTEGER DEFAULT 0,

            -- 轻量 OCR/文本（用于 FTS/Embedding/Search）
            ocr_text TEXT,

            -- 活动会话关联（对外主要暴露 session）
            activity_session_id INTEGER,

            -- 是否关键行为（由 VLM 分析阶段决定）
            is_key_action INTEGER DEFAULT 0,

            -- VLM 结构化输出（用于回放/上下文构建）
            vlm_summary TEXT,
            vlm_action_description TEXT,
            vlm_activity_type TEXT,
            vlm_confidence REAL,
            vlm_entities_json TEXT,
            vlm_raw_json TEXT,

            -- 向量
            embedding BLOB,

            -- 感知哈希（用于去重）
            phash BLOB,

            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),

            FOREIGN KEY (activity_session_id) REFERENCES activity_sessions(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_traces_timestamp ON traces(timestamp);
        CREATE INDEX IF NOT EXISTS idx_traces_app ON traces(app_name);
        CREATE INDEX IF NOT EXISTS idx_traces_session ON traces(activity_session_id);
        "#,
    )?;

    // 创建 FTS5 全文索引
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS traces_fts USING fts5(
            ocr_text,
            window_title,
            content='traces',
            content_rowid='id',
            tokenize='unicode61'
        );
        "#,
    )?;

    // vec0 向量索引表由 Database::ensure_vec_table() 按需创建
    // 支持任意维度的 embedding 模型（如 384、768、1024、2048 等）
    // 首次插入向量时会自动检测维度并创建表

    // 创建同步触发器
    conn.execute_batch(
        r#"
        CREATE TRIGGER IF NOT EXISTS traces_ai AFTER INSERT ON traces BEGIN
            INSERT INTO traces_fts(rowid, ocr_text, window_title)
            VALUES (new.id, new.ocr_text, new.window_title);
        END;

        CREATE TRIGGER IF NOT EXISTS traces_ad AFTER DELETE ON traces BEGIN
            INSERT INTO traces_fts(traces_fts, rowid, ocr_text, window_title)
            VALUES ('delete', old.id, old.ocr_text, old.window_title);
        END;

        CREATE TRIGGER IF NOT EXISTS traces_au AFTER UPDATE ON traces BEGIN
            INSERT INTO traces_fts(traces_fts, rowid, ocr_text, window_title)
            VALUES ('delete', old.id, old.ocr_text, old.window_title);
            INSERT INTO traces_fts(rowid, ocr_text, window_title)
            VALUES (new.id, new.ocr_text, new.window_title);
        END;
        "#,
    )?;

    // 创建 summaries 表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS summaries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            summary_type TEXT NOT NULL,
            content TEXT NOT NULL,
            structured_data TEXT,
            embedding BLOB,
            trace_count INTEGER,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_summaries_time ON summaries(start_time, end_time);
        CREATE INDEX IF NOT EXISTS idx_summaries_type ON summaries(summary_type);
        "#,
    )?;

    // 创建 entities 表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS entities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            type TEXT NOT NULL,
            mention_count INTEGER DEFAULT 1,
            first_seen INTEGER NOT NULL,
            last_seen INTEGER NOT NULL,
            metadata TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type);
        CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);
        "#,
    )?;

    // 创建 entity_traces 关联表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS entity_traces (
            entity_id INTEGER NOT NULL,
            trace_id INTEGER NOT NULL,
            PRIMARY KEY (entity_id, trace_id),
            FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE,
            FOREIGN KEY (trace_id) REFERENCES traces(id) ON DELETE CASCADE
        );
        "#,
    )?;

    // 创建 settings 表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );
        "#,
    )?;

    // 插入默认设置
    conn.execute_batch(
        r#"
        INSERT OR IGNORE INTO settings (key, value) VALUES
            ('capture_interval_ms', '2000'),
            ('idle_threshold_ms', '30000'),
            ('similarity_threshold', '5'),
            ('hot_data_days', '7'),
            ('warm_data_days', '30'),
            ('summary_interval_min', '15'),
            ('session_gap_threshold_ms', '300000');
        "#,
    )?;

    // 创建 blacklist 表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS blacklist (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_type TEXT NOT NULL,
            pattern TEXT NOT NULL,
            enabled INTEGER DEFAULT 1,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),
            UNIQUE(rule_type, pattern)
        );
        "#,
    )?;

    // 插入默认黑名单（使用 INSERT OR IGNORE 配合 UNIQUE 约束）
    conn.execute_batch(
        r#"
        INSERT OR IGNORE INTO blacklist (rule_type, pattern) VALUES
            ('app', '1Password'),
            ('app', 'Bitwarden'),
            ('app', 'KeePass'),
            ('app', 'KeePassXC'),
            ('title', 'Incognito'),
            ('title', 'Private Browsing'),
            ('title', 'InPrivate');
        "#,
    )?;

    // 迁移：清理已有的重复 blacklist 数据
    // 保留每个 (rule_type, pattern) 组合中 id 最小的记录
    conn.execute_batch(
        r#"
        DELETE FROM blacklist
        WHERE id NOT IN (
            SELECT MIN(id)
            FROM blacklist
            GROUP BY rule_type, pattern
        );
        "#,
    )?;

    // Chat：对话线程与消息（与“活动 Session”概念区分）
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS chat_threads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),
            updated_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE TABLE IF NOT EXISTS chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            thread_id INTEGER NOT NULL,
            role TEXT NOT NULL, -- 'user' | 'assistant' | 'system'
            content TEXT NOT NULL,
            -- JSON: { session_ids: [...], trace_ids: [...], time_range: {...} }
            context_json TEXT,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),

            FOREIGN KEY (thread_id) REFERENCES chat_threads(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_chat_messages_thread_time ON chat_messages(thread_id, created_at);
        "#,
    )?;

    conn.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION))?;

    info!("Database schema initialized successfully");
    Ok(())
}
