//! Engram - Local-first semantic memory augmentation system
//!
//! 核心库，提供屏幕捕获、VLM 理解、向量化和数据持久化功能。

pub mod ai;
pub mod commands;
pub mod daemon;
pub mod db;

use std::sync::Arc;
use tokio::sync::RwLock;

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

        Ok(Self { db, daemon, vlm, embedder })
    }

    /// 初始化 AI 模块（延迟加载）
    pub async fn initialize_ai(&self) -> anyhow::Result<()> {
        // 自动检测并初始化 VLM（优先本地 Ollama/vLLM）
        {
            match ai::VlmEngine::auto_detect().await {
                Ok(mut engine) => {
                    if let Err(e) = engine.initialize().await {
                        tracing::warn!("Failed to initialize VLM engine: {}", e);
                    } else {
                        tracing::info!("VLM engine initialized (backend: {})", engine.backend_name());
                        *self.vlm.write().await = Some(engine);
                    }
                }
                Err(e) => {
                    tracing::info!("VLM not available: {}", e);
                }
            }
        }

        // 初始化嵌入模型（支持 API 或回退到本地）
        {
            let mut embedder = self.embedder.write().await;
            if let Err(e) = embedder.initialize().await {
                tracing::warn!("Failed to initialize embedder: {}", e);
            } else {
                tracing::info!("Embedder initialized (backend: {})", embedder.backend_name());
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
