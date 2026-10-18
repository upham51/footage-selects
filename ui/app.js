"use strict";
/* The app window. Talks to the Rust side through Tauri's global API. */

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const listen = (name, fn) => tauri.event.listen(name, fn);

/* The product name comes from productName in src-tauri/tauri.conf.json, via tool_check. */
let product = { name: "", version: "" };
const appName = () => product.name || "the app";
const AppName = () => product.name || "The app";

/* ---------- small helpers ---------- */

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

function fmtDur(s) {
  if (!isFinite(s) || s <= 0) return "0 s";
  if (s < 10) return `${s.toFixed(1)} s`;
  if (s < 60) return `${Math.round(s)} s`;
  if (s < 3600) {
    const m = Math.floor(s / 60), r = Math.round(s - m * 60);
    return r === 60 ? `${m + 1} min` : r ? `${m} min ${r} s` : `${m} min`;
  }
  const h = Math.floor(s / 3600), m = Math.round((s - h * 3600) / 60);
  return m === 60 ? `${h + 1} h` : `${h} h ${String(m).padStart(2, "0")} min`;
}
function fmtClock(s) {
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  const h = Math.floor(m / 60);
  const mm = h ? String(m % 60).padStart(2, "0") : String(m);
  return `${h ? h + ":" : ""}${mm}:${r.toFixed(1).padStart(4, "0")}`;
}
const pct = (v) => Math.round((v ?? 0) * 100);
const plural = (n, one, many) => `${n} ${n === 1 ? one : many || one + "s"}`;

const ICON = {
  tick: '<path d="M2.5 7.5l3 3 6-7"/>',
  clock: '<circle cx="7" cy="7" r="5.2"/><path d="M7 4.2V7l2 1.4"/>',
  triangle: '<path d="M7 1.8l5.6 9.9H1.4z"/><path d="M7 5.6v2.6M7 10.1v.1"/>',
  dash: '<path d="M3.2 7h7.6"/>',
  ring: '<circle cx="7" cy="7" r="5.2" stroke-opacity=".3"/><path d="M7 1.8a5.2 5.2 0 0 1 5.2 5.2"/>',
  stack: '<path d="M2.5 4h9M2.5 7h9M2.5 10h5"/>',
  plus: '<path d="M7 2.5v9M2.5 7h9"/>',
  folder: '<path d="M1.8 4.2c0-.7.5-1.2 1.2-1.2h2.6l1.3 1.4H11c.7 0 1.2.5 1.2 1.2v4.6c0 .7-.5 1.2-1.2 1.2H3c-.7 0-1.2-.5-1.2-1.2z"/>',
  back: '<path d="M8.8 2.8L4.6 7l4.2 4.2"/>',
  stop: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.2"/>',
  play: '<path d="M4.5 2.8v8.4L11 7z"/>',
  down: '<path d="M7 2v7M4 6l3 3 3-3M2.5 11.5h9"/>',
  aside: '<path d="M2.5 7h6.5M6.5 4.2L9.3 7 6.5 9.8M11.5 3v8"/>',
  left: '<path d="M8.8 2.8L4.6 7l4.2 4.2"/>',
  right: '<path d="M5.2 2.8L9.4 7l-4.2 4.2"/>',
  film: '<rect x="2" y="2.5" width="10" height="9" rx="1.5"/><path d="M4.6 2.5v9M9.4 2.5v9M2 5.5h2.6M2 8.5h2.6M9.4 5.5H12M9.4 8.5H12"/>',
  star: '<path d="M7 1.9l1.55 3.2 3.5.48-2.55 2.44.63 3.48L7 9.85 3.87 11.5l.63-3.48L1.95 5.58l3.5-.48z"/>',
  cross: '<path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/>',
  hand: '<path d="M4.5 7.5V3.8a.9.9 0 0 1 1.8 0V7M6.3 6.5V2.9a.9.9 0 0 1 1.8 0v3.6M8.1 6.6V3.6a.9.9 0 0 1 1.8 0v4.6c0 2-1.4 3.6-3.4 3.6-1.3 0-2.2-.6-2.9-1.6L2.3 8.1a.9.9 0 0 1 1.4-1.1l.8.9"/>',
  sun: '<circle cx="7" cy="7" r="2.6"/><path d="M7 1.5v1.3M7 11.2v1.3M1.5 7h1.3M11.2 7h1.3M3.1 3.1l.9.9M10 10l.9.9M3.1 10.9l.9-.9M10 4l.9-.9"/>',
  wave: '<path d="M1.8 7h1.4M4.6 4.5v5M7 2.8v8.4M9.4 4.5v5M10.8 7h1.4"/>',
  level: '<path d="M1.8 9.2l10.4-3.4M4 11.5h6"/>',
};
const LIGHT_WORDS = { night: "Night", blue_hour: "Blue hour", golden_hour: "Golden hour", daylight: "Daylight", harsh_midday: "Harsh midday" };
const SOUND_WORDS = { wind: "Wind noise", clipped: "Sound clipped", dropout: "Sound drops out" };
const LIGHT_FILTER_WORDS = { any: "any light", golden_hour: "golden hour", blue_hour: "blue hour", daylight: "daylight", night: "night" };
const starsHtml = (n) => (n ? `<span class="stars" role="img" aria-label="${n} ${n === 1 ? "star" : "stars"}">${Array.from({ length: n }, () => icon("star", "icon icon-star")).join("")}</span>` : "");
const starText = (n) => (n ? `${n} ${n === 1 ? "star" : "stars"}` : "");
const icon = (name, cls = "icon") => `<svg class="${cls}" viewBox="0 0 14 14" aria-hidden="true">${ICON[name]}</svg>`;
const pill = (tone, shape, word) => `<span class="pill pill-${tone}">${icon(shape)}${esc(word)}</span>`;

const CLIP_STATUS = {
  analyzed: ["green", "tick", "Analyzed"],
  analyzing: ["navy", "ring", "Analyzing"],
  waiting: ["grey", "clock", "Waiting"],
  failed: ["red", "triangle", "Could not analyze"],
  unreadable: ["red", "triangle", "Could not read"],
  missing: ["amber", "dash", "File missing"],
  changed: ["amber", "clock", "File changed"],
  partial: ["amber", "dash", "Stopped part way"],
  not_analyzed: ["grey", "dash", "Not analyzed"],
};
const SEG_STATUS = {
  kept: ["green", "tick", "Kept"],
  below_threshold: ["red", "triangle", "Below threshold"],
  too_short: ["grey", "dash", "Too short"],
  over_limit: ["amber", "stack", "Over the per-clip limit"],
  set_aside: ["grey", "aside", "Set aside"],
};
const CAMERA_WORDS = { still: "Still", pan_left: "Pan left", pan_right: "Pan right", tilt_up: "Tilt up", tilt_down: "Tilt down", other: "Moving" };
const FILTER_WORDS = { any: "any movement", still: "a still camera", moving: "a moving camera", pans: "a pan", tilts: "a tilt" };
const ISSUE_WORDS = {
  clipped_highlights: "Highlights clipped.",
  underexposed: "Underexposed.",
  overexposed: "Overexposed.",
  deep_shadows: "Large areas lost in the shadows.",
  flat: "Very flat light.",
};
const fmtStops = (v) => (v == null || !isFinite(v) ? "–" : `${v > 0.05 ? "+" : v < -0.05 ? "−" : ""}${Math.abs(v).toFixed(1)}`);
/** Brightness against a normal exposure, in words: "1.1 stops brighter than normal". */
const stopsWords = (v) => (v == null || !isFinite(v) ? "–" : Math.abs(v) < 0.15 ? "normal" : `${Math.abs(v).toFixed(1)} ${Math.abs(v) >= 0.95 && Math.abs(v) < 1.05 ? "stop" : "stops"} ${v > 0 ? "brighter" : "darker"} than normal`);
const profileLabel = (id) => state?.profiles?.find((p) => p.id === id)?.label || id;
const SEG_FILL = { kept: "#E7F1EE", too_short: "#EFF2F4", over_limit: "#FDF2E2", below_threshold: "#FCEDEC", set_aside: "#E8EDF4" };
const SEG_INK = { kept: "#0F5C4E", too_short: "#7A8798", over_limit: "#9A5B00", below_threshold: "#B3261E", set_aside: "#1D3450" };
const clipName = (id) => state?.clips?.find((c) => c.id === id)?.name || "another clip";

/* ---------- state ---------- */

let state = null;          // last snapshot from Rust, null until the first one arrives
let loadError = null;      // set when the first snapshot fails
let toolError = null;      // set when ffmpeg cannot start
let probe = null;          // {done, total} while files are being read
let view = { mode: "list", clipId: null };
let detail = null;         // clip detail data, null while loading
let detailError = null;
let dragging = null;       // id of the slider being dragged
let graphWidth = 0;        // measured width of the detail graphs

/* ---------- top bar and summary ---------- */

function renderProjectLine() {
  const name = state?.project_name ? esc(state.project_name) : "New project";
  let tail = "";
  if (state?.dirty) tail = ' <span class="unsaved">· Unsaved changes</span>';
  else if (state?.project_path) tail = " · Saved";
  $("project-line").innerHTML = name + tail;
  const title = `${state?.project_name || "New project"} · ${appName()}`;
  if (document.title !== title) {
    document.title = title;
    try { tauri.window.getCurrentWindow().setTitle(title); } catch (_) { /* title is cosmetic */ }
  }
}

function renderSummary() {
  const s = state?.summary;
  const line = $("summary-line");
  if (!s || s.clips_total === 0) {
    line.innerHTML = '<span class="muted">No footage yet. Add a folder or clips to begin.</span>';
  } else {
    const parts = [
      `<span><span class="figure">${fmtDur(s.footage_seconds)}</span> of footage in</span>`,
      `<span class="sep">|</span>`,
      `<span><span class="figure">${fmtDur(s.selects_seconds)}</span> of selects out <span class="muted">(${plural(s.selects_count, "select")})</span></span>`,
      `<span class="sep">|</span>`,
      `<span><span class="figure">${s.clips_analyzed}</span> of ${s.clips_total} clips analyzed</span>`,
    ];
    if (s.clips_failed) parts.push(`<span class="sep">|</span><span class="muted">${plural(s.clips_failed, "clip")} could not be read and ${s.clips_failed === 1 ? "is" : "are"} left out</span>`);
    line.innerHTML = parts.join("");
  }

  const a = state?.analysis;
  const row = $("progress-row");
  const btn = $("btn-analysis");
  if (!a || (!a.running && a.pending === 0)) { row.hidden = true; return; }
  row.hidden = false;
  if (a.running) {
    const left = a.eta_seconds != null ? `about ${fmtDur(a.eta_seconds)} left` : "estimating time left";
    const speed = a.speed ? ` · ${Math.round(a.speed)}x real time` : "";
    $("progress-text").innerHTML = a.stopping
      ? "<strong>Stopping</strong> after the clips in progress"
      : `<strong>${a.clips_done} of ${plural(a.clips_total, "clip")} done</strong>, ${a.active} in progress · ${left}${speed}`;
    $("progress-fill").style.width = `${(a.fraction * 100).toFixed(1)}%`;
    $("progress-track").setAttribute("aria-valuenow", String(Math.round(a.fraction * 100)));
    btn.className = "btn btn-secondary btn-small";
    btn.innerHTML = `${icon("stop")}Stop analysis`;
    btn.disabled = a.stopping;
  } else {
    $("progress-text").innerHTML = `<strong>${plural(a.pending, "clip")} not analyzed yet.</strong> Their selects are missing or out of date.`;
    $("progress-fill").style.width = "0%";
    btn.className = "btn btn-primary btn-small";
    btn.innerHTML = `${icon("play")}Analyze footage`;
    btn.disabled = false;
  }
}

/* ---------- main panel: list ---------- */

function headerHtml() {
  const n = state?.clips?.length || 0;
  return `<div class="panel-head">
    <div class="left"><h2 class="eyebrow">Clips</h2><span class="count">${n ? plural(n, "clip") : ""}</span></div>
    <div class="right">
      <button class="btn btn-secondary btn-small" data-act="add-folder">${icon("folder")}Add folder</button>
      <button class="btn btn-secondary btn-small" data-act="add-files">${icon("plus")}Add clips</button>
    </div></div>`;
}

function loadingHtml(title) {
  const rows = Array.from({ length: 6 }, () => "<div class=\"skeleton-row\"><span></span><span></span><span></span><span></span></div>").join("");
  return `<div class="state"><div class="loading"><div class="loading-title"><span class="spinner" aria-hidden="true"></span>${esc(title)}</div>${rows}</div></div>`;
}

function errorHtml(title, body, next) {
  return `<div class="state"><div class="error-box" role="alert">${icon("triangle")}<div><strong>${esc(title)}</strong><p>${esc(body)}</p>${next ? `<p>${esc(next)}</p>` : ""}</div></div></div>`;
}

function emptyHtml() {
  return `<div class="state"><div class="empty-drop">
    ${icon("down", "icon icon-lg")}
    <h1>Drag a folder or clips here</h1>
    <p>${esc(AppName())} reads every clip, scores focus, exposure and camera stability, and picks the best stretches. Nothing leaves your computer.</p>
    <div class="row">
      <button class="btn btn-primary" data-act="add-folder">${icon("folder")}Add a folder</button>
      <button class="btn btn-secondary" data-act="add-files">${icon("plus")}Add clips</button>
    </div></div></div>`;
}

function stripSvg(c) {
  const d = c.duration || 1;
  let s = `<rect x="0" y="3" width="1000" height="8" rx="4" fill="#EFF2F4"/>`;
  if (c.status === "analyzing" || c.status === "partial") {
    s += `<rect x="0" y="3" width="${(c.progress * 1000).toFixed(1)}" height="8" rx="4" fill="#D3DEEC"/>`;
  }
  for (const k of c.kept) {
    const x = (k.start / d) * 1000, w = Math.max(6, ((k.end - k.start) / d) * 1000);
    s += `<rect x="${x.toFixed(1)}" y="1" width="${w.toFixed(1)}" height="12" rx="3" fill="#0F5C4E"/>`;
  }
  for (const [a, b] of c.used || []) {
    const x = (a / d) * 1000, w = Math.max(6, ((b - a) / d) * 1000);
    s += `<rect x="${x.toFixed(1)}" y="0.8" width="${w.toFixed(1)}" height="12.4" rx="3" fill="none" stroke="#9A5B00" stroke-width="1.6" stroke-dasharray="3 2" vector-effect="non-scaling-stroke"/>`;
  }
  return `<svg class="strip" viewBox="0 0 1000 14" preserveAspectRatio="none" aria-hidden="true">${s}</svg>`;
}

function statusCell(c) {
  const [tone, shape, word] = CLIP_STATUS[c.status] || CLIP_STATUS.not_analyzed;
  const label = c.status === "analyzing" ? `${word} ${pct(c.progress)}%` : word;
  return pill(tone, shape, label);
}

const SCORE_WORD = { focus: "Focus", exposure: "Exposure", stability: "Stability" };
/** What a low score means in the picture. */
const SCORE_PROBLEM = { focus: "Too soft", exposure: "Too dark or too bright", stability: "Too shaky" };

/** Why an analyzed clip has no selects, with a shape and a word as well as color. */
function noSelectHtml(r) {
  let p, text;
  if (r.kind === "too_short") {
    p = pill("grey", "dash", "Too short");
    text = `Good stretches last up to ${fmtDur(r.longest)}, the shortest select is ${fmtDur(r.needs)}`;
  } else if (r.kind === "set_aside") {
    p = pill("grey", "aside", "Set aside");
    const bits = [];
    if (r.camera) bits.push(`${plural(r.camera, "good stretch", "good stretches")} without ${FILTER_WORDS[state?.selection?.camera] || "the chosen movement"}`);
    if (r.repeats) bits.push(`${plural(r.repeats, "repeated take")} of a better select in another clip`);
    if (r.light) bits.push(`${plural(r.light, "good stretch", "good stretches")} not in ${LIGHT_FILTER_WORDS[state?.selection?.light] || "the chosen light"}`);
    if (r.sound) bits.push(`${plural(r.sound, "good stretch", "good stretches")} with sound problems`);
    if (r.dropped) bits.push(`${plural(r.dropped, "stretch", "stretches")} dropped by hand`);
    text = bits.join(", ");
  } else {
    p = pill("red", "triangle", "Below threshold");
    const word = (SCORE_WORD[r.score] || r.score).toLowerCase();
    const problem = SCORE_PROBLEM[r.score] ? `${SCORE_PROBLEM[r.score]}: ` : "";
    text = r.kind === "average"
      ? `${problem}${word} scores ${r.average} on average, and selects need ${r.needs}`
      : `${problem}${word} is under ${r.needs} for ${r.share}% of the clip`;
  }
  return `<div class="no-select">${p}<span>${esc(text)}</span></div>`;
}

