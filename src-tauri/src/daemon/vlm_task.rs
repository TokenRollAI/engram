//! VLM 分析后台任务
//!
//! 支持并发处理待分析的 traces，调用 VLM 进行屏幕理解，
//! 写入 trace 的轻量 ocr_text 并生成文本嵌入，同时把 VLM 结论聚合到活动 Session。

use crate::ai::embedding::TextEmbedder;
use crate::ai::vlm::VlmEngine;
use crate::config::SessionConfig;
use crate::db::{Database, Trace};
use futures::stream::{self, StreamExt};
use image::RgbImage;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::sync::Semaphore;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// VLM 任务处理间隔（毫秒）- 默认 5 秒
const DEFAULT_PROCESS_INTERVAL_MS: u64 = 5_000;

/// 每批处理的最大 traces 数量
const DEFAULT_BATCH_SIZE: u32 = 10;

/// 默认并发数
const DEFAULT_CONCURRENCY: u32 = 3;

/// VLM 分析任务配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VlmTaskConfig {
    /// 处理间隔（毫秒）
    pub interval_ms: u64,
    /// 每批处理数量
    pub batch_size: u32,
    /// 并发请求数量
    pub concurrency: u32,
    /// 是否启用
    pub enabled: bool,
}

impl Default for VlmTaskConfig {
    fn default() -> Self {
        Self {
            interval_ms: DEFAULT_PROCESS_INTERVAL_MS,
            batch_size: DEFAULT_BATCH_SIZE,
            concurrency: DEFAULT_CONCURRENCY,
            enabled: true,
        }
    }
}

/// VLM 分析任务状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct VlmTaskStatus {
    /// 是否正在运行
    pub is_running: bool,
    /// 已处理的 traces 数量
    pub processed_count: u64,
    /// 处理失败的 traces 数量
    pub failed_count: u64,
    /// 待处理的 traces 数量
    pub pending_count: u64,
    /// 当前并发数配置
    pub concurrency: u32,
}

