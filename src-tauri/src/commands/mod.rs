//! Tauri 命令处理模块
//!
//! 提供前端调用的 API 接口。

use crate::daemon::DaemonStatus;
use crate::db::models::{SearchResult, Settings, StorageStats, Trace};
use crate::AppState;
use tauri::State;
use tracing::{debug, info};

/// 获取截图状态
#[tauri::command]
pub async fn get_capture_status(state: State<'_, AppState>) -> Result<DaemonStatus, String> {
    let daemon = state.daemon.read().await;
    Ok(daemon.status())
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

    let _mode = mode.unwrap_or_else(|| "keyword".to_string());
    let limit = limit.unwrap_or(20);

    // 当前仅实现关键词搜索，语义搜索在 Phase 2 实现
    let traces = state
        .db
        .search_text(&query, limit)
        .map_err(|e| e.to_string())?;

    // 转换为 SearchResult
    let results: Vec<SearchResult> = traces
        .into_iter()
        .enumerate()
        .map(|(i, trace)| SearchResult {
            trace,
            score: 1.0 - (i as f32 * 0.1).min(0.9),
            highlights: vec![], // TODO: 实现高亮
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
