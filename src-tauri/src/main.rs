//! Engram 主入口

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use engram_lib::{commands, AppState};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent,
};
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() {
    // 初始化日志
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("engram=debug".parse().unwrap()))
        .init();

    info!("Starting Engram v{}", env!("CARGO_PKG_VERSION"));

    // 构建 Tauri 应用
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // 初始化应用状态
            let state = tauri::async_runtime::block_on(async { AppState::new().await })?;
            app.manage(state);

            // 创建系统托盘
            setup_tray(app)?;

            info!("Engram initialized successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_capture_status,
            commands::start_daemon,
            commands::stop_daemon,
            commands::toggle_capture,
            commands::capture_now,
            commands::get_traces,
            commands::get_image_path,
            commands::search_traces,
            commands::get_settings,
            commands::update_settings,
            commands::get_storage_stats,
            commands::initialize_ai,
            commands::get_ai_status,
            commands::get_ai_config,
            commands::update_ai_config,
            // Summary commands
            commands::get_summaries,
            commands::get_summary_by_id,
            commands::get_latest_summary,
            commands::delete_summary,
            // Entity commands
            commands::get_entities,
            commands::get_entity_by_name,
            commands::get_traces_by_entity,
            commands::search_entities,
            commands::delete_entity,
            // Chat commands
            commands::chat_with_memory,
            commands::get_available_apps,
        ])
        .build(tauri::generate_context!())
        .expect("Failed to build Tauri application")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                // 阻止默认退出，隐藏到托盘
                api.prevent_exit();
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
        });
}

/// 设置系统托盘
fn setup_tray(app: &tauri::App) -> anyhow::Result<()> {
    // 创建托盘菜单
    let toggle_item = MenuItem::with_id(app, "toggle", "暂停录制", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "打开主窗口", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle_item, &show_item, &settings_item, &quit_item])?;

    // 创建托盘图标
    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Engram - 语义记忆增强系统")
        .menu(&menu)
        .menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                info!("Toggle capture clicked");
                // TODO: 切换录制状态
            }
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "settings" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    // TODO: 导航到设置页面
                }
            }
            "quit" => {
                info!("Quit requested");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