function selectsCell(c) {
  const kept = c.kept.length;
  const secs = c.kept.reduce((a, k) => a + (k.end - k.start), 0);
  let text;
  if (c.status === "unreadable" || c.status === "failed") return `<div class="status-detail" title="${esc(c.status_detail)}">Left out. ${esc(c.status_detail || "")}</div>`;
  if ((c.status === "missing" || c.status === "changed") && c.status_detail && !kept) text = esc(c.status_detail);
  else if (!kept && (c.status === "waiting" || c.status === "not_analyzed")) text = "Not scored yet";
  else if (!kept && (c.status === "analyzing" || c.status === "partial")) text = "None so far";
  else if (!kept && c.no_select) return `${stripSvg(c)}${noSelectHtml(c.no_select)}`;
  else if (!kept) text = "None meet the thresholds";
  else text = `<strong class="num">${kept}</strong> ${kept === 1 ? "select" : "selects"} · <span class="num">${fmtDur(secs)}</span>`;
  let extra = c.over_limit ? ` · ${c.over_limit} more over the limit` : "";
  if (c.set_aside) extra += ` · ${c.set_aside} set aside`;
  const used = c.used?.length ? `<div class="sel-used">${icon("film")}${plural(c.used.length, "shot")} in your edits</div>` : "";
  return `${stripSvg(c)}<div class="sel-count">${text}${kept ? extra : ""}</div>${used}`;
}

function clipTags(c) {
  const bits = [];
  if (c.light) bits.push(`<span class="tag" title="From the sun's height when and where the clip was recorded">${icon("sun")}${esc(LIGHT_WORDS[c.light] || c.light)}</span>`);
  if (c.profile_is_log) bits.push(`<span class="tag" title="Exposure is judged through this profile's curve">${esc(c.profile_label)}</span>`);
  if (c.talking) bits.push(`<span class="tag" title="Someone talks in this clip, so selects start and end in pauses">${icon("wave")}Talking</span>`);
  if (c.marks) bits.push(`<span class="tag">${icon("hand")}${plural(c.marks, "call", "calls")} by hand</span>`);
  if (c.choppy_shutter) bits.push(pill("amber", "triangle", "Fast shutter"));
  if (c.crooked) bits.push(pill("amber", "triangle", "Crooked horizon"));
  if (c.flicker) bits.push(pill("amber", "triangle", "Flicker"));
  if (c.jello) bits.push(pill("amber", "triangle", "Jello"));
  for (const k of c.sound || []) bits.push(pill("amber", "triangle", SOUND_WORDS[k] || k));
  if (c.wb_note) bits.push(`<span class="tag" title="${esc(c.wb_note)}">White balance differs</span>`);
  return bits.length ? `<div class="clip-tags">${bits.join("")}</div>` : "";
}

function listHtml() {
  const rows = state.clips.map((c) => `
    <tr data-clip="${c.id}">
      <td class="name-cell"><div class="clip-name">${esc(c.name)}</div><div class="clip-sub">${c.group ? `${esc(c.group)} · ` : ""}${esc(c.folder)}</div>${clipTags(c)}</td>
      <td class="num tabular">${c.duration ? fmtClock(c.duration) : "–"}</td>
      <td class="tabular nowrap">${c.rate ? esc(c.rate) : "–"}${c.variable_frame_rate ? '<div class="clip-sub">Variable rate</div>' : ""}</td>
      <td class="tabular col-size">${c.width ? `${c.width}×${c.height}` : "–"}</td>
      <td>${statusCell(c)}</td>
      <td class="sel-cell">${selectsCell(c)}</td>
    </tr>`).join("");
  const banner = probe ? `<div class="empty-small" style="padding:10px 16px;text-align:left"><span class="spinner" style="display:inline-block;vertical-align:-3px;margin-right:8px"></span>Reading new clips: ${probe.done} of ${probe.total}</div>` : "";
  return `${headerHtml()}${banner}<div class="table-wrap"><table>
    <thead><tr><th>Clip</th><th class="num">Length</th><th>Frame rate</th><th class="col-size">Size</th><th>Status</th><th>Selects</th></tr></thead>
    <tbody>${rows}</tbody></table></div>`;
}

/* ---------- main panel: shoot report ---------- */

let report = null, reportError = null;

async function openReport() {
  view = { mode: "report", clipId: null };
  report = null; reportError = null;
  renderMain();
  try { report = await invoke("shoot_report"); } catch (e) { reportError = String(e); }
  if (view.mode === "report") renderMain();
}

const TONE_PILL = { good: ["green", "tick", "Good"], warn: ["amber", "triangle", "Worth fixing"], info: ["grey", "dash", "Note"] };

function renderReport() {
  const panel = $("main-panel");
  const back = `<button class="link" data-act="back">${icon("back")} Back to all clips</button>`;
  if (reportError) { panel.innerHTML = `<div class="detail-head">${back}</div>` + errorHtml("The report could not be made", reportError, "Nothing was changed. Go back and try again."); return; }
  if (!report) { panel.innerHTML = `<div class="detail-head">${back}</div>` + loadingHtml("Going through the shoot"); return; }
  if (!report.clips_analyzed) {
    panel.innerHTML = `<div class="detail-head">${back}</div><div class="empty-small"><strong>Nothing to report yet</strong>The report is made from analyzed clips. Add footage and let it analyze, then open the report again.</div>`;
    return;
  }
  const cards = report.findings.map((f) => {
    const [tone, shape, word] = TONE_PILL[f.tone] || TONE_PILL.info;
    const rows = f.rows?.length ? `<table class="report-table"><thead><tr><th>Group</th><th class="num">Clips</th><th>Result</th></tr></thead><tbody>${f.rows.map(([a, n, v]) => `<tr><td>${esc(a)}</td><td class="num tabular">${n}</td><td>${esc(v)}</td></tr>`).join("")}</tbody></table>` : "";
    return `<article class="finding">${pill(tone, shape, word)}<h3>${esc(f.title)}</h3><p>${esc(f.body)}</p>${rows}</article>`;
  }).join("");
  panel.innerHTML = `<div class="detail"><div class="detail-head">${back}
      <div class="detail-title"><h2>Shoot report</h2></div>
      <div class="meta"><span><span class="num">${report.clips_analyzed}</span> of ${plural(report.clips, "clip")} analyzed</span><span><span class="num">${fmtDur(report.footage_seconds)}</span> of footage</span><span><span class="num">${report.selects}</span> selects, <span class="num">${fmtDur(report.selects_seconds)}</span></span></div>
      <div class="row-actions"><button class="btn btn-secondary btn-small" data-act="save-report">${icon("down")}Save as a page</button></div>
    </div><div class="findings">${cards}</div></div>`;
}

async function saveReport() {
  try { const path = await invoke("save_report"); if (path) toast(`${icon("tick")} Saved to ${esc(path)}`); }
  catch (e) { showError("The report was not saved", String(e)); }
}

function renderMain() {
  const panel = $("main-panel");
  if (view.mode === "detail") { renderDetail(); return; }
  if (view.mode === "report") { renderReport(); return; }
  if (toolError) { panel.innerHTML = headerHtml() + errorHtml("The video tools could not start", toolError, `Nothing was analyzed. Reinstall ${esc(appName())} from the download page, then open it again.`); return; }
  if (loadError) { panel.innerHTML = errorHtml(`${esc(AppName())} could not load its state`, loadError, "Nothing was lost. Close the app and open it again."); return; }
  if (state === null) { panel.innerHTML = loadingHtml(`Opening ${esc(appName())}`); return; }
  if (state.clips.length === 0) {
    panel.innerHTML = probe ? loadingHtml(probe.total ? `Reading clips: ${probe.done} of ${probe.total}` : "Looking for video files") : headerHtml() + emptyHtml();
    return;
  }
  const wrap = panel.querySelector(".table-wrap");
  const scroll = wrap ? wrap.scrollTop : 0;
  panel.innerHTML = listHtml();
  const nw = panel.querySelector(".table-wrap");
  if (nw) nw.scrollTop = scroll;
}

/* ---------- main panel: clip detail ---------- */

function whyText(seg, sel, cap) {
  switch (seg.status) {
    case "kept":
      if (seg.manual) return `Kept by hand${seg.stars ? `, ${starText(seg.stars)}` : ""}. It stays whatever the sliders and filters say.`;
      if (seg.camera_move && seg.end - seg.start > sel.max_length + 0.01) return "Kept whole: one camera move, longer than the longest select.";
      return seg.camera_move ? "Every score is at or above its threshold. Camera move." : "Every score is at or above its threshold.";
    case "below_threshold": {
      const names = seg.failed.map((f) => `${f} (${pct(sel[f + "_min"])})`);
      const list = names.length > 1 ? names.slice(0, -1).join(", ") + " and " + names[names.length - 1] : names[0];
      const issue = seg.failed.includes("exposure") && seg.exposure_issue ? ` ${ISSUE_WORDS[seg.exposure_issue] || ""}` : "";
      return `Dropped: ${list} under the threshold.${issue}`;
    }
    case "too_short": return `Dropped: good, but shorter than the ${fmtDur(sel.min_length)} minimum.`;
    case "over_limit": return `Dropped: good, but ${plural(cap, "better select is", "better selects are")} already kept in this clip.`;
    case "set_aside":
      if (seg.aside === "repeat" && seg.similar_to) return `Set aside: looks like the select in ${clipName(seg.similar_to.clip)} at ${fmtClock(seg.similar_to.start)}, which scored higher.`;
      if (seg.aside === "dropped") return "Dropped by hand. The clip's next best stretch can take its place.";
      if (seg.aside === "light") return `Set aside: the light filter asks for ${LIGHT_FILTER_WORDS[sel.light] || "another light"}, and this clip ${detail?.scene?.light ? `was recorded in ${(LIGHT_WORDS[detail.scene.light] || "").toLowerCase()}` : "has no known time and place to judge the light by"}.`;
      if (seg.aside === "sound") return "Set aside: its sound has a problem (see the notes), and the filter sets those aside.";
      return `Set aside: the camera filter asks for ${FILTER_WORDS[sel.camera] || "another movement"}, and this stretch is ${(CAMERA_WORDS[seg.camera] || "moving").toLowerCase()}.`;
  }
  return "";
}

/** A note for a kept or over-limit stretch that looks like a better select elsewhere. */
function repeatNote(seg) {
  if (!seg.similar_to || seg.status === "set_aside") return "";
  return ` Looks like the select in ${clipName(seg.similar_to.clip)} at ${fmtClock(seg.similar_to.start)}.`;
}

/** Problems and speech notes for a stretch that passed the thresholds, as sentences. */
function checkNotes(seg) {
  if (seg.status === "below_threshold") return "";
  const bits = [];
  if (seg.roll != null && Math.abs(seg.roll) >= 1) bits.push(`Horizon tilts down to the ${seg.roll > 0 ? "right" : "left"} by ${Math.abs(seg.roll).toFixed(1)}°; rotate ${seg.roll > 0 ? "counter-clockwise" : "clockwise"} to level it.`);
  if (seg.flicker != null && seg.flicker >= 0.012) bits.push("Light flicker: bands roll through the picture.");
  if (seg.wobble != null && seg.wobble >= 0.35) bits.push("Jello: the picture wobbles within the frame.");
  for (const k of seg.sound || []) bits.push(`${SOUND_WORDS[k] || k}.`);
  if (seg.status === "kept" && seg.snapped) bits.push(seg.snapped === 2 ? "Both ends moved into pauses in the talking." : "One end moved into a pause in the talking.");
  if (seg.status === "kept" && seg.speech_cuts) {
    const why = localSel?.snap_to_pauses === false ? ", since Start and end in a pause is off." : ": no pause was close enough.";
    bits.push((seg.speech_cuts === 2 ? "Starts and ends mid-sentence" : "Cuts mid-sentence") + why);
  }
  return bits.length ? " " + bits.join(" ") : "";
}

function graphSvg(key, label, w, d) {
  const H = 92, top = 6, h = H - 12;
  const dur = d.duration || 1;
  const x = (t) => (t / dur) * w;
  const y = (v) => top + (1 - Math.max(0, Math.min(1, v))) * h;
  let bands = "";
  for (const s of d.segments) {
    let fill = null;
    if (s.status === "below_threshold") { if (s.failed.includes(key)) fill = SEG_FILL.below_threshold; }
    else fill = SEG_FILL[s.status];
    if (fill) bands += `<rect x="${x(s.start).toFixed(1)}" y="0" width="${Math.max(1, x(s.end) - x(s.start)).toFixed(1)}" height="${H}" fill="${fill}"/>`;
  }
  if (d.measured_until < dur - 0.01) {
    bands += `<rect x="${x(d.measured_until).toFixed(1)}" y="0" width="${(w - x(d.measured_until)).toFixed(1)}" height="${H}" fill="url(#hatch)"/>`;
  }
  const line = (series, color, width) => {
    if (!series.t.length) return "";
    const pts = series.t.map((t, i) => `${x(t).toFixed(1)},${y(series[key][i]).toFixed(1)}`).join(" ");
    return `<polyline points="${pts}" fill="none" stroke="${color}" stroke-width="${width}" stroke-linejoin="round" stroke-linecap="round"/>`;
  };
  const thr = d.selection[key + "_min"];
  const ty = y(thr).toFixed(1);
  return `<svg viewBox="0 0 ${w} ${H}" width="${w}" height="${H}" data-graph="${key}" role="img" aria-label="${esc(label)} score over time">
    <defs><pattern id="hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="6" height="6" fill="#FBFCFD"/><line x1="0" y1="0" x2="0" y2="6" stroke="#E4E0D6" stroke-width="2"/></pattern></defs>
    ${bands}
    ${line(d.raw, "#C9CFD8", 1)}
    ${line(d.smoothed, "#2C4A6E", 1.7)}
    <line x1="0" x2="${w}" y1="${ty}" y2="${ty}" stroke="#2C4A6E" stroke-opacity=".55" stroke-dasharray="4 4" stroke-width="1"/>
    <text x="${w - 6}" y="${Math.max(11, ty - 4)}" text-anchor="end" font-size="11" fill="#5E6878" font-family="Inter Tight">${pct(thr)}</text>
    <line class="pin" x1="${x(viewer.pin).toFixed(1)}" x2="${x(viewer.pin).toFixed(1)}" y1="0" y2="${H}" stroke="#C9A227" stroke-width="2"/>
    <line class="cursor" x1="0" x2="0" y1="0" y2="${H}" stroke="#1D3450" stroke-width="1.5" visibility="hidden"/>
    <circle class="dot" cx="0" cy="0" r="5" fill="#fff" stroke="#2C4A6E" stroke-width="2.5" visibility="hidden"/>
  </svg>`;
}

function laneSvg(w, d) {
  const H = 28, dur = d.duration || 1;
  const x = (t) => (t / dur) * w;
  let s = `<rect x="0" y="0" width="${w}" height="${H}" rx="6" fill="#F4F2EC"/>`;
  let n = 0;
  for (const seg of d.segments) {
    const x0 = x(seg.start), x1 = x(seg.end), bw = Math.max(1, x1 - x0);
    const kept = seg.status === "kept";
    if (kept) n++;
    s += `<rect x="${x0.toFixed(1)}" y="${kept ? 0 : 3}" width="${bw.toFixed(1)}" height="${kept ? H : H - 6}" rx="5" fill="${kept ? "#0F5C4E" : SEG_FILL[seg.status]}" stroke="#fff" stroke-width="1"/>`;
    // Every band carries its shape; kept ones also their number when there is room.
    const [, shape] = SEG_STATUS[seg.status];
    const ink = kept ? "#FFFFFF" : SEG_INK[seg.status];
    if (kept && bw >= 76) {
      s += `<g transform="translate(${(x0 + 8).toFixed(1)},8) scale(.857)" fill="none" stroke="${ink}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${ICON[shape]}</g><text x="${(x0 + 24).toFixed(1)}" y="18.5" font-size="11.5" font-weight="600" fill="#FFFFFF" font-family="Inter Tight">Select ${n}</text>`;
    } else if (bw >= 18) {
      s += `<g transform="translate(${(x0 + bw / 2 - 6).toFixed(1)},8) scale(.857)" fill="none" stroke="${ink}" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">${ICON[shape]}</g>`;
    }
  }
  for (const [a, b] of d.used || []) {
    const x0 = x(a), bw = Math.max(4, x(b) - x0);
    s += `<rect x="${x0.toFixed(1)}" y="1" width="${bw.toFixed(1)}" height="${H - 2}" rx="5" fill="none" stroke="#9A5B00" stroke-width="1.6" stroke-dasharray="4 3"/>`;
  }
  for (const m of d.marks || []) {
    const x0 = x(m.start), bw = Math.max(4, x(m.end) - x0);
    s += `<rect x="${x0.toFixed(1)}" y="0.8" width="${bw.toFixed(1)}" height="${H - 1.6}" rx="5" fill="none" stroke="${m.call === "keep" ? "#2C4A6E" : "#B3261E"}" stroke-width="2"/>`;
  }
  return `<svg viewBox="0 0 ${w} ${H}" width="${w}" height="${H}" aria-hidden="true">${s}</svg>`;
}

