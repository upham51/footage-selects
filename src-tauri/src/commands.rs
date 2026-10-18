//! Commands the window calls.

use crate::runner;
use crate::state::{AppState, StateDto, EVENT_PROBE, EVENT_STATE};
use selects_engine::probe::{collect_video_files, probe, VIDEO_EXTENSIONS};
use selects_engine::project::{Project, TimelineLayout, PROJECT_EXTENSION};
use selects_engine::rate::Rate;
use selects_engine::scores::ScoreSeries;
use selects_engine::project::ResolvedProfile;
use selects_engine::selection::{Call, Mark, Segment, SelectionSettings};
use selects_engine::sidecar::CameraNotes;
use selects_engine::taste::FitResult;
use selects_engine::tone::ToneProfile;
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
        s.focus_weight = s.focus_weight.clamp(0.0, 1.0);
        s.exposure_weight = s.exposure_weight.clamp(0.0, 1.0);
        s.stability_weight = s.stability_weight.clamp(0.0, 1.0);
        // keep_moves_whole is a plain on or off.
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
    #[serde(default)]
    pub handles: f32,
    #[serde(default)]
    pub stills: bool,
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
        p.export.handles = export.handles.clamp(0.0, 10.0);
        p.export.stills = export.stills;
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
    /// The profile used for exposure and how it was decided.
    pub profile: ResolvedProfile,
    /// The profile chosen by hand for this clip, if any.
    pub chosen_profile: Option<ToneProfile>,
    /// What the camera detected on its own, if anything.
    pub camera_profile: Option<String>,
    pub camera: Option<CameraNotes>,
    pub shutter_warning: Option<String>,
    /// Shots finished videos took from this clip: start, end, video name.
    pub used: Vec<(f64, f64, String)>,
    /// How the frame viewer turns brightness into stops for false colour.
    pub tone: selects_engine::frame::ToneGuide,
    /// When and where it was recorded, its light and scene.
    pub scene: Option<selects_engine::scene::ClipScene>,
    /// What the sound check found.
    pub sound: Option<selects_engine::audio::AudioSummary>,
    /// Stretches with someone talking.
    pub talking: Vec<(f64, f64)>,
    pub audio_error: Option<String>,
    /// Calls made by hand.
    pub marks: Vec<Mark>,
}

#[tauri::command]
pub fn clip_detail(state: State<'_, AppState>, id: u64) -> CmdResult<ClipDetail> {
    let p = state.project.lock().unwrap();
    let c = p
        .clip(id)
        .ok_or("That clip is no longer in the project. It may have been removed.")?;
    let fallback = p.fallback_profile;
    // Selection across the project, so repeated takes show here as in the list.
    let index = p.clips.iter().position(|x| x.id == id).unwrap_or(0);
    let scenes = selects_engine::scene::resolve(&p);
    let sel = p.selections_with(Some(&scenes)).swap_remove(index);
    let resolved = c.resolved_profile(fallback);
    let sound_map = c.analysis.as_ref().and_then(|a| a.sound.as_ref());
    Ok(ClipDetail {
        scene: scenes.get(index).cloned(),
        sound: sound_map.map(|s| s.summary.clone()),
        talking: sound_map.map(|s| s.talking.clone()).unwrap_or_default(),
        audio_error: c.analysis.as_ref().and_then(|a| a.audio_error.clone()),
        marks: c.marks.clone(),
        tone: selects_engine::frame::tone_guide(c, resolved.profile),
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
        raw: c.scores(fallback).unwrap_or_default(),
        smoothed: sel.smoothed.clone(),
        segments: sel.segments.clone(),
        selection: p.selection.clone(),
        sample_rate: c.analysis.as_ref().map(|a| a.sample_rate),
        decoder: c.analysis.as_ref().map(|a| a.decoder.clone()),
        analysis_seconds: c.analysis.as_ref().filter(|a| a.complete).map(|a| a.elapsed),
        profile: resolved,
        chosen_profile: c.profile,
        camera_profile: c
            .info
            .as_ref()
            .and_then(|i| i.detected_profile.as_ref())
            .map(|d| format!("{}, named by {}", d.profile.label(), d.source)),
        camera: c.info.as_ref().and_then(|i| i.camera.clone()),
        shutter_warning: c
            .info
            .as_ref()
            .and_then(|i| i.camera.as_ref().and_then(|n| n.shutter_warning(i.rate.fps()))),
        used: selects_engine::taste::moments_on_clip(&p, id),
    })
}