/// VLM 分析后台任务
pub struct VlmTask {
    db: Arc<Database>,
    vlm: Arc<RwLock<Option<VlmEngine>>>,
    embedder: Arc<RwLock<TextEmbedder>>,
    config: VlmTaskConfig,
    session_config: SessionConfig,
    is_running: Arc<AtomicBool>,
    processed_count: Arc<AtomicU64>,
    failed_count: Arc<AtomicU64>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl VlmTask {
    /// 创建新的 VLM 分析任务
    pub fn new(
        db: Arc<Database>,
        vlm: Arc<RwLock<Option<VlmEngine>>>,
        embedder: Arc<RwLock<TextEmbedder>>,
        config: VlmTaskConfig,
        session_config: SessionConfig,
    ) -> Self {
        Self {
            db,
            vlm,
            embedder,
            config,
            session_config,
            is_running: Arc::new(AtomicBool::new(false)),
            processed_count: Arc::new(AtomicU64::new(0)),
            failed_count: Arc::new(AtomicU64::new(0)),
            shutdown_tx: None,
        }
    }

    /// 启动 VLM 分析任务
    pub fn start(&mut self) -> anyhow::Result<()> {
        if !self.config.enabled {
            info!("VLM task is disabled");
            return Ok(());
        }

        if self.is_running.load(Ordering::SeqCst) {
            warn!("VLM task is already running");
            return Ok(());
        }

        info!("Starting VLM analysis task...");

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let is_running = self.is_running.clone();
        let processed_count = self.processed_count.clone();
        let failed_count = self.failed_count.clone();
        let db = self.db.clone();
        let vlm = self.vlm.clone();
        let embedder = self.embedder.clone();
        let config = self.config.clone();
        let session_config = self.session_config.clone();

        is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(config.interval_ms));

            // 创建并发控制信号量
            let semaphore = Arc::new(Semaphore::new(config.concurrency as usize));

            info!(
                "VLM task loop started (interval: {}ms, batch_size: {}, concurrency: {})",
                config.interval_ms, config.batch_size, config.concurrency
            );

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("VLM task received shutdown signal");
                        break;
                    }
                    _ = ticker.tick() => {
                        // 检查 VLM 是否可用
                        let vlm_ready = {
                            let vlm_guard = vlm.read().await;
                            vlm_guard.as_ref().map(|v| v.is_running()).unwrap_or(false)
                        };

                        if !vlm_ready {
                            debug!("VLM not ready, skipping processing");
                            continue;
                        }

                        // 并发处理待分析的 traces
                        match Self::process_pending_traces_concurrent(
                            &db,
                            &vlm,
                            &embedder,
                            &session_config,
                            &semaphore,
                            config.batch_size,
                            &processed_count,
                            &failed_count,
                        ).await {
                            Ok(count) => {
                                if count > 0 {
                                    info!("Processed {} traces this batch (concurrency: {})", count, config.concurrency);
                                }
                            }
                            Err(e) => {
                                error!("Error processing traces: {}", e);
                            }
                        }
                    }
                }
            }

            is_running.store(false, Ordering::SeqCst);
            info!("VLM task loop stopped");
        });

        Ok(())
    }

    /// 停止 VLM 分析任务
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
        self.is_running.store(false, Ordering::SeqCst);
        info!("VLM task stopped");
    }

    /// 获取任务状态
    pub fn status(&self, pending_count: u64) -> VlmTaskStatus {
        VlmTaskStatus {
            is_running: self.is_running.load(Ordering::SeqCst),
            processed_count: self.processed_count.load(Ordering::SeqCst),
            failed_count: self.failed_count.load(Ordering::SeqCst),
            pending_count,
            concurrency: self.config.concurrency,
        }
    }

    /// 获取当前配置
    pub fn config(&self) -> &VlmTaskConfig {
        &self.config
    }

    /// 并发处理待分析的 traces
    async fn process_pending_traces_concurrent(
        db: &Arc<Database>,
        vlm: &Arc<RwLock<Option<VlmEngine>>>,
        embedder: &Arc<RwLock<TextEmbedder>>,
        session_config: &SessionConfig,
        semaphore: &Arc<Semaphore>,
        batch_size: u32,
        processed_count: &Arc<AtomicU64>,
        failed_count: &Arc<AtomicU64>,
    ) -> anyhow::Result<u32> {
        // 获取待处理的 traces
        let pending_traces = db.get_traces_pending_ocr(batch_size)?;

        if pending_traces.is_empty() {
            return Ok(0);
        }

        let trace_count = pending_traces.len();
        debug!(
            "Found {} pending traces to process concurrently",
            trace_count
        );

        // 并发处理所有 traces
        let results: Vec<Result<i64, (i64, String)>> = stream::iter(pending_traces)
            .map(|trace| {
                let db = db.clone();
                let vlm = vlm.clone();
                let embedder = embedder.clone();
                let semaphore = semaphore.clone();
                let session_config = session_config.clone();

                async move {
                    // 获取信号量许可，控制并发数
                    let _permit = semaphore.acquire().await.unwrap();

                    let trace_id = trace.id;
                    match Self::process_single_trace(&db, &vlm, &embedder, &session_config, &trace)
                        .await
                    {
                        Ok(_) => Ok(trace_id),
                        Err(e) => Err((trace_id, e.to_string())),
                    }
                }
            })
            .buffer_unordered(batch_size as usize) // 允许并发执行
            .collect()
            .await;

        // 统计结果
        let mut success_count = 0u32;
        for result in results {
            match result {
                Ok(trace_id) => {
                    processed_count.fetch_add(1, Ordering::SeqCst);
                    success_count += 1;
                    debug!("Successfully processed trace {}", trace_id);
                }
                Err((trace_id, err)) => {
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    warn!("Failed to process trace {}: {}", trace_id, err);
                }
            }
        }

        Ok(success_count)
    }

    /// 处理单个 trace
    async fn process_single_trace(
        db: &Arc<Database>,
        vlm: &Arc<RwLock<Option<VlmEngine>>>,
        embedder: &Arc<RwLock<TextEmbedder>>,
        session_config: &SessionConfig,
        trace: &Trace,
    ) -> anyhow::Result<()> {
        // 1. 获取图片路径
        let image_path_str = trace
            .image_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Trace {} has no image_path", trace.id))?;

        // 2. 加载图片（同步操作，在 spawn_blocking 中执行）
        let path = db.get_full_path(image_path_str);
        let image = tokio::task::spawn_blocking(move || Self::load_image(&path)).await??;

        // 3. 多线程 Session：提供活跃线程列表作为上下文，并让模型选择 existing_session_id
        let now_ts = trace.timestamp;
        let mut parts: Vec<String> = Vec::new();

        parts.push(format!(
            "【Current Trace Meta】\napp_name: {}\nwindow_title: {}\ntime: {}",
            trace.app_name.as_deref().unwrap_or("-"),
            trace.window_title.as_deref().unwrap_or("-"),
            chrono::DateTime::from_timestamp_millis(now_ts)
                .map(|t| t
                    .with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M:%S")
                    .to_string())
                .unwrap_or_else(|| "?".to_string()),
        ));

        let active_sessions = db.get_active_sessions_for_routing(
            now_ts,
            session_config.active_window_ms as i64,
            session_config.max_active_sessions,
        )?;

        if !active_sessions.is_empty() {
            parts.push("【Active Sessions（Threads）】\n请在输出 JSON 中用 existing_session_id 选择其中一个 session_id；如果都不匹配则为 null。".to_string());
            for s in active_sessions.iter() {
                let st = chrono::DateTime::from_timestamp_millis(s.start_time)
                    .map(|t| {
                        t.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "?".to_string());
                let et = chrono::DateTime::from_timestamp_millis(s.end_time)
                    .map(|t| {
                        t.with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|| "?".to_string());

                let title = s
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                    .unwrap_or(&s.app_name);
                let desc = s
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                    .unwrap_or("");

                let mut block = format!(
                    "- session_id={} ({st}~{et}) title=\"{}\" app_name=\"{}\" trace_count={}",
                    s.id, title, s.app_name, s.trace_count
                );
                if !desc.is_empty() {
                    block.push_str(&format!("\n  description: {}", desc));
                }
                if let Some(kaj) = s
                    .key_actions_json
                    .as_deref()
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                {
                    if let Some(last3) = Self::format_key_actions_for_context_with_header(
                        kaj,
                        3,
                        "  last_key_actions:",
                    ) {
                        block.push_str(&format!("\n{}", last3));
                    }
                }
                parts.push(block);
            }
        }

        if let Ok(recent) = db.get_recent_traces_before(trace.timestamp, 2) {
            let mut lines = Vec::new();
            for t in recent {
                if let Some(text) = t.ocr_text {
                    let text = text.trim();
                    if !text.is_empty() {
                        let snippet: String = text.chars().take(220).collect();
                        lines.push(format!("- {}", snippet));
                    }
                }
            }
            if !lines.is_empty() {
                parts.push(format!(
                    "【Recent Global Traces OCR】\n{}",
                    lines.join("\n")
                ));
            }
        }

        let context = {
            const MAX_CONTEXT_CHARS: usize = 262_144; // 256K
            let joined = parts.join("\n\n");
            if joined.chars().count() > MAX_CONTEXT_CHARS {
                Some(
                    joined
                        .chars()
                        .rev()
                        .take(MAX_CONTEXT_CHARS)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect(),
                )
            } else {
                Some(joined)
            }
        };

        let description = {
            let vlm_guard = vlm.read().await;
            let vlm_engine = vlm_guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("VLM not available"))?;
            vlm_engine
                .analyze_screen_with_context(&image, context.as_deref())
                .await?
        };

        // 4. 轻量 OCR 文本（写回 trace）
        let ocr_text = description
            .text_content
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| description.summary.clone());

        // 5. 更新数据库（trace 仅保留轻量 OCR 文本）
        db.update_trace_ocr_text(trace.id, &ocr_text)?;

        // 6. 生成嵌入向量（可使用更丰富的文本，不必写回 trace）
        let embedding_text = VlmEngine::get_text_for_embedding(&description);
        let embedding = {
            let embedder_guard = embedder.read().await;
            embedder_guard.embed(&embedding_text).await?
        };

        // 7. 序列化嵌入向量
        let embedding_bytes = Self::serialize_embedding(&embedding);

        // 8. 更新数据库（嵌入向量）
        db.update_trace_embedding(trace.id, &embedding_bytes)?;

        // 9. 把 VLM 结论同步到 Session（对外的核心视图）
        let is_key_action = description.is_key_action;
        let action_description = description
            .action_description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let raw_json = serde_json::to_string(&description).ok();
        db.update_trace_vlm_analysis(
            trace.id,
            Some(description.summary.as_str()),
            action_description,
            description.activity_type.as_deref(),
            Some(description.confidence),
            &description.entities,
            raw_json.as_deref(),
            is_key_action,
        )?;

        // 10. 多线程 Session 路由：优先模型选择，其次 embedding 相似度兜底，否则新建
        let active_ids: std::collections::HashSet<i64> =
            active_sessions.iter().map(|s| s.id).collect();

        let mut chosen_session_id = description
            .existing_session_id
            .filter(|id| active_ids.contains(id));

        if chosen_session_id.is_none() {
            let candidates = db.get_active_session_last_embeddings(
                now_ts,
                session_config.active_window_ms as i64,
                session_config.max_active_sessions,
            )?;
            chosen_session_id = Self::pick_best_session_by_embedding(
                &embedding,
                &candidates,
                session_config.similarity_threshold,
            );
        }

        let chosen_session_id = match chosen_session_id {
            Some(id) => id,
            None => {
                let seed_app = trace
                    .app_name
                    .as_deref()
                    .or(description.detected_app.as_deref())
                    .unwrap_or("unknown");
                db.create_activity_session(seed_app, trace.timestamp)?
            }
        };

        db.update_activity_session_from_vlm(
            chosen_session_id,
            trace.id,
            trace.timestamp,
            Some(description.summary.as_str()),
            action_description,
            description.activity_type.as_deref(),
            &description.entities,
            is_key_action,
            description.session_title.as_deref(),
            description.session_description.as_deref(),
        )?;

        info!(
            "Trace {} processed: summary='{}', confidence={:.2}",
            trace.id,
            description.summary.chars().take(50).collect::<String>(),
            description.confidence
        );

        Ok(())
    }

    fn format_key_actions_for_context(key_actions_json: &str, take_last: usize) -> Option<String> {
        Self::format_key_actions_for_context_with_header(
            key_actions_json,
            take_last,
            "【Existing Key Actions（参考）】",
        )
    }

    fn format_key_actions_for_context_with_header(
        key_actions_json: &str,
        take_last: usize,
        header: &str,
    ) -> Option<String> {
        use serde_json::Value;

        let v: Value = serde_json::from_str(key_actions_json).ok()?;
        let arr = v.as_array()?;
        if arr.is_empty() {
            return None;
        }

        // 只给模型一个“已标记关键行为”清单，避免把 JSON 全量塞进上下文。
        let mut lines: Vec<String> = Vec::new();
        let take_last = take_last.max(1);
        for it in arr.iter().rev().take(take_last).rev() {
            let ts = it.get("timestamp").and_then(|x| x.as_i64()).unwrap_or(0);
            let text = it
                .get("action_description")
                .and_then(|x| x.as_str())
                .or_else(|| it.get("summary").and_then(|x| x.as_str()))
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                continue;
            }
            let time = chrono::DateTime::from_timestamp_millis(ts)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_else(|| "?".to_string());
            lines.push(format!("- [{}] {}", time, text));
        }

        Some(format!("{}\n{}", header, lines.join("\n")))
    }

    fn pick_best_session_by_embedding(
        embedding: &[f32],
        candidates: &[(i64, Vec<u8>)],
        threshold: f32,
    ) -> Option<i64> {
        let mut best: Option<(i64, f32)> = None;
        for (sid, bytes) in candidates {
            let Some(other) = Self::deserialize_embedding(bytes) else {
                continue;
            };
            if other.len() != embedding.len() || other.is_empty() {
                continue;
            }
            let sim = Self::cosine_similarity(embedding, &other);
            if sim >= threshold {
                if best.map(|(_, b)| sim > b).unwrap_or(true) {
                    best = Some((*sid, sim));
                }
            }
        }
        best.map(|(sid, _)| sid)
    }

    fn deserialize_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
        if bytes.len() % 4 != 0 {
            return None;
        }
        let mut v = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let arr: [u8; 4] = chunk.try_into().ok()?;
            v.push(f32::from_le_bytes(arr));
        }
        Some(v)
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut dot = 0.0f32;
        let mut na = 0.0f32;
        let mut nb = 0.0f32;
        for (x, y) in a.iter().zip(b.iter()) {
            dot += x * y;
            na += x * x;
            nb += y * y;
        }
        if na <= 0.0 || nb <= 0.0 {
            0.0
        } else {
            dot / (na.sqrt() * nb.sqrt())
        }
    }

    /// 加载图片
    fn load_image(path: &std::path::Path) -> anyhow::Result<RgbImage> {
        let img = image::open(path)?;
        Ok(img.to_rgb8())
    }

    /// 序列化嵌入向量为字节数组
    fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
    }
}
