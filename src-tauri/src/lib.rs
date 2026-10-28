//! Desktop shell: window, tray, background worker and file watcher around the
//! shared `upskill-core` API.

use notify::{RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri::{AppHandle, Emitter, Manager, WindowEvent, Wry};
use upskill_core::model::now_ms;
use upskill_core::paths::Paths;
use upskill_core::{update, Api, Upskill};

const CHANGED: &str = "upskill://changed";
const UPDATE: &str = "upskill://update";
const UPDATE_EVERY: Duration = Duration::from_secs(6 * 3600);

struct Shared {
    bin: Option<String>,
    status: MenuItem<Wry>,
    update: MenuItem<Wry>,
    /// Serialises update checks between the timer and the tray.
    updating: Mutex<()>,
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

#[tauri::command]
fn update_state() -> Value {
    json!({ "pending": update::pending(&Paths::detect()), "current": update::CURRENT })
}

#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
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

/// An update is on disk: offer a restart, or just restart if nobody is looking.
fn update_ready(app: &AppHandle, version: &str) {
    let shared = app.state::<Shared>();
    let _ = shared.update.set_text(format!("Restart to update to {version}"));
    let _ = app.emit(UPDATE, json!({ "pending": version }));
    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
    if !visible {
        app.restart();
    }
}

/// Check for a release and, unless the user turned that off, install it.
/// `by_user` installs regardless and reports failures.
fn check_updates(app: &AppHandle, by_user: bool) {
    let shared = app.state::<Shared>();
    let Ok(_guard) = shared.updating.try_lock() else { return };
    let paths = Paths::detect();
    if let Some(v) = update::pending(&paths) {
        return update_ready(app, &v);
    }
    let manual = Upskill::open().and_then(|up| up.config()).map(|c| c.manual_updates).unwrap_or(false);
    let _ = shared.update.set_text("Checking for updates…");
    let outcome = update::check(&paths).and_then(|c| {
        if !c.available {
            return Ok(None);
        }
        if let Some(why) = &c.blocked {
            anyhow::bail!("{} is available but can't be installed here: {why}", c.latest);
        }
        if manual && !by_user {
            let _ = app.emit(UPDATE, json!({ "available": c.latest }));
            return Ok(Some(format!("Install UpSkill {}", c.latest)));
        }
        let _ = shared.update.set_text(format!("Installing {}…", c.latest));
        let v = update::install(&paths)?;
        update_ready(app, &v);
        Ok(Some(format!("Restart to update to {v}")))
    });
    let text = match outcome {
        Ok(Some(t)) => t,
        Ok(None) => format!("Up to date ({})", update::CURRENT),
        Err(e) => {
            if by_user {
                let _ = app.emit(UPDATE, json!({ "error": format!("{e:#}") }));
            }
            "Check for updates".into()
        }
    };
    let _ = shared.update.set_text(text);
}

fn spawn_updater(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        loop {
            check_updates(&app, false);
            std::thread::sleep(UPDATE_EVERY);
        }
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
        .invoke_handler(tauri::generate_handler![api, update_state, restart_app])
        .setup(|app| {
            let handle = app.handle().clone();
            let status = MenuItem::with_id(app, "status", "UpSkill", false, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Open UpSkill", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
            let rescan = MenuItem::with_id(app, "rescan", "Rescan agents", true, None::<&str>)?;
            let update_item = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit UpSkill", true, None::<&str>)?;
            let sep = || PredefinedMenuItem::separator(app);
            let menu = Menu::with_items(app, &[&status, &sep()?, &open, &sync, &rescan, &sep()?, &update_item, &quit])?;
            app.manage(Shared { bin: find_cli(), status, update: update_item, updating: Mutex::new(()) });

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
                    "update" => match update::pending(&Paths::detect()) {
                        Some(_) => app.restart(),
                        None => {
                            let app = app.clone();
                            std::thread::spawn(move || check_updates(&app, true));
                        }
                    },
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            spawn_worker(handle.clone());
            spawn_watcher(handle.clone());
            spawn_updater(handle);
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
