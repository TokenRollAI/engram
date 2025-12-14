//! 数据库模块
//!
//! 使用 SQLite 存储痕迹数据、摘要和设置。
//! 使用 sqlite-vec 扩展进行向量搜索。

pub mod models;
mod schema;

use anyhow::Result;
use chrono::{Datelike, Utc};
use directories::ProjectDirs;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{debug, info};

pub use models::*;

/// 数据库管理器
pub struct Database {
    conn: Mutex<Connection>,
    data_dir: PathBuf,
}

impl Database {
    /// 创建或打开数据库
    pub fn new() -> Result<Self> {
        // 注册 sqlite-vec 扩展（必须在打开任何连接之前）
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let data_dir = Self::resolve_data_dir()?;
        fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("engram.db");
        info!("Opening database at: {:?}", db_path);

        let conn = Connection::open(&db_path)?;

        // 验证 sqlite-vec 是否加载成功
        let vec_version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        info!("sqlite-vec extension loaded: v{}", vec_version);

        // 初始化 Schema
        schema::init_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            data_dir,
        })
    }

    /// 获取数据目录（静态方法）
    fn resolve_data_dir() -> Result<PathBuf> {
        if let Some(proj_dirs) = ProjectDirs::from("com", "engram", "Engram") {
            Ok(proj_dirs.data_dir().to_path_buf())
        } else {
            // 回退到用户目录
            let home =
                dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
            Ok(home.join(".engram"))
        }
    }

    /// 保存截图文件（JPEG 格式）
    pub fn save_screenshot(&self, pixels: &[u8], width: u32, height: u32) -> Result<String> {
        use image::codecs::jpeg::JpegEncoder;
        use std::io::BufWriter;

        let now = Utc::now();
        let dir = self
            .data_dir
            .join("screenshots")
            .join(now.year().to_string())
            .join(format!("{:02}", now.month()))
            .join(format!("{:02}", now.day()));

        fs::create_dir_all(&dir)?;

        let filename = format!("{}.jpg", now.timestamp_millis());
        let path = dir.join(&filename);

        // 创建图像 (RGBA -> RGB)
        let rgba_img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Failed to create image from pixels"))?;

        // 转换为 RGB (去除 alpha 通道)
        let rgb_img = image::DynamicImage::ImageRgba8(rgba_img).to_rgb8();

        // 保存为 JPEG 格式（质量 80%）
        let file = fs::File::create(&path)?;
        let writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(writer, 80);

        encoder.encode(
            rgb_img.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )?;

        // 返回相对路径
        let relative_path = format!(
            "screenshots/{}/{:02}/{:02}/{}",
            now.year(),
            now.month(),
            now.day(),
            filename
        );

        debug!("Screenshot saved as JPEG: {}", relative_path);
        Ok(relative_path)
    }

    /// 插入痕迹记录
    pub fn insert_trace(&self, trace: &NewTrace) -> Result<(i64, Option<i64>)> {
        let mut conn = self.conn.lock().unwrap();
        let session_id = self.get_or_create_activity_session_id_inner(
            &conn,
            trace.timestamp,
            trace.app_name.as_deref(),
            trace.is_idle,
        )?;

        let tx = conn.transaction()?;

        tx.execute(
            r#"
            INSERT INTO traces (
                timestamp, image_path, app_name, window_title,
                is_fullscreen, window_x, window_y, window_w, window_h,
                is_idle, ocr_text, activity_session_id, phash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            rusqlite::params![
                trace.timestamp,
                trace.image_path,
                trace.app_name,
                trace.window_title,
                trace.is_fullscreen,
                trace.window_x,
                trace.window_y,
                trace.window_w,
                trace.window_h,
                trace.is_idle,
                trace.ocr_text,
                session_id,
                trace.phash,
            ],
        )?;

        let trace_id = tx.last_insert_rowid();

        if let Some(sid) = session_id {
            // 更新 session 的起止 trace 与时间、计数
            tx.execute(
                r#"
                UPDATE activity_sessions
                SET
                    start_trace_id = COALESCE(start_trace_id, ?1),
                    end_trace_id = ?1,
                    start_time = MIN(start_time, ?2),
                    end_time = MAX(end_time, ?2),
                    trace_count = trace_count + 1,
                    updated_at = (strftime('%s', 'now') * 1000)
                WHERE id = ?3
                "#,
                rusqlite::params![trace_id, trace.timestamp, sid],
            )?;
        }

        tx.commit()?;

        Ok((trace_id, session_id))
    }

    fn get_or_create_activity_session_id_inner(
        &self,
        conn: &Connection,
        timestamp: i64,
        app_name: Option<&str>,
        is_idle: bool,
    ) -> Result<Option<i64>> {
        if is_idle {
            return Ok(None);
        }

        let app_name = match app_name {
            Some(a) if !a.trim().is_empty() => a,
            _ => return Ok(None),
        };

        let gap_ms: i64 = self
            .get_setting_inner(conn, "session_gap_threshold_ms")?
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(300_000);

        // 找到该 app 最近的 session
        let last: Option<(i64, i64)> = conn
            .query_row(
                r#"
                SELECT id, end_time
                FROM activity_sessions
                WHERE app_name = ?1
                ORDER BY end_time DESC
                LIMIT 1
                "#,
                rusqlite::params![app_name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((sid, last_end)) = last {
            if timestamp - last_end <= gap_ms {
                return Ok(Some(sid));
            }
        }

        // 新建 session
        conn.execute(
            r#"
            INSERT INTO activity_sessions (app_name, start_time, end_time, trace_count)
            VALUES (?1, ?2, ?2, 0)
            "#,
            rusqlite::params![app_name, timestamp],
        )?;
        Ok(Some(conn.last_insert_rowid()))
    }

    fn get_setting_inner(&self, conn: &Connection, key: &str) -> Result<Option<String>> {
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 按时间范围查询痕迹
    pub fn get_traces(
        &self,
        start_time: i64,
        end_time: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, activity_session_id, is_key_action, created_at
            FROM traces
            WHERE timestamp BETWEEN ?1 AND ?2
            ORDER BY timestamp DESC
            LIMIT ?3 OFFSET ?4
            "#,
        )?;

        let traces = stmt.query_map(
            rusqlite::params![start_time, end_time, limit, offset],
            |row| {
                Ok(Trace {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    image_path: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    is_fullscreen: row.get(5)?,
                    window_x: row.get(6)?,
                    window_y: row.get(7)?,
                    window_w: row.get(8)?,
                    window_h: row.get(9)?,
                    is_idle: row.get(10)?,
                    ocr_text: row.get(11)?,
                    activity_session_id: row.get(12)?,
                    is_key_action: row.get(13)?,
                    created_at: row.get(14)?,
                })
            },
        )?;

        let mut result = Vec::new();
        for trace in traces {
            result.push(trace?);
        }

        Ok(result)
    }

    /// 按时间范围和应用过滤查询痕迹
    pub fn get_traces_filtered(
        &self,
        start_time: i64,
        end_time: i64,
        app_filter: Option<&Vec<String>>,
        limit: u32,
    ) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();

        // 构建动态 SQL
        let (sql, has_filter) = if let Some(apps) = app_filter {
            if apps.is_empty() {
                (
                    r#"
                    SELECT id, timestamp, image_path, app_name, window_title,
                           is_fullscreen, window_x, window_y, window_w, window_h,
                           is_idle, ocr_text, activity_session_id, is_key_action, created_at
                    FROM traces
                    WHERE timestamp BETWEEN ?1 AND ?2
                    ORDER BY timestamp DESC
                    LIMIT ?3
                    "#
                    .to_string(),
                    false,
                )
            } else {
                // 构建 IN 子句的占位符
                let placeholders: Vec<String> =
                    (0..apps.len()).map(|i| format!("?{}", i + 4)).collect();
                (
                    format!(
                        r#"
                        SELECT id, timestamp, image_path, app_name, window_title,
                               is_fullscreen, window_x, window_y, window_w, window_h,
                               is_idle, ocr_text, activity_session_id, is_key_action, created_at
                        FROM traces
                        WHERE timestamp BETWEEN ?1 AND ?2
                          AND app_name IN ({})
                        ORDER BY timestamp DESC
                        LIMIT ?3
                        "#,
                        placeholders.join(", ")
                    ),
                    true,
                )
            }
        } else {
            (
                r#"
                SELECT id, timestamp, image_path, app_name, window_title,
                       is_fullscreen, window_x, window_y, window_w, window_h,
                       is_idle, ocr_text, activity_session_id, is_key_action, created_at
                FROM traces
                WHERE timestamp BETWEEN ?1 AND ?2
                ORDER BY timestamp DESC
                LIMIT ?3
                "#
                .to_string(),
                false,
            )
        };

        let mut stmt = conn.prepare(&sql)?;

        // 构建参数
        let traces = if has_filter {
            let apps = app_filter.unwrap();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> =
                vec![Box::new(start_time), Box::new(end_time), Box::new(limit)];
            for app in apps {
                params.push(Box::new(app.clone()));
            }
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            stmt.query_map(params_refs.as_slice(), |row| {
                Ok(Trace {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    image_path: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    is_fullscreen: row.get(5)?,
                    window_x: row.get(6)?,
                    window_y: row.get(7)?,
                    window_w: row.get(8)?,
                    window_h: row.get(9)?,
                    is_idle: row.get(10)?,
                    ocr_text: row.get(11)?,
                    activity_session_id: row.get(12)?,
                    is_key_action: row.get(13)?,
                    created_at: row.get(14)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(rusqlite::params![start_time, end_time, limit], |row| {
                Ok(Trace {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    image_path: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    is_fullscreen: row.get(5)?,
                    window_x: row.get(6)?,
                    window_y: row.get(7)?,
                    window_w: row.get(8)?,
                    window_h: row.get(9)?,
                    is_idle: row.get(10)?,
                    ocr_text: row.get(11)?,
                    activity_session_id: row.get(12)?,
                    is_key_action: row.get(13)?,
                    created_at: row.get(14)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok(traces)
    }

    /// 全文搜索
    pub fn search_text(&self, query: &str, limit: u32) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT t.id, t.timestamp, t.image_path, t.app_name, t.window_title,
                   t.is_fullscreen, t.window_x, t.window_y, t.window_w, t.window_h,
                   t.is_idle, t.ocr_text, t.activity_session_id, t.is_key_action, t.created_at
            FROM traces t
            JOIN traces_fts fts ON t.id = fts.rowid
            WHERE traces_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
        )?;

        let traces = stmt.query_map(rusqlite::params![query, limit], |row| {
            Ok(Trace {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                image_path: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                is_fullscreen: row.get(5)?,
                window_x: row.get(6)?,
                window_y: row.get(7)?,
                window_w: row.get(8)?,
                window_h: row.get(9)?,
                is_idle: row.get(10)?,
                ocr_text: row.get(11)?,
                activity_session_id: row.get(12)?,
                is_key_action: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?;

        let mut result = Vec::new();
        for trace in traces {
            result.push(trace?);
        }

        Ok(result)
    }

    /// 获取存储统计
    pub fn get_storage_stats(&self) -> Result<StorageStats> {
        let conn = self.conn.lock().unwrap();

        let total_traces: i64 =
            conn.query_row("SELECT COUNT(*) FROM traces", [], |row| row.get(0))?;

        let total_summaries: i64 = conn
            .query_row("SELECT COUNT(*) FROM summaries", [], |row| row.get(0))
            .unwrap_or(0);

        let total_entities: i64 = conn
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .unwrap_or(0);

        let oldest_trace_time: Option<i64> = conn
            .query_row("SELECT MIN(timestamp) FROM traces", [], |row| row.get(0))
            .ok();

        // 计算数据库大小
        let db_path = self.data_dir.join("engram.db");
        let database_size_bytes = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

        // 计算截图目录大小
        let screenshots_dir = self.data_dir.join("screenshots");
        let screenshots_size_bytes = Self::dir_size(&screenshots_dir);

        Ok(StorageStats {
            total_traces: total_traces as u64,
            total_summaries: total_summaries as u64,
            total_entities: total_entities as u64,
            database_size_bytes,
            screenshots_size_bytes,
            oldest_trace_time,
        })
    }

    /// 计算目录大小
    fn dir_size(path: &PathBuf) -> u64 {
        if !path.exists() {
            return 0;
        }

        let mut size = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    size += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                } else if path.is_dir() {
                    size += Self::dir_size(&path);
                }
            }
        }
        size
    }

    /// 获取时间范围内的不同应用名称
    pub fn get_distinct_apps(&self, start_time: i64, end_time: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT app_name FROM traces
             WHERE timestamp >= ?1 AND timestamp <= ?2 AND app_name IS NOT NULL
             ORDER BY app_name",
        )?;

        let apps = stmt.query_map(rusqlite::params![start_time, end_time], |row| {
            row.get::<_, String>(0)
        })?;

        let mut result = Vec::new();
        for app in apps {
            result.push(app?);
        }

        Ok(result)
    }

    /// 获取活动会话列表（对外主入口）
    pub fn get_activity_sessions(
        &self,
        start_time: i64,
        end_time: i64,
        app_filter: Option<&Vec<String>>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ActivitySession>> {
        let conn = self.conn.lock().unwrap();

        let (sql, has_filter) = if let Some(apps) = app_filter {
            if apps.is_empty() {
                (
                    r#"
                    SELECT
                        id, app_name, start_time, end_time,
                        start_trace_id, end_trace_id, trace_count,
                        context_text, entities_json, key_actions_json,
                        created_at, updated_at
                    FROM activity_sessions
                    WHERE end_time BETWEEN ?1 AND ?2
                    ORDER BY end_time DESC
                    LIMIT ?3 OFFSET ?4
                    "#
                    .to_string(),
                    false,
                )
            } else {
                let placeholders: Vec<String> =
                    (0..apps.len()).map(|i| format!("?{}", i + 5)).collect();
                (
                    format!(
                        r#"
                        SELECT
                            id, app_name, start_time, end_time,
                            start_trace_id, end_trace_id, trace_count,
                            context_text, entities_json, key_actions_json,
                            created_at, updated_at
                        FROM activity_sessions
                        WHERE end_time BETWEEN ?1 AND ?2
                          AND app_name IN ({})
                        ORDER BY end_time DESC
                        LIMIT ?3 OFFSET ?4
                        "#,
                        placeholders.join(", ")
                    ),
                    true,
                )
            }
        } else {
            (
                r#"
                SELECT
                    id, app_name, start_time, end_time,
                    start_trace_id, end_trace_id, trace_count,
                    context_text, entities_json, key_actions_json,
                    created_at, updated_at
                FROM activity_sessions
                WHERE end_time BETWEEN ?1 AND ?2
                ORDER BY end_time DESC
                LIMIT ?3 OFFSET ?4
                "#
                .to_string(),
                false,
            )
        };

        let mut stmt = conn.prepare(&sql)?;

        let sessions = if has_filter {
            let apps = app_filter.unwrap();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(start_time),
                Box::new(end_time),
                Box::new(limit),
                Box::new(offset),
            ];
            for app in apps {
                params.push(Box::new(app.clone()));
            }
            let params_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            stmt.query_map(params_refs.as_slice(), |row| {
                Ok(ActivitySession {
                    id: row.get(0)?,
                    app_name: row.get(1)?,
                    start_time: row.get(2)?,
                    end_time: row.get(3)?,
                    start_trace_id: row.get(4)?,
                    end_trace_id: row.get(5)?,
                    trace_count: row.get::<_, i64>(6)? as u32,
                    context_text: row.get(7)?,
                    entities_json: row.get(8)?,
                    key_actions_json: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(
                rusqlite::params![start_time, end_time, limit, offset],
                |row| {
                    Ok(ActivitySession {
                        id: row.get(0)?,
                        app_name: row.get(1)?,
                        start_time: row.get(2)?,
                        end_time: row.get(3)?,
                        start_trace_id: row.get(4)?,
                        end_trace_id: row.get(5)?,
                        trace_count: row.get::<_, i64>(6)? as u32,
                        context_text: row.get(7)?,
                        entities_json: row.get(8)?,
                        key_actions_json: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok(sessions)
    }

    pub fn get_activity_session_by_id(&self, id: i64) -> Result<Option<ActivitySession>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            r#"
            SELECT
                id, app_name, start_time, end_time,
                start_trace_id, end_trace_id, trace_count,
                context_text, entities_json, key_actions_json,
                created_at, updated_at
            FROM activity_sessions
            WHERE id = ?1
            "#,
            rusqlite::params![id],
            |row| {
                Ok(ActivitySession {
                    id: row.get(0)?,
                    app_name: row.get(1)?,
                    start_time: row.get(2)?,
                    end_time: row.get(3)?,
                    start_trace_id: row.get(4)?,
                    end_trace_id: row.get(5)?,
                    trace_count: row.get::<_, i64>(6)? as u32,
                    context_text: row.get(7)?,
                    entities_json: row.get(8)?,
                    key_actions_json: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            },
        );

        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_traces_by_activity_session(
        &self,
        session_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, activity_session_id, is_key_action, created_at
            FROM traces
            WHERE activity_session_id = ?1
            ORDER BY timestamp DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let traces = stmt.query_map(rusqlite::params![session_id, limit, offset], |row| {
            Ok(Trace {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                image_path: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                is_fullscreen: row.get(5)?,
                window_x: row.get(6)?,
                window_y: row.get(7)?,
                window_w: row.get(8)?,
                window_h: row.get(9)?,
                is_idle: row.get(10)?,
                ocr_text: row.get(11)?,
                activity_session_id: row.get(12)?,
                is_key_action: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?;

        let mut result = Vec::new();
        for t in traces {
            result.push(t?);
        }
        Ok(result)
    }

    pub fn get_recent_traces_in_session_before(
        &self,
        session_id: i64,
        before_timestamp: i64,
        limit: u32,
    ) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, activity_session_id, is_key_action, created_at
            FROM traces
            WHERE activity_session_id = ?1
              AND timestamp < ?2
            ORDER BY timestamp DESC
            LIMIT ?3
            "#,
        )?;

        let traces = stmt.query_map(
            rusqlite::params![session_id, before_timestamp, limit],
            |row| {
                Ok(Trace {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    image_path: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    is_fullscreen: row.get(5)?,
                    window_x: row.get(6)?,
                    window_y: row.get(7)?,
                    window_w: row.get(8)?,
                    window_h: row.get(9)?,
                    is_idle: row.get(10)?,
                    ocr_text: row.get(11)?,
                    activity_session_id: row.get(12)?,
                    is_key_action: row.get(13)?,
                    created_at: row.get(14)?,
                })
            },
        )?;

        let mut result = Vec::new();
        for t in traces {
            result.push(t?);
        }
        Ok(result)
    }

    pub fn get_activity_session_events(
        &self,
        session_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ActivitySessionEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT
                id, session_id, trace_id, timestamp,
                summary, action_description, activity_type, confidence, entities_json, is_key_action,
                created_at
            FROM activity_session_events
            WHERE session_id = ?1
            ORDER BY timestamp DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let rows = stmt.query_map(rusqlite::params![session_id, limit, offset], |row| {
            Ok(ActivitySessionEvent {
                id: row.get(0)?,
                session_id: row.get(1)?,
                trace_id: row.get(2)?,
                timestamp: row.get(3)?,
                summary: row.get(4)?,
                action_description: row.get(5)?,
                activity_type: row.get(6)?,
                confidence: row.get(7)?,
                entities_json: row.get(8)?,
                is_key_action: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    pub fn append_activity_session_event(
        &self,
        session_id: i64,
        trace_id: i64,
        timestamp: i64,
        summary: Option<&str>,
        action_description: Option<&str>,
        activity_type: Option<&str>,
        confidence: Option<f32>,
        entities: &[String],
        raw_json: Option<&str>,
        is_key_action: bool,
    ) -> Result<()> {
        use chrono::{DateTime, Local};
        use serde_json::{Map, Value};

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let entities_json = if entities.is_empty() {
            None
        } else {
            Some(serde_json::to_string(entities).unwrap_or_else(|_| "[]".to_string()))
        };

        tx.execute(
            r#"
            INSERT INTO activity_session_events (
                session_id, trace_id, timestamp,
                summary, action_description, activity_type, confidence,
                entities_json, raw_json, is_key_action
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            rusqlite::params![
                session_id,
                trace_id,
                timestamp,
                summary,
                action_description,
                activity_type,
                confidence,
                entities_json,
                raw_json,
                is_key_action,
            ],
        )?;

        tx.execute(
            "UPDATE traces SET is_key_action = ?1 WHERE id = ?2",
            rusqlite::params![is_key_action, trace_id],
        )?;

        // 读取并合并 entities_json（计数）
        let existing_entities_json: Option<String> = tx
            .query_row(
                "SELECT entities_json FROM activity_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .ok();

        let mut counts: Map<String, Value> = existing_entities_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok())
            .unwrap_or_default();

        for e in entities {
            if e.trim().is_empty() {
                continue;
            }
            let current = counts.get(e).and_then(|v| v.as_i64()).unwrap_or(0);
            counts.insert(e.clone(), Value::from(current + 1));
        }

        let merged_entities_json = if counts.is_empty() {
            None
        } else {
            Some(Value::Object(counts).to_string())
        };

        // 追加 context_text（裁剪到一定长度，防止无限增长）
        let existing_context: Option<String> = tx
            .query_row(
                "SELECT context_text FROM activity_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .ok();

        let existing_key_actions: Option<String> = tx
            .query_row(
                "SELECT key_actions_json FROM activity_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .ok();

        let time_str = DateTime::from_timestamp_millis(timestamp)
            .map(|t| t.with_timezone(&Local).format("%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "?".to_string());

        let mut appended = existing_context.unwrap_or_default();
        if !appended.is_empty() && !appended.ends_with('\n') {
            appended.push('\n');
        }
        if let Some(s) = summary {
            if !s.trim().is_empty() {
                appended.push_str(&format!("[{}] {}\n", time_str, s.trim()));
            }
        }

        const MAX_CONTEXT_CHARS: usize = 20_000;
        if appended.chars().count() > MAX_CONTEXT_CHARS {
            let tail: String = appended
                .chars()
                .rev()
                .take(MAX_CONTEXT_CHARS)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            appended = tail;
        }

        // key_actions_json：仅追加关键行为（裁剪长度）
        let mut next_key_actions: Option<String> = existing_key_actions;
        if is_key_action {
            let mut arr = next_key_actions
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();

            let action_text = action_description
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| summary.unwrap_or("").trim());

            let action = serde_json::json!({
                "timestamp": timestamp,
                "trace_id": trace_id,
                "summary": summary.unwrap_or("").trim(),
                "action_description": action_text,
                "activity_type": activity_type,
                "entities": entities,
            });

            if arr
                .last()
                .map(|v| v.get("trace_id").and_then(|x| x.as_i64()) == Some(trace_id))
                .unwrap_or(false)
            {
                // no-op
            } else {
                arr.push(action);
            }

            const MAX_KEY_ACTIONS: usize = 80;
            if arr.len() > MAX_KEY_ACTIONS {
                arr = arr.split_off(arr.len() - MAX_KEY_ACTIONS);
            }
            next_key_actions = Some(Value::Array(arr).to_string());
        }

        tx.execute(
            r#"
            UPDATE activity_sessions
            SET
                context_text = ?1,
                entities_json = ?2,
                key_actions_json = ?3,
                updated_at = (strftime('%s', 'now') * 1000)
            WHERE id = ?4
            "#,
            rusqlite::params![
                if appended.is_empty() {
                    None
                } else {
                    Some(appended)
                },
                merged_entities_json,
                next_key_actions,
                session_id
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn create_chat_thread(&self, title: Option<&str>) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chat_threads (title) VALUES (?1)",
            rusqlite::params![title],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn append_chat_message(
        &self,
        thread_id: i64,
        role: &str,
        content: &str,
        context_json: Option<&str>,
    ) -> Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            r#"
            INSERT INTO chat_messages (thread_id, role, content, context_json)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            rusqlite::params![thread_id, role, content, context_json],
        )?;
        let message_id = tx.last_insert_rowid();

        tx.execute(
            "UPDATE chat_threads SET updated_at = (strftime('%s', 'now') * 1000) WHERE id = ?1",
            rusqlite::params![thread_id],
        )?;

        tx.commit()?;
        Ok(message_id)
    }

    pub fn get_chat_messages(
        &self,
        thread_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, thread_id, role, content, context_json, created_at
            FROM chat_messages
            WHERE thread_id = ?1
            ORDER BY created_at ASC
            LIMIT ?2 OFFSET ?3
            "#,
        )?;

        let rows = stmt.query_map(rusqlite::params![thread_id, limit, offset], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                context_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    /// 获取设置
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 更新设置
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, strftime('%s', 'now') * 1000)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    /// 获取完整文件路径
    /// 返回的路径使用正斜杠分隔符，确保跨平台兼容性
    pub fn get_full_path(&self, relative_path: &str) -> PathBuf {
        self.data_dir.join(relative_path)
    }

    /// 获取完整文件路径的字符串形式（统一使用正斜杠）
    /// 用于前端显示和 URL 转换
    pub fn get_full_path_string(&self, relative_path: &str) -> String {
        let path = self.data_dir.join(relative_path);
        // 统一使用正斜杠，确保 Tauri convertFileSrc() 能正确处理
        path.to_string_lossy().replace('\\', "/")
    }

    /// 获取模型目录
    pub fn get_models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// 获取数据目录
    pub fn get_data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// 更新 trace 的 OCR/文本数据（轻量）
    pub fn update_trace_ocr_text(&self, trace_id: i64, ocr_text: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE traces SET ocr_text = ?1 WHERE id = ?2",
            rusqlite::params![ocr_text, trace_id],
        )?;
        Ok(())
    }

    /// 更新 trace 的向量嵌入
    pub fn update_trace_embedding(&self, trace_id: i64, embedding: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // 更新 traces 表的 embedding 列（保留原有 BLOB 存储）
        conn.execute(
            "UPDATE traces SET embedding = ?1 WHERE id = ?2",
            rusqlite::params![embedding, trace_id],
        )?;

        // 计算向量维度（每个 f32 占 4 字节）
        let dimension = embedding.len() / 4;

        // 确保 vec0 表存在且维度正确
        Self::ensure_vec_table_inner(&conn, dimension)?;

        // 同时插入到 vec0 向量索引表
        // embedding 是 f32 数组的字节表示，直接传递给 sqlite-vec
        conn.execute(
            "INSERT OR REPLACE INTO traces_vec (trace_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![trace_id, embedding],
        )?;

        debug!(
            "Updated embedding for trace {} (vec index synced, dim={})",
            trace_id, dimension
        );
        Ok(())
    }

    /// 确保 vec0 向量索引表存在且维度正确
    fn ensure_vec_table_inner(conn: &Connection, dimension: usize) -> Result<()> {
        // 检查表是否存在
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='traces_vec'",
            [],
            |row| row.get(0),
        )?;

        if table_exists {
            // 尝试插入一个测试向量来检查维度
            // 创建一个正确维度的零向量
            let test_embedding: Vec<u8> = vec![0u8; dimension * 4];

            // 尝试执行一个查询来验证维度
            let dimension_ok = conn
                .execute(
                    "INSERT OR REPLACE INTO traces_vec (trace_id, embedding) VALUES (-1, ?1)",
                    rusqlite::params![test_embedding],
                )
                .is_ok();

            if dimension_ok {
                // 删除测试数据
                let _ = conn.execute("DELETE FROM traces_vec WHERE trace_id = -1", []);
                return Ok(());
            }

            // 维度不匹配，需要重建表
            info!(
                "Vector dimension changed, rebuilding traces_vec table with {} dimensions",
                dimension
            );
            conn.execute_batch("DROP TABLE IF EXISTS traces_vec")?;
        }

        // 表不存在或需要重建，创建新表
        let sql = format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS traces_vec USING vec0(
                trace_id INTEGER PRIMARY KEY,
                embedding FLOAT[{}]
            )",
            dimension
        );

        conn.execute_batch(&sql)?;
        info!("Created traces_vec table with dimension {}", dimension);

        Ok(())
    }

    /// 获取待处理 OCR 的 traces（没有 ocr_text 的）
    pub fn get_traces_pending_ocr(&self, limit: u32) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, activity_session_id, is_key_action, created_at
            FROM traces
            WHERE ocr_text IS NULL
            ORDER BY timestamp DESC
            LIMIT ?1
            "#,
        )?;

        let traces = stmt.query_map([limit], |row| {
            Ok(Trace {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                image_path: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                is_fullscreen: row.get(5)?,
                window_x: row.get(6)?,
                window_y: row.get(7)?,
                window_w: row.get(8)?,
                window_h: row.get(9)?,
                is_idle: row.get(10)?,
                ocr_text: row.get(11)?,
                activity_session_id: row.get(12)?,
                is_key_action: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?;

        let mut result = Vec::new();
        for trace in traces {
            result.push(trace?);
        }
        Ok(result)
    }

    /// 获取待处理嵌入的 traces（有 ocr_text 但没有 embedding 的）
    pub fn get_traces_pending_embedding(&self, limit: u32) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, activity_session_id, is_key_action, created_at
            FROM traces
            WHERE ocr_text IS NOT NULL AND embedding IS NULL
            ORDER BY timestamp DESC
            LIMIT ?1
            "#,
        )?;

        let traces = stmt.query_map([limit], |row| {
            Ok(Trace {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                image_path: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                is_fullscreen: row.get(5)?,
                window_x: row.get(6)?,
                window_y: row.get(7)?,
                window_w: row.get(8)?,
                window_h: row.get(9)?,
                is_idle: row.get(10)?,
                ocr_text: row.get(11)?,
                activity_session_id: row.get(12)?,
                is_key_action: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?;

        let mut result = Vec::new();
        for trace in traces {
            result.push(trace?);
        }
        Ok(result)
    }

    /// 向量相似度搜索（使用 sqlite-vec KNN 搜索）
    pub fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        limit: u32,
    ) -> Result<Vec<(Trace, f32)>> {
        let conn = self.conn.lock().unwrap();

        // 将查询向量转换为字节数组（sqlite-vec 接受的格式）
        let query_bytes: Vec<u8> = query_embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        // 使用 sqlite-vec 的 KNN 搜索
        // vec0 表通过 MATCH 子句进行相似度搜索，返回 distance（距离越小越相似）
        let mut stmt = conn.prepare(
            r#"
            SELECT
                t.id, t.timestamp, t.image_path, t.app_name, t.window_title,
                t.is_fullscreen, t.window_x, t.window_y, t.window_w, t.window_h,
                t.is_idle, t.ocr_text, t.activity_session_id, t.is_key_action, t.created_at,
                v.distance
            FROM traces_vec v
            INNER JOIN traces t ON v.trace_id = t.id
            WHERE v.embedding MATCH ?1
                AND k = ?2
            ORDER BY v.distance
            "#,
        )?;

        let traces = stmt.query_map(rusqlite::params![query_bytes, limit], |row| {
            let distance: f32 = row.get(15)?;
            // 将距离转换为相似度（距离越小，相似度越高）
            // 使用 1 / (1 + distance) 转换
            let similarity = 1.0 / (1.0 + distance);

            Ok((
                Trace {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    image_path: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    is_fullscreen: row.get(5)?,
                    window_x: row.get(6)?,
                    window_y: row.get(7)?,
                    window_w: row.get(8)?,
                    window_h: row.get(9)?,
                    is_idle: row.get(10)?,
                    ocr_text: row.get(11)?,
                    activity_session_id: row.get(12)?,
                    is_key_action: row.get(13)?,
                    created_at: row.get(14)?,
                },
                similarity,
            ))
        })?;

        let mut results = Vec::new();
        for trace_result in traces {
            if let Ok((trace, similarity)) = trace_result {
                results.push((trace, similarity));
            }
        }

        debug!("sqlite-vec KNN search returned {} results", results.len());
        Ok(results)
    }

    /// 混合搜索（FTS + 向量）
    pub fn hybrid_search(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: u32,
    ) -> Result<Vec<(Trace, f32)>> {
        // 1. FTS 搜索
        let fts_results = self.search_text(query, limit * 2)?;

        // 2. 如果没有向量，直接返回 FTS 结果
        let query_embedding = match query_embedding {
            Some(emb) => emb,
            None => {
                return Ok(fts_results
                    .into_iter()
                    .take(limit as usize)
                    .map(|t| (t, 1.0))
                    .collect());
            }
        };

        // 3. 向量搜索
        let vec_results = self.search_by_embedding(query_embedding, limit * 2)?;

        // 4. RRF 融合
        let mut scores: std::collections::HashMap<i64, f32> = std::collections::HashMap::new();
        let k = 60.0; // RRF 常数

        // FTS 分数
        for (rank, trace) in fts_results.iter().enumerate() {
            let score = 1.0 / (k + rank as f32 + 1.0);
            *scores.entry(trace.id).or_insert(0.0) += score;
        }

        // 向量分数
        for (rank, (trace, _)) in vec_results.iter().enumerate() {
            let score = 1.0 / (k + rank as f32 + 1.0);
            *scores.entry(trace.id).or_insert(0.0) += score;
        }

        // 收集所有 traces
        let all_traces: std::collections::HashMap<i64, Trace> = fts_results
            .into_iter()
            .chain(vec_results.into_iter().map(|(t, _)| t))
            .map(|t| (t.id, t))
            .collect();

        // 按 RRF 分数排序
        let mut results: Vec<(Trace, f32)> = scores
            .into_iter()
            .filter_map(|(id, score)| all_traces.get(&id).map(|t| (t.clone(), score)))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit as usize);

        Ok(results)
    }

    /// 反序列化向量
    #[allow(dead_code)]
    fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// 计算余弦相似度
    #[allow(dead_code)]
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }

    // ==================== Summary CRUD ====================

    /// 插入摘要记录
    pub fn insert_summary(&self, summary: &NewSummary) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO summaries (
                start_time, end_time, summary_type, content,
                structured_data, trace_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            rusqlite::params![
                summary.start_time,
                summary.end_time,
                summary.summary_type,
                summary.content,
                summary.structured_data,
                summary.trace_count,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 按时间范围查询摘要
    pub fn get_summaries(
        &self,
        start_time: i64,
        end_time: i64,
        summary_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Summary>> {
        let conn = self.conn.lock().unwrap();

        let mut result = Vec::new();

        if let Some(stype) = summary_type {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, start_time, end_time, summary_type, content,
                       structured_data, trace_count, created_at
                FROM summaries
                WHERE start_time >= ?1 AND end_time <= ?2 AND summary_type = ?3
                ORDER BY start_time DESC
                LIMIT ?4
                "#,
            )?;

            let summaries = stmt.query_map(
                rusqlite::params![start_time, end_time, stype, limit],
                |row| {
                    Ok(Summary {
                        id: row.get(0)?,
                        start_time: row.get(1)?,
                        end_time: row.get(2)?,
                        summary_type: row.get(3)?,
                        content: row.get(4)?,
                        structured_data: row.get(5)?,
                        trace_count: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )?;

            for summary in summaries {
                result.push(summary?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, start_time, end_time, summary_type, content,
                       structured_data, trace_count, created_at
                FROM summaries
                WHERE start_time >= ?1 AND end_time <= ?2
                ORDER BY start_time DESC
                LIMIT ?3
                "#,
            )?;

            let summaries =
                stmt.query_map(rusqlite::params![start_time, end_time, limit], |row| {
                    Ok(Summary {
                        id: row.get(0)?,
                        start_time: row.get(1)?,
                        end_time: row.get(2)?,
                        summary_type: row.get(3)?,
                        content: row.get(4)?,
                        structured_data: row.get(5)?,
                        trace_count: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                })?;

            for summary in summaries {
                result.push(summary?);
            }
        }

        Ok(result)
    }

    /// 按 ID 获取摘要
    pub fn get_summary_by_id(&self, id: i64) -> Result<Option<Summary>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            r#"
            SELECT id, start_time, end_time, summary_type, content,
                   structured_data, trace_count, created_at
            FROM summaries
            WHERE id = ?1
            "#,
            rusqlite::params![id],
            |row| {
                Ok(Summary {
                    id: row.get(0)?,
                    start_time: row.get(1)?,
                    end_time: row.get(2)?,
                    summary_type: row.get(3)?,
                    content: row.get(4)?,
                    structured_data: row.get(5)?,
                    trace_count: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        );

        match result {
            Ok(summary) => Ok(Some(summary)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 获取最近的摘要
    pub fn get_latest_summary(&self, summary_type: &str) -> Result<Option<Summary>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            r#"
            SELECT id, start_time, end_time, summary_type, content,
                   structured_data, trace_count, created_at
            FROM summaries
            WHERE summary_type = ?1
            ORDER BY end_time DESC
            LIMIT 1
            "#,
            rusqlite::params![summary_type],
            |row| {
                Ok(Summary {
                    id: row.get(0)?,
                    start_time: row.get(1)?,
                    end_time: row.get(2)?,
                    summary_type: row.get(3)?,
                    content: row.get(4)?,
                    structured_data: row.get(5)?,
                    trace_count: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        );

        match result {
            Ok(summary) => Ok(Some(summary)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 删除摘要
    pub fn delete_summary(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM summaries WHERE id = ?1", rusqlite::params![id])?;
        Ok(rows > 0)
    }

    // ==================== Entity CRUD ====================

    /// 插入或更新实体
    pub fn upsert_entity(&self, entity: &NewEntity) -> Result<i64> {
        let conn = self.conn.lock().unwrap();

        // 先尝试查找现有实体
        let existing: Result<i64, _> = conn.query_row(
            "SELECT id FROM entities WHERE name = ?1",
            rusqlite::params![entity.name],
            |row| row.get(0),
        );

        match existing {
            Ok(id) => {
                // 更新现有实体
                conn.execute(
                    r#"
                    UPDATE entities SET
                        mention_count = mention_count + 1,
                        last_seen = ?1,
                        metadata = COALESCE(?2, metadata)
                    WHERE id = ?3
                    "#,
                    rusqlite::params![entity.last_seen, entity.metadata, id],
                )?;
                Ok(id)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // 插入新实体
                conn.execute(
                    r#"
                    INSERT INTO entities (name, type, first_seen, last_seen, metadata)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    "#,
                    rusqlite::params![
                        entity.name,
                        entity.entity_type,
                        entity.first_seen,
                        entity.last_seen,
                        entity.metadata,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 获取实体列表
    pub fn get_entities(
        &self,
        entity_type: Option<&str>,
        limit: u32,
        order_by_mentions: bool,
    ) -> Result<Vec<Entity>> {
        let conn = self.conn.lock().unwrap();

        let order = if order_by_mentions {
            "mention_count DESC"
        } else {
            "last_seen DESC"
        };

        let mut result = Vec::new();

        if let Some(etype) = entity_type {
            let sql = format!(
                r#"
                SELECT id, name, type, mention_count, first_seen, last_seen, metadata
                FROM entities
                WHERE type = ?1
                ORDER BY {}
                LIMIT ?2
                "#,
                order
            );

            let mut stmt = conn.prepare(&sql)?;
            let entities = stmt.query_map(rusqlite::params![etype, limit], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    mention_count: row.get(3)?,
                    first_seen: row.get(4)?,
                    last_seen: row.get(5)?,
                    metadata: row.get(6)?,
                })
            })?;

            for entity in entities {
                result.push(entity?);
            }
        } else {
            let sql = format!(
                r#"
                SELECT id, name, type, mention_count, first_seen, last_seen, metadata
                FROM entities
                ORDER BY {}
                LIMIT ?1
                "#,
                order
            );

            let mut stmt = conn.prepare(&sql)?;
            let entities = stmt.query_map(rusqlite::params![limit], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    mention_count: row.get(3)?,
                    first_seen: row.get(4)?,
                    last_seen: row.get(5)?,
                    metadata: row.get(6)?,
                })
            })?;

            for entity in entities {
                result.push(entity?);
            }
        }

        Ok(result)
    }

    /// 按名称获取实体
    pub fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            r#"
            SELECT id, name, type, mention_count, first_seen, last_seen, metadata
            FROM entities
            WHERE name = ?1
            "#,
            rusqlite::params![name],
            |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    mention_count: row.get(3)?,
                    first_seen: row.get(4)?,
                    last_seen: row.get(5)?,
                    metadata: row.get(6)?,
                })
            },
        );

        match result {
            Ok(entity) => Ok(Some(entity)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 关联实体和痕迹
    pub fn link_entity_to_trace(&self, entity_id: i64, trace_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO entity_traces (entity_id, trace_id) VALUES (?1, ?2)",
            rusqlite::params![entity_id, trace_id],
        )?;
        Ok(())
    }

    /// 获取实体关联的痕迹
    pub fn get_traces_by_entity(&self, entity_id: i64, limit: u32) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT t.id, t.timestamp, t.image_path, t.app_name, t.window_title,
                   t.is_fullscreen, t.window_x, t.window_y, t.window_w, t.window_h,
                   t.is_idle, t.ocr_text, t.activity_session_id, t.is_key_action, t.created_at
            FROM traces t
            JOIN entity_traces et ON t.id = et.trace_id
            WHERE et.entity_id = ?1
            ORDER BY t.timestamp DESC
            LIMIT ?2
            "#,
        )?;

        let traces = stmt.query_map(rusqlite::params![entity_id, limit], |row| {
            Ok(Trace {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                image_path: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                is_fullscreen: row.get(5)?,
                window_x: row.get(6)?,
                window_y: row.get(7)?,
                window_w: row.get(8)?,
                window_h: row.get(9)?,
                is_idle: row.get(10)?,
                ocr_text: row.get(11)?,
                activity_session_id: row.get(12)?,
                is_key_action: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?;

        let mut result = Vec::new();
        for trace in traces {
            result.push(trace?);
        }
        Ok(result)
    }

    /// 删除实体
    pub fn delete_entity(&self, id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM entities WHERE id = ?1", rusqlite::params![id])?;
        Ok(rows > 0)
    }

    /// 搜索实体
    pub fn search_entities(&self, query: &str, limit: u32) -> Result<Vec<Entity>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, type, mention_count, first_seen, last_seen, metadata
            FROM entities
            WHERE name LIKE ?1
            ORDER BY mention_count DESC
            LIMIT ?2
            "#,
        )?;

        let entities = stmt.query_map(rusqlite::params![pattern, limit], |row| {
            Ok(Entity {
                id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                mention_count: row.get(3)?,
                first_seen: row.get(4)?,
                last_seen: row.get(5)?,
                metadata: row.get(6)?,
            })
        })?;

        let mut result = Vec::new();
        for entity in entities {
            result.push(entity?);
        }
        Ok(result)
    }
}

// 添加 dirs crate 作为辅助
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}