/// One full-colour still of a clip at `t` seconds, for the frame viewer.
/// The reply is raw bytes: width and height as little-endian u32s, then RGBA pixels.
#[tauri::command]
pub async fn clip_frame(state: State<'_, AppState>, id: u64, t: f64) -> CmdResult<tauri::ipc::Response> {
    let (info, path) = {
        let p = state.project.lock().unwrap();
        let c = p.clip(id).ok_or("That clip is no longer in the project. It may have been removed.")?;
        let info = c.info.clone().ok_or("This clip could not be read, so there is no picture to show.")?;
        (info, c.path.clone())
    };
    if !std::path::Path::new(&path).exists() {
        return Err("The file is no longer where it was, so its picture cannot be shown. Its scores are kept.".into());
    }
    let still = tauri::async_runtime::spawn_blocking(move || selects_engine::frame::grab_still(&info, &path, t))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(still.to_bytes()))
}

/// The profile for clips whose camera does not say. Also remembered for new projects.
#[tauri::command]
pub fn set_fallback_profile(app: AppHandle, state: State<'_, AppState>, profile: Option<ToneProfile>) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        if p.fallback_profile != profile {
            p.fallback_profile = profile;
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    if let Some(path) = crate::updates::settings_path(&app) {
        let mut s = crate::updates::Settings::load(&path);
        s.fallback_profile = profile;
        // Remembering is a convenience; the project keeps the choice either way.
        let _ = s.save(&path);
    }
    state.snapshot()
}

/// A profile for one clip, or None to go back to the automatic choice.
#[tauri::command]
pub fn set_clip_profile(state: State<'_, AppState>, id: u64, profile: Option<ToneProfile>) -> CmdResult<StateDto> {
    {
        let mut p = state.project.lock().unwrap();
        let c = p
            .clip_mut(id)
            .ok_or("That clip is no longer in the project. It may have been removed.")?;
        if c.profile != profile {
            c.profile = profile;
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    Ok(state.snapshot())
}

/// Keep, drop or clear a stretch of a clip by hand. Marks overlapping the
/// stretch are replaced. Stars apply to kept stretches.
#[tauri::command]
pub fn mark_stretch(state: State<'_, AppState>, id: u64, start: f64, end: f64, call: Option<Call>, stars: u8) -> CmdResult<StateDto> {
    {
        let mut p = state.project.lock().unwrap();
        let c = p.clip_mut(id).ok_or("That clip is no longer in the project. It may have been removed.")?;
        let (a, b) = (start.min(end).max(0.0), start.max(end));
        if b - a < 0.05 {
            return Err("That stretch is too short to mark. Choose a longer stretch.".into());
        }
        c.marks.retain(|m| m.end <= a + 1e-6 || m.start >= b - 1e-6);
        if let Some(call) = call {
            c.marks.push(Mark { start: a, end: b, call, stars: if call == Call::Keep { stars.min(5) } else { 0 } });
            c.marks.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal));
        }
        state.dirty.store(true, Ordering::Relaxed);
    }
    Ok(state.snapshot())
}

/// Where the footage was shot and the cameras' clock zone, for clips that do not say.
#[tauri::command]
pub fn set_places(state: State<'_, AppState>, location: Option<(f64, f64)>, clock_offset_minutes: Option<i32>) -> CmdResult<StateDto> {
    if let Some((lat, lon)) = location {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err("That place is not on the map: latitude runs from -90 to 90 and longitude from -180 to 180. Nothing was changed.".into());
        }
    }
    {
        let mut p = state.project.lock().unwrap();
        let next = selects_engine::scene::PlaceSettings { location, clock_offset_minutes: clock_offset_minutes.map(|o| o.clamp(-12 * 60, 14 * 60)) };
        if p.places != next {
            p.places = next;
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    Ok(state.snapshot())
}

#[tauri::command]
pub async fn shoot_report(app: AppHandle) -> selects_engine::report::ShootReport {
    let snapshot = app.state::<AppState>().project.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || selects_engine::report::shoot_report(&snapshot)).await.unwrap_or_default()
}

