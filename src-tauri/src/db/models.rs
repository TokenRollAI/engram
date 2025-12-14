//! 数据模型定义

use serde::{Deserialize, Serialize};

/// 活动会话（用户行为 Session）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySession {
    pub id: i64,
    pub app_name: String,
    pub start_time: i64,
    pub end_time: i64,
    pub start_trace_id: Option<i64>,
    pub end_trace_id: Option<i64>,
    pub trace_count: u32,
    pub context_text: Option<String>,
    pub entities_json: Option<String>,
    pub key_actions_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 会话事件：单条 trace 的 VLM 结论
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySessionEvent {
    pub id: i64,
    pub session_id: i64,
    pub trace_id: i64,
    pub timestamp: i64,
    pub summary: Option<String>,
    pub action_description: Option<String>,
    pub activity_type: Option<String>,
    pub confidence: Option<f32>,
    pub entities_json: Option<String>,
    pub is_key_action: bool,
    pub created_at: i64,
}

/// Chat 线程（与“活动 Session”概念区分）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatThread {
    pub id: i64,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Chat 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub thread_id: i64,
    pub role: String,
    pub content: String,
    pub context_json: Option<String>,
    pub created_at: i64,
}

/// 痕迹记录（用于插入）
#[derive(Debug, Clone)]
pub struct NewTrace {
    pub timestamp: i64,
    pub image_path: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub is_fullscreen: bool,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_w: Option<u32>,
    pub window_h: Option<u32>,
    pub is_idle: bool,
    pub ocr_text: Option<String>,
    pub phash: Option<Vec<u8>>,
}

/// 痕迹记录（从数据库读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub id: i64,
    pub timestamp: i64,
    pub image_path: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub is_fullscreen: bool,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_w: Option<u32>,
    pub window_h: Option<u32>,
    pub is_idle: bool,
    pub ocr_text: Option<String>,
    pub activity_session_id: Option<i64>,
    pub is_key_action: bool,
    pub created_at: i64,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub trace: Trace,
    pub score: f32,
    pub highlights: Vec<TextHighlight>,
}

/// 文本高亮
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextHighlight {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// 摘要记录（用于插入）
#[derive(Debug, Clone)]
pub struct NewSummary {
    pub start_time: i64,
    pub end_time: i64,
    pub summary_type: String,
    pub content: String,
    pub structured_data: Option<String>,
    pub trace_count: Option<u32>,
}

/// 摘要记录（从数据库读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub id: i64,
    pub start_time: i64,
    pub end_time: i64,
    pub summary_type: String,
    pub content: String,
    pub structured_data: Option<String>,
    pub trace_count: Option<u32>,
    pub created_at: i64,
}

/// 实体（用于插入）
#[derive(Debug, Clone)]
pub struct NewEntity {
    pub name: String,
    pub entity_type: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub metadata: Option<String>,
}

/// 实体（从数据库读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub mention_count: u32,
    pub first_seen: i64,
    pub last_seen: i64,
    pub metadata: Option<String>,
}

/// 简化的实体（用于 JSON 序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRef {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
}

/// 存储统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_traces: u64,
    pub total_summaries: u64,
    pub total_entities: u64,
    pub database_size_bytes: u64,
    pub screenshots_size_bytes: u64,
    pub oldest_trace_time: Option<i64>,
}

/// 应用使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStat {
    pub app_name: String,
    pub frame_count: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub duration_seconds: u64,
}

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub capture_interval_ms: u64,
    pub idle_threshold_ms: u64,
    pub similarity_threshold: u32,
    pub hot_data_days: u32,
    pub warm_data_days: u32,
    pub summary_interval_min: u32,
    pub session_gap_threshold_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_interval_ms: 2000,
            idle_threshold_ms: 30000,
            similarity_threshold: 5,
            hot_data_days: 7,
            warm_data_days: 30,
            summary_interval_min: 15,
            session_gap_threshold_ms: 300000,
        }
    }
}

/// 黑名单规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistRule {
    pub id: i64,
    pub rule_type: String,
    pub pattern: String,
    pub enabled: bool,
    pub created_at: i64,
}
