//! 后台守护进程模块
//!
//! 负责定时截图、上下文感知和图像处理。

mod capture;
mod context;
mod hasher;

pub use capture::ScreenCapture;
pub use context::{FocusContext, WindowWatcher};
pub use hasher::PerceptualHasher;

use crate::db::Database;
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub last_capture_time: Option<i64>,
    pub total_captures_today: u64,
}

/// Engram 后台守护进程
pub struct EngramDaemon {
    db: Arc<Database>,
    is_running: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    capture_interval_ms: u64,
    idle_threshold_ms: u64,
    similarity_threshold: u32,
    shutdown_tx: Option<mpsc::Sender<()>>,
    last_capture_time: Option<i64>,
    total_captures_today: u64,
}

impl EngramDaemon {
    /// 创建新的守护进程
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            is_running: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            capture_interval_ms: DEFAULT_CAPTURE_INTERVAL_MS,
            idle_threshold_ms: DEFAULT_IDLE_THRESHOLD_MS,
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            shutdown_tx: None,
            last_capture_time: None,
            total_captures_today: 0,
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
        let db = self.db.clone();
        let interval_ms = self.capture_interval_ms;
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
            let mut hasher = PerceptualHasher::new();
            let mut last_hash: Option<[u8; 8]> = None;

            info!("Daemon capture loop started");

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
                                if let Err(e) = Self::save_frame(&db, &frame, &context).await {
                                    error!("Failed to save frame: {}", e);
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
        DaemonStatus {
            is_running: self.is_running.load(Ordering::SeqCst),
            is_paused: self.is_paused.load(Ordering::SeqCst),
            is_idle: false, // TODO: 实现闲置检测
            last_capture_time: self.last_capture_time,
            total_captures_today: self.total_captures_today,
        }
    }

    /// 保存帧到数据库
    async fn save_frame(
        db: &Database,
        frame: &capture::CapturedFrame,
        context: &FocusContext,
    ) -> anyhow::Result<()> {
        use crate::db::models::NewTrace;

        // 压缩为 WebP 并保存文件
        let image_path = db.save_screenshot(&frame.pixels, frame.width, frame.height)?;

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
            ocr_json: None,
            phash: None,
        };

        db.insert_trace(&trace)?;
        debug!("Frame saved: {}", trace.image_path);

        Ok(())
    }
}
