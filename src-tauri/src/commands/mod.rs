//! Tauri 命令处理模块
//!
//! 提供前端调用的 API 接口。

use crate::ai::{EmbeddingConfig, VlmConfig};
use crate::daemon::{DaemonStatus, VlmTaskConfig};
use crate::db::models::{
    ActivitySession, ActivitySessionEvent, ChatMessage, Entity, SearchResult, Settings,
    StorageStats, Summary, Trace,
};
use crate::AppState;
use serde::Serialize;
use std::path::Path;
use tauri::State;
use tracing::{debug, info, warn};

/// 获取截图状态
#[tauri::command]
pub async fn get_capture_status(state: State<'_, AppState>) -> Result<DaemonStatus, String> {
    let daemon = state.daemon.read().await;
    Ok(daemon.status())
}

/// 启动守护进程
#[tauri::command]
pub async fn start_daemon(state: State<'_, AppState>) -> Result<(), String> {
    info!("Starting daemon...");
    let mut daemon = state.daemon.write().await;
    daemon.start().map_err(|e| e.to_string())
}

/// 停止守护进程
#[tauri::command]
pub async fn stop_daemon(state: State<'_, AppState>) -> Result<(), String> {
    info!("Stopping daemon...");
    let mut daemon = state.daemon.write().await;
    daemon.stop();
    Ok(())
}

/// 切换暂停/恢复
#[tauri::command]
pub async fn toggle_capture(state: State<'_, AppState>, paused: bool) -> Result<(), String> {
    info!("Toggle capture: paused={}", paused);
    let daemon = state.daemon.read().await;
    daemon.set_paused(paused);
    Ok(())
}

/// 立即截图
#[tauri::command]
pub async fn capture_now(state: State<'_, AppState>) -> Result<(), String> {
    info!("Manual capture requested");
    let daemon = state.daemon.read().await;
    daemon.capture_now().map_err(|e| e.to_string())?;
    Ok(())
}

