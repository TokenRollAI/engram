//! VLM 分析后台任务
//!
//! 支持并发处理待分析的 traces，调用 VLM 进行屏幕理解，
//! 更新 ocr_text 和 ocr_json，并生成文本嵌入。

use crate::ai::embedding::TextEmbedder;
use crate::ai::vlm::VlmEngine;
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
    ) -> Self {
        Self {
            db,
            vlm,
            embedder,
            config,
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
        debug!("Found {} pending traces to process concurrently", trace_count);

        // 并发处理所有 traces
        let results: Vec<Result<i64, (i64, String)>> = stream::iter(pending_traces)
            .map(|trace| {
                let db = db.clone();
                let vlm = vlm.clone();
                let embedder = embedder.clone();
                let semaphore = semaphore.clone();

                async move {
                    // 获取信号量许可，控制并发数
                    let _permit = semaphore.acquire().await.unwrap();

                    let trace_id = trace.id;
                    match Self::process_single_trace(&db, &vlm, &embedder, &trace).await {
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
        trace: &Trace,
    ) -> anyhow::Result<()> {
        // 1. 获取图片路径
        let image_path_str = trace.image_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Trace {} has no image_path", trace.id))?;

        // 2. 加载图片（同步操作，在 spawn_blocking 中执行）
        let path = db.get_full_path(image_path_str);
        let image = tokio::task::spawn_blocking(move || Self::load_image(&path))
            .await??;

        // 3. 调用 VLM 分析（异步 HTTP 请求，可并发）
        let description = {
            let vlm_guard = vlm.read().await;
            let vlm_engine = vlm_guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("VLM not available"))?;
            vlm_engine.analyze_screen(&image).await?
        };

        // 4. 提取 OCR 数据
        let ocr_text = VlmEngine::get_text_for_embedding(&description);
        let ocr_json = serde_json::to_string(&description)?;

        // 5. 更新数据库（OCR 数据）
        db.update_trace_ocr(trace.id, &ocr_text, &ocr_json)?;

        // 6. 生成嵌入向量（异步操作）
        let embedding = {
            let embedder_guard = embedder.read().await;
            embedder_guard.embed(&ocr_text).await?
        };

        // 7. 序列化嵌入向量
        let embedding_bytes = Self::serialize_embedding(&embedding);

        // 8. 更新数据库（嵌入向量）
        db.update_trace_embedding(trace.id, &embedding_bytes)?;

        info!(
            "Trace {} processed: summary='{}', confidence={:.2}",
            trace.id,
            description.summary.chars().take(50).collect::<String>(),
            description.confidence
        );

        Ok(())
    }

    /// 加载图片
    fn load_image(path: &std::path::Path) -> anyhow::Result<RgbImage> {
        let img = image::open(path)?;
        Ok(img.to_rgb8())
    }

    /// 序列化嵌入向量为字节数组
    fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
        embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect()
    }
}
