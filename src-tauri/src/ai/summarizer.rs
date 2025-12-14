//! 摘要生成模块
//!
//! 使用 OpenAI 兼容 API 生成屏幕活动摘要和提取实体。
//! 支持本地服务（Ollama、vLLM）和远程服务（OpenAI、Together AI 等）。

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::db::models::Trace;

/// 摘要生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizerConfig {
    /// API 端点
    pub endpoint: String,
    /// 模型名称（纯文本 LLM，如 qwen2.5:7b、gpt-4o-mini）
    pub model: String,
    /// API 密钥
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
    1024
}

fn default_temperature() -> f32 {
    0.3
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:11434/v1".to_string(),
            model: "qwen2.5:7b".to_string(),
            api_key: None,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
        }
    }
}

impl SummarizerConfig {
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

/// 生成的摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSummary {
    /// 摘要内容
    pub content: String,
    /// 主题列表
    pub topics: Vec<String>,
    /// 提取的实体
    pub entities: Vec<ExtractedEntity>,
    /// 相关链接/文件
    pub links: Vec<String>,
    /// 活动类型分布
    pub activity_breakdown: Vec<ActivityCount>,
}

/// 提取的实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    /// 实体名称
    pub name: String,
    /// 实体类型 (person/project/technology/url/file)
    #[serde(rename = "type")]
    pub entity_type: String,
    /// 置信度 (0.0 - 1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

/// 活动计数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityCount {
    /// 活动类型
    pub activity_type: String,
    /// 出现次数
    pub count: u32,
}

/// 摘要类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryType {
    /// 15 分钟短摘要
    Short,
    /// 每日摘要
    Daily,
}

impl SummaryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Daily => "daily",
        }
    }
}

/// 摘要生成器
pub struct Summarizer {
    config: SummarizerConfig,
    client: reqwest::Client,
    is_ready: bool,
}