/** A thin lane showing where someone talks. */
function talkSvg(w, d) {
  const H = 10, dur = d.duration || 1;
  let s = `<rect x="0" y="4" width="${w}" height="2" rx="1" fill="#EFF2F4"/>`;
  for (const [a, b] of d.talking || []) {
    const x0 = (a / dur) * w, bw = Math.max(1.5, ((b - a) / dur) * w);
    s += `<rect x="${x0.toFixed(1)}" y="1" width="${bw.toFixed(1)}" height="8" rx="2" fill="#7A8798"/>`;
  }
  return `<svg viewBox="0 0 ${w} ${H}" width="${w}" height="${H}" aria-hidden="true">${s}</svg>`;
}

const fmtWhen = (utc, offset) => {
  if (utc == null) return "";
  const d = new Date((utc + (offset || 0) * 60) * 1000);
  const p2 = (n) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${p2(d.getUTCMonth() + 1)}-${p2(d.getUTCDate())} ${p2(d.getUTCHours())}:${p2(d.getUTCMinutes())}`;
};

/** A drop call that covers most of this stretch, when the stretch itself shows no sign of it. */
function idleDrop(s, marks) {
  if (s.manual || s.aside === "dropped") return null;
  return (marks || []).find((m) => m.call === "drop" && Math.min(m.end, s.end) - Math.max(m.start, s.start) >= 0.5 * Math.min(s.end - s.start, m.end - m.start)) || null;
}

function callCell(s, marks) {
  if (s.manual) return `<span class="call call-keep">${icon("hand")}Kept${s.stars ? ` ${starsHtml(s.stars)}` : ""}</span>`;
  if (s.aside === "dropped" || idleDrop(s, marks)) return `<span class="call call-drop">${icon("cross")}Dropped</span>`;
  return '<span class="muted">–</span>';
}

/** Says when a drop call covers a stretch that was not a select anyway. */
function idleDropNote(s, marks) {
  return idleDrop(s, marks) ? " You dropped it by hand too; that changes nothing while it is not a select, and keeps it out if new settings would choose it." : "";
}

function sceneRowHtml(d) {
  const sc = d.scene;
  if (!sc) return "";
  const bits = [];
  if (sc.group) bits.push(`<strong>${esc(sc.group)}</strong>`);
  if (sc.utc != null) bits.push(`Recorded <span class="num">${fmtWhen(sc.utc, sc.offset_minutes)}</span> local time${sc.zone_estimated ? " (time zone estimated)" : ""}`);
  else if (sc.wall != null) bits.push(`Recorded <span class="num">${fmtWhen(sc.wall, 0)}</span> by the camera's clock, time zone unknown`);
  if (sc.light) bits.push(`${icon("sun")} ${esc(LIGHT_WORDS[sc.light])}, sun <span class="num">${sc.sun > 0 ? "" : "−"}${Math.abs(sc.sun).toFixed(0)}°</span> ${sc.sun >= 0 ? "above" : "below"} the horizon`);
  const src = [];
  if (sc.time_source) src.push(`time from ${esc(sc.time_source)}`);
  if (sc.place_source) src.push(`place from ${esc(sc.place_source)}`);
  let html = bits.length ? `<div class="meta">${bits.map((b) => `<span>${b}</span>`).join("")}${src.length ? `<span class="muted">${src.join(", ")}</span>` : ""}</div>` : "";
  if (!sc.light && sc.utc == null && sc.wall == null) html += `<div class="meta muted"><span>No recording time found, so the light cannot be worked out.</span></div>`;
  else if (!sc.light) html += `<div class="meta muted"><span>No place found, so the light cannot be worked out. Add where you shot under Places and time.</span></div>`;
  if (sc.wb_note) html += `<div class="note">${pill("grey", "dash", "White balance")}<span>${esc(sc.wb_note)}</span></div>`;
  return html;
}

function soundRowHtml(d) {
  if (d.audio_error) return `<div class="note">${pill("amber", "triangle", "Sound not checked")}<span>${esc(d.audio_error)}</span></div>`;
  const sm = d.sound;
  if (!sm) return "";
  if (sm.silent) return `<div class="meta muted"><span>${icon("wave")} The sound track is silent.</span></div>`;
  const bits = [];
  bits.push(sm.has_speech ? `Someone talks for <span class="num">${Math.round(sm.talk_share * 100)}%</span> of the clip` : "No talking found");
  let html = `<div class="meta"><span>${icon("wave")} ${bits.join("")}</span><span class="muted">Sound level around <span class="num">${Math.round(sm.median_db)}</span> dB, background <span class="num">${Math.round(sm.noise_floor_db)}</span> dB</span></div>`;
  const probs = [];
  if (sm.wind_share >= 0.2) probs.push(`${pill("amber", "triangle", "Wind noise")}<span>Wind for <span class="num">${Math.round(sm.wind_share * 100)}%</span> of the clip.</span>`);
  if (sm.clipped_events >= 3) probs.push(`${pill("amber", "triangle", "Sound clipped")}<span>The sound distorts <span class="num">${sm.clipped_events}</span> times: it was recorded too loud.</span>`);
  if (sm.dropouts?.length) probs.push(`${pill("amber", "triangle", "Sound drops out")}<span>The sound cuts out ${plural(sm.dropouts.length, "time")}, first at <span class="num">${fmtClock(sm.dropouts[0][0])}</span>.</span>`);
  for (const p of probs) html += `<div class="note">${p}</div>`;
  return html;
}

function axisSvg(w, dur) {
  const H = 18;
  const steps = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
  const step = steps.find((s) => (w / (dur / s)) >= 70) || 3600;
  let s = "";
  for (let t = 0; t <= dur + 1e-6; t += step) {
    const x = (t / dur) * w;
    const anchor = t === 0 ? "start" : x > w - 30 ? "end" : "middle";
    s += `<text x="${x.toFixed(1)}" y="13" font-size="11" fill="#5E6878" text-anchor="${anchor}" font-family="Inter Tight">${fmtClock(t).replace(/\.0$/, "")}</text>`;
  }
  return `<svg viewBox="0 0 ${w} ${H}" width="${w}" height="${H}" aria-hidden="true">${s}</svg>`;
}

function renderDetail() {
  const panel = $("main-panel");
  const row = state?.clips.find((c) => c.id === view.clipId);
  const back = `<button class="link" data-act="back">${icon("back")} Back to all clips</button>`;
  if (detailError) { panel.innerHTML = `<div class="detail-head">${back}</div>` + errorHtml("This clip could not be shown", detailError, "Nothing was changed. Go back to the list and try again."); return; }
  if (!detail || !row) { panel.innerHTML = `<div class="detail-head">${back}</div>` + loadingHtml("Loading scores"); return; }

  const d = detail, info = d.info;
  const [tone, shape, word] = CLIP_STATUS[row.status] || CLIP_STATUS.not_analyzed;
  const statusPill = pill(tone, shape, row.status === "analyzing" ? `${word} ${pct(row.progress)}%` : word);
  const meta = info ? [
    `<span class="num">${fmtClock(info.duration)}</span>`,
    `<span class="num">${info.width}×${info.height}</span>${info.rotation ? ` (rotated ${info.rotation}°)` : ""}`,
    `<span class="num">${esc(info.rate.den === 1 ? info.rate.num : (info.rate.num / info.rate.den).toFixed(3))}</span> fps${info.variable_frame_rate ? ", variable" : ""}`,
    esc(info.codec.toUpperCase()),
    info.has_audio ? `Audio, ${plural(info.audio_channels, "channel")}` : "No audio",
    info.timecode ? `Timecode <span class="num">${esc(info.timecode)}</span>` : "No timecode, starts at 00:00:00:00",
  ] : [];
  if (d.sample_rate) meta.push(`Scored at ${plural(d.sample_rate, "frame")} per second${d.analysis_seconds ? ` in ${fmtDur(d.analysis_seconds)}` : ""}`);
  if (d.decoder) meta.push(`Decoded by ${esc(d.decoder)}`);
  const isLog = d.profile && d.profile.profile !== "standard";
  const profileHtml = profileRowHtml(d);
  const cameraHtml = cameraRowHtml(d);

  // Draw at the real width of the graph area so text and strokes stay crisp.
  const w = graphWidth || Math.max(320, panel.clientWidth - 38 - LANE_LABEL);

  let body;
  if (!d.raw.t.length) {
    const msg = row.status === "unreadable" || row.status === "failed"
      ? errorHtml("This clip has no scores", row.status_detail || "It could not be read.", "Other clips are not affected. Check the file, then add it again.")
      : row.status === "waiting" || row.status === "analyzing"
        ? loadingHtml("Waiting for the first scores from this clip")
        : `<div class="empty-small"><strong>No scores yet</strong>This clip has not been analyzed. Use Analyze footage above.</div>`;
    body = msg;
  } else {
    const keptCount = d.segments.filter((s) => s.status === "kept").length;
    const mean = (key) => pct(d.smoothed[key].reduce((a, b) => a + b, 0) / d.smoothed[key].length);
    const graph = (key, label) => `<div class="lane-row"><div class="lane-meta"><span class="lane-name">${label}</span><span class="lane-value num" data-lane-value="${key}">–</span><span class="lane-stats">Average <span class="num">${mean(key)}</span><br>Needs <span class="num">${pct(d.selection[key + "_min"])}</span></span></div><div class="lane-plot graph">${graphSvg(key, label, w - 2, d)}</div></div>`;
    const frameCells = Array.from({ length: STRIP_FRAMES }, (_, j) => `<canvas width="192" height="108" data-a="${((j / STRIP_FRAMES) * d.duration).toFixed(3)}" data-b="${(((j + 1) / STRIP_FRAMES) * d.duration).toFixed(3)}"></canvas>`).join("");
    const keptSecs = d.segments.filter((s) => s.status === "kept").reduce((a, s) => a + (s.end - s.start), 0);
    const segRows = d.segments.map((s) => {
      const [t, sh, wd] = SEG_STATUS[s.status];
      const usedIn = (d.used || []).filter(([a, b, n]) => Math.min(b, s.end) - Math.max(a, s.start) > 0.25).map(([, , n]) => n);
      const usedText = usedIn.length ? ` Used in ${[...new Set(usedIn)].join(", ")}.` : "";
      const pinned = viewer.pin >= s.start && viewer.pin < s.end ? ' class="pinned"' : "";
      return `<tr data-seg="${s.start}:${s.end}"${pinned} title="Show this stretch in the frame viewer"><td class="tabular nowrap">${fmtClock(s.start)} – ${fmtClock(s.end)}</td><td class="num tabular">${fmtDur(s.end - s.start)}</td><td>${pill(t, sh, wd)}</td><td class="why">${esc(whyText(s, d.selection, keptCount) + repeatNote(s) + checkNotes(s) + idleDropNote(s, d.marks) + usedText)}</td>
        <td class="nowrap">${callCell(s, d.marks)}</td><td class="nowrap">${esc(CAMERA_WORDS[s.camera] || "Still")}</td><td class="num tabular">${pct(s.focus)}</td><td class="num tabular">${pct(s.exposure)}</td>${isLog ? `<td class="num tabular">${fmtStops(s.key_stops)}</td>` : ""}<td class="num tabular">${pct(s.stability)}</td><td class="num tabular">${pct(s.score)}</td></tr>`;
    }).join("");
    const markLegend = (d.marks || []).length ? `<span class="legend-used"><svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><rect x="1.5" y="3" width="11" height="8" rx="2" stroke="#2C4A6E"/></svg>Kept by hand</span><span class="legend-used"><svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><rect x="1.5" y="3" width="11" height="8" rx="2" stroke="#B3261E"/></svg>Dropped by hand</span>` : "";
    const usedLegend = (d.used || []).length ? `<span class="legend-used"><svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><rect x="1.5" y="3" width="11" height="8" rx="2" stroke="#9A5B00" stroke-dasharray="2.5 1.8"/></svg>Used in your edits</span>` : "";
    body = `${viewerHtml(d)}
      <div class="inst-bar">
        <span class="inst-held" id="inst-held">Held at</span><span class="inst-tc num" id="inst-tc"></span>
        <span id="inst-pill"></span>
        <span class="inst-why" id="inst-why"></span>
        <span class="inst-kept num">${keptCount ? `${fmtDur(keptSecs)} kept` : "Nothing kept"}</span>
      </div>
      <div class="legend"><span class="label">Stretches</span>${Object.values(SEG_STATUS).map(([t, s, wd]) => pill(t, s, wd)).join("")}${d.measured_until < d.duration - 0.01 ? pill("grey", "clock", "Not measured yet") : ""}${usedLegend}${markLegend}</div>
      <div class="graphs">
        <div class="lane-row lane-row-frames"><div class="lane-meta"><span class="lane-name">Frames</span></div><div class="lane-plot"><div class="frame-strip" id="frame-strip">${frameCells}</div></div></div>
        <div class="lane-row lane-row-sel"><div class="lane-meta"><span class="lane-name">Selects</span></div><div class="lane-plot">${laneSvg(w, d)}</div></div>
        ${(d.talking || []).length ? `<div class="lane-row lane-row-talk" title="Selects start and end in the gaps"><div class="lane-meta"><span class="lane-name">Talking</span></div><div class="lane-plot">${talkSvg(w, d)}</div></div>` : ""}
        ${graph("focus", "Focus")}${graph("exposure", "Exposure")}${graph("stability", "Stability")}
        <div class="lane-row"><div></div><div class="lane-plot graph-axis">${axisSvg(w, d.duration || 1)}</div></div>
      </div>
      <div class="readout" id="readout">Move the pointer over the graphs to read the scores and see the picture at that moment; click to hold it. Navy is the smoothed score used for selection, grey the raw per-frame score, the dashed line the threshold.</div>
      ${isLog ? stopsCard(d) : ""}
      <div class="seg-table"><h3 class="eyebrow">Every stretch of this clip</h3>
        <table><thead><tr><th>Time</th><th class="num">Length</th><th>Result</th><th>Why</th><th>Your call</th><th>Camera</th><th class="num">Focus</th><th class="num">Exposure</th>${isLog ? '<th class="num" title="How bright the middle tones are, in stops brighter (+) or darker (−) than a normal exposure">Brightness</th>' : ""}<th class="num">Stability</th><th class="num">Overall</th></tr></thead><tbody>${segRows}</tbody></table></div>`;
  }

  panel.innerHTML = `<div class="detail">
    <div class="detail-head">${back}
      <div class="detail-title"><h2 class="selectable">${esc(d.name)}</h2>${statusPill}</div>
      <div class="meta">${meta.map((m) => `<span>${m}</span>`).join("")}</div>
      <div class="meta selectable">${esc(d.path)}</div>
      ${cameraHtml}${sceneRowHtml(d)}${soundRowHtml(d)}${profileHtml}
    </div>${body}</div>`;
  const cp = $("clip-profile");
  if (cp) cp.value = cp.dataset.value;
  const holder = panel.querySelector(".graphs .lane-plot");
  if (holder) {
    const real = Math.floor(holder.clientWidth);
    if (real > 0 && Math.abs(real - w) > 1) { graphWidth = real; renderDetail(); return; }
  }
  attachHover(w, d);
  paintViewer();
  paintPoint(viewer.pin, false);
  loadStrip();
}

/* ---------- the instrument: readout bar, live values, dots and frame strip ---------- */

const LANE_LABEL = 116; // width of the label column beside the graphs, including the gap
const STRIP_FRAMES = 12;

function nearestIndex(ts, t) {
  let lo = 0, hi = ts.length - 1;
  while (lo < hi) { const m = (lo + hi) >> 1; if (ts[m] < t) lo = m + 1; else hi = m; }
  if (lo > 0 && Math.abs(ts[lo - 1] - t) < Math.abs(ts[lo] - t)) lo--;
  return lo;
}

