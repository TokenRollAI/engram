//! 文本嵌入模块
//!
//! 支持两种后端：
//! 1. OpenAI 兼容 API（优先）- 远程或本地服务
//! 2. 本地 fastembed（回退）- 离线可用
//!
//! 性能优化：
//! - 批量处理：累积文本后批量嵌入
//! - 内存管理：模型按需加载，空闲时可释放

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// 嵌入后端类型
#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddingBackend {
    /// OpenAI 兼容 API
    OpenAiCompatible {
        endpoint: String,
        model: String,
        api_key: Option<String>,
    },
    /// 本地 fastembed
    Local,
}

/// 嵌入配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// API 端点（如果为空则使用本地）
    #[serde(default)]
    pub endpoint: Option<String>,
    /// 模型名称
    #[serde(default = "default_model")]
    pub model: String,
    /// API 密钥
    #[serde(default)]
    pub api_key: Option<String>,
}

fn default_model() -> String {
    "text-embedding-3-small".to_string()
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            endpoint: None, // 默认使用本地
            model: default_model(),
            api_key: None,
        }
    }
}

impl EmbeddingConfig {
    /// 创建本地配置
    pub fn local() -> Self {
        Self::default()
    }

    /// 创建 OpenAI 配置
    pub fn openai(api_key: &str) -> Self {
        Self {
            endpoint: Some("https://api.openai.com/v1".to_string()),
            model: "text-embedding-3-small".to_string(),
            api_key: Some(api_key.to_string()),
        }
    }

    /// 创建自定义 API 配置
    pub fn custom(endpoint: &str, model: &str, api_key: Option<&str>) -> Self {
        Self {
            endpoint: Some(endpoint.to_string()),
            model: model.to_string(),
            api_key: api_key.map(|s| s.to_string()),
        }
    }

    /// 创建 Ollama 配置
    pub fn ollama(model: &str) -> Self {
        Self {
            endpoint: Some("http://127.0.0.1:11434/v1".to_string()),
            model: model.to_string(),
            api_key: None,
        }
    }
}

/// 文本嵌入器
pub struct TextEmbedder {
    /// 配置
    config: EmbeddingConfig,
    /// 当前使用的后端
    backend: EmbeddingBackend,
    /// HTTP 客户端（API 模式）
    client: reqwest::Client,
    /// fastembed 模型（本地模式）
    local_model: Mutex<Option<fastembed::TextEmbedding>>,
    /// 是否已初始化
    initialized: bool,
    /// 向量维度
    embedding_dim: usize,
    /// 最后使用时间（用于内存管理）
    last_used: Mutex<Instant>,
    /// 模型空闲超时（秒）
    idle_timeout_secs: u64,
}

impl TextEmbedder {
    /// 创建新的文本嵌入器
    pub fn new() -> Self {
        Self::with_config(EmbeddingConfig::default())
    }

    /// 使用配置创建嵌入器
    pub fn with_config(config: EmbeddingConfig) -> Self {
        let backend = if config.endpoint.is_some() {
            EmbeddingBackend::OpenAiCompatible {
                endpoint: config.endpoint.clone().unwrap(),
                model: config.model.clone(),
                api_key: config.api_key.clone(),
            }
        } else {
            EmbeddingBackend::Local
        };

        // 根据模型设置维度
        let embedding_dim = match config.model.as_str() {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => 384, // MiniLM 默认
        };

        Self {
            config,
            backend,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            local_model: Mutex::new(None),
            initialized: false,
            embedding_dim,
            last_used: Mutex::new(Instant::now()),
            idle_timeout_secs: 300, // 5 分钟空闲后可释放
        }
    }

    /// 初始化嵌入器
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        debug!(
            "Embedding config: endpoint={:?}, model={}, api_key_set={}",
            self.config.endpoint,
            self.config.model,
            self.config
                .api_key
                .as_deref()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        );

        match &self.backend {
            EmbeddingBackend::OpenAiCompatible {
                endpoint,
                model,
                api_key,
            } => {
                info!("Initializing embedding with OpenAI-compatible API");
                info!("  Endpoint: {}", endpoint);
                info!("  Model: {}", model);

                // 验证 API 连接
                if let Err(e) = self.test_api_connection(endpoint, api_key.as_deref()).await {
                    warn!("API connection failed: {}, falling back to local", e);
                    self.fallback_to_local()?;
                } else {
                    info!("Embedding API is ready");
                }
            }
            EmbeddingBackend::Local => {
                self.init_local_model()?;
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// 同步初始化（兼容旧接口）
    pub fn initialize_sync(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        // 如果配置了 API 但无法异步测试，直接回退到本地
        if matches!(self.backend, EmbeddingBackend::OpenAiCompatible { .. }) {
            warn!("Sync init with API config, falling back to local");
            self.fallback_to_local()?;
        } else {
            self.init_local_model()?;
        }

        self.initialized = true;
        Ok(())
    }

    /// 测试 API 连接
    async fn test_api_connection(&self, endpoint: &str, api_key: Option<&str>) -> Result<()> {
        let url = format!("{}/models", endpoint);
        let mut req = self.client.get(&url);

        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("API returned {}", resp.status()));
        }
        Ok(())
    }

