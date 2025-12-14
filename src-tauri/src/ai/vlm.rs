//! 视觉语言模型 (VLM) 引擎模块
//!
//! 使用 OpenAI 兼容 API 进行屏幕内容理解。
//! 支持本地服务（Ollama、vLLM、LM Studio）和远程服务（OpenAI、Together AI、OpenRouter 等）。

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::time::Duration;
use tracing::{debug, info, warn};

/// 屏幕描述结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenDescription {
    /// 屏幕内容摘要
    pub summary: String,
    /// 提取的文本内容
    pub text_content: Option<String>,
    /// 检测到的应用/网站
    pub detected_app: Option<String>,
    /// 主要活动类型
    pub activity_type: Option<String>,
    /// 关键实体（人名、项目名、URL 等）
    pub entities: Vec<String>,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
}

/// VLM 引擎配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmConfig {
    /// API 端点（如 http://localhost:11434/v1 或 https://api.openai.com/v1）
    pub endpoint: String,
    /// 模型名称（如 qwen3-vl:4b 或 gpt-4o）
    pub model: String,
    /// API 密钥（远程服务需要）
    #[serde(default)]
    pub api_key: Option<String>,
    /// 最大输出 tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// 温度参数
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_tokens() -> u32 {
    512
}

fn default_temperature() -> f32 {
    0.3
}

impl Default for VlmConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:11434/v1".to_string(),
            model: "qwen3-vl:4b".to_string(),
            api_key: None,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
        }
    }
}

impl VlmConfig {
    /// 创建 Ollama 配置
    pub fn ollama(model: &str) -> Self {
        Self {
            endpoint: "http://127.0.0.1:11434/v1".to_string(),
            model: model.to_string(),
            api_key: None,
            ..Default::default()
        }
    }

    /// 创建 OpenAI 配置
    pub fn openai(api_key: &str, model: &str) -> Self {
        Self {
            endpoint: "https://api.openai.com/v1".to_string(),
            model: model.to_string(),
            api_key: Some(api_key.to_string()),
            ..Default::default()
        }
    }

    /// 创建自定义端点配置
    pub fn custom(endpoint: &str, model: &str, api_key: Option<&str>) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            model: model.to_string(),
            api_key: api_key.map(|s| s.to_string()),
            ..Default::default()
        }
    }
}

/// VLM 引擎
pub struct VlmEngine {
    config: VlmConfig,
    client: reqwest::Client,
    is_ready: bool,
}

impl VlmEngine {
    /// 创建新的 VLM 引擎
    pub fn new(config: VlmConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap(),
            is_ready: false,
        }
    }

    /// 自动检测可用的本地服务
    pub async fn auto_detect() -> Result<Self> {
        // 常见的本地服务端点
        let endpoints = [
            ("http://127.0.0.1:11434/v1", "qwen3-vl:4b", "Ollama"),
            ("http://127.0.0.1:8000/v1", "qwen3-vl-4b", "vLLM"),
            ("http://127.0.0.1:1234/v1", "local-model", "LM Studio"),
        ];

        for (endpoint, model, name) in endpoints {
            if Self::check_endpoint(endpoint).await {
                info!("Detected {}, using endpoint: {}", name, endpoint);
                return Ok(Self::new(VlmConfig {
                    endpoint: endpoint.to_string(),
                    model: model.to_string(),
                    api_key: None,
                    ..Default::default()
                }));
            }
        }

        Err(anyhow!(
            "No VLM service detected.\n\
             Options:\n\
             1. Install Ollama: https://ollama.com/download\n\
             2. Run vLLM server: python -m vllm.entrypoints.openai.api_server\n\
             3. Use LM Studio with local server enabled\n\
             4. Configure remote API (OpenAI, Together AI, etc.)"
        ))
    }

    /// 检查端点是否可用
    async fn check_endpoint(endpoint: &str) -> bool {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();

        let url = format!("{}/models", endpoint);
        client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// 初始化引擎（验证连接）
    pub async fn initialize(&mut self) -> Result<()> {
        if self.is_ready {
            return Ok(());
        }

        info!("Initializing VLM engine...");
        info!("  Endpoint: {}", self.config.endpoint);
        info!("  Model: {}", self.config.model);

        // 验证端点可用
        let url = format!("{}/models", self.config.endpoint);
        let mut req = self.client.get(&url);

        if let Some(ref key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("VLM endpoint is ready");
                self.is_ready = true;
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                Err(anyhow!("Endpoint error {}: {}", status, text))
            }
            Err(e) => Err(anyhow!("Failed to connect: {}", e)),
        }
    }

    /// 检查是否就绪
    pub fn is_running(&self) -> bool {
        self.is_ready
    }

    /// 分析屏幕截图
    pub async fn analyze_screen(&self, image: &RgbImage) -> Result<ScreenDescription> {
        if !self.is_ready {
            return Err(anyhow!("VLM engine not initialized"));
        }

        let image_base64 = self.encode_image(image)?;
        self.call_api(&image_base64).await
    }

    /// 调用 OpenAI 兼容 API
    async fn call_api(&self, image_base64: &str) -> Result<ScreenDescription> {
        let request = serde_json::json!({
            "model": self.config.model,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": Self::build_prompt()
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/jpeg;base64,{}", image_base64)
                        }
                    }
                ]
            }],
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature
        });

        let url = format!("{}/chat/completions", self.config.endpoint);

        // 记录请求日志
        info!(
            "VLM API Request: endpoint={}, model={}, max_tokens={}, temperature={}, image_size={}KB",
            self.config.endpoint,
            self.config.model,
            self.config.max_tokens,
            self.config.temperature,
            image_base64.len() / 1024
        );
        debug!("VLM API URL: {}", url);

        let start_time = std::time::Instant::now();

        let mut req = self.client.post(&url).json(&request);

        if let Some(ref key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
            debug!("VLM API: Using API key ({}...)", &key[..key.len().min(8)]);
        }

        let response = req.send().await?;
        let status = response.status();
        let elapsed = start_time.elapsed();

        // 记录响应日志
        info!(
            "VLM API Response: status={}, elapsed={:.2}s",
            status,
            elapsed.as_secs_f64()
        );

        if !status.is_success() {
            let error = response.text().await.unwrap_or_default();
            warn!("VLM API Error: status={}, body={}", status, error);
            return Err(anyhow!("API error {}: {}", status, error));
        }

        let result: serde_json::Value = response.json().await?;

        // 记录使用情况（如果有）
        if let Some(usage) = result.get("usage") {
            info!(
                "VLM API Usage: prompt_tokens={}, completion_tokens={}, total_tokens={}",
                usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                usage.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
            );
        }

        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");

        debug!("VLM API Response content length: {} chars", content.len());

        Self::parse_response(content)
    }

    /// 编码图片为 base64 JPEG
    fn encode_image(&self, image: &RgbImage) -> Result<String> {
        let mut buffer = Cursor::new(Vec::new());

        // 缩放大图
        let image = if image.width() > 1280 || image.height() > 720 {
            let resized = image::imageops::resize(
                image,
                1280,
                720,
                image::imageops::FilterType::Triangle,
            );
            image::DynamicImage::ImageRgb8(resized)
        } else {
            image::DynamicImage::ImageRgb8(image.clone())
        };

        image.write_to(&mut buffer, image::ImageFormat::Jpeg)?;
        Ok(BASE64.encode(buffer.into_inner()))
    }

    /// 构建分析 Prompt
    fn build_prompt() -> &'static str {
        r#"请分析这个屏幕截图，输出以下 JSON 格式（不要输出其他内容）：
