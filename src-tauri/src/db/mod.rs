//! 数据库模块
//!
//! 使用 SQLite 存储痕迹数据、摘要和设置。

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
        let data_dir = Self::resolve_data_dir()?;
        fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("engram.db");
        info!("Opening database at: {:?}", db_path);

        let conn = Connection::open(&db_path)?;

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

    /// 保存截图文件（WebP 格式）
    pub fn save_screenshot(&self, pixels: &[u8], width: u32, height: u32) -> Result<String> {
        use image::codecs::webp::WebPEncoder;
        use std::io::BufWriter;

        let now = Utc::now();
        let dir = self.data_dir
            .join("screenshots")
            .join(now.year().to_string())
            .join(format!("{:02}", now.month()))
            .join(format!("{:02}", now.day()));

        fs::create_dir_all(&dir)?;

        let filename = format!("{}.webp", now.timestamp_millis());
        let path = dir.join(&filename);

        // 创建图像
        let img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Failed to create image from pixels"))?;

        // 保存为 WebP 格式（有损压缩，质量 75%）
        let file = fs::File::create(&path)?;
        let writer = BufWriter::new(file);
        let encoder = WebPEncoder::new_lossless(writer);

        // 使用 RGBA 编码
        encoder.encode(
            img.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?;

        // 返回相对路径
        let relative_path = format!(
            "screenshots/{}/{:02}/{:02}/{}",
            now.year(),
            now.month(),
            now.day(),
            filename
        );

        debug!("Screenshot saved as WebP: {}", relative_path);
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
    pub fn get_full_path(&self, relative_path: &str) -> PathBuf {
        self.data_dir.join(relative_path)
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
        conn.execute(
            "UPDATE traces SET embedding = ?1 WHERE id = ?2",
            rusqlite::params![embedding, trace_id],
        )?;
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

    /// 向量相似度搜索（暴力搜索，适用于小数据集）
    pub fn search_by_embedding(&self, query_embedding: &[f32], limit: u32) -> Result<Vec<(Trace, f32)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, timestamp, image_path, app_name, window_title,
                   is_fullscreen, window_x, window_y, window_w, window_h,
                   is_idle, ocr_text, created_at, embedding
            FROM traces
            WHERE embedding IS NOT NULL
            "#,
        )?;

        let traces = stmt.query_map([], |row| {
            let embedding_bytes: Vec<u8> = row.get(13)?;
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
                embedding_bytes,
            ))
        })?;

        // 计算相似度并排序
        let mut results: Vec<(Trace, f32)> = Vec::new();
        for trace_result in traces {
            if let Ok((trace, embedding_bytes)) = trace_result {
                let embedding = Self::deserialize_embedding(&embedding_bytes);
                let similarity = Self::cosine_similarity(query_embedding, &embedding);
                results.push((trace, similarity));
            }
        }

        // 按相似度降序排序
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit as usize);

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
