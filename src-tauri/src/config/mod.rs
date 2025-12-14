//! 配置管理模块
//!
//! 使用 TOML 文件存储配置，遵循 XDG 规范：
//! - Linux: ~/.config/engram/Engram/config.toml
//! - macOS: ~/Library/Application Support/com.engram.Engram/config.toml
//! - Windows: %APPDATA%\engram\Engram\config.toml

use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

// 重新导出 AI 相关配置（保持兼容性）
pub use crate::ai::embedding::EmbeddingConfig;
pub use crate::ai::vlm::VlmConfig;
pub use crate::daemon::vlm_task::VlmTaskConfig;

/// 截图捕获配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// 截图间隔（毫秒）
    #[serde(default = "default_capture_interval")]
    pub interval_ms: u64,
    /// 闲置检测阈值（毫秒）
    #[serde(default = "default_idle_threshold")]
    pub idle_threshold_ms: u64,
    /// 相似度阈值（pHash 汉明距离）
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: u32,
}

fn default_capture_interval() -> u64 {
    2000
}
fn default_idle_threshold() -> u64 {
    30000
}
fn default_similarity_threshold() -> u32 {
    5
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_capture_interval(),
            idle_threshold_ms: default_idle_threshold(),
            similarity_threshold: default_similarity_threshold(),
        }
    }
}

/// 数据存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// 热数据保留天数
    #[serde(default = "default_hot_data_days")]
    pub hot_data_days: u32,
    /// 温数据保留天数
    #[serde(default = "default_warm_data_days")]
    pub warm_data_days: u32,
}

fn default_hot_data_days() -> u32 {
    7
}
fn default_warm_data_days() -> u32 {
    30
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            hot_data_days: default_hot_data_days(),
            warm_data_days: default_warm_data_days(),
        }
    }
}

/// 会话管理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// 会话分割阈值（毫秒）- 超过此时间间隔则开启新会话
    #[serde(default = "default_session_gap")]
    pub gap_threshold_ms: u64,
}

fn default_session_gap() -> u64 {
    300000 // 5 分钟
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            gap_threshold_ms: default_session_gap(),
        }
    }
}

/// 摘要生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryConfig {
    /// 自动摘要生成间隔（分钟）
    #[serde(default = "default_summary_interval")]
    pub interval_min: u32,
}

fn default_summary_interval() -> u32 {
    15
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            interval_min: default_summary_interval(),
        }
    }
}

/// 应用配置（顶层结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 截图捕获配置
    #[serde(default)]
    pub capture: CaptureConfig,
    /// 数据存储配置
    #[serde(default)]
    pub storage: StorageConfig,
    /// 会话管理配置
    #[serde(default)]
    pub session: SessionConfig,
    /// 摘要生成配置
    #[serde(default)]
    pub summary: SummaryConfig,
    /// VLM 视觉模型配置
    #[serde(default)]
    pub vlm: VlmConfig,
    /// 文本嵌入配置
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    /// VLM 后台任务配置
    #[serde(default)]
    pub vlm_task: VlmTaskConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            capture: CaptureConfig::default(),
            storage: StorageConfig::default(),
            session: SessionConfig::default(),
            summary: SummaryConfig::default(),
            vlm: VlmConfig::default(),
            embedding: EmbeddingConfig::default(),
            vlm_task: VlmTaskConfig::default(),
        }
    }
}

impl AppConfig {
    /// 获取配置目录路径
    pub fn config_dir() -> Result<PathBuf> {
        if let Some(proj_dirs) = ProjectDirs::from("com", "engram", "Engram") {
            Ok(proj_dirs.config_dir().to_path_buf())
        } else {
            // 回退到 ~/.engram
            let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot find home directory"))?;
            Ok(home.join(".engram"))
        }
    }

    /// 获取配置文件完整路径
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// 从文件加载配置
    ///
    /// 如果文件不存在，返回默认配置并创建文件
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        debug!("Loading config from: {}", path.display());

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: Self = toml::from_str(&content).map_err(|e| {
                warn!("Failed to parse config file: {}, using defaults", e);
                e
            })?;
            info!("Config loaded from: {}", path.display());
            Ok(config)
        } else {
            info!("Config file not found, creating default at: {}", path.display());
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let dir = path.parent().ok_or_else(|| anyhow!("Invalid config path"))?;

        // 确保目录存在
        if !dir.exists() {
            fs::create_dir_all(dir)?;
            debug!("Created config directory: {}", dir.display());
        }

        // 序列化为 TOML
        let content = toml::to_string_pretty(self)?;

        // 写入文件
        fs::write(&path, &content)?;

        // 设置文件权限 (Unix only) - 仅用户可读写
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }

        info!("Config saved to: {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.capture.interval_ms, 2000);
        assert_eq!(config.vlm.model, "qwen3-vl:4b");
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[capture]"));
        assert!(toml_str.contains("[vlm]"));

        // 反序列化回来
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.capture.interval_ms, config.capture.interval_ms);
    }
}