```json
{
  "summary": "简短描述用户正在做什么（50字以内）",
  "text_content": "屏幕上的重要文本内容",
  "detected_app": "检测到的应用或网站名称",
  "activity_type": "活动类型：coding/browsing/reading/writing/communication/media/other",
  "entities": ["提取的关键实体：人名、项目名、URL、文件名等"],
  "confidence": 0.95
}
```"#
    }

    /// 解析 VLM 响应
    fn parse_response(content: &str) -> Result<ScreenDescription> {
        let json_str = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        match serde_json::from_str::<ScreenDescription>(json_str) {
            Ok(desc) => Ok(desc),
            Err(e) => {
                warn!("Failed to parse response: {}", e);
                debug!("Raw: {}", content);

                // 降级：返回原始内容
                Ok(ScreenDescription {
                    summary: content.chars().take(200).collect(),
                    text_content: Some(content.to_string()),
                    detected_app: None,
                    activity_type: Some("other".to_string()),
                    entities: Vec::new(),
                    confidence: 0.5,
                })
            }
        }
    }

    /// 获取用于嵌入的文本
    pub fn get_text_for_embedding(desc: &ScreenDescription) -> String {
        let mut parts = vec![desc.summary.clone()];

        if let Some(ref text) = desc.text_content {
            parts.push(text.clone());
        }
        if let Some(ref app) = desc.detected_app {
            parts.push(app.clone());
        }
        if !desc.entities.is_empty() {
            parts.push(desc.entities.join(" "));
        }

        parts.join(" ")
    }

    /// 获取后端名称
    pub fn backend_name(&self) -> String {
        if self.config.endpoint.contains("openai.com") {
            "OpenAI".to_string()
        } else if self.config.endpoint.contains("11434") {
            "Ollama".to_string()
        } else if self.config.endpoint.contains("8000") {
            "vLLM".to_string()
        } else {
            format!("Custom ({})", self.config.endpoint)
        }
    }

    /// 获取配置
    pub fn config(&self) -> &VlmConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response() {
        let json = r#"{
            "summary": "用户在 VS Code 中编写 Rust 代码",
            "text_content": "fn main() { println!(\"Hello\"); }",
            "detected_app": "Visual Studio Code",
            "activity_type": "coding",
            "entities": ["main.rs", "Rust"],
            "confidence": 0.95
        }"#;

        let result = VlmEngine::parse_response(json).unwrap();
        assert_eq!(result.activity_type, Some("coding".to_string()));
        assert_eq!(result.detected_app, Some("Visual Studio Code".to_string()));
    }

    #[test]
    fn test_config_presets() {
        let ollama = VlmConfig::ollama("qwen3-vl:4b");
        assert!(ollama.endpoint.contains("11434"));
        assert!(ollama.api_key.is_none());

        let openai = VlmConfig::openai("sk-test", "gpt-4o");
        assert!(openai.endpoint.contains("openai.com"));
        assert!(openai.api_key.is_some());
    }

    #[test]
    fn test_config_serialization() {
        let config = VlmConfig::openai("sk-test", "gpt-4o");
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: VlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, config.model);
    }
}
