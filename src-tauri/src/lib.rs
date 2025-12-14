//! Engram - Local-first semantic memory augmentation system
//!
//! 核心库，提供屏幕捕获、OCR、向量化和数据持久化功能。

pub mod commands;
pub mod daemon;
pub mod db;

use std::sync::Arc;
use tokio::sync::RwLock;

pub use daemon::EngramDaemon;
pub use db::Database;

/// 应用全局状态
pub struct AppState {
    /// 数据库连接
    pub db: Arc<Database>,
    /// 后台守护进程
    pub daemon: Arc<RwLock<EngramDaemon>>,
}

impl AppState {
    /// 创建新的应用状态
    pub async fn new() -> anyhow::Result<Self> {
        let db = Arc::new(Database::new()?);
        let daemon = Arc::new(RwLock::new(EngramDaemon::new(db.clone())?));

        Ok(Self { db, daemon })
    }
}
