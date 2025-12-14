//! 数据库 Schema 初始化

use anyhow::Result;
use rusqlite::Connection;
use tracing::info;

/// 初始化数据库 Schema
pub fn init_schema(conn: &Connection) -> Result<()> {
    info!("Initializing database schema...");

    // 启用 WAL 模式
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;

    // 创建 traces 表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS traces (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            image_path TEXT NOT NULL,
            app_name TEXT,
            window_title TEXT,
            is_fullscreen INTEGER DEFAULT 0,
            window_x INTEGER,
            window_y INTEGER,
            window_w INTEGER,
            window_h INTEGER,
            is_idle INTEGER DEFAULT 0,
            ocr_text TEXT,
            ocr_json TEXT,
            embedding BLOB,
            phash BLOB,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_traces_timestamp ON traces(timestamp);
        CREATE INDEX IF NOT EXISTS idx_traces_app ON traces(app_name);
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
            ('summary_interval_min', '15');
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

    info!("Database schema initialized successfully");
    Ok(())
}
