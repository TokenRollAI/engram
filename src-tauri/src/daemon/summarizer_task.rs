//! 定时摘要任务
//!
//! 定期生成屏幕活动摘要并存储到数据库。

use crate::ai::summarizer::{GeneratedSummary, Summarizer, SummarizerConfig, SummaryType};
use crate::db::models::{NewEntity, NewSummary};
use crate::db::Database;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// 摘要间隔（毫秒）- 15 分钟
const DEFAULT_SUMMARY_INTERVAL_MS: u64 = 15 * 60 * 1000;

/// 每日摘要时间 (UTC 小时) - 默认 23:00
const DAILY_SUMMARY_HOUR: u32 = 23;

/// 摘要任务配置
#[derive(Debug, Clone)]
pub struct SummarizerTaskConfig {
    /// 摘要间隔（毫秒）
    pub interval_ms: u64,
    /// LLM 配置
    pub llm_config: SummarizerConfig,
    /// 是否启用
    pub enabled: bool,
}

impl Default for SummarizerTaskConfig {
    fn default() -> Self {
        Self {
            interval_ms: DEFAULT_SUMMARY_INTERVAL_MS,
            llm_config: SummarizerConfig::default(),
            enabled: true,
        }
    }
}

/// 定时摘要任务
pub struct SummarizerTask {
    db: Arc<Database>,
    summarizer: Arc<Mutex<Option<Summarizer>>>,
    config: SummarizerTaskConfig,
    is_running: Arc<AtomicBool>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl SummarizerTask {
    /// 创建新的摘要任务
    pub fn new(db: Arc<Database>, config: SummarizerTaskConfig) -> Self {
        Self {
            db,
            summarizer: Arc::new(Mutex::new(None)),
            config,
            is_running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
        }
    }

    /// 启动摘要任务
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if !self.config.enabled {
            info!("Summarizer task is disabled");
            return Ok(());
        }

        if self.is_running.load(Ordering::SeqCst) {
            warn!("Summarizer task is already running");
            return Ok(());
        }

        info!("Starting summarizer task...");

        // 初始化 Summarizer
        let mut summarizer = Summarizer::new(self.config.llm_config.clone());
        match summarizer.initialize().await {
            Ok(_) => {
                info!("Summarizer initialized: {}", summarizer.backend_name());
                *self.summarizer.lock().await = Some(summarizer);
            }
            Err(e) => {
                warn!("Failed to initialize summarizer: {}. Task will retry later.", e);
                // 不返回错误，任务仍然启动，后续会重试
            }
        }

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let is_running = self.is_running.clone();
        let db = self.db.clone();
        let summarizer = self.summarizer.clone();
        let interval_ms = self.config.interval_ms;
        let llm_config = self.config.llm_config.clone();

        is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(interval_ms));
            let mut last_daily_summary_day: Option<u32> = None;

