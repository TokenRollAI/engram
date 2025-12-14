//! AI 推理模块
//!
//! 包含视觉语言模型 (VLM)、文本嵌入和摘要生成功能。
//! 所有功能都支持 OpenAI 兼容 API，并可回退到本地模型。

pub mod embedding;
pub mod summarizer;
pub mod vlm;

pub use embedding::{EmbeddingConfig, EmbeddingQueue, TextEmbedder};
pub use summarizer::{
    ExtractedEntity, GeneratedSummary, Summarizer, SummarizerConfig, SummaryType,
};
pub use vlm::{ScreenDescription, VlmConfig, VlmEngine};