/** Shows the scores and result at moment t: the bar above the graphs, each lane's value and dot, and the frame under it. */
function paintPoint(t, live) {
  const d = detail, tc = $("inst-tc");
  if (!d || !tc || !d.smoothed.t.length) return;
  const dur = d.duration || 1, i = nearestIndex(d.smoothed.t, t);
  const kept = d.segments.filter((s) => s.status === "kept");
  const seg = d.segments.find((s) => t >= s.start && t < s.end);
  $("inst-held").textContent = live ? "At" : "Held at";
  tc.textContent = fmtClock(t);
  if (seg) {
    const [tone, shape, word] = SEG_STATUS[seg.status];
    $("inst-pill").innerHTML = pill(tone, shape, seg.status === "kept" ? `Kept · Select ${kept.indexOf(seg) + 1}` : word);
    $("inst-why").textContent = whyText(seg, d.selection, kept.length) + repeatNote(seg);
  } else {
    $("inst-pill").innerHTML = pill("grey", "clock", "Not measured yet");
    $("inst-why").textContent = "This part of the clip has not been scored yet.";
  }
  for (const key of ["focus", "exposure", "stability"]) {
    const v = d.smoothed[key][i], low = v < d.selection[key + "_min"];
    const out = document.querySelector(`[data-lane-value="${key}"]`);
    if (out) { out.textContent = String(pct(v)); out.classList.toggle("low", low); }
    const svg = document.querySelector(`svg[data-graph="${key}"]`);
    const dot = svg?.querySelector(".dot");
    if (dot) {
      const sw = svg.viewBox.baseVal.width;
      dot.setAttribute("cx", ((t / dur) * sw).toFixed(1));
      dot.setAttribute("cy", (6 + (1 - Math.max(0, Math.min(1, v))) * 80).toFixed(1));
      dot.setAttribute("stroke", low ? "#B3261E" : "#2C4A6E");
      dot.setAttribute("visibility", "visible");
    }
  }
  for (const cv of document.querySelectorAll("#frame-strip canvas")) cv.classList.toggle("cur", t >= +cv.dataset.a && t < +cv.dataset.b);
}

// Small frames across each clip, read once per clip and kept for the last few clips opened.
const stripCache = new Map();
function thumbOf(f) {
  const src = document.createElement("canvas");
  src.width = f.w; src.height = f.h;
  src.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(f.rgba), f.w, f.h), 0, 0);
  const c = document.createElement("canvas");
  c.width = 192; c.height = 108;
  const r = Math.max(192 / f.w, 108 / f.h), w = f.w * r, h = f.h * r;
  c.getContext("2d").drawImage(src, (192 - w) / 2, (108 - h) / 2, w, h);
  return c;
}
function paintStrip() {
  const entry = stripCache.get(view.clipId), box = $("frame-strip");
  if (!box || !entry) return;
  // A clip whose frames cannot be read shows no strip rather than a row of blanks.
  box.closest(".lane-row").hidden = entry.failed && !entry.frames.some(Boolean);
  box.querySelectorAll("canvas").forEach((cv, j) => { if (entry.frames[j]) cv.getContext("2d").drawImage(entry.frames[j], 0, 0, cv.width, cv.height); });
}
async function loadStrip() {
  const d = detail, id = view.clipId;
  if (!d || !d.raw.t.length || !$("frame-strip")) return;
  let entry = stripCache.get(id);
  if (!entry || entry.duration !== d.duration) {
    entry = { frames: new Array(STRIP_FRAMES).fill(null), failed: false, busy: false, duration: d.duration };
    stripCache.set(id, entry);
    if (stripCache.size > 6) stripCache.delete(stripCache.keys().next().value);
  }
  paintStrip();
  if (entry.busy || entry.failed) return;
  entry.busy = true;
  try {
    for (let j = 0; j < STRIP_FRAMES; j++) {
      if (entry.frames[j]) continue;
      if (view.mode !== "detail" || view.clipId !== id) return;
      try { entry.frames[j] = thumbOf(parseStill(await invoke("clip_frame", { id, t: ((j + 0.5) / STRIP_FRAMES) * d.duration }))); }
      catch (_) { entry.failed = true; break; }
      if (view.clipId === id) paintStrip();
    }
  } finally {
    entry.busy = false;
  }
  if (view.clipId === id) paintStrip();
}

/** How the clip's middle tones sit in stops over its length, for log footage. */
function stopsCard(d) {
  const ks = (d.raw.key_stops || []).filter((v) => v != null && isFinite(v));
  if (!ks.length) return "";
  const LO = -7, HI = 5.5, STEP = 0.5, n = Math.round((HI - LO) / STEP);
  const bins = new Array(n).fill(0);
  for (const v of ks) bins[Math.max(0, Math.min(n - 1, Math.floor((v - LO) / STEP)))]++;
  const mx = Math.max(...bins) || 1, W = 1000, H = 120, bw = W / n;
  const at = (v) => ((v - LO) / (HI - LO)) * W;
  const cl = d.raw.clipped || [];
  const clipped = cl.length ? cl.reduce((a, b) => a + b, 0) / cl.length : 0;
  const mean = ks.reduce((a, b) => a + b, 0) / ks.length;
  let bars = "";
  bins.forEach((c, k) => {
    if (!c) return;
    const h = Math.max(2, (c / mx) * (H - 6)), lo = LO + k * STEP;
    bars += `<rect x="${(k * bw + 1).toFixed(1)}" y="${(H - h).toFixed(1)}" width="${(bw - 2).toFixed(1)}" height="${h.toFixed(1)}" rx="2" fill="${lo >= 4.5 || lo < -5 ? "#B3261E" : "#2C4A6E"}"/>`;
  });
  const zones = `<rect x="0" y="0" width="${at(-5)}" height="${H}" fill="#FCEDEC"/><rect x="${at(-5)}" y="0" width="${at(-1.5) - at(-5)}" height="${H}" fill="#FDF2E2"/><rect x="${at(4.5)}" y="0" width="${W - at(4.5)}" height="${H}" fill="#FCEDEC"/><line x1="${at(0)}" x2="${at(0)}" y1="0" y2="${H}" stroke="#C9A227" stroke-width="2"/>`;
  const ticks = [-6, -4, -2, 0, 2, 4].map((v) => `<span class="${v === 0 ? "zero" : ""}" style="left:${((at(v) / W) * 100).toFixed(2)}%">${v > 0 ? "+" + v : v < 0 ? "−" + -v : "0"}</span>`).join("");
  return `<div class="stops-card">
    <div class="stops-head"><h3 class="eyebrow eyebrow-dark">How bright, in stops</h3><span class="stops-note num">On average ${stopsWords(mean)}${clipped > 0.0005 ? ` · ${sharePct(clipped)} clipped` : ""}</span></div>
    <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" class="stops-svg" role="img" aria-label="How often the middle tones sit at each level, in stops from middle grey">${zones}${bars}</svg>
    <div class="stops-axis num">${ticks}</div>
    <p class="hint">Each bar counts the scored moments whose middle tones sit at that level. Gold is middle grey; red is more than 5 stops under, or near the clip point.</p>
  </div>`;
}

function profileRowHtml(d) {
  if (!d.profile || !state?.profiles) return "";
  const p = d.profile;
  const opts = [`<option value="auto">Automatic</option>`].concat(state.profiles.map((o) => `<option value="${o.id}">${esc(o.label)}</option>`)).join("");
  let note = "";
  if (p.profile === "d_log_m") note = " DJI has not published the D-Log M curve, so D-Log's curve stands in and readings in stops are approximate.";
  else if (p.profile === "generic_log") note = " An average of four published log curves stands in, so readings in stops are approximate.";
  const how = p.profile === "standard"
    ? "Judged on how the picture looks: blown highlights, crushed shadows and flat contrast count against it."
    : "Judged in stops through the camera's curve: clipped highlights and underexposure count against it, the flat log look does not.";
  return `<div class="profile-row">
    <div><span class="profile-name">Exposure judged as ${esc(profileLabel(p.profile))}.</span> <span class="muted">${esc(p.explanation)}${esc(note)}</span><div class="muted">${esc(how)}</div></div>
    <label class="inline-select"><span>Profile for this clip</span><select id="clip-profile" class="input input-small" data-value="${d.chosen_profile || "auto"}">${opts}</select></label>
  </div>`;
}

function cameraRowHtml(d) {
  const n = d.camera;
  let html = "";
  if (n) {
    const bits = [];
    if (n.camera) bits.push(esc(n.camera));
    if (n.lens) bits.push(esc(n.lens));
    if (n.iso) bits.push(`ISO <span class="num">${Math.round(n.iso)}</span>`);
    if (n.shutter) bits.push(`Shutter <span class="num">1/${Math.round(1 / n.shutter)}</span>`);
    if (n.aperture) bits.push(`<span class="num">f/${n.aperture.toFixed(1)}</span>`);
    if (n.max_height != null) bits.push(`Up to <span class="num">${Math.round(n.max_height)}</span> m above take-off`);
    if (n.latitude != null && n.longitude != null) bits.push(`<span class="num selectable">${n.latitude.toFixed(5)}, ${n.longitude.toFixed(5)}</span>`);
    if (bits.length) html += `<div class="meta">${bits.map((b) => `<span>${b}</span>`).join("")}<span class="muted">From ${esc(n.source)}</span></div>`;
  }
  if (d.shutter_warning) html += `<div class="note">${pill("amber", "triangle", "Fast shutter")}<span>${esc(d.shutter_warning)}</span></div>`;
  return html;
}

function attachHover(w, d) {
  const box = $("main-panel").querySelector(".graphs");
  if (!box) return;
  const svgs = [...box.querySelectorAll("svg[data-graph]")];
  const readout = $("readout");
  box.addEventListener("mousemove", (e) => {
    const r = svgs[0].getBoundingClientRect();
    const px = Math.max(0, Math.min(w, ((e.clientX - r.left) / r.width) * w));
    const t = (px / w) * (d.duration || 1);
    for (const s of svgs) { const c = s.querySelector(".cursor"); c.setAttribute("x1", px); c.setAttribute("x2", px); c.setAttribute("visibility", "visible"); }
    const ts = d.smoothed.t;
    if (!ts.length) return;
    let i = 0, best = Infinity;
    for (let k = 0; k < ts.length; k++) { const dd = Math.abs(ts[k] - t); if (dd < best) { best = dd; i = k; } }
    const seg = d.segments.find((s) => t >= s.start && t < s.end);
    const word = seg ? SEG_STATUS[seg.status][2] : "Not measured yet";
    const ks = d.raw.key_stops || [], cl = d.raw.clipped || [];
    const logBits = ks.length ? ` · Brightness <span class="num">${stopsWords(ks[i])}</span>${cl[i] > 0.005 ? ` · <span class="num">${(cl[i] * 100).toFixed(cl[i] < 0.1 ? 1 : 0)}%</span> clipped` : ""}` : "";
    if (viewer.mode !== "off") showFrameAt(t);
    paintPoint(t, true);
    readout.innerHTML = `At <span class="num">${fmtClock(t)}</span> · Focus <span class="num">${pct(d.smoothed.focus[i])}</span> · Exposure <span class="num">${pct(d.smoothed.exposure[i])}</span>${logBits} · Stability <span class="num">${pct(d.smoothed.stability[i])}</span> · ${esc(word)}`;
  });
  const timeAt = (e) => {
    const r = svgs[0].getBoundingClientRect();
    return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * (d.duration || 1);
  };
  box.addEventListener("click", (e) => pinAt(timeAt(e)));
  box.addEventListener("mouseleave", () => {
    for (const s of svgs) s.querySelector(".cursor").setAttribute("visibility", "hidden");
    showFrameAt(viewer.pin);
    paintPoint(viewer.pin, false);
  });
}

async function openDetail(id) {
  view = { mode: "detail", clipId: id };
  detail = null; detailError = null;
  viewer.clipId = null;
  renderMain();
  await refreshDetail();
}

async function refreshDetail() {
  if (view.mode !== "detail") return;
  try {
    detail = await invoke("clip_detail", { id: view.clipId });
    detailError = null;
  } catch (e) {
    detailError = String(e);
  }
  if (detail && viewer.clipId !== view.clipId) resetViewer(detail);
  renderDetail();
}

/* ---------- frame viewer ---------- */

/*
 * One still of the clip, read by ffmpeg at the moment under the pointer, or
 * at the moment last clicked ("held"). Overlays are drawn here from the
 * still's pixels: false colour reads brightness in stops from middle grey
 * through the clip's picture profile (detail.tone), the same way the
 * exposure score does.
 */
const VIEW_MODES = [["picture", "Picture"], ["false_color", "False color"], ["peaking", "Focus peaking"], ["clipping", "Clipping"]];
const FALSE_BANDS = [
  // [lowest stops, rgb, words]; checked top down after the clip point.
  [2.5, [240, 146, 36], "2.5 or more stops over"],
  [1, [236, 214, 120], "1 to 2.5 over"],
  [0.5, null, "Bright middle tones"],
  [-0.5, [64, 170, 98], "Middle grey"],
  [-2.5, null, "Darker middle tones"],
  [-5, [58, 100, 206], "2.5 to 5 under"],
  [-Infinity, [116, 52, 150], "More than 5 under"],
];
const CLIP_RGB = [222, 52, 40];
const PEAK_RGB = [255, 214, 10];

let viewer = { clipId: null, pin: 0, want: null, frame: null, loading: false, error: null, mode: "picture" };
try { viewer.mode = localStorage.getItem("viewer-mode") || "picture"; } catch (_) { /* a remembered view is a convenience */ }
const frameCache = new Map();
const FRAME_CACHE_SIZE = 24;
const frameKey = (id, t) => `${id}@${(Math.round(t * 10) / 10).toFixed(1)}`;

function resetViewer(d) {
  const kept = d.segments.find((s) => s.status === "kept");
  const pin = kept ? (kept.start + kept.end) / 2 : (d.duration || 0) * 0.25;
  viewer = { clipId: view.clipId, pin, want: null, frame: null, loading: false, error: null, mode: viewer.mode };
  showFrameAt(pin);
}

function parseStill(buf) {
  const u8 = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf);
  const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
  const w = dv.getUint32(0, true), h = dv.getUint32(4, true);
  return { w, h, rgba: new Uint8ClampedArray(u8.buffer, u8.byteOffset + 8, w * h * 4), modes: {} };
}

function showFrameAt(t) {
  if (view.mode !== "detail" || !detail) return;
  viewer.want = t;
  const hit = frameCache.get(frameKey(view.clipId, t));
  if (hit) { viewer.frame = hit; viewer.frameT = t; viewer.error = null; paintViewer(); return; }
  fetchFrame();
}

async function fetchFrame() {
  if (viewer.loading) return; // the newest wanted moment is read when this one returns
  const id = view.clipId, t = viewer.want;
  viewer.loading = true;
  paintViewerState();
  try {
    const f = parseStill(await invoke("clip_frame", { id, t }));
    frameCache.set(frameKey(id, t), f);
    if (frameCache.size > FRAME_CACHE_SIZE) frameCache.delete(frameCache.keys().next().value);
    if (view.clipId === id) { viewer.frame = f; viewer.frameT = t; viewer.error = null; }
  } catch (e) {
    if (view.clipId === id) viewer.error = String(e);
  } finally {
    viewer.loading = false;
  }
  if (view.mode === "detail" && view.clipId === id && viewer.want !== t) showFrameAt(viewer.want);
  else paintViewer();
}

function pinAt(t) {
  viewer.pin = Math.max(0, Math.min(detail?.duration || 0, t));
  for (const line of document.querySelectorAll(".graphs line.pin")) {
    const svg = line.ownerSVGElement, w = svg.viewBox.baseVal.width;
    const x = ((viewer.pin / (detail.duration || 1)) * w).toFixed(1);
    line.setAttribute("x1", x); line.setAttribute("x2", x);
  }
  for (const tr of document.querySelectorAll(".seg-table tr[data-seg]")) {
    const [a, b] = tr.dataset.seg.split(":").map(Number);
    tr.classList.toggle("pinned", viewer.pin >= a && viewer.pin < b);
  }
  const calls = $("calls");
  if (calls) { const h = callsHtml(); calls.innerHTML = h; calls.dataset.html = h; }
  showFrameAt(viewer.pin);
  paintPoint(viewer.pin, false);
}

