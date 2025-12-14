//! VLM 分析后台任务
//!
//! 定期处理待分析的 traces，调用 VLM 进行屏幕理解，
//! 更新 ocr_text 和 ocr_json，并生成文本嵌入。

use crate::ai::embedding::TextEmbedder;
use crate::ai::vlm::VlmEngine;
use crate::db::{Database, Trace};
use image::RgbImage;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// VLM 任务处理间隔（毫秒）- 默认 10 秒
const DEFAULT_PROCESS_INTERVAL_MS: u64 = 10_000;

/// 每批处理的最大 traces 数量
const DEFAULT_BATCH_SIZE: u32 = 5;

/// VLM 分析任务配置
#[derive(Debug, Clone)]
pub struct VlmTaskConfig {
    /// 处理间隔（毫秒）
    pub interval_ms: u64,
    /// 每批处理数量
    pub batch_size: u32,
    /// 是否启用
    pub enabled: bool,
}

impl Default for VlmTaskConfig {
    fn default() -> Self {
        Self {
            interval_ms: DEFAULT_PROCESS_INTERVAL_MS,
            batch_size: DEFAULT_BATCH_SIZE,
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
        let interval_ms = self.config.interval_ms;
        let batch_size = self.config.batch_size;

        is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(interval_ms));

            info!(
                "VLM task loop started (interval: {}ms, batch_size: {})",
                interval_ms, batch_size
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

                        // 处理待分析的 traces
                        match Self::process_pending_traces(
                            &db,
                            &vlm,
                            &embedder,
                            batch_size,
                            &processed_count,
                            &failed_count,
                        ).await {
                            Ok(count) => {
                                if count > 0 {
                                    debug!("Processed {} traces this batch", count);
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
        }
    }

    /// 处理待分析的 traces
    async fn process_pending_traces(
        db: &Arc<Database>,
        vlm: &Arc<RwLock<Option<VlmEngine>>>,
        embedder: &Arc<RwLock<TextEmbedder>>,
        batch_size: u32,
        processed_count: &Arc<AtomicU64>,
        failed_count: &Arc<AtomicU64>,
    ) -> anyhow::Result<u32> {
        // 获取待处理的 traces
        let pending_traces = db.get_traces_pending_ocr(batch_size)?;

        if pending_traces.is_empty() {
            return Ok(0);
        }

        debug!("Found {} pending traces to process", pending_traces.len());

        let mut success_count = 0u32;

        for trace in pending_traces {
            match Self::process_single_trace(db, vlm, embedder, &trace).await {
                Ok(_) => {
                    processed_count.fetch_add(1, Ordering::SeqCst);
                    success_count += 1;
                    debug!("Successfully processed trace {}", trace.id);
                }
                Err(e) => {
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    warn!("Failed to process trace {}: {}", trace.id, e);
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
        // 1. 加载图片
        let image_path = db.get_full_path(&trace.image_path);
        let image = Self::load_image(&image_path)?;

        // 2. 调用 VLM 分析
        let description = {
            let vlm_guard = vlm.read().await;
            let vlm_engine = vlm_guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("VLM not available"))?;
            vlm_engine.analyze_screen(&image).await?
        };

        // 3. 提取 OCR 数据
        let ocr_text = VlmEngine::get_text_for_embedding(&description);
        let ocr_json = serde_json::to_string(&description)?;

        // 4. 更新数据库（OCR 数据）
        db.update_trace_ocr(trace.id, &ocr_text, &ocr_json)?;

        // 5. 生成嵌入向量
        let embedding = {
            let embedder_guard = embedder.read().await;
            embedder_guard.embed(&ocr_text).await?
        };

        // 6. 序列化嵌入向量
        let embedding_bytes = Self::serialize_embedding(&embedding);

        // 7. 更新数据库（嵌入向量）
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
