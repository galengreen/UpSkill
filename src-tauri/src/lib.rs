//! Desktop shell: window, tray, background worker and file watcher around the
//! shared `upskill-core` API.

use notify::{RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent, Wry};
use upskill_core::model::now_ms;
use upskill_core::{Api, Upskill};

const CHANGED: &str = "upskill://changed";

struct Shared {
    bin: Option<String>,
    status: MenuItem<Wry>,
}

/// The `upskill` CLI ships next to the app binary (as a sidecar in bundles).
fn find_cli() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let sibling = exe.parent()?.join("upskill");
    if sibling.is_file() {
        return Some(sibling.to_string_lossy().into());
    }
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join("upskill")).find(|c| c.is_file()))
        .flatten()
        .map(|p: PathBuf| p.to_string_lossy().into())
}

fn make_api(bin: &Option<String>) -> Result<Api, String> {
    let up = Upskill::open().map_err(|e| format!("{e:#}"))?;
    let mut api = Api::new(up, "you");
    api.bin = bin.clone();
    Ok(api)
}

#[tauri::command]
async fn api(state: tauri::State<'_, Shared>, method: String, params: Option<Value>) -> Result<Value, String> {
    let bin = state.bin.clone();
    let params = params.unwrap_or_else(|| json!({}));
    tauri::async_runtime::spawn_blocking(move || {
        let api = make_api(&bin)?;
        api.call(&method, &params).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn update_tray(app: &AppHandle) {
    let Some(shared) = app.try_state::<Shared>() else { return };
    let Ok(api) = make_api(&shared.bin) else { return };
    let text = match api.call("status", &json!({})) {
        Ok(s) => {
            let skills = s["skills"].as_u64().unwrap_or(0);
            let agents = s["agents"].as_array().map(|a| a.len()).unwrap_or(0);
            let sync = match s["sync"]["last"].as_i64() {
                Some(t) => {
                    let mins = (now_ms() - t) / 60_000;
                    if mins < 1 { " · synced just now".to_string() } else if mins < 60 { format!(" · synced {mins}m ago") } else { format!(" · synced {}h ago", mins / 60) }
                }
                None => String::new(),
            };
            format!("{skills} skills · {agents} agents{sync}")
        }
        Err(_) => "UpSkill".into(),
    };
    let _ = shared.status.set_text(text);
}

/// Refresh, mine usage, re-place and (when due) sync. Runs off the UI thread.
fn tick(app: &AppHandle, force_sync: bool) {
    let bin = app.state::<Shared>().bin.clone();
    let Ok(api) = make_api(&bin) else { return };
    let up = &api.up;
    let _ = up.refresh();
    if let Ok(mut db) = up.usage_db() {
        let _ = db.ingest(&up.paths);
    }
    let _ = up.deploy(false);
    if let Ok(cfg) = up.config() {
        let due = cfg.auto_sync_minutes > 0 && cfg.last_sync.is_none_or(|t| now_ms() - t > cfg.auto_sync_minutes as i64 * 60_000);
        if cfg.sync.is_some() && (force_sync || due) {
            let _ = upskill_core::sync::sync_now(up);
        }
    }
    let _ = app.emit(CHANGED, ());
    update_tray(app);
}

fn spawn_worker(app: AppHandle) {
    std::thread::spawn(move || loop {
        tick(&app, false);
        std::thread::sleep(Duration::from_secs(90));
    });
}

/// Library edits (agents writing through symlinks, editors) are picked up and
/// versioned within a second; state changes from the MCP server refresh the UI.
fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(up) = Upskill::open() else { return };
        let library = up.paths.library();
        let state_file = up.paths.state_file();
        let (tx, rx) = mpsc::channel();
        let Ok(mut watcher) = notify::recommended_watcher(tx) else { return };
        let _ = watcher.watch(&library, RecursiveMode::Recursive);
        let _ = watcher.watch(&up.paths.root, RecursiveMode::NonRecursive);
        loop {
            let Ok(first) = rx.recv() else { return };
            let mut events = vec![first];
            while let Ok(e) = rx.recv_timeout(Duration::from_millis(700)) {
                events.push(e);
            }
            let paths: Vec<PathBuf> = events.into_iter().flatten().flat_map(|e: notify::Event| e.paths).collect();
            let library_changed = paths.iter().any(|p| p.starts_with(&library) && !p.to_string_lossy().contains(".upskill-"));
            let state_changed = paths.iter().any(|p| p == &state_file);
            if library_changed {
                if let Ok(changed) = up.refresh() {
                    if !changed.is_empty() {
                        let _ = up.deploy(false);
                    }
                }
            }
            if library_changed || state_changed {
                let _ = app.emit(CHANGED, ());
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    upskill_core::agents::fix_path(&upskill_core::paths::Paths::detect());
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![api])
        .setup(|app| {
            let handle = app.handle().clone();
            let status = MenuItem::with_id(app, "status", "UpSkill", false, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Open UpSkill", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
            let rescan = MenuItem::with_id(app, "rescan", "Rescan agents", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit UpSkill", true, None::<&str>)?;
            let sep = || PredefinedMenuItem::separator(app);
            let menu = Menu::with_items(app, &[&status, &sep()?, &open, &sync, &rescan, &sep()?, &quit])?;
            app.manage(Shared { bin: find_cli(), status });

            TrayIconBuilder::with_id("upskill")
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .tooltip("UpSkill")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "sync" => {
                        let app = app.clone();
                        std::thread::spawn(move || tick(&app, true));
                    }
                    "rescan" => {
                        let app = app.clone();
                        std::thread::spawn(move || tick(&app, false));
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            spawn_worker(handle.clone());
            spawn_watcher(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps UpSkill alive in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building UpSkill");

    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
        let _ = (app, event);
    });
}