/// 获取痕迹列表
#[tauri::command]
pub async fn get_traces(
    state: State<'_, AppState>,
    start_time: i64,
    end_time: i64,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<Trace>, String> {
    debug!(
        "get_traces: start={}, end={}, limit={:?}, offset={:?}",
        start_time, end_time, limit, offset
    );

    state
        .db
        .get_traces(
            start_time,
            end_time,
            limit.unwrap_or(100),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
}

/// 获取活动会话列表（对外主入口）
#[tauri::command]
pub async fn get_activity_sessions(
    state: State<'_, AppState>,
    start_time: i64,
    end_time: i64,
    limit: Option<u32>,
    offset: Option<u32>,
    app_filter: Option<Vec<String>>,
) -> Result<Vec<ActivitySession>, String> {
    debug!(
        "get_activity_sessions: start={}, end={}, limit={:?}, offset={:?}, apps={:?}",
        start_time, end_time, limit, offset, app_filter
    );

    state
        .db
        .get_activity_sessions(
            start_time,
            end_time,
            app_filter.as_ref(),
            limit.unwrap_or(100),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_activity_session_traces(
    state: State<'_, AppState>,
    session_id: i64,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<Trace>, String> {
    debug!(
        "get_activity_session_traces: session_id={}, limit={:?}, offset={:?}",
        session_id, limit, offset
    );

    state
        .db
        .get_traces_by_activity_session(session_id, limit.unwrap_or(200), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_activity_session_events(
    state: State<'_, AppState>,
    session_id: i64,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<ActivitySessionEvent>, String> {
    debug!(
        "get_activity_session_events: session_id={}, limit={:?}, offset={:?}",
        session_id, limit, offset
    );

    state
        .db
        .get_activity_session_events(session_id, limit.unwrap_or(200), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

/// 获取图片完整路径
/// 返回统一使用正斜杠的路径，确保跨平台兼容性
#[tauri::command]
pub async fn get_image_path(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<String, String> {
    Ok(state.db.get_full_path_string(&relative_path))
}

#[derive(Serialize)]
pub struct ImageData {
    pub mime: String,
    pub bytes: Vec<u8>,
}

fn infer_mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// 获取图片数据（返回 bytes + mime）
/// 用于前端通过 Blob URL 展示，绕开 Windows WebView2 的 asset protocol 限制。
#[tauri::command]
pub async fn get_image_data(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<ImageData, String> {
    let full_path = state.db.get_full_path(&relative_path);
    let mime = infer_mime_from_path(&full_path).to_string();
    let bytes = std::fs::read(&full_path).map_err(|e| e.to_string())?;
    Ok(ImageData { mime, bytes })
}

#[cfg(test)]
mod tests {
    use super::infer_mime_from_path;
    use std::path::Path;

    #[test]
    fn infer_mime_from_path_handles_common_extensions() {
        assert_eq!(infer_mime_from_path(Path::new("a.jpg")), "image/jpeg");
        assert_eq!(infer_mime_from_path(Path::new("a.JPEG")), "image/jpeg");
        assert_eq!(infer_mime_from_path(Path::new("a.png")), "image/png");
        assert_eq!(infer_mime_from_path(Path::new("a.webp")), "image/webp");
        assert_eq!(
            infer_mime_from_path(Path::new("a.unknownext")),
            "application/octet-stream"
        );
    }
}

/// 搜索痕迹
#[tauri::command]
pub async fn search_traces(
    state: State<'_, AppState>,
    query: String,
    mode: Option<String>,
    start_time: Option<i64>,
    end_time: Option<i64>,
    app_filter: Option<Vec<String>>,
    limit: Option<u32>,
) -> Result<Vec<SearchResult>, String> {
    debug!(
        "search_traces: query='{}', mode={:?}, limit={:?}",
        query, mode, limit
    );

    let mode = mode.unwrap_or_else(|| "keyword".to_string());
    let limit = limit.unwrap_or(20);

    let results = if mode == "semantic" {
        // 语义搜索模式
        // 先检查 embedder 是否初始化，然后释放锁再调用异步方法
        let is_initialized = {
            let embedder = state.embedder.read().await;
            embedder.is_initialized()
        };

        if is_initialized {
            // 生成查询向量（使用异步方法支持 API 后端）
            let embed_result = {
                let embedder = state.embedder.read().await;
                embedder.embed(&query).await
            };

            match embed_result {
                Ok(query_embedding) => {
                    // 混合搜索
                    let hybrid_results = state
                        .db
                        .hybrid_search(&query, Some(&query_embedding), limit)
                        .map_err(|e| e.to_string())?;

                    let filtered = apply_trace_filters(
                        hybrid_results,
                        start_time,
                        end_time,
                        app_filter.as_ref(),
                    );
                    filtered
                        .into_iter()
                        .map(|(trace, score)| SearchResult {
                            trace,
                            score,
                            highlights: vec![],
                        })
                        .collect()
                }
                Err(e) => {
                    warn!("Failed to embed query: {}", e);
                    // 回退到 FTS
                    fallback_fts_search(
                        &state.db,
                        &query,
                        limit,
                        start_time,
                        end_time,
                        app_filter.as_ref(),
                    )?
                }
            }
        } else {
            warn!("Embedder not initialized, falling back to FTS");
            fallback_fts_search(
                &state.db,
                &query,
                limit,
                start_time,
                end_time,
                app_filter.as_ref(),
            )?
        }
    } else {
        // 关键词搜索模式
        fallback_fts_search(
            &state.db,
            &query,
            limit,
            start_time,
            end_time,
            app_filter.as_ref(),
        )?
    };

    Ok(results)
}

/// FTS 回退搜索
fn fallback_fts_search(
    db: &crate::db::Database,
    query: &str,
    limit: u32,
    start_time: Option<i64>,
    end_time: Option<i64>,
    app_filter: Option<&Vec<String>>,
) -> Result<Vec<SearchResult>, String> {
    let traces = db.search_text(query, limit).map_err(|e| e.to_string())?;

    let filtered = apply_trace_filters(
        traces.into_iter().map(|t| (t, 1.0)).collect(),
        start_time,
        end_time,
        app_filter,
    );

    let results: Vec<SearchResult> = filtered
        .into_iter()
        .enumerate()
        .map(|(i, (trace, score))| SearchResult {
            trace,
            score: (score - (i as f32 * 0.02)).max(0.1),
            highlights: vec![],
        })
        .collect();

    Ok(results)
}

fn apply_trace_filters(
    items: Vec<(Trace, f32)>,
    start_time: Option<i64>,
    end_time: Option<i64>,
    app_filter: Option<&Vec<String>>,
) -> Vec<(Trace, f32)> {
    items
        .into_iter()
        .filter(|(t, _)| {
            if let Some(s) = start_time {
                if t.timestamp < s {
                    return false;
                }
            }
            if let Some(e) = end_time {
                if t.timestamp > e {
                    return false;
                }
            }
            if let Some(apps) = app_filter {
                if !apps.is_empty() {
                    let name = t.app_name.as_deref().unwrap_or("");
                    if !apps.iter().any(|a| a == name) {
                        return false;
                    }
                }
            }
            true
        })
        .collect()
}

/// 获取设置
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    debug!("get_settings");

    let mut settings = Settings::default();

    if let Ok(Some(v)) = state.db.get_setting("capture_interval_ms") {
        settings.capture_interval_ms = v.parse().unwrap_or(settings.capture_interval_ms);
    }
    if let Ok(Some(v)) = state.db.get_setting("idle_threshold_ms") {
        settings.idle_threshold_ms = v.parse().unwrap_or(settings.idle_threshold_ms);
    }
    if let Ok(Some(v)) = state.db.get_setting("similarity_threshold") {
        settings.similarity_threshold = v.parse().unwrap_or(settings.similarity_threshold);
    }
    if let Ok(Some(v)) = state.db.get_setting("hot_data_days") {
        settings.hot_data_days = v.parse().unwrap_or(settings.hot_data_days);
    }
    if let Ok(Some(v)) = state.db.get_setting("warm_data_days") {
        settings.warm_data_days = v.parse().unwrap_or(settings.warm_data_days);
    }
    if let Ok(Some(v)) = state.db.get_setting("summary_interval_min") {
        settings.summary_interval_min = v.parse().unwrap_or(settings.summary_interval_min);
    }
    if let Ok(Some(v)) = state.db.get_setting("session_gap_threshold_ms") {
        settings.session_gap_threshold_ms = v.parse().unwrap_or(settings.session_gap_threshold_ms);
    }

    Ok(settings)
}

/// 更新设置
#[tauri::command]
pub async fn update_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    info!("update_settings: {:?}", settings);

    state
        .db
        .set_setting(
            "capture_interval_ms",
            &settings.capture_interval_ms.to_string(),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("idle_threshold_ms", &settings.idle_threshold_ms.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "similarity_threshold",
            &settings.similarity_threshold.to_string(),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("hot_data_days", &settings.hot_data_days.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("warm_data_days", &settings.warm_data_days.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "summary_interval_min",
            &settings.summary_interval_min.to_string(),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "session_gap_threshold_ms",
            &settings.session_gap_threshold_ms.to_string(),
        )
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 获取存储统计
#[tauri::command]
pub async fn get_storage_stats(state: State<'_, AppState>) -> Result<StorageStats, String> {
    debug!("get_storage_stats");
    state.db.get_storage_stats().map_err(|e| e.to_string())
}

/// 初始化 AI 模块
#[tauri::command]
pub async fn initialize_ai(state: State<'_, AppState>) -> Result<bool, String> {
    info!("Initializing AI modules...");
    state.initialize_ai().await.map_err(|e| e.to_string())?;

    let vlm_ready = state.is_vlm_ready().await;
    let embedder = state.embedder.read().await;
    let embedder_ready = embedder.is_initialized();

    info!(
        "AI initialization complete: VLM={}, Embedder={}",
        vlm_ready, embedder_ready
    );

    Ok(vlm_ready || embedder_ready)
}

/// 获取 AI 状态
#[tauri::command]
pub async fn get_ai_status(state: State<'_, AppState>) -> Result<AiStatus, String> {
    let vlm_ready = state.is_vlm_ready().await;
    let embedder = state.embedder.read().await;

    Ok(AiStatus {
        vlm_ready,
        embedder_ready: embedder.is_initialized(),
        pending_analysis_count: state
            .db
            .get_traces_pending_ocr(1)
            .map(|v| v.len())
            .unwrap_or(0) as u64,
        pending_embedding_count: state
            .db
            .get_traces_pending_embedding(1)
            .map(|v| v.len())
            .unwrap_or(0) as u64,
    })
}

/// AI 状态响应
#[derive(Debug, Clone, serde::Serialize)]
pub struct AiStatus {
    pub vlm_ready: bool,
    pub embedder_ready: bool,
    pub pending_analysis_count: u64,
    pub pending_embedding_count: u64,
}

/// AI 配置响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiConfig {
    pub vlm: VlmConfig,
    pub embedding: EmbeddingConfig,
    pub vlm_task: VlmTaskConfig,
}

/// 获取 AI 配置
#[tauri::command]
pub async fn get_ai_config(state: State<'_, AppState>) -> Result<AiConfig, String> {
    debug!("get_ai_config");

    // 从数据库读取配置
    let vlm_config = load_vlm_config_from_db(&state.db);
    let embedding_config = load_embedding_config_from_db(&state.db);
    let vlm_task_config = load_vlm_task_config_from_db(&state.db);

    Ok(AiConfig {
        vlm: vlm_config,
        embedding: embedding_config,
        vlm_task: vlm_task_config,
    })
}

/// 更新 AI 配置
#[tauri::command]
pub async fn update_ai_config(state: State<'_, AppState>, config: AiConfig) -> Result<(), String> {
    info!(
        "update_ai_config: vlm.endpoint={}, embedding.endpoint={:?}, vlm_task.concurrency={}",
        config.vlm.endpoint, config.embedding.endpoint, config.vlm_task.concurrency
    );

    // 保存 VLM 配置到数据库
    state
        .db
        .set_setting("vlm_endpoint", &config.vlm.endpoint)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("vlm_model", &config.vlm.model)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("vlm_api_key", config.vlm.api_key.as_deref().unwrap_or(""))
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("vlm_max_tokens", &config.vlm.max_tokens.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("vlm_temperature", &config.vlm.temperature.to_string())
        .map_err(|e| e.to_string())?;

    // 保存 Embedding 配置到数据库
    state
        .db
        .set_setting(
            "embedding_endpoint",
            config.embedding.endpoint.as_deref().unwrap_or(""),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("embedding_model", &config.embedding.model)
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "embedding_api_key",
            config.embedding.api_key.as_deref().unwrap_or(""),
        )
        .map_err(|e| e.to_string())?;

    // 保存 VLM 任务配置到数据库
    state
        .db
        .set_setting(
            "vlm_task_interval_ms",
            &config.vlm_task.interval_ms.to_string(),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "vlm_task_batch_size",
            &config.vlm_task.batch_size.to_string(),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting(
            "vlm_task_concurrency",
            &config.vlm_task.concurrency.to_string(),
        )
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("vlm_task_enabled", &config.vlm_task.enabled.to_string())
        .map_err(|e| e.to_string())?;

    // 重新初始化 AI 模块
    reinitialize_ai(&state, &config).await?;

    Ok(())
}

/// 从数据库加载 VLM 配置
pub fn load_vlm_config_from_db(db: &crate::db::Database) -> VlmConfig {
    let mut config = VlmConfig::default();

    if let Ok(Some(v)) = db.get_setting("vlm_endpoint") {
        if !v.is_empty() {
            config.endpoint = v;
        }
    }
    if let Ok(Some(v)) = db.get_setting("vlm_model") {
        if !v.is_empty() {
            config.model = v;
        }
    }
    if let Ok(Some(v)) = db.get_setting("vlm_api_key") {
        if !v.is_empty() {
            config.api_key = Some(v);
        }
    }
    if let Ok(Some(v)) = db.get_setting("vlm_max_tokens") {
        config.max_tokens = v.parse().unwrap_or(config.max_tokens);
    }
    if let Ok(Some(v)) = db.get_setting("vlm_temperature") {
        config.temperature = v.parse().unwrap_or(config.temperature);
    }

    config
}

/// 从数据库加载 Embedding 配置
pub fn load_embedding_config_from_db(db: &crate::db::Database) -> EmbeddingConfig {
    let mut config = EmbeddingConfig::default();

    if let Ok(Some(v)) = db.get_setting("embedding_endpoint") {
        if !v.is_empty() {
            config.endpoint = Some(v);
        }
    }
    if let Ok(Some(v)) = db.get_setting("embedding_model") {
        if !v.is_empty() {
            config.model = v;
        }
    }
    if let Ok(Some(v)) = db.get_setting("embedding_api_key") {
        if !v.is_empty() {
            config.api_key = Some(v);
        }
    }

    config
}

/// 从数据库加载 VLM 任务配置
pub fn load_vlm_task_config_from_db(db: &crate::db::Database) -> VlmTaskConfig {
    let mut config = VlmTaskConfig::default();

    if let Ok(Some(v)) = db.get_setting("vlm_task_interval_ms") {
        config.interval_ms = v.parse().unwrap_or(config.interval_ms);
    }
    if let Ok(Some(v)) = db.get_setting("vlm_task_batch_size") {
        config.batch_size = v.parse().unwrap_or(config.batch_size);
    }
    if let Ok(Some(v)) = db.get_setting("vlm_task_concurrency") {
        config.concurrency = v.parse().unwrap_or(config.concurrency);
    }
    if let Ok(Some(v)) = db.get_setting("vlm_task_enabled") {
        config.enabled = v.parse().unwrap_or(config.enabled);
    }

    config
}

/// 重新初始化 AI 模块
async fn reinitialize_ai(state: &State<'_, AppState>, config: &AiConfig) -> Result<(), String> {
    let mut vlm_initialized = false;

    // 重新初始化 VLM
    {
        let vlm_config = config.vlm.clone();
        let mut engine = crate::ai::VlmEngine::new(vlm_config);
        match engine.initialize().await {
            Ok(_) => {
                info!("VLM re-initialized with new config");
                *state.vlm.write().await = Some(engine);
                vlm_initialized = true;
            }
            Err(e) => {
                warn!("Failed to initialize VLM with new config: {}", e);
                *state.vlm.write().await = None;
            }
        }
    }

    // 重新初始化 Embedding
    {
        let embedding_config = config.embedding.clone();
        let mut embedder = crate::ai::TextEmbedder::with_config(embedding_config);
        match embedder.initialize().await {
            Ok(_) => {
                info!("Embedder re-initialized with new config");
                *state.embedder.write().await = embedder;
            }
            Err(e) => {
                warn!("Failed to initialize embedder with new config: {}", e);
            }
        }
    }

    // 使用新配置重启 VLM 分析任务
    if vlm_initialized {
        if let Err(e) = state.restart_vlm_task(config.vlm_task.clone()).await {
            warn!("Failed to restart VLM task: {}", e);
        } else {
            info!(
                "VLM task restarted with new config (concurrency: {})",
                config.vlm_task.concurrency
            );
        }
    }

    Ok(())
}

// ==================== Summary Commands ====================

/// 获取摘要列表
#[tauri::command]
pub async fn get_summaries(
    state: State<'_, AppState>,
    start_time: i64,
    end_time: i64,
    summary_type: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<Summary>, String> {
    debug!(
        "get_summaries: start={}, end={}, type={:?}, limit={:?}",
        start_time, end_time, summary_type, limit
    );

    state
        .db
        .get_summaries(
            start_time,
            end_time,
            summary_type.as_deref(),
            limit.unwrap_or(50),
        )
        .map_err(|e| e.to_string())
}

/// 获取单个摘要
#[tauri::command]
pub async fn get_summary_by_id(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<Summary>, String> {
    debug!("get_summary_by_id: id={}", id);
    state.db.get_summary_by_id(id).map_err(|e| e.to_string())
}

/// 获取最近的摘要
#[tauri::command]
pub async fn get_latest_summary(
    state: State<'_, AppState>,
    summary_type: String,
) -> Result<Option<Summary>, String> {
    debug!("get_latest_summary: type={}", summary_type);
    state
        .db
        .get_latest_summary(&summary_type)
        .map_err(|e| e.to_string())
}

/// 删除摘要
#[tauri::command]
pub async fn delete_summary(state: State<'_, AppState>, id: i64) -> Result<bool, String> {
    info!("delete_summary: id={}", id);
    state.db.delete_summary(id).map_err(|e| e.to_string())
}

// ==================== Entity Commands ====================

/// 获取实体列表
#[tauri::command]
pub async fn get_entities(
    state: State<'_, AppState>,
    entity_type: Option<String>,
    limit: Option<u32>,
    order_by_mentions: Option<bool>,
) -> Result<Vec<Entity>, String> {
    debug!(
        "get_entities: type={:?}, limit={:?}, order_by_mentions={:?}",
        entity_type, limit, order_by_mentions
    );

    state
        .db
        .get_entities(
            entity_type.as_deref(),
            limit.unwrap_or(100),
            order_by_mentions.unwrap_or(true),
        )
        .map_err(|e| e.to_string())
}

/// 按名称获取实体
#[tauri::command]
pub async fn get_entity_by_name(
    state: State<'_, AppState>,
    name: String,
) -> Result<Option<Entity>, String> {
    debug!("get_entity_by_name: name={}", name);
    state
        .db
        .get_entity_by_name(&name)
        .map_err(|e| e.to_string())
}

/// 获取实体关联的痕迹
#[tauri::command]
pub async fn get_traces_by_entity(
    state: State<'_, AppState>,
    entity_id: i64,
    limit: Option<u32>,
) -> Result<Vec<Trace>, String> {
    debug!(
        "get_traces_by_entity: entity_id={}, limit={:?}",
        entity_id, limit
    );
    state
        .db
        .get_traces_by_entity(entity_id, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

/// 搜索实体
#[tauri::command]
pub async fn search_entities(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<Entity>, String> {
    debug!("search_entities: query='{}', limit={:?}", query, limit);
    state
        .db
        .search_entities(&query, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

/// 删除实体
#[tauri::command]
pub async fn delete_entity(state: State<'_, AppState>, id: i64) -> Result<bool, String> {
    info!("delete_entity: id={}", id);
    state.db.delete_entity(id).map_err(|e| e.to_string())
}

// ==================== Chat Commands ====================

/// Chat 请求参数
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatRequest {
    /// 用户消息
    pub message: String,
    /// 开始时间戳（可选）
    pub start_time: Option<i64>,
    /// 结束时间戳（可选）
    pub end_time: Option<i64>,
    /// 应用过滤（可选）
    pub app_filter: Option<Vec<String>>,

    /// 对话线程 ID（可选；用于持久化历史）
    pub thread_id: Option<i64>,
}

/// Chat 响应
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatResponse {
    /// 回复内容
    pub content: String,
    /// 使用的上下文数量
    pub context_count: u32,
    /// 时间范围描述
    pub time_range: Option<String>,

    /// 对话线程 ID（用于后续继续对话）
    pub thread_id: i64,
}

/// 与记忆进行对话
#[tauri::command]
pub async fn chat_with_memory(
    state: State<'_, AppState>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    info!(
        "chat_with_memory: message='{}', start={:?}, end={:?}, apps={:?}",
        request.message.chars().take(50).collect::<String>(),
        request.start_time,
        request.end_time,
        request.app_filter
    );

    // 确定时间范围（毫秒级，与数据库保持一致）
    let now = chrono::Utc::now().timestamp_millis();
    let day_ms = 24 * 3600 * 1000i64;
    let (start_time, end_time) = match (request.start_time, request.end_time) {
        (Some(s), Some(e)) => (s, e),
        (Some(s), None) => (s, now),
        (None, Some(e)) => (e - day_ms, e),  // 默认向前24小时
        (None, None) => (now - day_ms, now), // 默认最近24小时
    };

    // 获取时间范围内的活动 sessions（对外主视图）
    let sessions = state
        .db
        .get_activity_sessions(start_time, end_time, request.app_filter.as_ref(), 30, 0)
        .map_err(|e| e.to_string())?;

    // 同时取最近 1-2 条 trace 作为细节补充（可选）
    let recent_traces = state
        .db
        .get_traces_filtered(start_time, end_time, request.app_filter.as_ref(), 2)
        .map_err(|e| e.to_string())?;

    if sessions.is_empty() && recent_traces.is_empty() {
        return Ok(ChatResponse {
            content: "在指定的时间范围内没有找到相关的屏幕记录。请尝试扩大时间范围或选择其他应用。"
                .to_string(),
            context_count: 0,
            time_range: Some(format_time_range(start_time, end_time)),
            thread_id: request.thread_id.unwrap_or(0),
        });
    }

    // 构建上下文：Session（聚合）+ 最近 1-2 条 Trace（细节）
    let context = build_chat_context_from_sessions(&sessions, &recent_traces);
    let context_count = (sessions.len() + recent_traces.len()) as u32;

    // 获取 VLM 引擎进行对话
    let vlm_guard = state.vlm.read().await;
    let vlm = vlm_guard
        .as_ref()
        .ok_or("VLM 未初始化。请先在设置中配置 AI 模型。")?;

    // 构建 prompt
    let system_prompt = r#"你是 Engram 智能助手，帮助用户回忆和理解他们的屏幕活动记录。
用户会提供一段时间内的屏幕活动摘要，你需要基于这些信息回答用户的问题。

注意：
- 只基于提供的上下文回答，不要编造信息
- 如果信息不足，诚实告知用户
- 回答要简洁、有帮助
- 使用中文回复"#;

    let user_prompt = format!(
        "以下是用户的屏幕活动记录：\n\n{}\n\n用户问题：{}",
        context, request.message
    );

    // 调用 LLM
    let response = vlm
        .chat(&system_prompt, &user_prompt)
        .await
        .map_err(|e| format!("Chat 失败: {}", e))?;

    // 持久化对话历史（thread）
    let thread_id = match request.thread_id {
        Some(id) if id > 0 => id,
        _ => state
            .db
            .create_chat_thread(Some("与记忆对话"))
            .map_err(|e| e.to_string())?,
    };

    let context_json = serde_json::json!({
        "time_range": { "start": start_time, "end": end_time },
        "session_ids": sessions.iter().map(|s| s.id).collect::<Vec<_>>(),
        "trace_ids": recent_traces.iter().map(|t| t.id).collect::<Vec<_>>(),
    })
    .to_string();

    let _ = state
        .db
        .append_chat_message(thread_id, "user", &request.message, Some(&context_json));
    let _ = state
        .db
        .append_chat_message(thread_id, "assistant", &response, Some(&context_json));

    Ok(ChatResponse {
        content: response,
        context_count,
        time_range: Some(format_time_range(start_time, end_time)),
        thread_id,
    })
}

/// 获取 chat 历史
#[tauri::command]
pub async fn get_chat_messages(
    state: State<'_, AppState>,
    thread_id: i64,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<ChatMessage>, String> {
    state
        .db
        .get_chat_messages(thread_id, limit.unwrap_or(200), offset.unwrap_or(0))
        .map_err(|e| e.to_string())
}

/// 获取可用的应用列表（用于过滤）
#[tauri::command]
pub async fn get_available_apps(
    state: State<'_, AppState>,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Result<Vec<String>, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let week_ms = 7 * 24 * 3600 * 1000i64;
    let start = start_time.unwrap_or(now - week_ms); // 默认最近7天
    let end = end_time.unwrap_or(now);

    state
        .db
        .get_distinct_apps(start, end)
        .map_err(|e| e.to_string())
}

/// 构建 chat 上下文
fn build_chat_context_from_sessions(
    sessions: &[ActivitySession],
    recent_traces: &[Trace],
) -> String {
    let mut context = String::new();

    // Sessions（尽量按时间正序，便于 LLM 理解）
    let mut sessions_sorted = sessions.to_vec();
    sessions_sorted.sort_by_key(|s| s.start_time);

    for session in sessions_sorted.iter().take(20) {
        let start = chrono::DateTime::from_timestamp_millis(session.start_time)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let end = chrono::DateTime::from_timestamp_millis(session.end_time)
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default();

        context.push_str(&format!(
            "[Session {}-{}] {}\n",
            start, end, session.app_name
        ));

        if let Some(ctx) = &session.context_text {
            let text: String = ctx
                .chars()
                .rev()
                .take(800)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if !text.trim().is_empty() {
                context.push_str(&format!("  结论: {}\n", text.trim()));
            }
        }
        if let Some(key_actions_json) = &session.key_actions_json {
            if !key_actions_json.trim().is_empty() {
                context.push_str(&format!("  关键行为: {}\n", key_actions_json));
            }
        }
        context.push('\n');
    }

    // 最近 traces（细节补充）
    if !recent_traces.is_empty() {
        context.push_str("【最近截图细节（OCR）】\n");
        for trace in recent_traces.iter().take(2) {
            let time = chrono::DateTime::from_timestamp_millis(trace.timestamp)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            let app = trace.app_name.as_deref().unwrap_or("未知应用");

            context.push_str(&format!("- [{}] {}\n", time, app));
            if let Some(ocr_text) = &trace.ocr_text {
                let snippet: String = ocr_text.chars().take(250).collect();
                if !snippet.trim().is_empty() {
                    context.push_str(&format!("  {}\n", snippet.trim()));
                }
            }
            context.push('\n');
        }
    }

    context
}

/// 格式化时间范围描述（毫秒时间戳，使用本地时区）
fn format_time_range(start: i64, end: i64) -> String {
    let start_dt = chrono::DateTime::from_timestamp_millis(start)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let end_dt = chrono::DateTime::from_timestamp_millis(end)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();

    format!("{} 至 {}", start_dt, end_dt)
}

/// 手动触发摘要生成
#[tauri::command]
pub async fn trigger_summary(
    state: State<'_, AppState>,
    summary_type: String,
) -> Result<String, String> {
    use crate::ai::summarizer::SummaryType;

    let stype = match summary_type.as_str() {
        "short" => SummaryType::Short,
        "daily" => SummaryType::Daily,
        _ => return Err("无效的摘要类型，请使用 'short' 或 'daily'".to_string()),
    };

    // 确保使用用户配置的 VLM 模型
    // 先获取 VLM 配置，然后用它初始化 SummarizerTask
    let vlm_guard = state.vlm.read().await;
    if let Some(vlm) = vlm_guard.as_ref() {
        let vlm_config = vlm.config().clone();
        drop(vlm_guard); // 释放锁

        // 使用 VLM 配置重新初始化 SummarizerTask
        state
            .start_summarizer_task_with_config(vlm_config)
            .await
            .map_err(|e| format!("初始化摘要任务失败: {}", e))?;
    } else {
        drop(vlm_guard);
        return Err("VLM 未初始化。请先在设置中配置 AI 模型。".to_string());
    }

    let task = state.summarizer_task.read().await;
    task.trigger_summary(stype)
        .await
        .map_err(|e| format!("生成摘要失败: {}", e))?;

    Ok(format!(
        "{}摘要生成成功",
        if summary_type == "daily" {
            "每日"
        } else {
            "15分钟"
        }
    ))
}
