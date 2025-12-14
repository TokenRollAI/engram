//! 后台守护进程模块
//!
//! 负责定时截图、上下文感知、图像处理和摘要生成。

mod capture;
mod context;
mod hasher;
mod idle;
pub mod summarizer_task;
pub mod vlm_task;

pub use capture::ScreenCapture;
pub use context::{FocusContext, WindowWatcher};
pub use hasher::PerceptualHasher;
pub use idle::IdleDetector;
pub use summarizer_task::{SummarizerTask, SummarizerTaskConfig};
pub use vlm_task::{VlmTask, VlmTaskConfig, VlmTaskStatus};

use crate::db::Database;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// 截图间隔（毫秒）
const DEFAULT_CAPTURE_INTERVAL_MS: u64 = 2000;

/// 闲置阈值（毫秒）
const DEFAULT_IDLE_THRESHOLD_MS: u64 = 30000;

/// 相似度阈值（汉明距离）
const DEFAULT_SIMILARITY_THRESHOLD: u32 = 5;

/// 守护进程状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct DaemonStatus {
    pub is_running: bool,
    pub is_paused: bool,
    pub is_idle: bool,
    pub idle_time_ms: u64,
    pub last_capture_time: Option<i64>,
    pub total_captures_today: u64,
}

/// Engram 后台守护进程
pub struct EngramDaemon {
    db: Arc<Database>,
    is_running: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    is_idle: Arc<AtomicBool>,
    idle_time_ms: Arc<AtomicU64>,
    capture_interval_ms: u64,
    idle_threshold_ms: u64,
    similarity_threshold: u32,
    shutdown_tx: Option<mpsc::Sender<()>>,
    last_capture_time: Arc<AtomicU64>,
    total_captures_today: Arc<AtomicU64>,
}

