//! Analysing several clips at once on worker threads.

use crate::state::{AppState, EVENT_ANALYSIS, EVENT_STATE};
use selects_engine::analyze::{analysis_size, analyze_clip, AnalyzeError, AnalyzeOptions, ClipAnalysis, Progress};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct RunnerInner {
    pub running: bool,
    pub cancel: Arc<AtomicBool>,
    pub queue: VecDeque<u64>,
    pub active: HashSet<u64>,
    pub workers: usize,
    pub started: Option<Instant>,
    /// Seconds of footage queued in this run.
    pub total_seconds: f64,
    /// Seconds of footage in clips finished in this run.
    pub finished_seconds: f64,
    pub clips_total: usize,
    pub clips_done: usize,
}

/// How many clips to decode at once. Each ffmpeg process uses several threads
/// of its own, so this stays well under the core count.
pub fn worker_count() -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (cores / 2).clamp(2, 6)
}

static LAST_EMIT: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Tell the window something changed, at most about four times a second.
pub fn emit_throttled(app: &AppHandle) {
    let now = now_ms();
    let last = LAST_EMIT.load(Ordering::Relaxed);
    if now.saturating_sub(last) >= 250
        && LAST_EMIT
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        let _ = app.emit(EVENT_ANALYSIS, ());
    }
}

/// Queue the given clips and make sure workers are running.
pub fn enqueue(app: &AppHandle, ids: Vec<u64>) {
    let state = app.state::<AppState>();
    let durations: Vec<(u64, f64)> = {
        let project = state.project.lock().unwrap();
        ids.iter()
            .filter_map(|id| {
                project
                    .clip(*id)
                    .and_then(|c| c.info.as_ref())
                    .map(|i| (*id, i.duration))
            })
            .collect()
    };
    if durations.is_empty() {
        return;
    }
    let spawn = {
        let mut r = state.runner.lock().unwrap();
        if !r.running {
            r.running = true;
            r.cancel = Arc::new(AtomicBool::new(false));
            r.started = Some(Instant::now());
            r.total_seconds = 0.0;
            r.finished_seconds = 0.0;
            r.clips_total = 0;
            r.clips_done = 0;
        }
        for (id, d) in durations {
            if !r.queue.contains(&id) && !r.active.contains(&id) {
                r.queue.push_back(id);
                r.total_seconds += d;
                r.clips_total += 1;
            }
        }
        let want = worker_count().min(r.queue.len() + r.active.len());
        let spawn = want.saturating_sub(r.workers);
        r.workers += spawn;
        spawn
    };
    for _ in 0..spawn {
        let app = app.clone();
        std::thread::spawn(move || worker(app));
    }
    let _ = app.emit(EVENT_ANALYSIS, ());
}

/// Stop after the clips in progress are abandoned. Finished clips are kept.
pub fn stop(app: &AppHandle) {
    let state = app.state::<AppState>();
    let r = state.runner.lock().unwrap();
    r.cancel.store(true, Ordering::Relaxed);
}

fn worker(app: AppHandle) {
    let state = app.state::<AppState>();
    loop {
        let (id, cancel) = {
            let mut r = state.runner.lock().unwrap();
            if r.cancel.load(Ordering::Relaxed) {
                r.queue.clear();
                break;
            }
            match r.queue.pop_front() {
                Some(id) => {
                    r.active.insert(id);
                    (id, r.cancel.clone())
                }
                None => break,
            }
        };
        let (info, opts) = {
            let mut project = state.project.lock().unwrap();
            let opts = AnalyzeOptions {
                sample_rate: project.sample_rate,
                width: project.analysis_width,
                hardware_decode: true,
                audio: true,
            };
            let info = project.clip(id).and_then(|c| c.info.clone());
            if let (Some(info), Some(clip)) = (info.as_ref(), project.clip_mut(id)) {
                let (w, h) = analysis_size(info, opts.width);
                clip.analysis = Some(ClipAnalysis::empty(opts.sample_rate, w, h));
                clip.error = None;
            }
            (info, opts)
        };
        let Some(info) = info else {
            finish(&app, id, 0.0);
            continue;
        };
        let result = analyze_clip(&info, &opts, &cancel, |p| {
            {
                let mut project = state.project.lock().unwrap();
                if let Some(a) = project.clip_mut(id).and_then(|c| c.analysis.as_mut()) {
                    match p {
                        Progress::Frames(t) => a.frames.extend_from(t),
                        Progress::Restart => a.frames = Default::default(),
                    }
                }
            }
            emit_throttled(&app);
        });
        {
            let mut project = state.project.lock().unwrap();
            if let Some(clip) = project.clip_mut(id) {
                match result {
                    Ok(a) => clip.analysis = Some(a),
                    // Partial measurements stay; the clip is offered for analysis again.
                    Err(AnalyzeError::Cancelled) => {}
                    Err(e) => {
                        clip.analysis = None;
                        clip.error = Some(e.to_string());
                    }
                }
            }
        }
        state.dirty.store(true, Ordering::Relaxed);
        finish(&app, id, info.duration);
    }
    let last = {
        let mut r = state.runner.lock().unwrap();
        r.workers -= 1;
        if r.workers == 0 {
            r.running = false;
            r.active.clear();
            true
        } else {
            false
        }
    };
    if last {
        // Newly analysed clips may hold shots from the finished videos.
        let mut p = state.project.lock().unwrap();
        if !p.finished_videos.is_empty() {
            selects_engine::taste::retrace_all(&mut p);
        }
        drop(p);
        let _ = app.emit(EVENT_STATE, ());
    }
}

fn finish(app: &AppHandle, id: u64, duration: f64) {
    let state = app.state::<AppState>();
    {
        let mut r = state.runner.lock().unwrap();
        r.active.remove(&id);
        r.clips_done += 1;
        r.finished_seconds += duration;
    }
    let _ = app.emit(EVENT_ANALYSIS, ());
}
