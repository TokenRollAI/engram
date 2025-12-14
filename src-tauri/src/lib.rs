//! Engram - Local-first semantic memory augmentation system
//!
//! 核心库，提供屏幕捕获、VLM 理解、向量化和数据持久化功能。

pub mod ai;
pub mod commands;
pub mod daemon;
pub mod db;

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

pub use ai::{VlmEngine, ScreenDescription, TextEmbedder};
pub use daemon::{EngramDaemon, VlmTask, VlmTaskConfig, SummarizerTask, SummarizerTaskConfig};
pub use db::Database;

/// 应用全局状态
pub struct AppState {
    /// 数据库连接
    pub db: Arc<Database>,
    /// 后台守护进程
    pub daemon: Arc<RwLock<EngramDaemon>>,
    /// VLM 引擎 (Qwen3-VL)
    pub vlm: Arc<RwLock<Option<VlmEngine>>>,
    /// 文本嵌入器
    pub embedder: Arc<RwLock<TextEmbedder>>,
    /// VLM 分析后台任务
    pub vlm_task: Arc<RwLock<VlmTask>>,
    /// 摘要生成后台任务
    pub summarizer_task: Arc<RwLock<SummarizerTask>>,
}

impl AppState {
    /// 创建新的应用状态
    pub async fn new() -> anyhow::Result<Self> {
        let db = Arc::new(Database::new()?);
        let daemon = Arc::new(RwLock::new(EngramDaemon::new(db.clone())?));
        let vlm = Arc::new(RwLock::new(None)); // 延迟初始化
        let embedder = Arc::new(RwLock::new(TextEmbedder::new()));

        // 创建 VLM 任务（默认启用）
        let vlm_task = Arc::new(RwLock::new(VlmTask::new(
            db.clone(),
            vlm.clone(),
            embedder.clone(),
            VlmTaskConfig::default(),
        )));

        // 创建摘要任务（默认启用）
        let summarizer_task = Arc::new(RwLock::new(SummarizerTask::new(
            db.clone(),
            SummarizerTaskConfig::default(),
        )));

        let state = Self { db, daemon, vlm, embedder, vlm_task, summarizer_task };

        // 检查是否有保存的 AI 配置，如果有则自动初始化
        state.try_auto_initialize_ai().await;

        Ok(state)
    }

    /// 尝试自动初始化 AI（如果有保存的配置）
    async fn try_auto_initialize_ai(&self) {
        // 从数据库加载配置
        let vlm_config = commands::load_vlm_config_from_db(&self.db);
        let embedding_config = commands::load_embedding_config_from_db(&self.db);

        // 检查是否有有效的 VLM 配置（非默认端点或有 API Key）
        let has_custom_vlm = vlm_config.api_key.is_some()
            || !vlm_config.endpoint.contains("127.0.0.1:11434");

        // 检查是否有有效的 Embedding 配置
        let has_custom_embedding = embedding_config.api_key.is_some()
            || embedding_config.endpoint.is_some();

        let mut vlm_initialized = false;

        if has_custom_vlm || has_custom_embedding {
            info!("Found saved AI config, auto-initializing...");

            // 初始化 VLM
            if has_custom_vlm || vlm_config.api_key.is_some() {
                info!("  VLM endpoint: {}, model: {}", vlm_config.endpoint, vlm_config.model);
                let mut engine = ai::VlmEngine::new(vlm_config.clone());
                match engine.initialize().await {
                    Ok(_) => {
                        info!("  VLM initialized successfully (backend: {})", engine.backend_name());
                        *self.vlm.write().await = Some(engine);
                        vlm_initialized = true;
                    }
                    Err(e) => {
                        warn!("  Failed to auto-initialize VLM: {}", e);
                    }
                }
            }

            // 初始化 Embedding
            {
                info!("  Embedding endpoint: {:?}, model: {}",
                      embedding_config.endpoint, embedding_config.model);
                let mut embedder = ai::TextEmbedder::with_config(embedding_config);
                match embedder.initialize().await {
                    Ok(_) => {
                        info!("  Embedder initialized successfully (backend: {})", embedder.backend_name());
                        *self.embedder.write().await = embedder;
                    }
                    Err(e) => {
                        warn!("  Failed to auto-initialize embedder: {}", e);
                    }
                }
            }

            // 如果 VLM 初始化成功，启动 VLM 分析任务和摘要任务
            if vlm_initialized {
                if let Err(e) = self.start_vlm_task().await {
                    warn!("Failed to start VLM task: {}", e);
                }
                // 启动摘要任务（使用相同的 LLM 端点）
                if let Err(e) = self.start_summarizer_task_with_config(vlm_config).await {
                    warn!("Failed to start summarizer task: {}", e);
                }
            }
        } else {
            info!("No saved AI config found, skipping auto-initialization");
        }
    }