function stepSelect(dir) {
  if (!detail) return;
  const kept = detail.segments.filter((s) => s.status === "kept");
  const mids = kept.map((s) => (s.start + s.end) / 2);
  const next = dir > 0 ? mids.find((m) => m > viewer.pin + 0.05) : [...mids].reverse().find((m) => m < viewer.pin - 0.05);
  if (next != null) pinAt(next);
}

function viewerHtml(d) {
  const modes = VIEW_MODES.map(([id, word]) => `<button class="chip" role="radio" aria-checked="${viewer.mode === id}" data-viewmode="${id}">${word}</button>`).join("");
  const kept = d.segments.filter((s) => s.status === "kept").length;
  const steps = kept ? `<div class="viewer-steps"><button class="btn btn-secondary btn-small" data-act="prev-select" title="Previous select">${icon("left")}Previous select</button><button class="btn btn-secondary btn-small" data-act="next-select" title="Next select">Next select${icon("right")}</button></div>` : "";
  return `<div class="viewer">
    <div class="viewer-bar"><div class="chips" role="radiogroup" aria-label="How the frame is shown">${modes}</div>${steps}</div>
    <div class="viewer-well"><canvas id="viewer-canvas" aria-label="The clip at the held moment"></canvas><div class="viewer-state" id="viewer-state"></div></div>
    <div class="viewer-foot"><div class="viewer-time" id="viewer-time"></div><div class="viewer-legend" id="viewer-legend"></div></div>
    <div class="calls" id="calls"></div>
  </div>`;
}

/** The stretch a call made now would cover: the hand call at the held moment, the stretch under it, or for a long dropped stretch, a select-length piece around it. */
function callTarget() {
  if (!detail) return null;
  const t = viewer.pin;
  const mark = (detail.marks || []).find((m) => t >= m.start && t < m.end);
  if (mark) return { start: mark.start, end: mark.end, mark };
  const seg = detail.segments.find((s) => t >= s.start && t < s.end);
  if (!seg) return null;
  if (seg.status === "below_threshold" || seg.end - seg.start > detail.selection.max_length * 2) {
    const len = Math.min(seg.end - seg.start, detail.selection.max_length);
    let a = Math.max(seg.start, t - len / 2);
    const b = Math.min(seg.end, a + len);
    a = Math.max(seg.start, b - len);
    return { start: a, end: b, seg };
  }
  return { start: seg.start, end: seg.end, seg };
}

function callsHtml() {
  const tg = callTarget();
  if (!tg) return "";
  const m = tg.mark;
  const isKeep = m?.call === "keep" || (!m && tg.seg?.manual);
  const isDrop = m?.call === "drop";
  const stars = m?.stars || 0;
  const starBtns = [1, 2, 3, 4, 5].map((n) => `<button class="star-btn" data-act="star" data-stars="${n}" aria-pressed="${stars >= n}" title="${n} ${n === 1 ? "star" : "stars"} (key ${n})">${icon("star", "icon icon-star")}</button>`).join("");
  const what = m ? (isKeep ? `Kept by hand${stars ? `, ${starText(stars)}` : ""}` : "Dropped by hand") : "Your call on";
  return `<div class="calls-row">
    <span class="calls-what">${esc(what)} <span class="num">${fmtClock(tg.start)} – ${fmtClock(tg.end)}</span></span>
    <button class="btn btn-secondary btn-small" data-act="keep" aria-pressed="${isKeep}" title="Keep this stretch whatever the settings say (K)">${icon("tick")}Keep this</button>
    <button class="btn btn-secondary btn-small" data-act="drop" aria-pressed="${isDrop}" title="Leave this stretch out (X)">${icon("cross")}Drop this</button>
    <span class="star-row" role="group" aria-label="Stars">${starBtns}</span>
    ${m ? `<button class="link" data-act="clear-call" title="Go back to what the settings choose (0)">Go back to the settings' choice</button>` : ""}
  </div>`;
}

async function makeCall(call, stars) {
  const tg = callTarget();
  if (!tg || !detail) return;
  try {
    const wasSelect = tg.seg ? tg.seg.status === "kept" : tg.mark?.call === "keep";
    state = await invoke("mark_stretch", { id: view.clipId, start: tg.start, end: tg.end, call, stars: stars || 0 });
    renderAllButSide();
    syncSide();
    await refreshDetail();
    if (call === "drop" && !wasSelect) toast(`Dropped <span class="num">${fmtClock(tg.start)} – ${fmtClock(tg.end)}</span>. It was not a select, so nothing changes now; it stays out if new settings would choose it.`);
    else if (call === "keep" && !wasSelect) toast(`Kept <span class="num">${fmtClock(tg.start)} – ${fmtClock(tg.end)}</span> by hand. It is now a select whatever its scores, and goes into the export.`);
  } catch (e) { showError("Your call was not saved", String(e)); }
}

function lumaOf(f) {
  if (f.luma) return f.luma;
  const n = f.w * f.h, y = new Uint8Array(n), p = f.rgba;
  for (let i = 0, j = 0; i < n; i++, j += 4) y[i] = (p[j] * 0.2126 + p[j + 1] * 0.7152 + p[j + 2] * 0.0722 + 0.5) | 0;
  f.luma = y;
  return y;
}

/** The still with the current overlay, plus what it measured. Cached per still and mode. */
function overlay(f, mode, tone) {
  const key = `${mode}:${tone.profile}:${tone.clip_luma}`;
  if (f.modes[key]) return f.modes[key];
  const n = f.w * f.h, src = f.rgba, out = new Uint8ClampedArray(src.length), Y = lumaOf(f);
  const set = (j, c) => { out[j] = c[0]; out[j + 1] = c[1]; out[j + 2] = c[2]; out[j + 3] = 255; };
  const grey = (j, v) => { out[j] = v; out[j + 1] = v; out[j + 2] = v; out[j + 3] = 255; };
  const stats = {};
  if (mode === "false_color") {
    const lut = new Array(256);
    for (let v = 0; v < 256; v++) {
      if (v >= tone.clip_luma) { lut[v] = CLIP_RGB; continue; }
      const s = tone.stops[v];
      const band = FALSE_BANDS.find((b) => s >= b[0]);
      lut[v] = band[1];
    }
    for (let i = 0, j = 0; i < n; i++, j += 4) { const c = lut[Y[i]]; if (c) set(j, c); else grey(j, (Y[i] * 0.8 + 18) | 0); }
  } else if (mode === "peaking") {
    const w = f.w, h = f.h, g = new Uint16Array(n);
    const hist = new Uint32Array(1021);
    for (let y = 1; y < h - 1; y++) {
      for (let x = 1; x < w - 1; x++) {
        const i = y * w + x;
        const v = Math.abs(Y[i + 1] - Y[i - 1]) + Math.abs(Y[i + w] - Y[i - w]);
        g[i] = v; hist[v]++;
      }
    }
    // Mark the strongest 6% of edges, but never weak ones: a soft picture shows few or none.
    let need = n * 0.06, thr = 1020;
    while (thr > 0 && need > 0) { need -= hist[thr]; thr--; }
    thr = Math.max(thr, 56);
    let peaks = 0;
    for (let i = 0, j = 0; i < n; i++, j += 4) {
      if (g[i] > thr) { set(j, PEAK_RGB); peaks++; } else grey(j, (Y[i] * 0.55 + 14) | 0);
    }
    stats.peaks = peaks / n;
  } else if (mode === "clipping") {
    const w = f.w, standard = tone.profile === "standard";
    let clipped = 0, crushed = 0;
    for (let i = 0, j = 0; i < n; i++, j += 4) {
      const v = Y[i];
      if (v >= tone.clip_luma) {
        const x = i % w, y = (i / w) | 0;
        grey(j, ((x + y) >> 3) & 1 ? 250 : 40);
        clipped++;
      } else if (standard ? v <= 4 : tone.stops[v] <= -5) {
        set(j, [58, 100, 206]);
        crushed++;
      } else {
        out[j] = src[j]; out[j + 1] = src[j + 1]; out[j + 2] = src[j + 2]; out[j + 3] = 255;
      }
    }
    stats.clipped = clipped / n;
    stats.crushed = crushed / n;
  } else {
    out.set(src);
  }
  const result = { img: new ImageData(out, f.w, f.h), stats };
  f.modes[key] = result;
  return result;
}

const sharePct = (v) => (v < 0.001 ? "under 0.1%" : v < 0.1 ? `${(v * 100).toFixed(1)}%` : `${Math.round(v * 100)}%`);

function legendHtml(mode, stats, tone) {
  const sw = (c) => `<span class="swatch" style="background:rgb(${c.join(",")})"></span>`;
  if (mode === "false_color") {
    const bands = [[CLIP_RGB, "Clipped"], ...FALSE_BANDS.filter((b) => b[1]).map((b) => [b[1], b[2]])];
    const how = tone.profile === "standard" ? "Stops from middle grey, as the picture looks." : `Stops from middle grey through the ${esc(profileLabel(tone.profile))} curve.`;
    return `${bands.map(([c, w]) => `<span class="legend-item">${sw(c)}${esc(w)}</span>`).join("")}<span class="legend-note">Grey areas sit in the middle tones. ${how}</span>`;
  }
  if (mode === "peaking") {
    return `<span class="legend-item">${sw(PEAK_RGB)}Sharpest edges</span><span class="legend-note">${stats.peaks < 0.004 ? "Almost nothing is marked, so nothing in this frame is crisp." : "Yellow shows where the picture is in focus. A soft frame shows few marks or none."}</span>`;
  }
  if (mode === "clipping") {
    const c = stats.clipped, k = stats.crushed;
    const said = c > 0.0005 || k > 0.0005
      ? `${c > 0.0005 ? `${sharePct(c)} of this frame is clipped and cannot be recovered in the grade.` : "Nothing is clipped."} ${k > 0.0005 ? `${sharePct(k)} is ${tone.profile === "standard" ? "crushed to black" : "more than 5 stops under, mostly noise"}.` : ""}`
      : "Nothing in this frame is clipped or crushed.";
    return `<span class="legend-item"><span class="swatch zebra"></span>Clipped</span><span class="legend-item">${sw([58, 100, 206])}${tone.profile === "standard" ? "Crushed to black" : "More than 5 under"}</span><span class="legend-note">${esc(said)}</span>`;
  }
  return `<span class="legend-note">The full picture as recorded${tone.profile !== "standard" ? ", before any grade, so log footage looks flat" : ""}.</span>`;
}

function paintViewerState() {
  const box = $("viewer-state");
  if (!box) return;
  const canvas = $("viewer-canvas");
  if (viewer.error) {
    canvas.hidden = true;
    box.className = "viewer-state is-error";
    box.innerHTML = `<div class="error-box error-small" role="alert">${icon("triangle")}<div><strong>This frame could not be shown</strong><p>${esc(viewer.error)}</p><p>Try another moment on the graphs. If every moment fails, check that the file opens in another player.</p></div></div>`;
    return;
  }
  canvas.hidden = !viewer.frame;
  if (viewer.loading && !viewer.frame) {
    box.className = "viewer-state is-loading";
    box.innerHTML = `<div class="loading-title"><span class="spinner" aria-hidden="true"></span>Reading the frame at <span class="num">${fmtClock(viewer.want || 0)}</span></div>`;
  } else if (viewer.loading) {
    box.className = "viewer-state is-busy";
    box.innerHTML = `<span class="spinner" aria-hidden="true"></span>`;
  } else if (!viewer.frame) {
    box.className = "viewer-state is-empty";
    box.innerHTML = `<div class="empty-small"><strong>No frame shown</strong>Point at a graph or a stretch below to see that moment.</div>`;
  } else {
    box.className = "viewer-state";
    box.innerHTML = "";
  }
}

function paintViewer() {
  const canvas = $("viewer-canvas");
  if (!canvas || !detail) return;
  paintViewerState();
  const f = viewer.frame;
  if (f && !viewer.error) {
    const { img, stats } = overlay(f, viewer.mode, detail.tone);
    if (canvas.width !== f.w || canvas.height !== f.h) { canvas.width = f.w; canvas.height = f.h; }
    canvas.getContext("2d").putImageData(img, 0, 0);
    $("viewer-legend").innerHTML = legendHtml(viewer.mode, stats, detail.tone);
  } else {
    $("viewer-legend").innerHTML = "";
  }
  const t = viewer.frameT ?? viewer.want ?? viewer.pin;
  const seg = detail.segments.find((s) => t >= s.start && t < s.end);
  const held = Math.abs(t - viewer.pin) < 0.05;
  const calls = $("calls");
  if (calls) { const h = callsHtml(); if (calls.dataset.html !== h) { calls.innerHTML = h; calls.dataset.html = h; } }
  $("viewer-time").innerHTML = `${held ? "Held at" : "At"} <span class="num">${fmtClock(t)}</span>${seg ? ` · ${esc(SEG_STATUS[seg.status][2])} · ${esc(CAMERA_WORDS[seg.camera] || "Still")}` : ""}`;
}

function setViewMode(mode) {
  viewer.mode = mode;
  try { localStorage.setItem("viewer-mode", mode); } catch (_) { /* a remembered view is a convenience */ }
  for (const b of document.querySelectorAll("[data-viewmode]")) b.setAttribute("aria-checked", String(b.dataset.viewmode === mode));
  paintViewer();
}

/* ---------- side panel ---------- */

const SLIDERS = {
  focus_min: ["s-focus", "o-focus"], exposure_min: ["s-exposure", "o-exposure"], stability_min: ["s-stability", "o-stability"],
  smoothing: ["s-smoothing", "o-smoothing"], min_length: ["s-min", "o-min"], max_length: ["s-max", "o-max"], max_per_clip: ["s-cap", "o-cap"],
  focus_weight: ["s-wfocus", "o-wfocus"], exposure_weight: ["s-wexposure", "o-wexposure"], stability_weight: ["s-wstability", "o-wstability"],
};
const isWeight = (key) => key.endsWith("_weight");

const DEFAULT_SELECTION = { focus_min: 0.5, exposure_min: 0.5, stability_min: 0.5, smoothing: 1, min_length: 2, max_length: 10, max_per_clip: 3, keep_moves_whole: true, focus_weight: 1, exposure_weight: 1, stability_weight: 1 };
/* Presets set the sliders only; the camera filter and repeated takes stay as they are. */
const PRESETS = [
  { id: "balanced", label: "Balanced", hint: "The starting settings. Suits most footage, up to three selects from each clip.", v: DEFAULT_SELECTION },
  { id: "long", label: "Long takes", hint: "For interviews, talks and locked-off shots: selects of 5 to 30 seconds, with focus and steadiness deciding which win. Selects start and end in a pause when someone talks.",
    v: { focus_min: 0.6, exposure_min: 0.5, stability_min: 0.65, smoothing: 1.5, min_length: 5, max_length: 30, max_per_clip: 2, keep_moves_whole: false, focus_weight: 1, exposure_weight: 0.6, stability_weight: 0.8 } },
  { id: "short", label: "Short clips", hint: "For reels and social edits: clean selects of 1.5 to 5 seconds, up to five from each clip.",
    v: { focus_min: 0.55, exposure_min: 0.55, stability_min: 0.55, smoothing: 0.5, min_length: 1.5, max_length: 5, max_per_clip: 5, keep_moves_whole: false, focus_weight: 1, exposure_weight: 1, stability_weight: 1 } },
  { id: "handheld", label: "Handheld and action", hint: "For run-and-gun, sport and walking shots: more shake is allowed, and sharpness decides which selects win.",
    v: { focus_min: 0.45, exposure_min: 0.45, stability_min: 0.3, smoothing: 1.5, min_length: 1.5, max_length: 8, max_per_clip: 4, keep_moves_whole: true, focus_weight: 1, exposure_weight: 0.7, stability_weight: 0.4 } },
  { id: "strict", label: "Only the cleanest", hint: "The single best moment of each clip, and only if it is clearly good. Expect some clips to have none.",
    v: { focus_min: 0.7, exposure_min: 0.65, stability_min: 0.7, smoothing: 1, min_length: 2.5, max_length: 10, max_per_clip: 1, keep_moves_whole: true, focus_weight: 1, exposure_weight: 1, stability_weight: 1 } },
];
const presetMatching = (sel) => PRESETS.find((p) => Object.entries(p.v).every(([k, v]) => (typeof v === "boolean" ? !!sel[k] === v : Math.abs((sel[k] ?? DEFAULT_SELECTION[k]) - v) < 1e-4)));

function sliderToValue(key, raw) {
  const v = Number(raw);
  if (isWeight(key)) return v / 100;
  if (key.endsWith("_min") && !key.startsWith("min")) return v / 100;
  if (key === "max_per_clip") return v >= 21 ? 0 : v;
  return v;
}
function valueToSlider(key, v) {
  if (isWeight(key)) return Math.round((v ?? 1) * 100);
  if (key.endsWith("_min") && !key.startsWith("min")) return Math.round(v * 100);
  if (key === "max_per_clip") return v === 0 ? 21 : v;
  return v;
}
function label(key, v) {
  if (isWeight(key)) return String(Math.round((v ?? 1) * 100));
  if (key.endsWith("_min") && !key.startsWith("min")) return String(Math.round(v * 100));
  if (key === "smoothing") return v === 0 ? "Off" : `${v} s`;
  if (key === "max_per_clip") return v === 0 ? "No limit" : String(v);
  return `${v} s`;
}
function setFill(input) {
  const p = ((input.value - input.min) / (input.max - input.min)) * 100;
  input.style.setProperty("--fill", `${p}%`);
}

let localSel = null;
function syncSide() {
  if (!state) return;
  if (!dragging) localSel = { ...state.selection };
  for (const [key, [sid, oid]] of Object.entries(SLIDERS)) {
    const input = $(sid);
    if (dragging !== sid) input.value = valueToSlider(key, localSel[key]);
    $(oid).textContent = label(key, localSel[key]);
    setFill(input);
  }
  $("keep-moves").checked = !!localSel.keep_moves_whole;
  renderPresets();
  renderFilters();
  renderExtraFilters();
  renderPlaces();
  renderReel();
  renderStillsStatus();
  const sr = $("sample-rate");
  if (document.activeElement !== sr) sr.value = String(state.sample_rate);

  const fp = $("fallback-profile");
  const popts = [`<option value="auto">Decide from each clip's tones</option>`].concat((state.profiles || []).map((o) => `<option value="${o.id}">${esc(o.label)}</option>`)).join("");
  if (fp.dataset.opts !== popts) { fp.innerHTML = popts; fp.dataset.opts = popts; }
  if (document.activeElement !== fp) fp.value = state.fallback_profile || "auto";
  $("profile-hint").innerHTML = profileHint();
  renderTaste();

  const ex = state.export;
  const rateSel = $("timeline-rate");
  const opts = [`<option value="auto">Most common in the footage (${esc(ex.auto_rate_label)})</option>`]
    .concat(ex.rates.map((r) => `<option value="${r.num}/${r.den}">${esc(r.label)}</option>`)).join("");
  if (rateSel.dataset.opts !== opts) { rateSel.innerHTML = opts; rateSel.dataset.opts = opts; }
  rateSel.value = ex.rate_is_auto ? "auto" : `${ex.timeline_rate.num}/${ex.timeline_rate.den}`;
  $("drop-frame-row").hidden = !ex.timeline_rate.drop_frame_capable;
  $("drop-frame").checked = ex.drop_frame;
  for (const r of document.querySelectorAll('input[name="layout"]')) r.checked = r.value === ex.layout;
  const hs = $("handles");
  if (document.activeElement !== hs) hs.value = String(ex.handles || 0);
  $("export-stills").checked = !!ex.stills;
  const nameInput = $("timeline-name");
  if (document.activeElement !== nameInput) nameInput.value = ex.timeline_name;

  const hasSelects = state.summary.selects_count > 0;
  $("btn-export").disabled = !hasSelects;
  $("export-hint").textContent = hasSelects
    ? `Writes four files side by side: .xml for Premiere Pro and DaVinci Resolve, .fcpxml for Final Cut Pro, an .edl, and a .csv spreadsheet. Timeline: ${ex.timeline_rate.label} fps${ex.timeline_rate.drop_frame_capable ? (ex.drop_frame ? " drop frame" : " non drop frame") : ""}, ${ex.resolution[0]}×${ex.resolution[1]}.`
    : "There are no selects to export yet. Add footage, or lower a threshold if every clip is analyzed.";
}