/// Save the shoot report as a page.
#[tauri::command]
pub async fn save_report(app: AppHandle) -> CmdResult<Option<String>> {
    let state = app.state::<AppState>();
    let (snapshot, name) = {
        let p = state.project.lock().unwrap();
        (p.clone(), state.project_path.lock().unwrap().as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Shoot".into()))
    };
    let mut dialog = app.dialog().file();
    if let Some(dir) = save_folder(&app) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.set_title("Save the shoot report").set_file_name(format!("{name} shoot report.html")).add_filter("Web page", &["html"]).blocking_save_file() else {
        return Ok(None);
    };
    let mut path = picked.into_path().map_err(|e| e.to_string())?;
    if path.extension().map(|e| e != "html").unwrap_or(true) {
        path.set_extension("html");
    }
    let r = selects_engine::report::shoot_report(&snapshot);
    std::fs::write(&path, selects_engine::report::report_html(&format!("{name} shoot report"), &r))
        .map_err(|e| format!("The report could not be saved ({e}). Nothing was written. Choose a folder you can write to."))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[derive(Serialize)]
pub struct StillsResult {
    pub folder: String,
    pub sheet: String,
    pub written: usize,
    pub failed: Vec<(String, String)>,
    pub cancelled: bool,
}

fn write_stills(app: &AppHandle, project: &Project, parent: &std::path::Path, name: &str) -> CmdResult<StillsResult> {
    let state = app.state::<AppState>();
    state.stills_cancel.store(false, Ordering::Relaxed);
    *state.stills.lock().unwrap() = Some((0, 0));
    let _ = app.emit(EVENT_STATE, ());
    let r = selects_engine::stills::export_stills(project, parent, name, &state.stills_cancel, |d, t| {
        *state.stills.lock().unwrap() = Some((d, t));
        let _ = app.emit("stills-progress", serde_json::json!({"done": d, "total": t}));
    });
    *state.stills.lock().unwrap() = None;
    let _ = app.emit(EVENT_STATE, ());
    let w = r?;
    Ok(StillsResult {
        folder: w.folder.to_string_lossy().to_string(),
        sheet: w.sheet.to_string_lossy().to_string(),
        written: w.written,
        failed: w.failed,
        cancelled: w.cancelled,
    })
}

/// Stills of the best frame of every select and a contact sheet, in a folder the person picks.
#[tauri::command]
pub async fn export_stills(app: AppHandle) -> CmdResult<Option<StillsResult>> {
    let state = app.state::<AppState>();
    if state.stills.lock().unwrap().is_some() {
        return Err("Stills are already being written. Wait for them to finish.".into());
    }
    let (project, name) = {
        let p = state.project.lock().unwrap();
        (p.clone(), p.export.timeline_name.clone())
    };
    let mut dialog = app.dialog().file();
    if let Some(dir) = save_folder(&app) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.set_title("Choose where the stills folder goes").blocking_pick_folder() else {
        return Ok(None);
    };
    let parent = picked.into_path().map_err(|e| e.to_string())?;
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || write_stills(&app2, &project, &parent, &name)).await.map_err(|e| e.to_string())?.map(Some)
}

#[tauri::command]
pub fn stop_stills(state: State<'_, AppState>) {
    state.stills_cancel.store(true, Ordering::Relaxed);
}

/// Choose a song for a reel and find its beats.
#[tauri::command]
pub async fn choose_music(app: AppHandle) -> CmdResult<Option<StateDto>> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose a song to cut the reel to")
        .add_filter("Music", &["mp3", "m4a", "aac", "wav", "aif", "aiff", "flac", "ogg", "opus", "mp4", "mov"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    if state.reading_music.swap(true, Ordering::Relaxed) {
        return Err("A song is already being read. Wait for it to finish.".into());
    }
    let _ = app.emit(EVENT_STATE, ());
    let never = std::sync::atomic::AtomicBool::new(false);
    let found = tauri::async_runtime::spawn_blocking(move || selects_engine::beats::beat_grid(&path, &never)).await.map_err(|e| e.to_string());
    state.reading_music.store(false, Ordering::Relaxed);
    let (music, grid) = match found {
        Ok(Ok(v)) => v,
        Ok(Err(e)) | Err(e) => {
            let _ = app.emit(EVENT_STATE, ());
            return Err(e);
        }
    };
    {
        let mut p = state.project.lock().unwrap();
        p.reel.music = Some(music);
        p.reel.grid = Some(grid);
        state.dirty.store(true, Ordering::Relaxed);
    }
    Ok(Some(state.snapshot()))
}

#[derive(Deserialize)]
pub struct ReelInput {
    pub pace: selects_engine::reel::Pace,
    pub length: Option<f64>,
    pub order: selects_engine::reel::ReelOrder,
    pub shape: selects_engine::reel::ReelShape,
}

#[tauri::command]
pub fn set_reel(state: State<'_, AppState>, reel: ReelInput) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        let r = &mut p.reel;
        let length = reel.length.map(|l| l.clamp(3.0, 3600.0));
        if r.pace != reel.pace || r.length != length || r.order != reel.order || r.shape != reel.shape {
            r.pace = reel.pace;
            r.length = length;
            r.order = reel.order;
            r.shape = reel.shape;
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    state.snapshot()
}

fn current_plan(p: &Project) -> CmdResult<selects_engine::reel::ReelPlan> {
    let scenes = selects_engine::scene::resolve(p);
    let (sels, _) = p.select_all_with(&scenes);
    selects_engine::reel::plan(p, &sels, &scenes, &p.reel)
}

/// The reel the current selects and song would make, without writing anything.
#[tauri::command]
pub async fn plan_reel(app: AppHandle) -> CmdResult<selects_engine::reel::ReelPlan> {
    let snapshot = app.state::<AppState>().project.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || current_plan(&snapshot)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn export_reel(app: AppHandle) -> CmdResult<Option<ExportResult>> {
    let state = app.state::<AppState>();
    let project = state.project.lock().unwrap().clone();
    let plan = current_plan(&project)?;
    let music = project.reel.music.clone().ok_or("Choose a song first.")?;
    let name = format!("{} reel", selects_engine::export::strip_known_extensions(project.export.timeline_name.trim()));
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    let mut dialog = app.dialog().file();
    if let Some(dir) = save_folder(&app) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.set_title("Export reel (four files are written side by side)").set_file_name(format!("{}.xml", safe.trim())).blocking_save_file() else {
        return Ok(None);
    };
    let base = selects_engine::export::export_base(&picked.into_path().map_err(|e| e.to_string())?);
    let tl = selects_engine::export::build_reel_timeline(&project, &plan, &music, project.reel.shape, &name);
    let written = selects_engine::export::write_timeline(&tl, &base, plan.notes.clone()).map_err(|e| e.to_string())?;
    Ok(Some(ExportResult {
        folder: base.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        files: written.files.iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).collect(),
        selects: written.selects,
        warnings: written.warnings,
        stills: None,
    }))
}

#[derive(Serialize)]
pub struct TraceResult {
    pub added: Vec<String>,
    /// File name and plain-language reason.
    pub failed: Vec<(String, String)>,
    pub cancelled: bool,
}

/// Choose finished videos, fingerprint them and trace their shots to the clips.
#[tauri::command]
pub async fn add_finished_videos(app: AppHandle) -> CmdResult<Option<TraceResult>> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose finished videos you made from this footage")
        .add_filter("Video", VIDEO_EXTENSIONS)
        .blocking_pick_files()
    else {
        return Ok(None);
    };
    let paths: Vec<PathBuf> = picked.into_iter().filter_map(|p| p.into_path().ok()).collect();
    let state = app.state::<AppState>();
    if state.tracing.lock().unwrap().is_some() {
        return Err("Finished videos are already being read. Wait for them to finish, then add more.".into());
    }
    state.tracing_cancel.store(false, Ordering::Relaxed);
    let total = paths.len();
    let mut out = TraceResult { added: vec![], failed: vec![], cancelled: false };
    for (i, path) in paths.iter().enumerate() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        *state.tracing.lock().unwrap() = Some((name.clone(), i, total));
        let _ = app.emit(EVENT_STATE, ());
        let print = selects_engine::reference::fingerprint_reference(
            path,
            selects_engine::reference::REFERENCE_SAMPLE_RATE,
            &state.tracing_cancel,
        );
        match print {
            Ok(print) => {
                let mut p = state.project.lock().unwrap();
                let fv = selects_engine::taste::trace(&p, &print);
                p.finished_videos.retain(|f| f.path != fv.path);
                p.finished_videos.push(fv);
                state.dirty.store(true, Ordering::Relaxed);
                out.added.push(name);
            }
            Err(selects_engine::analyze::AnalyzeError::Cancelled) => {
                out.cancelled = true;
                break;
            }
            Err(e) => out.failed.push((name, e.to_string())),
        }
    }
    *state.tracing.lock().unwrap() = None;
    let _ = app.emit(EVENT_STATE, ());
    Ok(Some(out))
}

#[tauri::command]
pub fn stop_tracing(state: State<'_, AppState>) {
    state.tracing_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn remove_finished_video(state: State<'_, AppState>, path: String) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        let before = p.finished_videos.len();
        p.finished_videos.retain(|f| f.path != path);
        if p.finished_videos.len() != before {
            state.dirty.store(true, Ordering::Relaxed);
        }
    }
    state.snapshot()
}

/// Trace every finished video again, for example after adding footage.
#[tauri::command]
pub fn retrace_finished_videos(state: State<'_, AppState>) -> StateDto {
    {
        let mut p = state.project.lock().unwrap();
        selects_engine::taste::retrace_all(&mut p);
        state.dirty.store(true, Ordering::Relaxed);
    }
    state.snapshot()
}

/// Settings that keep more of the shots the finished videos used. Nothing
/// changes until the person applies them with set_selection.
#[tauri::command]
pub async fn fit_to_finished_videos(app: AppHandle) -> CmdResult<FitResult> {
    let snapshot = app.state::<AppState>().project.lock().unwrap().clone();
    selects_engine::taste::fit(&snapshot).ok_or_else(|| {
        "There is nothing to learn from yet: no shots from your finished videos were found in these clips, and no stretch is kept by hand. Keep a few stretches you like with K, or add a video you cut from this footage, then try again.".to_string()
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
    let mut fresh = Project::new();
    fresh.fallback_profile = remembered_fallback(&app);
    *state.project.lock().unwrap() = fresh;
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

/// Where save and export dialogs open: beside the saved project if there is
/// one, otherwise the Documents folder, so files never land among the
/// footage by accident.
pub fn default_save_folder(project_path: Option<&std::path::Path>, documents: Option<PathBuf>) -> Option<PathBuf> {
    project_path
        .and_then(|p| p.parent())
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.to_path_buf())
        .or(documents)
}

fn save_folder(app: &AppHandle) -> Option<PathBuf> {
    let saved = app.state::<AppState>().project_path.lock().unwrap().clone();
    default_save_folder(saved.as_deref(), app.path().document_dir().ok())
}

fn save_to(app: &AppHandle, save_as: bool) -> CmdResult<Option<PathBuf>> {
    let state = app.state::<AppState>();
    let existing = state.project_path.lock().unwrap().clone();
    let path = match (existing, save_as) {
        (Some(p), false) => p,
        _ => {
            let mut dialog = app.dialog().file();
            if let Some(dir) = save_folder(app) {
                dialog = dialog.set_directory(dir);
            }
            let Some(picked) = dialog
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
    /// Stills written alongside, when asked for.
    pub stills: Option<StillsResult>,
}

#[tauri::command]
pub async fn export_timeline(app: AppHandle) -> CmdResult<Option<ExportResult>> {
    let state = app.state::<AppState>();
    let (project, name) = {
        let p = state.project.lock().unwrap();
        (p.clone(), selects_engine::export::strip_known_extensions(p.export.timeline_name.trim()).to_string())
    };
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let mut dialog = app.dialog().file();
    if let Some(dir) = save_folder(&app) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog
        .set_title("Export timeline (four files are written side by side)")
        .set_file_name(format!("{}.xml", safe.trim()))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let base = selects_engine::export::export_base(&path);
    let written = selects_engine::export::write_all(&project, &base).map_err(|e| e.to_string())?;
    let mut stills = None;
    let mut warnings = written.warnings.clone();
    if project.export.stills {
        if let Some(parent) = base.parent().map(|p| p.to_path_buf()) {
            let app2 = app.clone();
            let proj = project.clone();
            let name2 = name.clone();
            match tauri::async_runtime::spawn_blocking(move || write_stills(&app2, &proj, &parent, &name2)).await {
                Ok(Ok(r)) => stills = Some(r),
                Ok(Err(e)) => warnings.push(format!("The timeline was written, but the stills were not: {e}")),
                Err(e) => warnings.push(format!("The timeline was written, but the stills were not: {e}")),
            }
        }
    }
    Ok(Some(ExportResult {
        stills,
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
        warnings,
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

/// The profile last chosen for clips whose camera does not say.
pub fn remembered_fallback(app: &AppHandle) -> Option<ToneProfile> {
    crate::updates::settings_path(app)
        .map(|p| crate::updates::Settings::load(&p))
        .and_then(|s| s.fallback_profile)
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

#[cfg(test)]
mod tests {
    use super::default_save_folder;
    use std::path::{Path, PathBuf};

    #[test]
    fn dialogs_open_in_documents_until_the_project_is_saved() {
        let docs = Some(PathBuf::from("/Users/me/Documents"));
        assert_eq!(default_save_folder(None, docs.clone()), docs);
        assert_eq!(
            default_save_folder(Some(Path::new("/Users/me/Projects/trip.selects")), docs.clone()),
            Some(PathBuf::from("/Users/me/Projects"))
        );
        assert_eq!(default_save_folder(Some(Path::new("trip.selects")), docs.clone()), docs);
        assert_eq!(default_save_folder(None, None), None);
    }
}
