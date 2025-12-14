//! Tauri 命令处理模块
//!
//! 提供前端调用的 API 接口。

use crate::ai::{EmbeddingConfig, VlmConfig};
use crate::daemon::DaemonStatus;
use crate::db::models::{Entity, SearchResult, Settings, StorageStats, Summary, Trace};
use crate::AppState;
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
    // TODO: 实现立即截图
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
        .get_traces(start_time, end_time, limit.unwrap_or(100), offset.unwrap_or(0))
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
        let embedder = state.embedder.read().await;
        if embedder.is_initialized() {
            // 生成查询向量
            match embedder.embed_sync(&query) {
                Ok(query_embedding) => {
                    // 混合搜索
                    let hybrid_results = state
                        .db
                        .hybrid_search(&query, Some(&query_embedding), limit)
                        .map_err(|e| e.to_string())?;

                    hybrid_results
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
                    fallback_fts_search(&state.db, &query, limit)?
                }
            }
        } else {
            warn!("Embedder not initialized, falling back to FTS");
            fallback_fts_search(&state.db, &query, limit)?
        }
    } else {
        // 关键词搜索模式
        fallback_fts_search(&state.db, &query, limit)?
    };

    Ok(results)
}

/// FTS 回退搜索
fn fallback_fts_search(
    db: &crate::db::Database,
    query: &str,
    limit: u32,
) -> Result<Vec<SearchResult>, String> {
    let traces = db.search_text(query, limit).map_err(|e| e.to_string())?;

    let results: Vec<SearchResult> = traces
        .into_iter()
        .enumerate()
        .map(|(i, trace)| SearchResult {
            trace,
            score: 1.0 - (i as f32 * 0.1).min(0.9),
            highlights: vec![],
        })
        .collect();

    Ok(results)
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

    Ok(settings)
}

/// 更新设置
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    info!("update_settings: {:?}", settings);

    state
        .db
        .set_setting("capture_interval_ms", &settings.capture_interval_ms.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("idle_threshold_ms", &settings.idle_threshold_ms.to_string())
        .map_err(|e| e.to_string())?;
    state
        .db
        .set_setting("similarity_threshold", &settings.similarity_threshold.to_string())
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
        .set_setting("summary_interval_min", &settings.summary_interval_min.to_string())
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
}

/// 获取 AI 配置
#[tauri::command]
pub async fn get_ai_config(state: State<'_, AppState>) -> Result<AiConfig, String> {
    debug!("get_ai_config");

    // 从数据库读取配置
    let vlm_config = load_vlm_config_from_db(&state.db);
    let embedding_config = load_embedding_config_from_db(&state.db);

    Ok(AiConfig {
        vlm: vlm_config,
        embedding: embedding_config,
    })
}

/// 更新 AI 配置
#[tauri::command]
pub async fn update_ai_config(
    state: State<'_, AppState>,
    config: AiConfig,
) -> Result<(), String> {
    info!("update_ai_config: vlm.endpoint={}, embedding.endpoint={:?}",
          config.vlm.endpoint, config.embedding.endpoint);

    // 保存 VLM 配置到数据库
    state.db.set_setting("vlm_endpoint", &config.vlm.endpoint).map_err(|e| e.to_string())?;
    state.db.set_setting("vlm_model", &config.vlm.model).map_err(|e| e.to_string())?;
    state.db.set_setting("vlm_api_key", config.vlm.api_key.as_deref().unwrap_or("")).map_err(|e| e.to_string())?;
    state.db.set_setting("vlm_max_tokens", &config.vlm.max_tokens.to_string()).map_err(|e| e.to_string())?;
    state.db.set_setting("vlm_temperature", &config.vlm.temperature.to_string()).map_err(|e| e.to_string())?;

    // 保存 Embedding 配置到数据库
    state.db.set_setting("embedding_endpoint", config.embedding.endpoint.as_deref().unwrap_or("")).map_err(|e| e.to_string())?;
    state.db.set_setting("embedding_model", &config.embedding.model).map_err(|e| e.to_string())?;
    state.db.set_setting("embedding_api_key", config.embedding.api_key.as_deref().unwrap_or("")).map_err(|e| e.to_string())?;

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

    // 如果 VLM 初始化成功，启动 VLM 分析任务
    if vlm_initialized {
        if let Err(e) = state.start_vlm_task().await {
            warn!("Failed to start VLM task: {}", e);
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
    debug!("get_traces_by_entity: entity_id={}, limit={:?}", entity_id, limit);
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