function renderPresets() {
  const on = presetMatching(localSel);
  const html = PRESETS.map((p) => `<button class="chip" aria-pressed="${on?.id === p.id}" data-preset="${p.id}">${esc(p.label)}</button>`).join("");
  const box = $("presets");
  if (box.dataset.html !== html) { box.innerHTML = html; box.dataset.html = html; }
  $("preset-hint").textContent = on ? on.hint : "Your own settings. Pick a preset to start again from one.";
}

function renderFilters() {
  const cam = localSel.camera || "any";
  for (const b of document.querySelectorAll("[data-camera]")) b.setAttribute("aria-checked", String(b.dataset.camera === cam));
  const a = state.aside || { camera: 0, repeats: 0, repeats_found: 0 };
  $("camera-hint").textContent = cam === "any"
    ? "Keep only stretches where the camera is still, or moving a certain way. Movement means sliding sideways or up and down, so a push-in or zoom reads as still."
    : a.camera
      ? `${plural(a.camera, "good stretch is", "good stretches are")} set aside because the camera does something else. Good stretches are cut where the camera starts or stops moving.`
      : `Every good stretch so far shows ${FILTER_WORDS[cam]}. Good stretches are cut where the camera starts or stops moving.`;
  const rep = $("set-aside-repeats");
  rep.checked = !!localSel.set_aside_repeats;
  $("repeats-hint").textContent = localSel.set_aside_repeats
    ? a.repeats
      ? `${plural(a.repeats, "repeated take is", "repeated takes are")} set aside. The best take of each shot stays, and each clip's next best stretch can take a freed place.`
      : "No select looks like a better one in another clip, so nothing is set aside."
    : a.repeats_found
      ? `${plural(a.repeats_found, "select looks", "selects look")} like a better select in another clip, such as a second take from the same spot. Tick this to keep only the best take.`
      : "Finds selects that look like a better select in another clip, such as a second take from the same spot, and keeps only the best.";
}

/** Selects kept by hand pass every filter; say so, or the counts look stuck. */
function handNote(a) {
  return a.hand_kept ? ` ${plural(a.hand_kept, "select")} you kept by hand ${a.hand_kept === 1 ? "stays" : "stay"} whatever the filter says.` : "";
}

function renderExtraFilters() {
  const a = state.aside || {};
  const light = localSel.light || "any";
  for (const b of document.querySelectorAll("[data-light]")) b.setAttribute("aria-checked", String(b.dataset.light === light));
  const known = state.light_known || 0, total = (state.clips || []).filter((c) => c.status !== "unreadable").length;
  const unknown = Math.max(0, total - known);
  $("light-hint").textContent = light === "any"
    ? known
      ? `Light is known for ${known} of ${plural(total, "clip")}, from the sun's height when and where each was recorded.${unknown ? ` ${plural(unknown, "clip")} ${unknown === 1 ? "has" : "have"} no time or place; add where you shot under Places and time.` : ""}`
      : "Keep only footage shot in a certain light. It needs each clip's recording time and place; phones and DJI drones record both, and you can add the place for other cameras under Places and time."
    : `${a.light ? `${plural(a.light, "good stretch is", "good stretches are")} set aside.` : "Nothing is set aside."} Clips with no known light are set aside too${unknown ? ` (${unknown} now)` : ""}.${handNote(a)}`;
  $("set-aside-sound").checked = !!localSel.set_aside_sound_problems;
  $("sound-hint").textContent = localSel.set_aside_sound_problems
    ? (a.sound ? `${plural(a.sound, "good stretch is", "good stretches are")} set aside for strong wind, clipped sound or a dropout; a light breeze is listed but not set aside.` : "No select has a sound problem, so nothing is set aside.") + handNote(a)
    : "Wind noise, clipped sound and dropouts are always listed. Tick this when you will use the sound, to keep only selects where it is clean.";
  $("snap-pauses").checked = localSel.snap_to_pauses !== false;
  const talk = state.talking_clips || 0;
  $("snap-hint").textContent = talk
    ? `Someone talks in ${plural(talk, "clip")}. With this on, a select there starts just before a sentence and ends just after one, moving up to 2 seconds to find a pause.`
    : "When someone talks, selects start just before a sentence and end just after one. No talking has been found yet.";
}

/** Familiar places for common time zones, so the list reads without knowing UTC offsets. */
const ZONE_PLACES = {
  "-600": "Hawaii", "-540": "Alaska", "-480": "Los Angeles in winter", "-420": "Los Angeles in summer, Denver in winter",
  "-360": "Denver in summer, Chicago in winter", "-300": "Chicago in summer, New York in winter", "-240": "New York in summer",
  "-180": "Rio de Janeiro", "0": "the clock is on UTC, London in winter", "60": "London in summer, Paris in winter",
  "120": "Paris in summer, Cape Town", "180": "Istanbul, Nairobi", "240": "Dubai", "330": "India", "420": "Bangkok",
  "480": "Singapore, Perth", "540": "Tokyo", "600": "Sydney in winter", "660": "Sydney in summer", "720": "Auckland in winter", "780": "Auckland in summer",
};

function renderPlaces() {
  const pl = state.places || {};
  const inp = $("place-input");
  if (document.activeElement !== inp) inp.value = pl.location ? `${pl.location[0]}, ${pl.location[1]}` : "";
  const zs = $("clock-zone");
  if (!zs.dataset.filled) {
    const opts = ['<option value="auto">Guess from the clips</option>'];
    for (let m = -12 * 60; m <= 14 * 60; m += 30) {
      if (m % 60 !== 0 && ![-570, -210, 210, 270, 330, 345, 390, 525, 570, 630].includes(m) && ![-150, 150, 330, 570, 630].includes(m)) continue;
      const h = Math.floor(Math.abs(m) / 60), mm = Math.abs(m) % 60;
      const where = ZONE_PLACES[m] ? ` · ${ZONE_PLACES[m]}` : "";
      opts.push(`<option value="${m}">UTC${m < 0 ? "−" : "+"}${h}${mm ? ":" + String(mm).padStart(2, "0") : ""}${where}</option>`);
    }
    zs.innerHTML = opts.join(""); zs.dataset.filled = "1";
  }
  if (document.activeElement !== zs) zs.value = pl.clock_offset_minutes == null ? "auto" : String(pl.clock_offset_minutes);
  $("places-hint").textContent = "Phones and DJI drones record where and when, and Sony cameras record the time in UTC. Other cameras often record only the time on their clock, without saying which time zone it was set to. Guess from the clips uses the zone other clips state, or else picks between UTC and the place's local time by which one puts bright pictures in daylight. If times or light look wrong, choose the zone the camera's clock was set to.";
}

async function pushPlaces() {
  const raw = $("place-input").value.trim();
  let location = null;
  if (raw) {
    const m = raw.match(/^\s*(-?\d+(?:\.\d+)?)\s*[,;\s]\s*(-?\d+(?:\.\d+)?)\s*$/);
    if (!m) { showError("That place was not understood", "Type a latitude and longitude separated by a comma, for example 38.7223, -9.1393. You can copy them from a map app. Nothing was changed."); return; }
    location = [Number(m[1]), Number(m[2])];
  }
  const z = $("clock-zone").value;
  try { state = await invoke("set_places", { location, clockOffsetMinutes: z === "auto" ? null : Number(z) }); renderAll(); if (view.mode === "detail") refreshDetail(); }
  catch (e) { showError("The place was not changed", String(e)); renderPlaces(); }
}

const PACES = [["auto", "About 2 seconds"], ["every_beat", "Every beat"], ["every_two_beats", "Every 2 beats"], ["every_bar", "Every bar"], ["every_two_bars", "Every 2 bars"]];
const SHAPES = [["landscape", "16:9"], ["vertical", "9:16"], ["square", "1:1"]];
const ORDERS = [["as_shot", "As shot"], ["best_first", "Best first"]];
const LENGTHS = [["", "Whole song"], ["15", "15 s"], ["30", "30 s"], ["60", "60 s"], ["90", "90 s"]];

function renderReel() {
  const box = $("reel-body");
  if (!box || !state) return;
  const r = state.reel;
  let html = "";
  if (r.reading) {
    html += `<p class="update-line"><span class="spinner" aria-hidden="true"></span>Finding the beats in the song</p>`;
  } else if (!r.music_name) {
    html += `<p class="hint">Pick a song and ${esc(appName())} cuts your best selects to its beat, with the song under them, as a timeline for your editor. Choose how often to cut and how long the reel runs.</p>`;
    html += `<button class="btn btn-secondary btn-block" data-act="choose-music">${icon("plus")}Choose a song</button>`;
  } else {
    const steady = r.beat_confidence != null && r.beat_confidence < 0.35 ? " The beat is not very steady, so some cuts may land a little off." : "";
    html += `<div class="fv-row"><div class="fv-text"><div class="fv-name">${esc(r.music_name)}</div><div class="fv-sub">${r.tempo ? `<span class="num">${r.tempo}</span> beats per minute · ` : ""}<span class="num">${fmtDur(r.music_seconds || 0)}</span>${esc(steady)}</div></div><button class="link" data-act="choose-music">Change</button></div>`;
    const chips = (list, cur, attr) => list.map(([id, word]) => `<button class="chip" role="radio" aria-checked="${String(cur) === id}" data-${attr}="${id}">${esc(word)}</button>`).join("");
    html += `<div class="field-label">Cut</div><div class="chips" role="radiogroup" aria-label="How often to cut">${chips(PACES, r.pace, "pace")}</div>`;
    html += `<div class="field-label">Length</div><div class="chips" role="radiogroup" aria-label="Reel length">${chips(LENGTHS, r.length == null ? "" : String(r.length), "reel-length")}</div>`;
    html += `<div class="field-label">Order</div><div class="chips" role="radiogroup" aria-label="Order">${chips(ORDERS, r.order, "order")}</div>`;
    html += `<div class="field-label">Shape</div><div class="chips" role="radiogroup" aria-label="Shape">${chips(SHAPES, r.shape, "shape")}</div>`;
    html += `<p class="hint">As shot keeps the order the moments happened. 9:16 and 1:1 are cut from the centre of the picture; check the framing in your editor.</p>`;
    const none = !state.summary.selects_count;
    html += `<button class="btn btn-secondary btn-block" data-act="preview-reel" ${none ? "disabled" : ""}>See the shot list</button>`;
    html += `<button class="btn btn-primary btn-block" data-act="export-reel" ${none ? "disabled" : ""}>Export reel timeline</button>`;
    if (none) html += `<p class="hint">There are no selects to cut yet.</p>`;
  }
  if (box.dataset.html !== html) { box.innerHTML = html; box.dataset.html = html; }
}

async function pushReel(patch) {
  const r = { ...state.reel, ...patch };
  try { state = await invoke("set_reel", { reel: { pace: r.pace, length: r.length, order: r.order, shape: r.shape } }); renderAll(); }
  catch (e) { showError("The reel settings were not changed", String(e)); }
}

async function chooseMusic() {
  try {
    const s = await invoke("choose_music");
    if (s) { state = s; renderAll(); toast(`${icon("tick")} Found the beat of ${esc(s.reel.music_name)}: ${s.reel.tempo || "no steady"} beats per minute.`); }
  } catch (e) { await refresh(); showError("The song could not be used", String(e)); }
}

async function previewReel() {
  let plan;
  try { plan = await invoke("plan_reel"); } catch (e) { showError("The reel could not be planned", String(e)); return; }
  const rows = plan.shots.map((s, i) => `<tr><td class="num tabular">${i + 1}</td><td class="num tabular">${fmtClock(s.at)}</td><td class="num tabular">${s.length.toFixed(1)} s</td><td>${esc(s.clip_name)}</td><td class="num tabular">${fmtClock(s.source_start)}</td><td>${esc(CAMERA_WORDS[s.camera] || "Still")}${s.stretched ? " · stretched" : ""}${s.manual ? " · kept by hand" : ""}</td></tr>`).join("");
  const notes = plan.notes.length ? `<p class="muted">${plan.notes.map(esc).join(" ")}</p>` : "";
  const go = await modal("Reel shot list", `<p><strong class="num">${plan.shots.length}</strong> shots from ${plural(plan.clips_used, "clip")}, <span class="num">${fmtDur(plan.length)}</span>, cutting every ${plural(plan.beats_per_cut, "beat")} at <span class="num">${plan.tempo}</span> beats per minute. Chosen from ${plural(plan.selects_available, "select")}.</p>${notes}
    <div class="modal-scroll"><table class="fit-table"><thead><tr><th class="num">Shot</th><th class="num">At</th><th class="num">Length</th><th>Clip</th><th class="num">From</th><th>Camera</th></tr></thead><tbody>${rows}</tbody></table></div>`,
    [{ label: "Keep adjusting", value: false }, { label: "Export reel timeline", value: true, primary: true }]);
  if (go) exportReel();
}

