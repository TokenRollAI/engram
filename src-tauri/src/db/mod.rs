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
            let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
            Ok(home.join(".engram"))
        }
    }

    /// 保存截图文件（JPEG 格式）
    pub fn save_screenshot(&self, pixels: &[u8], width: u32, height: u32) -> Result<String> {
        use image::codecs::jpeg::JpegEncoder;
        use std::io::BufWriter;

        let now = Utc::now();
        let dir = self.data_dir
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
    pub fn insert_trace(&self, trace: &NewTrace) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO traces (
                timestamp, image_path, app_name, window_title,
                is_fullscreen, window_x, window_y, window_w, window_h,
                is_idle, ocr_text, ocr_json, phash
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
                trace.ocr_json,
                trace.phash,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// 按时间范围查询痕迹
    pub fn get_traces(&self, start_time: i64, end_time: i64, limit: u32, offset: u32) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, created_at
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
                    created_at: row.get(12)?,
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
                           is_idle, ocr_text, created_at
                    FROM traces
                    WHERE timestamp BETWEEN ?1 AND ?2
                    ORDER BY timestamp DESC
                    LIMIT ?3
                    "#.to_string(),
                    false,
                )
            } else {
                // 构建 IN 子句的占位符
                let placeholders: Vec<String> = (0..apps.len()).map(|i| format!("?{}", i + 4)).collect();
                (
                    format!(
                        r#"
                        SELECT id, timestamp, image_path, app_name, window_title,
                               is_fullscreen, window_x, window_y, window_w, window_h,
                               is_idle, ocr_text, created_at
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
                       is_idle, ocr_text, created_at
                FROM traces
                WHERE timestamp BETWEEN ?1 AND ?2
                ORDER BY timestamp DESC
                LIMIT ?3
                "#.to_string(),
                false,
            )
        };

        let mut stmt = conn.prepare(&sql)?;

        // 构建参数
        let traces = if has_filter {
            let apps = app_filter.unwrap();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(start_time),
                Box::new(end_time),
                Box::new(limit),
            ];
            for app in apps {
                params.push(Box::new(app.clone()));
            }
            let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
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
                    created_at: row.get(12)?,
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
                    created_at: row.get(12)?,
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
                   t.is_idle, t.ocr_text, t.created_at
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
                created_at: row.get(12)?,
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

        let total_traces: i64 = conn.query_row("SELECT COUNT(*) FROM traces", [], |row| row.get(0))?;

        let total_summaries: i64 =
            conn.query_row("SELECT COUNT(*) FROM summaries", [], |row| row.get(0)).unwrap_or(0);

        let total_entities: i64 =
            conn.query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0)).unwrap_or(0);

        let oldest_trace_time: Option<i64> =
            conn.query_row("SELECT MIN(timestamp) FROM traces", [], |row| row.get(0)).ok();

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
             ORDER BY app_name"
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

    /// 更新 trace 的 OCR 数据
    pub fn update_trace_ocr(&self, trace_id: i64, ocr_text: &str, ocr_json: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE traces SET ocr_text = ?1, ocr_json = ?2 WHERE id = ?3",
            rusqlite::params![ocr_text, ocr_json, trace_id],
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

        // 同时插入到 vec0 向量索引表
        // embedding 是 f32 数组的字节表示，直接传递给 sqlite-vec
        conn.execute(
            "INSERT OR REPLACE INTO traces_vec (trace_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![trace_id, embedding],
        )?;

        debug!("Updated embedding for trace {} (vec index synced)", trace_id);
        Ok(())
    }

    /// 获取待处理 OCR 的 traces（没有 ocr_text 的）
    pub fn get_traces_pending_ocr(&self, limit: u32) -> Result<Vec<Trace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, created_at
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
                created_at: row.get(12)?,
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
                   is_idle, ocr_text, created_at
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
                created_at: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for trace in traces {
            result.push(trace?);
        }
        Ok(result)
    }

    /// 向量相似度搜索（使用 sqlite-vec KNN 搜索）
    pub fn search_by_embedding(&self, query_embedding: &[f32], limit: u32) -> Result<Vec<(Trace, f32)>> {
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
                t.is_idle, t.ocr_text, t.created_at,
                v.distance
            FROM traces_vec v
            INNER JOIN traces t ON v.trace_id = t.id
            WHERE v.embedding MATCH ?1
                AND k = ?2
            ORDER BY v.distance
            "#,
        )?;

        let traces = stmt.query_map(rusqlite::params![query_bytes, limit], |row| {
            let distance: f32 = row.get(13)?;
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
                    created_at: row.get(12)?,
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
    pub fn hybrid_search(&self, query: &str, query_embedding: Option<&[f32]>, limit: u32) -> Result<Vec<(Trace, f32)>> {
        // 1. FTS 搜索
        let fts_results = self.search_text(query, limit * 2)?;

        // 2. 如果没有向量，直接返回 FTS 结果
        let query_embedding = match query_embedding {
            Some(emb) => emb,
            None => {
                return Ok(fts_results.into_iter()
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
            .filter_map(|(id, score)| {
                all_traces.get(&id).map(|t| (t.clone(), score))
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit as usize);

        Ok(results)
    }

    /// 反序列化向量
    fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// 计算余弦相似度
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

            let summaries = stmt.query_map(
                rusqlite::params![start_time, end_time, limit],
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

        let order = if order_by_mentions { "mention_count DESC" } else { "last_seen DESC" };

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
                   t.is_idle, t.ocr_text, t.created_at
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
                created_at: row.get(12)?,
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