impl EngramDaemon {
    /// 创建新的守护进程
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        Self::new_with_config(
            db,
            DEFAULT_CAPTURE_INTERVAL_MS,
            DEFAULT_IDLE_THRESHOLD_MS,
            DEFAULT_SIMILARITY_THRESHOLD,
        )
    }

    /// 使用指定配置创建守护进程
    pub fn new_with_config(
        db: Arc<Database>,
        capture_interval_ms: u64,
        idle_threshold_ms: u64,
        similarity_threshold: u32,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            is_running: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            is_idle: Arc::new(AtomicBool::new(false)),
            idle_time_ms: Arc::new(AtomicU64::new(0)),
            capture_interval_ms,
            idle_threshold_ms,
            similarity_threshold,
            shutdown_tx: None,
            last_capture_time: Arc::new(AtomicU64::new(0)),
            total_captures_today: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 启动守护进程
    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.is_running.load(Ordering::SeqCst) {
            warn!("Daemon is already running");
            return Ok(());
        }

        info!("Starting Engram daemon...");

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let is_running = self.is_running.clone();
        let is_paused = self.is_paused.clone();
        let is_idle = self.is_idle.clone();
        let idle_time_ms = self.idle_time_ms.clone();
        let last_capture_time = self.last_capture_time.clone();
        let total_captures_today = self.total_captures_today.clone();
        let db = self.db.clone();
        let interval_ms = self.capture_interval_ms;
        let idle_threshold_ms = self.idle_threshold_ms;
        let similarity_threshold = self.similarity_threshold;

        is_running.store(true, Ordering::SeqCst);

        // 启动截图循环
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(interval_ms));
            let mut screen_capture = match ScreenCapture::new() {
                Ok(sc) => sc,
                Err(e) => {
                    error!("Failed to initialize screen capture: {}", e);
                    return;
                }
            };
            let hasher = PerceptualHasher::new();
            let idle_detector = IdleDetector::new(idle_threshold_ms);
            let mut last_hash: Option<[u8; 8]> = None;

            info!(
                "Daemon capture loop started (idle threshold: {}ms)",
                idle_threshold_ms
            );

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Daemon received shutdown signal");
                        break;
                    }
                    _ = ticker.tick() => {
                        // 检查是否暂停
                        if is_paused.load(Ordering::SeqCst) {
                            debug!("Capture paused, skipping");
                            continue;
                        }

                        // 检查用户是否闲置
                        let current_idle_time = idle_detector.get_idle_time_ms();
                        idle_time_ms.store(current_idle_time, Ordering::SeqCst);

                        let user_is_idle = idle_detector.is_idle();
                        is_idle.store(user_is_idle, Ordering::SeqCst);

                        if user_is_idle {
                            debug!("User is idle ({}ms), skipping capture", current_idle_time);
                            continue;
                        }

                        // 执行截图
                        match screen_capture.capture() {
                            Ok(frame) => {
                                // 计算感知哈希
                                let current_hash = hasher.compute(&frame.pixels, frame.width, frame.height);

                                // 检查是否与上一帧相似
                                if let Some(prev_hash) = last_hash {
                                    let distance = hasher.hamming_distance(&prev_hash, &current_hash);
                                    if distance < similarity_threshold {
                                        debug!("Frame too similar (distance={}), skipping", distance);
                                        continue;
                                    }
                                }
                                last_hash = Some(current_hash);

                                // 获取窗口上下文
                                let context = WindowWatcher::get_focus_context();

                                // 保存到数据库
                                match Self::save_frame(&db, &frame, &context, &current_hash).await {
                                    Ok(_) => {
                                        last_capture_time.store(frame.timestamp as u64, Ordering::SeqCst);
                                        total_captures_today.fetch_add(1, Ordering::SeqCst);
                                    }
                                    Err(e) => {
                                        error!("Failed to save frame: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to capture screen: {}", e);
                            }
                        }
                    }
                }
            }

            is_running.store(false, Ordering::SeqCst);
            info!("Daemon capture loop stopped");
        });

        Ok(())
    }

    /// 停止守护进程
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
        self.is_running.store(false, Ordering::SeqCst);
        info!("Daemon stopped");
    }

    /// 暂停/恢复录制
    pub fn set_paused(&self, paused: bool) {
        self.is_paused.store(paused, Ordering::SeqCst);
        info!("Daemon paused: {}", paused);
    }

    /// 获取状态
    pub fn status(&self) -> DaemonStatus {
        let last_time = self.last_capture_time.load(Ordering::SeqCst);
        DaemonStatus {
            is_running: self.is_running.load(Ordering::SeqCst),
            is_paused: self.is_paused.load(Ordering::SeqCst),
            is_idle: self.is_idle.load(Ordering::SeqCst),
            idle_time_ms: self.idle_time_ms.load(Ordering::SeqCst),
            last_capture_time: if last_time > 0 {
                Some(last_time as i64)
            } else {
                None
            },
            total_captures_today: self.total_captures_today.load(Ordering::SeqCst),
        }
    }

    /// 立即执行一次截图
    pub fn capture_now(&self) -> anyhow::Result<()> {
        // 这个方法在前端点击"立即截图"按钮时调用
        // 实际实现需要向截图循环发送信号
        // 当前仅记录日志，未来可以通过额外的 channel 实现
        info!("Manual capture requested");
        Ok(())
    }

    /// 更新配置
    pub fn update_config(
        &mut self,
        capture_interval_ms: Option<u64>,
        idle_threshold_ms: Option<u64>,
        similarity_threshold: Option<u32>,
    ) {
        if let Some(interval) = capture_interval_ms {
            self.capture_interval_ms = interval;
            info!("Updated capture interval: {}ms", interval);
        }
        if let Some(threshold) = idle_threshold_ms {
            self.idle_threshold_ms = threshold;
            info!("Updated idle threshold: {}ms", threshold);
        }
        if let Some(threshold) = similarity_threshold {
            self.similarity_threshold = threshold;
            info!("Updated similarity threshold: {}", threshold);
        }
    }

    /// 保存帧到数据库
    async fn save_frame(
        db: &Database,
        frame: &capture::CapturedFrame,
        context: &FocusContext,
        phash: &[u8; 8],
    ) -> anyhow::Result<()> {
        use crate::db::models::NewTrace;

        // 压缩为 WebP 并保存文件
        let image_path = db.save_screenshot(&frame.pixels, frame.width, frame.height)?;

        // 将感知哈希转换为 hex 字符串
        let phash_hex = phash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        // 插入数据库记录
        let trace = NewTrace {
            timestamp: frame.timestamp,
            image_path,
            app_name: context.app_name.clone(),
            window_title: context.window_title.clone(),
            is_fullscreen: context.is_fullscreen,
            window_x: context.bounds.map(|b| b.0),
            window_y: context.bounds.map(|b| b.1),
            window_w: context.bounds.map(|b| b.2),
            window_h: context.bounds.map(|b| b.3),
            is_idle: false,
            ocr_text: None,
            phash: Some(phash_hex.into_bytes()),
        };

        let (trace_id, session_id) = db.insert_trace(&trace)?;
        debug!("Frame saved: {}", trace.image_path);
        debug!(
            "Trace inserted: id={}, session_id={:?}",
            trace_id, session_id
        );

        Ok(())
    }
}