async function exportReel() {
  try {
    const r = await invoke("export_reel");
    if (!r) return;
    const warn = r.warnings.length ? `<p class="muted">${r.warnings.map(esc).join("<br>")}</p>` : "";
    await modal("Reel exported", `<p>A reel of ${plural(r.selects, "shot")} with the song under it, written to <span class="selectable">${esc(r.folder)}</span>:</p><ul>${r.files.map((f) => `<li class="selectable">${esc(f)}</li>`).join("")}</ul>
      <p class="muted">Import it the same way as the selects timeline. The shots carry no sound of their own, so you hear only the song.</p>${warn}`, [{ label: "Done", value: true, primary: true }]);
  } catch (e) { showError("The reel was not exported", String(e)); }
}

function renderStillsStatus() {
  const p = state.stills_progress;
  const box = $("stills-status");
  const html = p ? `<p class="update-line"><span class="spinner" aria-hidden="true"></span>Writing stills: <span class="num">${p[0]}</span> of <span class="num">${p[1] || "…"}</span></p><button class="btn btn-secondary btn-small" data-act="stop-stills">${icon("stop")}Stop writing stills</button>` : "";
  if (box.dataset.html !== html) { box.innerHTML = html; box.dataset.html = html; }
  $("btn-stills").disabled = !!p || !state.summary.selects_count;
}

function stillsSummary(st) {
  if (!st) return "";
  const failed = st.failed.length ? ` ${plural(st.failed.length, "still")} could not be written: ${st.failed.map(([n, w]) => `${n} (${w})`).join("; ")}.` : "";
  return `<p>${plural(st.written, "still")} and a contact sheet in <span class="selectable">${esc(st.folder)}</span>.${st.cancelled ? " Stopped before the rest were written; the ones already written are kept." : ""}${esc(failed)}</p><p class="muted">Open Contact sheet.html in a browser to see them all; print it to save a PDF.</p>`;
}

async function exportStills() {
  try {
    const r = await invoke("export_stills");
    if (!r) return;
    await modal("Stills saved", stillsSummary(r), [{ label: "Done", value: true, primary: true }]);
  } catch (e) { showError("The stills were not saved", String(e)); }
}

function profileHint() {
  const rows = (state?.clips || []).filter((c) => c.status !== "unreadable");
  const log = rows.filter((c) => c.profile_is_log);
  const base = "Sony and DJI cameras name their profile in a file beside each clip, and that comes first. Log is judged in stops through the camera's curve, so clipped highlights and underexposure count against a shot and the flat look does not.";
  if (!rows.length) return esc(base);
  const by = (src) => log.filter((c) => c.profile_source === src).length;
  const parts = [];
  if (by("camera")) parts.push(`${by("camera")} named by the camera`);
  if (by("project")) parts.push(`${by("project")} from the choice above`);
  if (by("picture")) parts.push(`${by("picture")} recognised from their tones`);
  if (by("chosen")) parts.push(`${by("chosen")} chosen by hand`);
  const std = rows.length - log.length;
  const line = log.length
    ? `<strong class="num">${log.length}</strong> of ${plural(rows.length, "clip")} judged as log (${parts.join(", ")}).${std ? ` ${plural(std, "clip")} judged as standard.` : ""}`
    : `Every clip is judged as standard footage.`;
  return `${line} ${esc(base)}`;
}

function renderTaste() {
  const box = $("taste-body");
  if (!box || !state) return;
  const fv = state.finished_videos || [];
  const tr = state.tracing;
  let html = "";
  if (!fv.length && !tr) {
    html += `<p class="hint">Add videos you already cut from footage in this project. ${esc(AppName())} finds the shots you used, shows how many these settings would keep, and can fit the sliders to your choices. The videos are only read, on this computer.</p>`;
  }
  for (const f of fv) {
    html += `<div class="fv-row"><div class="fv-text"><div class="fv-name" title="${esc(f.path)}">${esc(f.name)}</div>
      <div class="fv-sub"><span class="num">${f.traced}</span> of ${plural(f.shots, "shot")} found in these clips${f.stale ? " · more clips analyzed since" : ""}</div></div>
      <button class="link" data-act="remove-finished" data-path="${esc(f.path)}">Remove</button></div>`;
  }
  if (fv.some((f) => f.stale) && !tr) html += `<button class="link fv-retrace" data-act="retrace">Look for their shots in the new clips</button>`;
  if (tr) {
    html += `<p class="update-line"><span class="spinner" aria-hidden="true"></span>Reading ${esc(tr.name)}${tr.total > 1 ? ` (${tr.done + 1} of ${tr.total})` : ""}</p>
      <button class="btn btn-secondary btn-small" data-act="stop-tracing">${icon("stop")}Stop reading</button>`;
  }
  const t = state.taste?.report;
  if (t && t.used) {
    const dropped = t.used - t.kept;
    const reasons = t.misses.map(([, n], i) => `${n} ${state.taste.miss_words[i]}`).join("; ");
    html += `<p class="taste-line"><strong class="num">${t.kept} of ${t.used}</strong> shots your edits used are kept by these settings, from ${plural(t.clips_used, "clip")}.</p>`;
    if (dropped) html += `<p class="hint">${plural(dropped, "used shot")} would be left out: ${esc(reasons)}.</p>`;
  } else if (fv.length && !tr) {
    html += `<p class="hint">None of these videos' shots were found in the clips analyzed so far. They may come from footage that is not in this project, or from clips still waiting to be analyzed.</p>`;
  }
  if (t && (t.hand || t.dropped)) {
    const bits = [];
    if (t.hand) bits.push(`the settings alone would keep <span class="num">${t.hand_kept} of ${t.hand}</span> stretches you kept by hand`);
    if (t.dropped) bits.push(`<span class="num">${t.dropped_kept} of ${t.dropped}</span> you dropped`);
    html += `<p class="hint">Your calls by hand count too: ${bits.join(", and ")}.</p>`;
  }
  if (t && (t.used || t.hand)) {
    html += `<button class="btn btn-primary btn-block" data-act="fit" ${tr ? "disabled" : ""}>Fit settings to my edits</button>`;
  } else if (t && t.dropped && !fv.length) {
    html += `<p class="hint">Keep a few stretches you like by hand (K) and the sliders can be fitted to your calls.</p>`;
  }
  html += `<button class="btn btn-secondary btn-block fv-add" data-act="add-finished" ${tr ? "disabled" : ""}>${icon("film")}Add finished videos</button>`;
  if (box.dataset.html !== html) { box.innerHTML = html; box.dataset.html = html; }
}

async function addFinished() {
  try {
    const r = await invoke("add_finished_videos");
    if (!r) return;
    await refresh();
    const bits = [];
    if (r.added.length) bits.push(`Read ${plural(r.added.length, "finished video")}`);
    if (r.cancelled) bits.push("Stopped before the rest were read; nothing already read was lost");
    if (r.failed.length) {
      await showError("Some finished videos could not be read", r.failed.map(([n, why]) => `${n}: ${why}`).join(" ") + " The others were read normally.");
    }
    if (bits.length) toast(`${icon("tick")} ${esc(bits.join(". "))}.`);
  } catch (e) { showError("The finished videos were not added", String(e)); }
}

function settingRows(a, b) {
  const rows = [
    ["Focus at least", "focus_min", (v) => pct(v)],
    ["Exposure at least", "exposure_min", (v) => pct(v)],
    ["Stability at least", "stability_min", (v) => pct(v)],
    ["Smoothing", "smoothing", (v) => (v === 0 ? "Off" : `${v} s`)],
    ["Shortest select", "min_length", (v) => `${v} s`],
    ["Selects per clip", "max_per_clip", (v) => (v === 0 ? "No limit" : String(v))],
  ];
  return rows.map(([name, k, f]) => {
    const changed = a[k] !== b[k];
    return `<tr class="${changed ? "changed" : ""}"><td>${name}</td><td class="num tabular">${f(a[k])}</td><td class="num tabular">${changed ? "<strong>" + f(b[k]) + "</strong>" : f(b[k])}</td></tr>`;
  }).join("");
}

async function fitToEdits() {
  toast(`<span class="spinner" aria-hidden="true"></span> Trying settings against your edits. This takes a few seconds.`);
  let f;
  try { f = await invoke("fit_to_finished_videos"); }
  catch (e) { $("toast").hidden = true; showError("The settings could not be fitted", String(e)); return; }
  $("toast").hidden = true;
  const b = f.before, a = f.after;
  const same = ["focus_min", "exposure_min", "stability_min", "smoothing", "min_length", "max_per_clip"].every((k) => f.settings[k] === state.selection[k]);
  // In a fit, used and kept count stretches kept by hand as well; split them out.
  const vb = { used: b.used - b.hand, kept: b.kept - b.hand_kept }, va = { used: a.used - a.hand, kept: a.kept - a.hand_kept };
  const nowLine = [];
  if (vb.used) nowLine.push(`<strong class="num">${vb.kept} of ${vb.used}</strong> shots your edits used`);
  if (b.hand) nowLine.push(`<strong class="num">${b.hand_kept} of ${b.hand}</strong> stretches you kept by hand`);
  if (same) {
    await modal("Your settings already fit your edits", `<p>No nearby settings keep more of what you chose without making the selects much longer. They keep ${nowLine.join(" and ")} now.</p>`, [{ label: "Done", value: true, primary: true }]);
    return;
  }
  let held;
  if (f.held_out_kept_share != null) {
    held = `<p>Checked on each finished video with settings learned from the others only: they kept <strong class="num">${Math.round(f.held_out_kept_share * 100)}%</strong> of that video's shots, against <span class="num">${Math.round(f.held_out_before_share * 100)}%</span> with your current settings.</p>`;
  } else if (b.videos) {
    held = `<p class="muted">With one finished video, these settings could be tuned to that one edit. Add another and the fit is also checked on a video it has not seen.</p>`;
  } else {
    held = `<p class="muted">Learned from your calls by hand only, so with a few calls the fit follows them closely. Stretches you kept by hand stay kept whatever the sliders say; this only changes what the settings choose on their own.</p>`;
  }
  const lines = [];
  if (vb.used) lines.push(`Shots your edits used that are kept: <strong class="num">${vb.kept} of ${vb.used}</strong> now, <strong class="num">${va.kept} of ${va.used}</strong> with these settings.`);
  if (b.hand) lines.push(`Stretches you kept by hand that the settings alone would keep: <strong class="num">${b.hand_kept} of ${b.hand}</strong> now, <strong class="num">${a.hand_kept} of ${a.hand}</strong> with these settings.`);
  if (b.dropped) lines.push(`Stretches you dropped that the settings would still choose: <span class="num">${b.dropped_kept} of ${b.dropped}</span> now, <span class="num">${a.dropped_kept} of ${a.dropped}</span> with these settings.`);
  const body = `<p>${lines.join(" ")} Selects in total: <span class="num">${fmtDur(b.selects_seconds)}</span> now, <span class="num">${fmtDur(a.selects_seconds)}</span> with these settings.</p>
    <table class="fit-table"><thead><tr><th>Setting</th><th class="num">Now</th><th class="num">Fitted</th></tr></thead><tbody>${settingRows(state.selection, f.settings)}</tbody></table>
    ${held}<p class="muted">Only the sliders move. You can change them by hand afterwards, and nothing is analyzed again.</p>`;
  const go = await modal("Settings fitted to your edits", body, [
    { label: "Keep my settings", value: false },
    { label: "Use these settings", value: true, primary: true },
  ]);
  if (go) { localSel = { ...state.selection, ...f.settings }; pushSelection(); }
}

let selInFlight = false, selQueued = false;
async function pushSelection() {
  if (selInFlight) { selQueued = true; return; }
  selInFlight = true; selQueued = false;
  try {
    state = await invoke("set_selection", { selection: localSel });
    renderAllButSide();
    syncSide();
    if (view.mode === "detail") refreshDetail();
  } catch (e) {
    showError("The selects could not be updated", String(e));
  } finally {
    selInFlight = false;
    if (selQueued) pushSelection();
  }
}

function wireSliders() {
  for (const [key, [sid, oid]] of Object.entries(SLIDERS)) {
    const input = $(sid);
    input.addEventListener("pointerdown", () => { dragging = sid; });
    input.addEventListener("input", () => {
      if (!localSel) return;
      dragging = dragging || sid;
      localSel[key] = sliderToValue(key, input.value);
      // Keep shortest at or below longest.
      if (key === "min_length" && localSel.min_length > localSel.max_length) { localSel.max_length = Math.ceil(localSel.min_length); $("s-max").value = localSel.max_length; }
      if (key === "max_length" && localSel.max_length < localSel.min_length) { localSel.min_length = localSel.max_length; $("s-min").value = localSel.min_length; }
      $(oid).textContent = label(key, localSel[key]);
      setFill(input);
      pushSelection();
    });
    const done = () => { if (dragging === sid) { dragging = null; syncSide(); } };
    input.addEventListener("change", done);
    input.addEventListener("pointerup", done);
    input.addEventListener("blur", done);
  }
  $("keep-moves").addEventListener("change", (e) => {
    if (!localSel) return;
    localSel.keep_moves_whole = e.target.checked;
    pushSelection();
  });
  $("btn-reset").addEventListener("click", () => {
    localSel = { ...localSel, ...DEFAULT_SELECTION, camera: "any", set_aside_repeats: false, light: "any", set_aside_sound_problems: false, snap_to_pauses: true };
    pushSelection();
  });
  $("set-aside-repeats").addEventListener("change", (e) => {
    if (!localSel) return;
    localSel.set_aside_repeats = e.target.checked;
    pushSelection();
  });
}

function currentExport() {
  const v = $("timeline-rate").value;
  const rate = v === "auto" ? null : { num: Number(v.split("/")[0]), den: Number(v.split("/")[1]) };
  const layout = document.querySelector('input[name="layout"]:checked')?.value || "full_clips_below";
  return { rate, drop_frame: $("drop-frame-row").hidden ? null : $("drop-frame").checked, layout, timeline_name: $("timeline-name").value, handles: Number($("handles").value), stills: $("export-stills").checked };
}
async function pushExport(resetDrop) {
  const ex = currentExport();
  if (resetDrop) ex.drop_frame = null;
  try { state = await invoke("set_export", { export: ex }); renderAll(); }
  catch (e) { showError("The export settings could not be changed", String(e)); }
}

/* ---------- modal and toast ---------- */

function modal(title, bodyHtml, actions) {
  return new Promise((resolve) => {
    $("modal-title").textContent = title;
    $("modal-body").innerHTML = bodyHtml;
    const box = $("modal-actions");
    box.innerHTML = "";
    for (const a of actions) {
      const b = document.createElement("button");
      b.className = `btn ${a.primary ? "btn-primary" : "btn-secondary"}`;
      b.textContent = a.label;
      b.addEventListener("click", () => { $("modal").hidden = true; resolve(a.value); });
      box.appendChild(b);
    }
    $("modal").hidden = false;
    box.querySelector(".btn-primary")?.focus();
  });
}
function showError(title, message) {
  return modal(title, `<div class="error-box" role="alert" style="max-width:none">${icon("triangle")}<div><p>${esc(message)}</p></div></div>`, [{ label: "Close this message", value: true, primary: true }]);
}
let toastTimer = null;
function toast(html) {
  const t = $("toast");
  t.innerHTML = html;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, 7000);
}

/* ---------- actions ---------- */

async function addFiles(folder) {
  try { await invoke(folder ? "pick_folder" : "pick_files"); }
  catch (e) { showError("The footage could not be added", String(e)); }
}

async function confirmDiscard(verb) {
  if (!state?.dirty) return "go";
  return modal("Save your changes first?", `<p>This project has changes that are not saved. If you ${verb} without saving, the scores and settings since the last save are lost. The footage itself is never changed.</p>`, [
    { label: "Keep working", value: "cancel" },
    { label: `${verb[0].toUpperCase()}${verb.slice(1)} without saving`, value: "go" },
    { label: "Save first", value: "save", primary: true },
  ]);
}

async function saveProject(saveAs) {
  try {
    const s = await invoke("save_project", { saveAs: !!saveAs });
    if (s) { state = s; renderAll(); toast(`${icon("tick")} Saved to ${esc(s.project_path)}`); return true; }
    return false;
  } catch (e) { showError("The project was not saved", String(e)); return false; }
}

async function newProject() {
  const c = await confirmDiscard("start a new project");
  if (c === "cancel") return;
  if (c === "save" && !(await saveProject(false))) return;
  state = await invoke("new_project");
  view = { mode: "list", clipId: null };
  renderAll();
}

