//! 应用入口接线:Accessory 窗口策略、全局老板键、自选股持久化、轮询、invoke 命令。

mod minute;
mod poll;
mod quote;

use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "settings.json";
const WATCHLIST_KEY: &str = "watchlist";

/// 默认自选:上证指数 / 深证成指 / 贵州茅台
fn default_watchlist() -> Vec<String> {
    vec![
        "sh000001".to_string(),
        "sz399001".to_string(),
        "sh600519".to_string(),
    ]
}

/// 共享自选股列表,轮询线程与命令共用。
struct AppState {
    watchlist: Arc<Mutex<Vec<String>>>,
}

#[tauri::command]
fn get_watchlist(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.watchlist.lock().unwrap().clone()
}

/// 写入自选股:更新共享状态 + 持久化到 store。
#[tauri::command]
fn set_watchlist(
    codes: Vec<String>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let cleaned: Vec<String> = codes
        .into_iter()
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    {
        let mut w = state.watchlist.lock().unwrap();
        *w = cleaned.clone();
    }
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(WATCHLIST_KEY, serde_json::json!(cleaned));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// 立即抓一次行情(用于启动/手动刷新,不受交易时段限制)。
#[tauri::command]
fn quotes_now(state: tauri::State<'_, AppState>) -> Result<Vec<quote::Quote>, String> {
    let codes = state.watchlist.lock().unwrap().clone();
    quote::fetch_quotes(&codes)
}

/// 抓某只股的当日分时。
#[tauri::command]
fn fetch_minute(code: String) -> Result<minute::MinuteData, String> {
    minute::fetch_minute(&code)
}

/// 红叉:彻底退出整个应用进程。
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 老板键:切换主窗口显隐。
fn toggle_window(win: &WebviewWindow) {
    if win.is_visible().unwrap_or(true) {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let boss_key = Shortcut::new(Some(Modifiers::ALT | Modifiers::SUPER), Code::KeyH);

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &boss_key && event.state() == ShortcutState::Pressed {
                        if let Some(win) = app.get_webview_window("main") {
                            toggle_window(&win);
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // 不进 Dock、不在 ⌘Tab 出现
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // 从 store 读自选股,无则用默认
            let store = app.store(STORE_FILE)?;
            let watchlist = store
                .get(WATCHLIST_KEY)
                .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(default_watchlist);

            let shared = Arc::new(Mutex::new(watchlist));
            app.manage(AppState {
                watchlist: shared.clone(),
            });

            // 注册老板键(权限不足时不致命,窗口内按钮仍可隐藏)
            if let Err(e) = app.global_shortcut().register(boss_key) {
                eprintln!("global shortcut register failed: {e}");
            }

            // 顶部状态栏托盘图标:左键弹出菜单(显隐 / 刷新 / 退出)
            let toggle_i = MenuItem::with_id(app, "toggle", "显示/隐藏浮窗", true, None::<&str>)?;
            let refresh_i = MenuItem::with_id(app, "refresh", "刷新行情", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &toggle_i,
                    &PredefinedMenuItem::separator(app)?,
                    &refresh_i,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_i,
                ],
            )?;
            // 单色模板图标:macOS 自动随菜单栏明暗反色,低调不显眼
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!(
                "../icons/tray-template.png"
            ))?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .tooltip("行情")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => {
                        if let Some(win) = app.get_webview_window("main") {
                            toggle_window(&win);
                        }
                    }
                    "refresh" => {
                        let _ = app.emit("tray-refresh", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // 启动后台轮询
            poll::start_polling(app.handle().clone(), shared);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_watchlist,
            set_watchlist,
            quotes_now,
            fetch_minute,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