    /// 初始化 AI 模块（延迟加载）
    pub async fn initialize_ai(&self) -> anyhow::Result<()> {
        let mut vlm_initialized = false;

        // 自动检测并初始化 VLM（优先本地 Ollama/vLLM）
        {
            match ai::VlmEngine::auto_detect().await {
                Ok(mut engine) => {
                    if let Err(e) = engine.initialize().await {
                        warn!("Failed to initialize VLM engine: {}", e);
                    } else {
                        info!("VLM engine initialized (backend: {})", engine.backend_name());
                        *self.vlm.write().await = Some(engine);
                        vlm_initialized = true;
                    }
                }
                Err(e) => {
                    info!("VLM not available: {}", e);
                }
            }
        }

        // 初始化嵌入模型（支持 API 或回退到本地）
        {
            let mut embedder = self.embedder.write().await;
            if let Err(e) = embedder.initialize().await {
                warn!("Failed to initialize embedder: {}", e);
            } else {
                info!("Embedder initialized (backend: {})", embedder.backend_name());
            }
        }

        // 如果 VLM 初始化成功，启动 VLM 分析任务和摘要任务
        if vlm_initialized {
            self.start_vlm_task().await?;
            // 启动摘要任务（使用默认配置）
            if let Err(e) = self.start_summarizer_task().await {
                warn!("Failed to start summarizer task: {}", e);
            }
        }

        Ok(())
    }

    /// 启动 VLM 分析后台任务
    pub async fn start_vlm_task(&self) -> anyhow::Result<()> {
        let mut task = self.vlm_task.write().await;
        task.start()
    }

    /// 停止 VLM 分析后台任务
    pub async fn stop_vlm_task(&self) {
        let mut task = self.vlm_task.write().await;
        task.stop();
    }

    /// 使用新配置重启 VLM 分析后台任务
    pub async fn restart_vlm_task(&self, config: VlmTaskConfig) -> anyhow::Result<()> {
        // 停止旧任务
        {
            let mut task = self.vlm_task.write().await;
            task.stop();
        }

        // 创建新任务
        let new_task = VlmTask::new(
            self.db.clone(),
            self.vlm.clone(),
            self.embedder.clone(),
            config,
        );

        // 替换并启动
        {
            let mut task = self.vlm_task.write().await;
            *task = new_task;
            task.start()?;
        }

        Ok(())
    }

    /// 启动摘要任务
    pub async fn start_summarizer_task(&self) -> anyhow::Result<()> {
        let mut task = self.summarizer_task.write().await;
        task.start().await
    }

    /// 使用 VLM 配置启动摘要任务
    pub async fn start_summarizer_task_with_config(&self, vlm_config: ai::VlmConfig) -> anyhow::Result<()> {
        // 从 VLM 配置创建 Summarizer 配置（复用 endpoint 和 api_key）
        // 视觉模型也能很好地处理纯文本任务，直接使用用户配置的模型
        let summarizer_config = ai::SummarizerConfig {
            endpoint: vlm_config.endpoint,
            model: vlm_config.model,
            api_key: vlm_config.api_key,
            max_tokens: 1024,
            temperature: 0.3,
        };

        // 创建新的摘要任务配置
        let task_config = SummarizerTaskConfig {
            interval_ms: 15 * 60 * 1000, // 15 分钟
            llm_config: summarizer_config,
            enabled: true,
        };

        // 停止旧任务
        {
            let mut task = self.summarizer_task.write().await;
            task.stop();
        }

        // 创建新任务
        let new_task = SummarizerTask::new(self.db.clone(), task_config);

        // 替换并启动
        {
            let mut task = self.summarizer_task.write().await;
            *task = new_task;
            task.start().await?;
        }

        info!("Summarizer task started (interval: 15 min)");
        Ok(())
    }

    /// 停止摘要任务
    pub async fn stop_summarizer_task(&self) {
        let mut task = self.summarizer_task.write().await;
        task.stop();
    }

    /// 检查 VLM 是否可用
    pub async fn is_vlm_ready(&self) -> bool {
        let vlm = self.vlm.read().await;
        vlm.as_ref().map(|v| v.is_running()).unwrap_or(false)
    }
}