async function openProject() {
  const c = await confirmDiscard("open another project");
  if (c === "cancel") return;
  if (c === "save" && !(await saveProject(false))) return;
  try {
    const s = await invoke("open_project");
    if (s) { state = s; view = { mode: "list", clipId: null }; renderAll(); notePending(); }
  } catch (e) { showError("The project could not be opened", String(e)); }
}

function notePending() {
  const a = state?.analysis;
  const missing = state.clips.filter((c) => c.status === "missing").length;
  const bits = [];
  if (missing) bits.push(`${plural(missing, "clip")} ${missing === 1 ? "is" : "are"} missing from ${missing === 1 ? "its" : "their"} folder; their saved scores are still used`);
  if (a && a.pending) bits.push(`${plural(a.pending, "clip")} ${a.pending === 1 ? "needs" : "need"} analyzing`);
  toast(bits.length ? `${icon("clock")} Project opened. ${esc(bits.join(". "))}.` : `${icon("tick")} Project opened. Every clip's scores were loaded, nothing needs analyzing.`);
}

async function exportTimeline() {
  try {
    const r = await invoke("export_timeline");
    if (!r) return;
    const warn = r.warnings.length ? `<p class="muted">${r.warnings.map(esc).join("<br>")}</p>` : "";
    await modal("Timeline exported", `<p>${plural(r.selects, "select")} written to <span class="selectable">${esc(r.folder)}</span>:</p>
      <ul>${r.files.map((f) => `<li class="selectable">${esc(f)}</li>`).join("")}</ul>
      <p class="muted">In DaVinci Resolve choose File, Import, Timeline and pick the .xml or .fcpxml. In Premiere Pro choose File, Import and pick the .xml. In Final Cut Pro choose File, Import, XML and pick the .fcpxml. Scene, light, camera movement and problems ride along as keywords and log notes, so you can search and collect by them.</p>${stillsSummary(r.stills)}${warn}`,
      [{ label: "Done", value: true, primary: true }]);
  } catch (e) { showError("The timeline was not exported", String(e)); }
}

async function changeSampleRate() {
  const sel = $("sample-rate");
  const rate = Number(sel.value);
  if (!state || rate === state.sample_rate) return;
  const analyzed = state.clips.filter((c) => c.status === "analyzed" || c.status === "partial").length;
  if (!analyzed) { state = await invoke("set_sample_rate", { rate, reanalyze: true }); renderAll(); return; }
  const speed = state.analysis.speed;
  const est = speed ? ` That took about ${fmtDur(state.summary.analyzed_seconds / speed)} last time at ${state.sample_rate} per second.` : "";
  const choice = await modal("Analyze the footage again?", `<p>Scoring ${plural(rate, "frame")} per second instead of ${state.sample_rate} means analyzing ${plural(analyzed, "clip")} again.${esc(est)}</p><p class="muted">Current selects stay on screen and are replaced clip by clip as the new scores arrive.</p>`, [
    { label: `Keep ${state.sample_rate} per second`, value: false },
    { label: `Analyze again at ${rate} per second`, value: true, primary: true },
  ]);
  if (!choice) { sel.value = String(state.sample_rate); return; }
  state = await invoke("set_sample_rate", { rate, reanalyze: true });
  renderAll();
}

/* ---------- updates ---------- */

let update = { phase: "idle", version: null, message: "" }; // idle | checking | current | available | installing | failed | off

function renderUpdates() {
  $("version-line").textContent = product.version ? `You have version ${product.version}.` : "";
  const box = $("update-status"), btn = $("btn-update");
  let html = "";
  btn.disabled = update.phase === "checking" || update.phase === "installing" || update.phase === "off";
  btn.innerHTML = "Check for updates";
  if (update.phase === "checking") html = `<p class="update-line"><span class="spinner" aria-hidden="true"></span>Asking GitHub for the newest version</p>`;
  else if (update.phase === "current") html = `<p class="update-line">${pill("green", "tick", "Up to date")} This is the newest version.</p>`;
  else if (update.phase === "available") {
    html = `<p class="update-line">${pill("amber", "clock", "Update ready")} Version ${esc(update.version)} is available. Your open project is saved first if it has a file; footage is never touched.</p>`;
    btn.innerHTML = `Install version ${esc(update.version)} and restart`;
  } else if (update.phase === "installing") html = `<p class="update-line"><span class="spinner" aria-hidden="true"></span>Downloading and installing version ${esc(update.version)}. The app restarts when it is done.</p>`;
  else if (update.phase === "failed") html = `<div class="error-box error-small" role="alert">${icon("triangle")}<div><p>${esc(update.message)}</p></div></div>`;
  else if (update.phase === "off") html = `<p class="update-line">${pill("grey", "dash", "Not available")} ${esc(update.message)}</p>`;
  box.innerHTML = html;
}

async function checkForUpdate(quiet) {
  update = { phase: "checking", version: null, message: "" }; renderUpdates();
  try {
    const r = await invoke("check_for_update");
    update = r.available ? { phase: "available", version: r.available, message: "" } : { phase: quiet ? "idle" : "current", version: null, message: "" };
  } catch (e) {
    update = quiet ? { phase: "idle", version: null, message: "" } : { phase: "failed", version: null, message: String(e) };
  }
  renderUpdates();
}

async function installUpdate() {
  if (state?.dirty && state?.project_path) {
    try { await invoke("save_project", { saveAs: false }); } catch (e) { showError("The project was not saved, so the update did not start", String(e)); return; }
  } else if (state?.dirty) {
    const c = await modal("Install the update without saving?", "<p>This project has never been saved, so its scores will be lost when the app restarts. The footage itself is never changed.</p>", [
      { label: "Keep working", value: "cancel" },
      { label: "Install and lose unsaved scores", value: "go", primary: true },
    ]);
    if (c !== "go") return;
  }
  update = { ...update, phase: "installing" }; renderUpdates();
  try { await invoke("install_update"); }
  catch (e) { update = { phase: "failed", version: null, message: String(e) }; renderUpdates(); }
}

async function startUpdates() {
  try {
    const s = await invoke("update_settings");
    $("auto-update").checked = s.check_updates_on_open;
    if (!s.configured) {
      update = { phase: "off", version: null, message: "This copy cannot update itself. Download newer versions from the release page." };
      $("auto-update").disabled = true;
    } else if (s.check_updates_on_open) { renderUpdates(); checkForUpdate(true); return; }
  } catch (_) { /* updates are optional; the rest of the app carries on */ }
  renderUpdates();
}

/* ---------- refresh ---------- */

function renderAllButSide() { renderProjectLine(); renderSummary(); if (view.mode === "list") renderMain(); }
function renderAll() { renderAllButSide(); if (view.mode === "detail") renderDetail(); syncSide(); }

let refreshTimer = null, lastRefresh = 0;
function scheduleRefresh(immediate) {
  const wait = immediate ? 0 : Math.max(0, 400 - (Date.now() - lastRefresh));
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(refresh, wait);
}
async function refresh() {
  lastRefresh = Date.now();
  try {
    state = await invoke("get_state");
    loadError = null;
  } catch (e) {
    if (!state) loadError = String(e);
  }
  renderAllButSide();
  syncSide();
  if (view.mode === "detail") {
    const row = state?.clips.find((c) => c.id === view.clipId);
    if (!row) { view = { mode: "list", clipId: null }; renderMain(); }
    else if (row.status === "analyzing" || !detail) refreshDetail();
    else renderDetail();
  }
}

/* ---------- wiring ---------- */

function wire() {
  document.addEventListener("click", (e) => {
    const act = e.target.closest("[data-act]")?.dataset.act;
    if (act === "add-files") addFiles(false);
    else if (act === "add-folder") addFiles(true);
    else if (act === "back") { view = { mode: "list", clipId: null }; detail = null; renderMain(); }
    else if (act === "add-finished") addFinished();
    else if (act === "stop-tracing") invoke("stop_tracing");
    else if (act === "fit") fitToEdits();
    else if (act === "retrace") invoke("retrace_finished_videos").then((s) => { state = s; renderAll(); });
    else if (act === "keep") makeCall("keep", callTarget()?.mark?.stars || 0);
    else if (act === "drop") makeCall("drop", 0);
    else if (act === "clear-call") makeCall(null, 0);
    else if (act === "star") { const n = Number(e.target.closest("[data-stars]").dataset.stars); const cur = callTarget()?.mark; makeCall("keep", cur?.call === "keep" && cur.stars === n ? n - 1 : n); }
    else if (act === "report") openReport();
    else if (act === "save-report") saveReport();
    else if (act === "choose-music") chooseMusic();
    else if (act === "preview-reel") previewReel();
    else if (act === "export-reel") exportReel();
    else if (act === "stop-stills") invoke("stop_stills");
    else if (act === "prev-select") stepSelect(-1);
    else if (act === "next-select") stepSelect(1);
    else if (e.target.closest("[data-viewmode]")) setViewMode(e.target.closest("[data-viewmode]").dataset.viewmode);
    else if (e.target.closest("[data-preset]")) {
      const p = PRESETS.find((x) => x.id === e.target.closest("[data-preset]").dataset.preset);
      if (p && localSel) { localSel = { ...localSel, ...p.v }; renderPresets(); pushSelection(); }
    }
    else if (e.target.closest("[data-light]")) {
      if (localSel) { localSel.light = e.target.closest("[data-light]").dataset.light; renderExtraFilters(); pushSelection(); }
    }
    else if (e.target.closest("[data-pace]")) pushReel({ pace: e.target.closest("[data-pace]").dataset.pace });
    else if (e.target.closest("[data-reel-length]")) { const v = e.target.closest("[data-reel-length]").dataset.reelLength; pushReel({ length: v ? Number(v) : null }); }
    else if (e.target.closest("[data-order]")) pushReel({ order: e.target.closest("[data-order]").dataset.order });
    else if (e.target.closest("[data-shape]")) pushReel({ shape: e.target.closest("[data-shape]").dataset.shape });
    else if (e.target.closest("[data-camera]")) {
      if (localSel) { localSel.camera = e.target.closest("[data-camera]").dataset.camera; renderFilters(); pushSelection(); }
    }
    else if (e.target.closest("tr[data-seg]")) {
      const [a, b] = e.target.closest("tr[data-seg]").dataset.seg.split(":").map(Number);
      pinAt((a + b) / 2);
      $("main-panel").querySelector(".viewer")?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
    else if (act === "remove-finished") {
      const path = e.target.closest("[data-path]").dataset.path;
      invoke("remove_finished_video", { path }).then((s) => { state = s; renderAll(); });
    }
    else {
      const tr = e.target.closest("tr[data-clip]");
      if (tr) openDetail(Number(tr.dataset.clip));
    }
  });
  $("btn-new").addEventListener("click", newProject);
  $("btn-open").addEventListener("click", openProject);
  $("btn-save").addEventListener("click", (e) => saveProject(e.shiftKey));
  $("btn-export").addEventListener("click", exportTimeline);
  $("btn-stills").addEventListener("click", exportStills);
  $("btn-report").addEventListener("click", openReport);
  $("handles").addEventListener("change", () => pushExport(false));
  $("export-stills").addEventListener("change", () => pushExport(false));
  $("place-input").addEventListener("change", pushPlaces);
  $("clock-zone").addEventListener("change", pushPlaces);
  $("snap-pauses").addEventListener("change", (e) => { if (localSel) { localSel.snap_to_pauses = e.target.checked; pushSelection(); } });
  $("set-aside-sound").addEventListener("change", (e) => { if (localSel) { localSel.set_aside_sound_problems = e.target.checked; pushSelection(); } });
  $("btn-analysis").addEventListener("click", async () => {
    if (state?.analysis.running) await invoke("stop_analysis");
    else await invoke("start_analysis");
    scheduleRefresh(true);
  });
  $("sample-rate").addEventListener("change", changeSampleRate);
  $("fallback-profile").addEventListener("change", async (e) => {
    const v = e.target.value;
    try { state = await invoke("set_fallback_profile", { profile: v === "auto" ? null : v }); renderAll(); if (view.mode === "detail") refreshDetail(); }
    catch (err) { showError("The profile was not changed", String(err)); }
  });
  document.addEventListener("change", async (e) => {
    if (e.target.id !== "clip-profile") return;
    const v = e.target.value;
    try { state = await invoke("set_clip_profile", { id: view.clipId, profile: v === "auto" ? null : v }); renderAll(); refreshDetail(); }
    catch (err) { showError("The profile was not changed", String(err)); }
  });
  $("btn-update").addEventListener("click", () => (update.phase === "available" ? installUpdate() : checkForUpdate(false)));
  $("auto-update").addEventListener("change", async (e) => {
    try { await invoke("set_check_updates_on_open", { on: e.target.checked }); }
    catch (err) { e.target.checked = !e.target.checked; showError("The setting was not changed", String(err)); }
  });
  $("timeline-rate").addEventListener("change", () => pushExport(true));
  $("drop-frame").addEventListener("change", () => pushExport(false));
  for (const r of document.querySelectorAll('input[name="layout"]')) r.addEventListener("change", () => pushExport(false));
  $("timeline-name").addEventListener("change", () => pushExport(false));
  document.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); saveProject(e.shiftKey); }
    if (mod && e.key.toLowerCase() === "o") { e.preventDefault(); openProject(); }
    if (e.key === "Escape" && (view.mode === "detail" || view.mode === "report") && $("modal").hidden) { view = { mode: "list", clipId: null }; renderMain(); }
    const typing = ["INPUT", "SELECT", "TEXTAREA"].includes(document.activeElement?.tagName);
    if (view.mode === "detail" && detail && !typing && !mod && $("modal").hidden) {
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") { e.preventDefault(); pinAt(viewer.pin + (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 5 : 1)); }
      if (e.key === "[" || e.key === "]") { e.preventDefault(); stepSelect(e.key === "[" ? -1 : 1); }
      const k = e.key.toLowerCase();
      if (k === "k") { e.preventDefault(); makeCall("keep", callTarget()?.mark?.stars || 0); }
      if (k === "x") { e.preventDefault(); makeCall("drop", 0); }
      if (k === "0" || k === "u") { e.preventDefault(); makeCall(null, 0); }
      if (/^[1-5]$/.test(k)) { e.preventDefault(); makeCall("keep", Number(k)); }
    }
  });
  window.addEventListener("resize", () => { graphWidth = 0; if (view.mode === "detail") renderDetail(); });
  wireSliders();

  listen("analysis-changed", () => scheduleRefresh(false));
  listen("state-changed", () => scheduleRefresh(true));
  listen("probe-progress", (e) => {
    const p = e.payload;
    probe = p.done >= p.total ? null : p;
    if (!probe) scheduleRefresh(true); else renderMain();
  });
  listen("drop-hover", (e) => { $("drop-overlay").hidden = !e.payload; });
  listen("stills-progress", (e) => { if (state) { state.stills_progress = e.payload.done >= e.payload.total ? null : [e.payload.done, e.payload.total]; renderStillsStatus(); } });
  listen("add-result", (e) => {
    const r = e.payload;
    probe = null;
    if (r.found === 0) { toast(`${icon("dash")} No video files were found there. ${esc(AppName())} looks for .mov, .mp4, .mxf, .mts and similar files, including inside subfolders.`); return; }
    const bits = [`Added ${plural(r.added, "clip")}`];
    if (r.already_there) bits.push(`${r.already_there} ${r.already_there === 1 ? "was" : "were"} already in the project`);
    if (r.unreadable) bits.push(`${plural(r.unreadable, "file")} could not be read as video and ${r.unreadable === 1 ? "is" : "are"} listed with the reason`);
    toast(`${icon(r.unreadable ? "triangle" : "tick")} ${esc(bits.join(". "))}.`);
  });
  listen("close-requested", async () => {
    const c = await modal("Save your changes before closing?", "<p>This project has changes that are not saved. The footage itself is never changed.</p>", [
      { label: "Keep working", value: "cancel" },
      { label: "Close without saving", value: "discard" },
      { label: "Save and close", value: "save", primary: true },
    ]);
    if (c === "cancel") return;
    try { await invoke("close_app", { save: c === "save" }); }
    catch (e) { showError("The project was not saved, so the app stayed open", String(e)); }
  });
}

async function start() {
  renderMain();
  wire();
  try {
    const t = await invoke("tool_check");
    product = { name: t.product_name || "", version: t.product_version || "" };
    document.querySelectorAll("[data-product-name]").forEach((el) => { el.textContent = product.name; });
    if (!t.ok) toolError = "The video reader that ships with the app (ffmpeg) did not start.";
  } catch (_) { /* reported by get_state below if the bridge is down */ }
  await refresh();
  startUpdates();
}

start();