    /// 初始化本地模型
    fn init_local_model(&mut self) -> Result<()> {
        info!("Initializing local embedding model (all-MiniLM-L6-v2)...");

        let model = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(true),
        )?;

        *self.local_model.lock().unwrap() = Some(model);
        self.embedding_dim = 384;
        self.backend = EmbeddingBackend::Local;

        info!("Local embedding model initialized");
        Ok(())
    }

    /// 回退到本地模式
    fn fallback_to_local(&mut self) -> Result<()> {
        self.backend = EmbeddingBackend::Local;
        self.init_local_model()
    }

    /// 检查是否已初始化
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// 获取向量维度
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }

    /// 获取后端名称
    pub fn backend_name(&self) -> String {
        match &self.backend {
            EmbeddingBackend::OpenAiCompatible { endpoint, .. } => {
                if endpoint.contains("openai.com") {
                    "OpenAI".to_string()
                } else if endpoint.contains("11434") {
                    "Ollama".to_string()
                } else {
                    format!("API ({})", endpoint)
                }
            }
            EmbeddingBackend::Local => "Local (MiniLM)".to_string(),
        }
    }

    /// 嵌入单个文本
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No embedding returned"))
    }

    /// 嵌入单个文本（同步版本，仅本地模式）
    pub fn embed_sync(&self, text: &str) -> Result<Vec<f32>> {
        // 更新最后使用时间
        *self.last_used.lock().unwrap() = Instant::now();

        match &self.backend {
            EmbeddingBackend::Local => {
                let model_guard = self.local_model.lock().unwrap();
                let model = model_guard
                    .as_ref()
                    .ok_or_else(|| anyhow!("Local model not initialized"))?;

                let truncated = Self::truncate_text(text, 512);
                let embeddings = model.embed(vec![truncated], None)?;

                embeddings
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow!("No embedding returned"))
            }
            EmbeddingBackend::OpenAiCompatible { .. } => {
                Err(anyhow!("Sync embed not supported for API backend"))
            }
        }
    }

    /// 批量嵌入文本
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // 更新最后使用时间
        *self.last_used.lock().unwrap() = Instant::now();

        match &self.backend {
            EmbeddingBackend::OpenAiCompatible {
                endpoint,
                model,
                api_key,
            } => {
                self.embed_with_api(endpoint, model, api_key.as_deref(), texts)
                    .await
            }
            EmbeddingBackend::Local => self.embed_with_local(texts),
        }
    }

    /// 使用 API 嵌入
    async fn embed_with_api(
        &self,
        endpoint: &str,
        model: &str,
        api_key: Option<&str>,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let truncated: Vec<String> = texts
            .iter()
            .map(|t| Self::truncate_text(t, 8000)) // OpenAI 支持更长的文本
            .collect();

        let request = serde_json::json!({
            "model": model,
            "input": truncated
        });

        let url = format!("{}/embeddings", endpoint);
        let mut req = self.client.post(&url).json(&request);

        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(anyhow!("Embedding API error: {}", error));
        }

        #[derive(Deserialize)]
        struct EmbeddingResponse {
            data: Vec<EmbeddingData>,
        }

        #[derive(Deserialize)]
        struct EmbeddingData {
            embedding: Vec<f32>,
        }

        let result: EmbeddingResponse = response.json().await?;
        let embeddings: Vec<Vec<f32>> = result.data.into_iter().map(|d| d.embedding).collect();

        debug!("Embedded {} texts via API", embeddings.len());
        Ok(embeddings)
    }

    /// 使用本地模型嵌入
    fn embed_with_local(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let model_guard = self.local_model.lock().unwrap();
        let model = model_guard
            .as_ref()
            .ok_or_else(|| anyhow!("Local model not initialized"))?;

        let truncated: Vec<String> = texts.iter().map(|t| Self::truncate_text(t, 512)).collect();

        let embeddings = model.embed(truncated, None)?;
        debug!("Embedded {} texts locally", embeddings.len());

        Ok(embeddings)
    }

    /// 截断文本到指定字符数
    fn truncate_text(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            text.to_string()
        } else {
            text.chars().take(max_chars).collect()
        }
    }

    /// 计算两个向量的余弦相似度
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }

    /// L2 归一化向量
    pub fn l2_normalize(vec: &mut [f32]) {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in vec.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// 将向量序列化为二进制
    pub fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// 从二进制反序列化向量
    pub fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// 检查模型是否空闲过久
    pub fn is_idle(&self) -> bool {
        let last_used = *self.last_used.lock().unwrap();
        last_used.elapsed().as_secs() > self.idle_timeout_secs
    }

    /// 获取空闲时间（秒）
    pub fn idle_time_secs(&self) -> u64 {
        self.last_used.lock().unwrap().elapsed().as_secs()
    }

    /// 释放本地模型以节省内存
    pub fn unload_model(&self) {
        if matches!(self.backend, EmbeddingBackend::Local) {
            let mut model_guard = self.local_model.lock().unwrap();
            if model_guard.is_some() {
                *model_guard = None;
                info!("Local embedding model unloaded to save memory");
            }
        }
    }

    /// 检查本地模型是否已加载
    pub fn is_model_loaded(&self) -> bool {
        self.local_model.lock().unwrap().is_some()
    }

    /// 确保模型已加载（按需加载）
    pub fn ensure_model_loaded(&mut self) -> Result<()> {
        if matches!(self.backend, EmbeddingBackend::Local) {
            let model_guard = self.local_model.lock().unwrap();
            if model_guard.is_none() {
                drop(model_guard); // 释放锁
                info!("Re-loading local embedding model...");
                self.init_local_model()?;
            }
        }
        Ok(())
    }

    /// 获取内存使用估计（MB）
    pub fn estimated_memory_mb(&self) -> f64 {
        if matches!(self.backend, EmbeddingBackend::Local) && self.is_model_loaded() {
            // MiniLM-L6 约 90MB
            90.0
        } else {
            0.0
        }
    }
}

