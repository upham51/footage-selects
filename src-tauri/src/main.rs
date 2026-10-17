// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod runner;
mod state;
mod updates;

use state::{AppState, EVENT_CLOSE_REQUESTED, EVENT_DROP_HOVER};
use std::sync::atomic::Ordering;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

fn main() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    if let Some(updater) = updates::plugin() {
        builder = builder.plugin(updater);
    }
    builder
        .manage(AppState::default())
        .manage(updates::Pending::default())
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(selects_engine::PRODUCT_NAME);
            }
            // Paths passed on the command line (or by "Open with") are added on start.
            let paths: Vec<std::path::PathBuf> = std::env::args()
                .skip(1)
                .map(std::path::PathBuf::from)
                .filter(|p| p.exists())
                .collect();
            if !paths.is_empty() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    commands::add_paths_blocking(&handle, paths);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::add_paths,
            commands::pick_files,
            commands::pick_folder,
            commands::start_analysis,
            commands::stop_analysis,
            commands::set_selection,
            commands::set_sample_rate,
            commands::set_export,
            commands::clip_detail,
            commands::remove_clip,
            commands::new_project,
            commands::open_project,
            commands::save_project,
            commands::export_timeline,
            commands::close_app,
            commands::tool_check,
            updates::update_settings,
            updates::set_check_updates_on_open,
            updates::check_for_update,
            updates::install_update,
        ])
        .on_window_event(|window, event| match event {
            WindowEvent::DragDrop(DragDropEvent::Enter { .. }) | WindowEvent::DragDrop(DragDropEvent::Over { .. }) => {
                let _ = window.emit(EVENT_DROP_HOVER, true);
            }
            WindowEvent::DragDrop(DragDropEvent::Leave) => {
                let _ = window.emit(EVENT_DROP_HOVER, false);
            }
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                let _ = window.emit(EVENT_DROP_HOVER, false);
                let app = window.app_handle().clone();
                let paths = paths.clone();
                std::thread::spawn(move || commands::add_paths_blocking(&app, paths));
            }
            WindowEvent::CloseRequested { api, .. } => {
                let state = window.state::<AppState>();
                if state.dirty.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.emit(EVENT_CLOSE_REQUESTED, ());
                } else {
                    runner::stop(window.app_handle());
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("the app could not start");
}
