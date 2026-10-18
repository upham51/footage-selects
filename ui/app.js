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
};
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
    s += `<rect x="${x.toFixed(1)}" y="1" width="${w.toFixed(1)}" height="12" rx="3" fill="#2C4A6E"/>`;
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
    text = bits.join(", ");
  } else {
    p = pill("red", "triangle", "Below threshold");
    const word = SCORE_WORD[r.score] || r.score;
    text = r.kind === "average"
      ? `${word} averaged ${r.average}, needs ${r.needs}`
      : `${word} under ${r.needs} for ${r.share}% of the clip`;
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
  if (c.profile_is_log) bits.push(`<span class="tag" title="Exposure is judged through this profile's curve">${esc(c.profile_label)}</span>`);
  if (c.choppy_shutter) bits.push(pill("amber", "triangle", "Fast shutter"));
  return bits.length ? `<div class="clip-tags">${bits.join("")}</div>` : "";
}

function listHtml() {
  const rows = state.clips.map((c) => `
    <tr data-clip="${c.id}">
      <td class="name-cell"><div class="clip-name">${esc(c.name)}</div><div class="clip-sub">${esc(c.folder)}</div>${clipTags(c)}</td>
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

function renderMain() {
  const panel = $("main-panel");
  if (view.mode === "detail") { renderDetail(); return; }
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
      return `Set aside: the camera filter asks for ${FILTER_WORDS[sel.camera] || "another movement"}, and this stretch is ${(CAMERA_WORDS[seg.camera] || "moving").toLowerCase()}.`;
  }
  return "";
}

/** A note for a kept or over-limit stretch that looks like a better select elsewhere. */
function repeatNote(seg) {
  if (!seg.similar_to || seg.status === "set_aside") return "";
  return ` Looks like the select in ${clipName(seg.similar_to.clip)} at ${fmtClock(seg.similar_to.start)}.`;
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
    <line class="cursor" x1="0" x2="0" y1="0" y2="${H}" stroke="#1D3450" stroke-width="1" visibility="hidden"/>
  </svg>`;
}

function laneSvg(w, d) {
  const H = 24, dur = d.duration || 1;
  const x = (t) => (t / dur) * w;
  let s = "";
  for (const seg of d.segments) {
    const x0 = x(seg.start), x1 = x(seg.end), bw = Math.max(1, x1 - x0);
    s += `<rect x="${x0.toFixed(1)}" y="2" width="${bw.toFixed(1)}" height="${H - 4}" rx="4" fill="${SEG_FILL[seg.status]}" stroke="#fff" stroke-width="1"/>`;
    if (bw >= 18) {
      const [, shape] = SEG_STATUS[seg.status];
      s += `<g transform="translate(${(x0 + bw / 2 - 6).toFixed(1)},6) scale(.857)" fill="none" stroke="${SEG_INK[seg.status]}" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">${ICON[shape]}</g>`;
    }
  }
  for (const [a, b] of d.used || []) {
    const x0 = x(a), bw = Math.max(4, x(b) - x0);
    s += `<rect x="${x0.toFixed(1)}" y="1" width="${bw.toFixed(1)}" height="${H - 2}" rx="5" fill="none" stroke="#9A5B00" stroke-width="1.6" stroke-dasharray="4 3"/>`;
  }
  return `<svg viewBox="0 0 ${w} ${H}" width="${w}" height="${H}" aria-hidden="true">${s}</svg>`;
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
  const w = graphWidth || Math.max(320, panel.clientWidth - 38);

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
    const keys = d.raw.key_stops || [];
    const keyMean = keys.length ? keys.reduce((a, b) => a + b, 0) / keys.length : null;
    const extraStats = (key) => key === "exposure" && isLog && keyMean != null ? ` · middle tones <span class="num">${fmtStops(keyMean)}</span> stops` : "";
    const graph = (key, label) => `<div class="graph"><div class="graph-head"><span class="name">${label}</span><span class="stats">Average <span class="num">${mean(key)}</span>${extraStats(key)} · threshold <span class="num">${pct(d.selection[key + "_min"])}</span></span></div>${graphSvg(key, label, w - 2, d)}</div>`;
    const segRows = d.segments.map((s) => {
      const [t, sh, wd] = SEG_STATUS[s.status];
      const usedIn = (d.used || []).filter(([a, b, n]) => Math.min(b, s.end) - Math.max(a, s.start) > 0.25).map(([, , n]) => n);
      const usedText = usedIn.length ? ` Used in ${[...new Set(usedIn)].join(", ")}.` : "";
      const pinned = viewer.pin >= s.start && viewer.pin < s.end ? ' class="pinned"' : "";
      return `<tr data-seg="${s.start}:${s.end}"${pinned} title="Show this stretch in the frame viewer"><td class="tabular nowrap">${fmtClock(s.start)} – ${fmtClock(s.end)}</td><td class="num tabular">${fmtDur(s.end - s.start)}</td><td>${pill(t, sh, wd)}</td><td class="why">${esc(whyText(s, d.selection, keptCount) + repeatNote(s) + usedText)}</td>
        <td class="nowrap">${esc(CAMERA_WORDS[s.camera] || "Still")}</td><td class="num tabular">${pct(s.focus)}</td><td class="num tabular">${pct(s.exposure)}</td>${isLog ? `<td class="num tabular">${fmtStops(s.key_stops)}</td>` : ""}<td class="num tabular">${pct(s.stability)}</td><td class="num tabular">${pct(s.score)}</td></tr>`;
    }).join("");
    const usedLegend = (d.used || []).length ? `<span class="legend-used"><svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><rect x="1.5" y="3" width="11" height="8" rx="2" stroke="#9A5B00" stroke-dasharray="2.5 1.8"/></svg>Used in your edits</span>` : "";
    body = `${viewerHtml(d)}<div class="legend"><span class="label">Stretches</span>${Object.values(SEG_STATUS).map(([t, s, wd]) => pill(t, s, wd)).join("")}${d.measured_until < d.duration - 0.01 ? pill("grey", "clock", "Not measured yet") : ""}${usedLegend}</div>
      <div class="graphs">
        <div class="graph">${laneSvg(w, d)}</div>
        ${graph("focus", "Focus")}${graph("exposure", "Exposure")}${graph("stability", "Stability")}
        <div class="graph-axis">${axisSvg(w, d.duration || 1)}</div>
      </div>
      <div class="readout" id="readout">Move the pointer over a graph to read the scores and see the picture at that moment; click to hold it. Navy is the smoothed score used for selection, grey the raw per-frame score, the dashed line the threshold.</div>
      <div class="seg-table"><h3 class="eyebrow" style="padding-left:18px">Every stretch of this clip</h3>
        <table><thead><tr><th>Time</th><th class="num">Length</th><th>Result</th><th>Why</th><th>Camera</th><th class="num">Focus</th><th class="num">Exposure</th>${isLog ? '<th class="num" title="Middle tones in stops from middle grey">Stops</th>' : ""}<th class="num">Stability</th><th class="num">Overall</th></tr></thead><tbody>${segRows}</tbody></table></div>`;
  }

  panel.innerHTML = `<div class="detail">
    <div class="detail-head">${back}
      <div class="detail-title"><h2 class="selectable">${esc(d.name)}</h2>${statusPill}</div>
      <div class="meta">${meta.map((m) => `<span>${m}</span>`).join("")}</div>
      <div class="meta selectable">${esc(d.path)}</div>
      ${cameraHtml}${profileHtml}
    </div>${body}</div>`;
  const cp = $("clip-profile");
  if (cp) cp.value = cp.dataset.value;
  const holder = panel.querySelector(".graphs .graph");
  if (holder) {
    const real = Math.floor(holder.clientWidth);
    if (real > 0 && Math.abs(real - w) > 1) { graphWidth = real; renderDetail(); return; }
  }
  attachHover(w, d);
  paintViewer();
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
    const logBits = ks.length ? ` · Middle tones <span class="num">${fmtStops(ks[i])}</span> stops${cl[i] > 0.005 ? ` · <span class="num">${(cl[i] * 100).toFixed(cl[i] < 0.1 ? 1 : 0)}%</span> clipped` : ""}` : "";
    if (viewer.mode !== "off") showFrameAt(t);
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
  showFrameAt(viewer.pin);
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
  </div>`;
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
  { id: "long", label: "Long takes", hint: "For interviews, talks and locked-off shots: selects of 5 to 30 seconds, with focus and steadiness deciding which win. Sound is not checked, so a select can start mid-sentence.",
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
    html += `<button class="btn btn-primary btn-block" data-act="fit" ${tr ? "disabled" : ""}>Fit settings to my edits</button>`;
  } else if (fv.length && !tr) {
    html += `<p class="hint">None of these videos' shots were found in the clips analyzed so far. They may come from footage that is not in this project, or from clips still waiting to be analyzed.</p>`;
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
  if (same) {
    await modal("Your settings already fit your edits", `<p>No nearby settings keep more of the shots you used without making the selects much longer. They keep <strong class="num">${b.kept} of ${b.used}</strong> now.</p>`, [{ label: "Done", value: true, primary: true }]);
    return;
  }
  let held;
  if (f.held_out_kept_share != null) {
    held = `<p>Checked on each finished video with settings learned from the others only: they kept <strong class="num">${Math.round(f.held_out_kept_share * 100)}%</strong> of that video's shots, against <span class="num">${Math.round(f.held_out_before_share * 100)}%</span> with your current settings.</p>`;
  } else {
    held = `<p class="muted">With one finished video, these settings could be tuned to that one edit. Add another and the fit is also checked on a video it has not seen.</p>`;
  }
  const body = `<p>Shots your edits used that are kept: <strong class="num">${b.kept} of ${b.used}</strong> now, <strong class="num">${a.kept} of ${a.used}</strong> with these settings. Selects in total: <span class="num">${fmtDur(b.selects_seconds)}</span> now, <span class="num">${fmtDur(a.selects_seconds)}</span> with these settings.</p>
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
    localSel = { ...localSel, ...DEFAULT_SELECTION, camera: "any", set_aside_repeats: false };
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
  return { rate, drop_frame: $("drop-frame-row").hidden ? null : $("drop-frame").checked, layout, timeline_name: $("timeline-name").value };
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
      <p class="muted">In DaVinci Resolve choose File, Import, Timeline and pick the .xml or .fcpxml. In Premiere Pro choose File, Import and pick the .xml. In Final Cut Pro choose File, Import, XML and pick the .fcpxml.</p>${warn}`,
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
    else if (act === "prev-select") stepSelect(-1);
    else if (act === "next-select") stepSelect(1);
    else if (e.target.closest("[data-viewmode]")) setViewMode(e.target.closest("[data-viewmode]").dataset.viewmode);
    else if (e.target.closest("[data-preset]")) {
      const p = PRESETS.find((x) => x.id === e.target.closest("[data-preset]").dataset.preset);
      if (p && localSel) { localSel = { ...localSel, ...p.v }; renderPresets(); pushSelection(); }
    }
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
    if (e.key === "Escape" && view.mode === "detail" && $("modal").hidden) { view = { mode: "list", clipId: null }; renderMain(); }
    const typing = ["INPUT", "SELECT", "TEXTAREA"].includes(document.activeElement?.tagName);
    if (view.mode === "detail" && detail && !typing && !mod && $("modal").hidden) {
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") { e.preventDefault(); pinAt(viewer.pin + (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 5 : 1)); }
      if (e.key === "[" || e.key === "]") { e.preventDefault(); stepSelect(e.key === "[" ? -1 : 1); }
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