impl Summarizer {
    /// 创建新的摘要生成器
    pub fn new(config: SummarizerConfig) -> Self {
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
        let endpoints = [
            ("http://127.0.0.1:11434/v1", "qwen2.5:7b", "Ollama"),
            (
                "http://127.0.0.1:8000/v1",
                "Qwen/Qwen2.5-7B-Instruct",
                "vLLM",
            ),
            ("http://127.0.0.1:1234/v1", "local-model", "LM Studio"),
        ];

        for (endpoint, model, name) in endpoints {
            if Self::check_endpoint(endpoint).await {
                info!("Detected {} for summarization: {}", name, endpoint);
                return Ok(Self::new(SummarizerConfig {
                    endpoint: endpoint.to_string(),
                    model: model.to_string(),
                    api_key: None,
                    ..Default::default()
                }));
            }
        }

        Err(anyhow!(
            "No LLM service detected for summarization.\n\
             Please ensure Ollama, vLLM, or LM Studio is running."
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

    /// 初始化（验证连接）
    pub async fn initialize(&mut self) -> Result<()> {
        if self.is_ready {
            return Ok(());
        }

        info!("Initializing Summarizer...");
        info!("  Endpoint: {}", self.config.endpoint);
        info!("  Model: {}", self.config.model);

        let url = format!("{}/models", self.config.endpoint);
        let mut req = self.client.get(&url);

        if let Some(ref key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Summarizer is ready");
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

    /// 生成摘要
    pub async fn generate_summary(
        &self,
        traces: &[Trace],
        summary_type: SummaryType,
    ) -> Result<GeneratedSummary> {
        if !self.is_ready {
            return Err(anyhow!("Summarizer not initialized"));
        }

        if traces.is_empty() {
            return Ok(GeneratedSummary {
                content: "No activity recorded in this period.".to_string(),
                topics: Vec::new(),
                entities: Vec::new(),
                links: Vec::new(),
                activity_breakdown: Vec::new(),
            });
        }

        let context = self.build_context(traces);
        let prompt = self.build_summary_prompt(&context, summary_type);

        let result = self.call_api(&prompt).await?;
        self.parse_summary_response(&result)
    }

    /// 从摘要中提取实体
    pub async fn extract_entities(&self, text: &str) -> Result<Vec<ExtractedEntity>> {
        if !self.is_ready {
            return Err(anyhow!("Summarizer not initialized"));
        }

        let prompt = self.build_entity_prompt(text);
        let result = self.call_api(&prompt).await?;
        self.parse_entity_response(&result)
    }

    /// 构建上下文
    fn build_context(&self, traces: &[Trace]) -> String {
        let mut context = String::new();

        for trace in traces.iter().take(50) {
            // 限制上下文大小
            let time = chrono::DateTime::from_timestamp_millis(trace.timestamp)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();

            let app = trace.app_name.as_deref().unwrap_or("Unknown");
            let title = trace.window_title.as_deref().unwrap_or("");
            let ocr = trace
                .ocr_text
                .as_ref()
                .map(|t| t.chars().take(200).collect::<String>())
                .unwrap_or_default();

            context.push_str(&format!(
                "[{}] {} - {}\n{}\n---\n",
                time,
                app,
                title.chars().take(100).collect::<String>(),
                ocr
            ));
        }

        context
    }

    /// 构建摘要 Prompt
    fn build_summary_prompt(&self, context: &str, summary_type: SummaryType) -> String {
        let period = match summary_type {
            SummaryType::Short => "15 分钟",
            SummaryType::Daily => "一天",
        };

        format!(
            r#"你是一个工作记录助手。根据以下屏幕活动记录，生成{period}的工作摘要。

活动记录：
{context}

请输出以下 JSON 格式（不要输出其他内容）：
```json
{{
  "content": "简洁的工作摘要（100-200字）",
  "topics": ["主题1", "主题2"],
  "entities": [
    {{"name": "实体名", "type": "person/project/technology/url/file", "confidence": 0.9}}
  ],
  "links": ["相关链接或文件路径"],
  "activity_breakdown": [
    {{"activity_type": "coding/browsing/reading/writing/communication/media/other", "count": 10}}
  ]
}}
```"#,
            period = period,
            context = context
        )
    }

    /// 构建实体提取 Prompt
    fn build_entity_prompt(&self, text: &str) -> String {
        format!(
            r#"从以下文本中提取命名实体。

文本：
{text}

请输出以下 JSON 格式（不要输出其他内容）：
```json
{{
  "entities": [
    {{"name": "实体名称", "type": "person/project/technology/url/file", "confidence": 0.9}}
  ]
}}
```

实体类型说明：
- person: 人名
- project: 项目名称、仓库名
- technology: 编程语言、框架、工具
- url: 网址
- file: 文件名或路径"#,
            text = text
        )
    }

    /// 调用 API
    async fn call_api(&self, prompt: &str) -> Result<String> {
        let request = serde_json::json!({
            "model": self.config.model,
            "messages": [{
                "role": "user",
                "content": prompt
            }],
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature
        });

        let url = format!("{}/chat/completions", self.config.endpoint);

        debug!("Summarizer API request to: {}", url);

        let start = std::time::Instant::now();

        let mut req = self.client.post(&url).json(&request);

        if let Some(ref key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await?;
        let status = response.status();
        let elapsed = start.elapsed();

        info!(
            "Summarizer API response: status={}, elapsed={:.2}s",
            status,
            elapsed.as_secs_f64()
        );

        if !status.is_success() {
            let error = response.text().await.unwrap_or_default();
            warn!("Summarizer API error: {}", error);
            return Err(anyhow!("API error {}: {}", status, error));
        }

        let result: serde_json::Value = response.json().await?;
        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        debug!("Summarizer response length: {} chars", content.len());

        Ok(content)
    }

    /// 解析摘要响应
    fn parse_summary_response(&self, content: &str) -> Result<GeneratedSummary> {
        let json_str = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        match serde_json::from_str::<GeneratedSummary>(json_str) {
            Ok(summary) => Ok(summary),
            Err(e) => {
                warn!("Failed to parse summary response: {}", e);
                debug!("Raw content: {}", content);

                // 降级：返回原始内容作为摘要
                Ok(GeneratedSummary {
                    content: content.chars().take(500).collect(),
                    topics: Vec::new(),
                    entities: Vec::new(),
                    links: Vec::new(),
                    activity_breakdown: Vec::new(),
                })
            }
        }
    }

    /// 解析实体响应
    fn parse_entity_response(&self, content: &str) -> Result<Vec<ExtractedEntity>> {
        let json_str = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        #[derive(Deserialize)]
        struct EntityResponse {
            entities: Vec<ExtractedEntity>,
        }

        match serde_json::from_str::<EntityResponse>(json_str) {
            Ok(resp) => Ok(resp.entities),
            Err(e) => {
                warn!("Failed to parse entity response: {}", e);
                Ok(Vec::new())
            }
        }
    }

    /// 获取配置
    pub fn config(&self) -> &SummarizerConfig {
        &self.config
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_presets() {
        let ollama = SummarizerConfig::ollama("qwen2.5:7b");
        assert!(ollama.endpoint.contains("11434"));
        assert!(ollama.api_key.is_none());

        let openai = SummarizerConfig::openai("sk-test", "gpt-4o-mini");
        assert!(openai.endpoint.contains("openai.com"));
        assert!(openai.api_key.is_some());
    }

    #[test]
    fn test_summary_type() {
        assert_eq!(SummaryType::Short.as_str(), "short");
        assert_eq!(SummaryType::Daily.as_str(), "daily");
    }

    #[test]
    fn test_parse_summary() {
        let json = r#"{
            "content": "用户在编写 Rust 代码",
            "topics": ["Rust", "编程"],
            "entities": [
                {"name": "Rust", "type": "technology", "confidence": 0.95}
            ],
            "links": ["src/main.rs"],
            "activity_breakdown": [
                {"activity_type": "coding", "count": 5}
            ]
        }"#;

        let summarizer = Summarizer::new(SummarizerConfig::default());
        let result = summarizer.parse_summary_response(json).unwrap();

        assert_eq!(result.content, "用户在编写 Rust 代码");
        assert_eq!(result.topics.len(), 2);
        assert_eq!(result.entities.len(), 1);
        assert_eq!(result.entities[0].entity_type, "technology");
    }
}
