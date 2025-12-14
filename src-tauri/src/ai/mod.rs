//! AI 推理模块
//!
//! 包含视觉语言模型 (VLM) 和文本嵌入功能。
//! 两者都支持 OpenAI 兼容 API，并可回退到本地模型。

pub mod vlm;
pub mod embedding;

pub use vlm::{VlmEngine, VlmConfig, ScreenDescription};
pub use embedding::{TextEmbedder, EmbeddingConfig, EmbeddingQueue};
