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
pub use daemon::EngramDaemon;
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
}

impl AppState {
    /// 创建新的应用状态
    pub async fn new() -> anyhow::Result<Self> {
        let db = Arc::new(Database::new()?);
        let daemon = Arc::new(RwLock::new(EngramDaemon::new(db.clone())?));
        let vlm = Arc::new(RwLock::new(None)); // 延迟初始化
        let embedder = Arc::new(RwLock::new(TextEmbedder::new()));

        let state = Self { db, daemon, vlm, embedder };

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
        } else {
            info!("No saved AI config found, skipping auto-initialization");
        }
    }

    /// 初始化 AI 模块（延迟加载）
    pub async fn initialize_ai(&self) -> anyhow::Result<()> {
        // 自动检测并初始化 VLM（优先本地 Ollama/vLLM）
        {
            match ai::VlmEngine::auto_detect().await {
                Ok(mut engine) => {
                    if let Err(e) = engine.initialize().await {
                        warn!("Failed to initialize VLM engine: {}", e);
                    } else {
                        info!("VLM engine initialized (backend: {})", engine.backend_name());
                        *self.vlm.write().await = Some(engine);
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

        Ok(())
    }

    /// 检查 VLM 是否可用
    pub async fn is_vlm_ready(&self) -> bool {
        let vlm = self.vlm.read().await;
        vlm.as_ref().map(|v| v.is_running()).unwrap_or(false)
    }
}
