//! Commands the window calls.

use crate::runner;
use crate::state::{AppState, StateDto, EVENT_PROBE, EVENT_STATE};
use selects_engine::probe::{collect_video_files, probe, VIDEO_EXTENSIONS};
use selects_engine::project::{Project, TimelineLayout, PROJECT_EXTENSION};
use selects_engine::rate::Rate;
use selects_engine::scores::ScoreSeries;
use selects_engine::selection::{Segment, SelectionSettings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> StateDto {
    state.snapshot()
}

#[derive(Serialize)]
pub struct AddResult {
    pub added: usize,
    pub already_there: usize,
    pub unreadable: usize,
    pub found: usize,
}

/// Probe files (folders are searched for video) in parallel and add them.
pub fn add_paths_blocking(app: &AppHandle, paths: Vec<PathBuf>) -> AddResult {
    let state = app.state::<AppState>();
    let files = collect_video_files(&paths);
    let total = files.len();
    state.probing.store(true, Ordering::Relaxed);
    let _ = app.emit(EVENT_PROBE, serde_json::json!({"done": 0, "total": total}));
    let _ = app.emit(EVENT_STATE, ());

    let results = std::sync::Mutex::new(Vec::with_capacity(total));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= files.len() {
                    break;
                }
                let r = probe(&files[i]).map_err(|e| e.to_string());
                results.lock().unwrap().push((i, r));
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 4 == 0 || d == total {
                    let _ = app.emit(EVENT_PROBE, serde_json::json!({"done": d, "total": total}));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| r.0);

    let mut out = AddResult {
        added: 0,
        already_there: 0,
        unreadable: 0,
        found: total,
    };
    let mut new_ids = Vec::new();
    {
        let mut project = state.project.lock().unwrap();
        for (i, r) in results {
            if r.is_err() {
                out.unreadable += 1;
            }
            match project.add_clip(&files[i].to_string_lossy(), r) {
                Some(id) => {
                    out.added += 1;
                    new_ids.push(id);
                }
                None => out.already_there += 1,
            }
        }
    }
    state.probing.store(false, Ordering::Relaxed);
    if out.added > 0 {
        state.dirty.store(true, Ordering::Relaxed);
    }
    runner::enqueue(app, new_ids);
    let _ = app.emit(EVENT_STATE, ());
    let _ = app.emit("add-result", serde_json::json!({
        "added": out.added, "already_there": out.already_there,
        "unreadable": out.unreadable, "found": out.found,
    }));
    out
}

#[tauri::command]
pub async fn add_paths(app: AppHandle, paths: Vec<String>) -> AddResult {
    add_paths_blocking(&app, paths.into_iter().map(PathBuf::from).collect())
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Option<AddResult> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose clips to add")
        .add_filter("Video", VIDEO_EXTENSIONS)
        .blocking_pick_files()?;
    let paths: Vec<PathBuf> = picked.into_iter().filter_map(|p| p.into_path().ok()).collect();
    Some(add_paths_blocking(&app, paths))
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<AddResult> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose a folder of footage")
        .blocking_pick_folder()?;
    let path = picked.into_path().ok()?;
    Some(add_paths_blocking(&app, vec![path]))
}

#[tauri::command]
pub fn start_analysis(app: AppHandle, state: State<'_, AppState>) {
    let ids = state.project.lock().unwrap().clips_needing_analysis();
    runner::enqueue(&app, ids);
}

#[tauri::command]
pub fn stop_analysis(app: AppHandle) {
    runner::stop(&app);
    let _ = app.emit(EVENT_STATE, ());
}

#[tauri::command]
pub fn set_selection(state: State<'_, AppState>, selection: SelectionSettings) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        let mut s = selection;
        s.focus_min = s.focus_min.clamp(0.0, 1.0);
        s.exposure_min = s.exposure_min.clamp(0.0, 1.0);
        s.stability_min = s.stability_min.clamp(0.0, 1.0);
        s.smoothing = s.smoothing.clamp(0.0, 10.0);
        s.min_length = s.min_length.clamp(0.25, 60.0);
        s.max_length = s.max_length.clamp(s.min_length, 300.0);
        if p.selection != s {
            p.selection = s;
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    state.snapshot()
}

#[tauri::command]
pub fn set_sample_rate(app: AppHandle, state: State<'_, AppState>, rate: f64, reanalyze: bool) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        if (p.sample_rate - rate).abs() > 1e-9 {
            p.set_sample_rate(rate);
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    if reanalyze {
        // Clips queued at the old rate would be measured at the wrong rate, so restart.
        let running = state.runner.lock().unwrap().running;
        if running {
            runner::stop(&app);
            let app2 = app.clone();
            std::thread::spawn(move || {
                let st = app2.state::<AppState>();
                for _ in 0..600 {
                    if !st.runner.lock().unwrap().running {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let ids = st.project.lock().unwrap().clips_needing_analysis();
                runner::enqueue(&app2, ids);
            });
        } else {
            let ids = state.project.lock().unwrap().clips_needing_analysis();
            runner::enqueue(&app, ids);
        }
    }
    state.snapshot()
}

#[derive(Deserialize)]
pub struct ExportInput {
    /// None for "most common in the footage".
    pub rate: Option<Rate>,
    pub drop_frame: Option<bool>,
    pub layout: TimelineLayout,
    pub timeline_name: String,
}

#[tauri::command]
pub fn set_export(state: State<'_, AppState>, export: ExportInput) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        let before = p.export.clone();
        p.export.timeline_rate = export.rate;
        p.export.drop_frame = export.drop_frame;
        p.export.layout = export.layout;
        let name = export.timeline_name.trim();
        p.export.timeline_name = if name.is_empty() { "Selects".into() } else { name.to_string() };
        if p.export != before {
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    state.snapshot()
}

#[derive(Serialize)]
pub struct ClipDetail {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub duration: f64,
    pub measured_until: f64,
    pub info: Option<selects_engine::probe::ClipInfo>,
    pub raw: ScoreSeries,
    pub smoothed: ScoreSeries,
    pub segments: Vec<Segment>,
    pub selection: SelectionSettings,
    pub sample_rate: Option<f64>,
    pub decoder: Option<String>,
    pub analysis_seconds: Option<f64>,
}

#[tauri::command]
pub fn clip_detail(state: State<'_, AppState>, id: u64) -> CmdResult<ClipDetail> {
    let p = state.project.lock().unwrap();
    let c = p
        .clip(id)
        .ok_or("That clip is no longer in the project. It may have been removed.")?;
    let sel = c.select(&p.selection);
    Ok(ClipDetail {
        id,
        name: c
            .info
            .as_ref()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| c.path.clone()),
        path: c.path.clone(),
        duration: c.info.as_ref().map(|i| i.duration).unwrap_or(0.0),
        measured_until: c.measured_until(),
        info: c.info.clone(),
        raw: c.scores().unwrap_or_default(),
        smoothed: sel.smoothed.clone(),
        segments: sel.segments.clone(),
        selection: p.selection.clone(),
        sample_rate: c.analysis.as_ref().map(|a| a.sample_rate),
        decoder: c.analysis.as_ref().map(|a| a.decoder.clone()),
        analysis_seconds: c.analysis.as_ref().filter(|a| a.complete).map(|a| a.elapsed),
    })
}

#[tauri::command]
pub fn remove_clip(state: State<'_, AppState>, id: u64) -> StateDto {
    {
        let mut r = state.runner.lock().unwrap();
        r.queue.retain(|q| *q != id);
    }
    state.project.lock().unwrap().remove_clip(id);
    state.dirty.store(true, Ordering::Relaxed);
    state.snapshot()
}

fn stop_and_wait(app: &AppHandle) {
    runner::stop(app);
    let st = app.state::<AppState>();
    for _ in 0..200 {
        if !st.runner.lock().unwrap().running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[tauri::command]
pub async fn new_project(app: AppHandle) -> StateDto {
    stop_and_wait(&app);
    let state = app.state::<AppState>();
    *state.project.lock().unwrap() = Project::new();
    *state.project_path.lock().unwrap() = None;
    state.dirty.store(false, Ordering::Relaxed);
    state.snapshot()
}

#[tauri::command]
pub async fn open_project(app: AppHandle) -> CmdResult<Option<StateDto>> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_title(format!("Open a {} project", selects_engine::PRODUCT_NAME))
        .add_filter(format!("{} project", selects_engine::PRODUCT_NAME), &[PROJECT_EXTENSION])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let loaded = Project::load(&path).map_err(|e| e.to_string())?;
    stop_and_wait(&app);
    let state = app.state::<AppState>();
    *state.project.lock().unwrap() = loaded;
    *state.project_path.lock().unwrap() = Some(path);
    state.dirty.store(false, Ordering::Relaxed);
    Ok(Some(state.snapshot()))
}

fn save_to(app: &AppHandle, save_as: bool) -> CmdResult<Option<PathBuf>> {
    let state = app.state::<AppState>();
    let existing = state.project_path.lock().unwrap().clone();
    let path = match (existing, save_as) {
        (Some(p), false) => p,
        _ => {
            let Some(picked) = app
                .dialog()
                .file()
                .set_title("Save project")
                .set_file_name(format!("Selects.{PROJECT_EXTENSION}"))
                .add_filter(format!("{} project", selects_engine::PRODUCT_NAME), &[PROJECT_EXTENSION])
                .blocking_save_file()
            else {
                return Ok(None);
            };
            let mut p = picked.into_path().map_err(|e| e.to_string())?;
            if p.extension().map(|e| e != PROJECT_EXTENSION).unwrap_or(true) {
                p.set_extension(PROJECT_EXTENSION);
            }
            p
        }
    };
    let snapshot = state.project.lock().unwrap().clone();
    snapshot.save(&path).map_err(|e| e.to_string())?;
    *state.project_path.lock().unwrap() = Some(path.clone());
    state.dirty.store(false, Ordering::Relaxed);
    Ok(Some(path))
}

#[tauri::command]
pub async fn save_project(app: AppHandle, save_as: bool) -> CmdResult<Option<StateDto>> {
    match save_to(&app, save_as)? {
        Some(_) => Ok(Some(app.state::<AppState>().snapshot())),
        None => Ok(None),
    }
}

#[derive(Serialize)]
pub struct ExportResult {
    pub folder: String,
    pub files: Vec<String>,
    pub selects: usize,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn export_timeline(app: AppHandle) -> CmdResult<Option<ExportResult>> {
    let state = app.state::<AppState>();
    let (project, name) = {
        let p = state.project.lock().unwrap();
        (p.clone(), p.export.timeline_name.clone())
    };
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Export timeline (four files are written side by side)")
        .set_file_name(format!("{}.xml", safe.trim()))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let base = path.with_extension("");
    let written = selects_engine::export::write_all(&project, &base).map_err(|e| e.to_string())?;
    Ok(Some(ExportResult {
        folder: base
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        files: written
            .files
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .collect(),
        selects: written.selects,
        warnings: written.warnings,
    }))
}

#[tauri::command]
pub async fn close_app(app: AppHandle, save: bool) -> CmdResult<bool> {
    if save {
        match save_to(&app, false)? {
            Some(_) => {}
            None => return Ok(false),
        }
    }
    runner::stop(&app);
    app.state::<AppState>().dirty.store(false, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.destroy();
    }
    Ok(true)
}

#[derive(Serialize)]
pub struct ToolCheck {
    pub ok: bool,
    pub version: Option<String>,
    pub product_name: &'static str,
    pub product_version: &'static str,
}

#[tauri::command]
pub async fn tool_check() -> ToolCheck {
    let v = selects_engine::ffmpeg::ffmpeg_version();
    ToolCheck {
        ok: v.is_some(),
        version: v,
        product_name: selects_engine::PRODUCT_NAME,
        product_version: selects_engine::PRODUCT_VERSION,
    }
}