impl Default for TextEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

/// 嵌入队列 - 用于批处理
pub struct EmbeddingQueue {
    pending: Vec<(String, i64)>,
    batch_size: usize,
    last_flush: Instant,
    flush_interval_secs: u64,
}

impl EmbeddingQueue {
    pub fn new(batch_size: usize) -> Self {
        Self {
            pending: Vec::new(),
            batch_size,
            last_flush: Instant::now(),
            flush_interval_secs: 30, // 30 秒强制刷新
        }
    }

    /// 设置强制刷新间隔
    pub fn with_flush_interval(mut self, secs: u64) -> Self {
        self.flush_interval_secs = secs;
        self
    }

    pub fn enqueue(&mut self, text: String, trace_id: i64) {
        self.pending.push((text, trace_id));
    }

    pub fn should_flush(&self) -> bool {
        self.pending.len() >= self.batch_size
            || (self.pending.len() > 0
                && self.last_flush.elapsed().as_secs() >= self.flush_interval_secs)
    }

    pub fn drain(&mut self) -> Vec<(String, i64)> {
        self.last_flush = Instant::now();
        std::mem::take(&mut self.pending)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// 获取自上次刷新以来的秒数
    pub fn secs_since_flush(&self) -> u64 {
        self.last_flush.elapsed().as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_presets() {
        let local = EmbeddingConfig::local();
        assert!(local.endpoint.is_none());

        let openai = EmbeddingConfig::openai("sk-test");
        assert!(openai.endpoint.unwrap().contains("openai.com"));
        assert!(openai.api_key.is_some());

        let ollama = EmbeddingConfig::ollama("nomic-embed-text");
        assert!(ollama.endpoint.unwrap().contains("11434"));
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((TextEmbedder::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!(TextEmbedder::cosine_similarity(&a, &c).abs() < 1e-6);
    }

    #[test]
    fn test_serialize_deserialize() {
        let embedding = vec![1.0, 2.0, 3.0, -4.5];
        let bytes = TextEmbedder::serialize_embedding(&embedding);
        let restored = TextEmbedder::deserialize_embedding(&bytes);
        assert_eq!(embedding, restored);
    }

    #[test]
    fn test_l2_normalize() {
        let mut vec = vec![3.0, 4.0];
        TextEmbedder::l2_normalize(&mut vec);
        assert!((vec[0] - 0.6).abs() < 1e-6);
        assert!((vec[1] - 0.8).abs() < 1e-6);
    }
}