            info!(
                "Summarizer task loop started (interval: {}ms)",
                interval_ms
            );

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Summarizer task received shutdown signal");
                        break;
                    }
                    _ = ticker.tick() => {
                        // 尝试重新初始化 summarizer（如果之前失败）
                        {
                            let mut guard = summarizer.lock().await;
                            if guard.is_none() {
                                let mut new_summarizer = Summarizer::new(llm_config.clone());
                                if new_summarizer.initialize().await.is_ok() {
                                    info!("Summarizer re-initialized successfully");
                                    *guard = Some(new_summarizer);
                                } else {
                                    debug!("Summarizer still not available, will retry");
                                    continue;
                                }
                            }
                        }

                        // 生成短周期摘要
                        let now = chrono::Utc::now();
                        let end_time = now.timestamp_millis();
                        let start_time = end_time - interval_ms as i64;

                        if let Err(e) = Self::generate_short_summary(
                            &db,
                            &summarizer,
                            start_time,
                            end_time,
                        ).await {
                            error!("Failed to generate short summary: {}", e);
                        }

                        // 检查是否需要生成每日摘要
                        let current_hour = now.hour();
                        let current_day = now.day();

                        if current_hour == DAILY_SUMMARY_HOUR {
                            let should_generate = match last_daily_summary_day {
                                Some(day) => day != current_day,
                                None => true,
                            };

                            if should_generate {
                                info!("Generating daily summary...");
                                if let Err(e) = Self::generate_daily_summary(
                                    &db,
                                    &summarizer,
                                ).await {
                                    error!("Failed to generate daily summary: {}", e);
                                } else {
                                    last_daily_summary_day = Some(current_day);
                                }
                            }
                        }
                    }
                }
            }

            is_running.store(false, Ordering::SeqCst);
            info!("Summarizer task loop stopped");
        });

        Ok(())
    }

    /// 停止摘要任务
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
        self.is_running.store(false, Ordering::SeqCst);
        info!("Summarizer task stopped");
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// 生成短周期摘要（15分钟）
    async fn generate_short_summary(
        db: &Arc<Database>,
        summarizer: &Arc<Mutex<Option<Summarizer>>>,
        start_time: i64,
        end_time: i64,
    ) -> anyhow::Result<()> {
        // 获取时间范围内的 traces
        let traces = db.get_traces(start_time, end_time, 100, 0)?;

        if traces.is_empty() {
            debug!("No traces in time range, skipping summary");
            return Ok(());
        }

        info!(
            "Generating short summary for {} traces ({} to {})",
            traces.len(),
            start_time,
            end_time
        );

        // 生成摘要
        let guard = summarizer.lock().await;
        let summarizer_ref = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Summarizer not initialized")
        })?;

        let summary = summarizer_ref
            .generate_summary(&traces, SummaryType::Short)
            .await?;

        // 保存摘要
        Self::save_summary(db, start_time, end_time, "short", &summary, traces.len() as u32)?;

        // 保存实体
        Self::save_entities(db, &summary.entities, end_time)?;

        info!("Short summary saved successfully");
        Ok(())
    }

    /// 生成每日摘要
    async fn generate_daily_summary(
        db: &Arc<Database>,
        summarizer: &Arc<Mutex<Option<Summarizer>>>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let today_end = now.timestamp_millis();

        // 获取今日所有短摘要
        let short_summaries = db.get_summaries(today_start, today_end, Some("short"), 100)?;

        if short_summaries.is_empty() {
            info!("No short summaries for today, skipping daily summary");
            return Ok(());
        }

        // 合并短摘要内容
        let combined_content: String = short_summaries
            .iter()
            .map(|s| s.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        // 获取今日 traces 数量
        let traces = db.get_traces(today_start, today_end, 1000, 0)?;

        info!(
            "Generating daily summary from {} short summaries, {} traces",
            short_summaries.len(),
            traces.len()
        );

        // 使用 LLM 生成每日摘要
        let guard = summarizer.lock().await;
        let summarizer_ref = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Summarizer not initialized")
        })?;

        let summary = summarizer_ref
            .generate_summary(&traces, SummaryType::Daily)
            .await?;

        // 保存每日摘要
        Self::save_summary(
            db,
            today_start,
            today_end,
            "daily",
            &summary,
            traces.len() as u32,
        )?;

        // 保存实体
        Self::save_entities(db, &summary.entities, today_end)?;

        info!("Daily summary saved successfully");
        Ok(())
    }

    /// 保存摘要到数据库
    fn save_summary(
        db: &Arc<Database>,
        start_time: i64,
        end_time: i64,
        summary_type: &str,
        summary: &GeneratedSummary,
        trace_count: u32,
    ) -> anyhow::Result<i64> {
        let structured_data = serde_json::to_string(&serde_json::json!({
            "topics": summary.topics,
            "links": summary.links,
            "activity_breakdown": summary.activity_breakdown,
            "entities": summary.entities,
        }))?;

        let new_summary = NewSummary {
            start_time,
            end_time,
            summary_type: summary_type.to_string(),
            content: summary.content.clone(),
            structured_data: Some(structured_data),
            trace_count: Some(trace_count),
        };

        db.insert_summary(&new_summary)
    }

    /// 保存实体到数据库
    fn save_entities(
        db: &Arc<Database>,
        entities: &[crate::ai::summarizer::ExtractedEntity],
        timestamp: i64,
    ) -> anyhow::Result<()> {
        for entity in entities {
            let new_entity = NewEntity {
                name: entity.name.clone(),
                entity_type: entity.entity_type.clone(),
                first_seen: timestamp,
                last_seen: timestamp,
                metadata: Some(serde_json::to_string(&serde_json::json!({
                    "confidence": entity.confidence,
                }))?),
            };

            if let Err(e) = db.upsert_entity(&new_entity) {
                warn!("Failed to save entity '{}': {}", entity.name, e);
            }
        }

        Ok(())
    }

    /// 手动触发摘要生成
    pub async fn trigger_summary(&self, summary_type: SummaryType) -> anyhow::Result<()> {
        let now = chrono::Utc::now();

        match summary_type {
            SummaryType::Short => {
                let end_time = now.timestamp_millis();
                let start_time = end_time - self.config.interval_ms as i64;
                Self::generate_short_summary(&self.db, &self.summarizer, start_time, end_time)
                    .await
            }
            SummaryType::Daily => {
                Self::generate_daily_summary(&self.db, &self.summarizer).await
            }
        }
    }
}

use chrono::{Datelike, Timelike};
