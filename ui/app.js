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
};
const SEG_FILL = { kept: "#E7F1EE", too_short: "#EFF2F4", over_limit: "#FDF2E2", below_threshold: "#FCEDEC" };
const SEG_INK = { kept: "#0F5C4E", too_short: "#7A8798", over_limit: "#9A5B00", below_threshold: "#B3261E" };

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
  return `<svg class="strip" viewBox="0 0 1000 14" preserveAspectRatio="none" aria-hidden="true">${s}</svg>`;
}

function statusCell(c) {
  const [tone, shape, word] = CLIP_STATUS[c.status] || CLIP_STATUS.not_analyzed;
  const label = c.status === "analyzing" ? `${word} ${pct(c.progress)}%` : word;
  return pill(tone, shape, label);
}

function selectsCell(c) {
  const kept = c.kept.length;
  const secs = c.kept.reduce((a, k) => a + (k.end - k.start), 0);
  let text;
  if (c.status === "unreadable" || c.status === "failed") return `<div class="status-detail" title="${esc(c.status_detail)}">Left out. ${esc(c.status_detail || "")}</div>`;
  if ((c.status === "missing" || c.status === "changed") && c.status_detail && !kept) text = esc(c.status_detail);
  else if (!kept && (c.status === "waiting" || c.status === "not_analyzed")) text = "Not scored yet";
  else if (!kept && (c.status === "analyzing" || c.status === "partial")) text = "None so far";
  else if (!kept) text = "None meet the thresholds";
  else text = `<strong class="num">${kept}</strong> ${kept === 1 ? "select" : "selects"} · <span class="num">${fmtDur(secs)}</span>`;
  const extra = c.over_limit ? ` · ${c.over_limit} more over the limit` : "";
  return `${stripSvg(c)}<div class="sel-count">${text}${kept ? extra : ""}</div>`;
}

function listHtml() {
  const rows = state.clips.map((c) => `
    <tr data-clip="${c.id}">
      <td class="name-cell"><div class="clip-name">${esc(c.name)}</div><div class="clip-sub">${esc(c.folder)}</div></td>
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
    case "kept": return "Every score is at or above its threshold.";
    case "below_threshold": {
      const names = seg.failed.map((f) => `${f} (${pct(sel[f + "_min"])})`);
      const list = names.length > 1 ? names.slice(0, -1).join(", ") + " and " + names[names.length - 1] : names[0];
      return `Dropped: ${list} under the threshold.`;
    }
    case "too_short": return `Dropped: good, but shorter than the ${fmtDur(sel.min_length)} minimum.`;
    case "over_limit": return `Dropped: good, but ${plural(cap, "better select is", "better selects are")} already kept in this clip.`;
  }
  return "";
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
  if (d.sample_rate) meta.push(`Scored at ${d.sample_rate} frames per second${d.analysis_seconds ? ` in ${fmtDur(d.analysis_seconds)}` : ""}`);

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
    const graph = (key, label) => `<div class="graph"><div class="graph-head"><span class="name">${label}</span><span class="stats">Average <span class="num">${mean(key)}</span> · threshold <span class="num">${pct(d.selection[key + "_min"])}</span></span></div>${graphSvg(key, label, w - 2, d)}</div>`;
    const segRows = d.segments.map((s) => {
      const [t, sh, wd] = SEG_STATUS[s.status];
      return `<tr><td class="tabular">${fmtClock(s.start)} – ${fmtClock(s.end)}</td><td class="num tabular">${fmtDur(s.end - s.start)}</td><td>${pill(t, sh, wd)}</td><td class="why">${esc(whyText(s, d.selection, keptCount))}</td>
        <td class="num tabular">${pct(s.focus)}</td><td class="num tabular">${pct(s.exposure)}</td><td class="num tabular">${pct(s.stability)}</td><td class="num tabular">${pct(s.score)}</td></tr>`;
    }).join("");
    body = `<div class="legend"><span class="label">Stretches</span>${Object.values(SEG_STATUS).map(([t, s, wd]) => pill(t, s, wd)).join("")}${d.measured_until < d.duration - 0.01 ? pill("grey", "clock", "Not measured yet") : ""}</div>
      <div class="graphs">
        <div class="graph">${laneSvg(w, d)}</div>
        ${graph("focus", "Focus")}${graph("exposure", "Exposure")}${graph("stability", "Stability")}
        <div class="graph-axis">${axisSvg(w, d.duration || 1)}</div>
      </div>
      <div class="readout" id="readout">Move the pointer over a graph to read the scores at that moment. Navy is the smoothed score used for selection, grey the raw per-frame score, the dashed line the threshold.</div>
      <div class="seg-table"><h3 class="eyebrow" style="padding-left:18px">Every stretch of this clip</h3>
        <table><thead><tr><th>Time</th><th class="num">Length</th><th>Result</th><th>Why</th><th class="num">Focus</th><th class="num">Exposure</th><th class="num">Stability</th><th class="num">Overall</th></tr></thead><tbody>${segRows}</tbody></table></div>`;
  }

  panel.innerHTML = `<div class="detail">
    <div class="detail-head">${back}
      <div class="detail-title"><h2 class="selectable">${esc(d.name)}</h2>${statusPill}</div>
      <div class="meta">${meta.map((m) => `<span>${m}</span>`).join("")}</div>
      <div class="meta selectable">${esc(d.path)}</div>
    </div>${body}</div>`;
  const holder = panel.querySelector(".graphs .graph");
  if (holder) {
    const real = Math.floor(holder.clientWidth);
    if (real > 0 && Math.abs(real - w) > 1) { graphWidth = real; renderDetail(); return; }
  }
  attachHover(w, d);
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
    readout.innerHTML = `At <span class="num">${fmtClock(t)}</span> · Focus <span class="num">${pct(d.smoothed.focus[i])}</span> · Exposure <span class="num">${pct(d.smoothed.exposure[i])}</span> · Stability <span class="num">${pct(d.smoothed.stability[i])}</span> · ${esc(word)}`;
  });
}

async function openDetail(id) {
  view = { mode: "detail", clipId: id };
  detail = null; detailError = null;
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
  renderDetail();
}

/* ---------- side panel ---------- */

const SLIDERS = {
  focus_min: ["s-focus", "o-focus"], exposure_min: ["s-exposure", "o-exposure"], stability_min: ["s-stability", "o-stability"],
  smoothing: ["s-smoothing", "o-smoothing"], min_length: ["s-min", "o-min"], max_length: ["s-max", "o-max"], max_per_clip: ["s-cap", "o-cap"],
};

function sliderToValue(key, raw) {
  const v = Number(raw);
  if (key.endsWith("_min") && !key.startsWith("min")) return v / 100;
  if (key === "max_per_clip") return v >= 21 ? 0 : v;
  return v;
}
function valueToSlider(key, v) {
  if (key.endsWith("_min") && !key.startsWith("min")) return Math.round(v * 100);
  if (key === "max_per_clip") return v === 0 ? 21 : v;
  return v;
}
function label(key, v) {
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
  const sr = $("sample-rate");
  if (document.activeElement !== sr) sr.value = String(state.sample_rate);

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
  $("btn-reset").addEventListener("click", () => {
    localSel = { focus_min: 0.5, exposure_min: 0.5, stability_min: 0.5, smoothing: 1, min_length: 2, max_length: 10, max_per_clip: 3 };
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
  const choice = await modal("Analyze the footage again?", `<p>Scoring ${rate} frames per second instead of ${state.sample_rate} means analyzing ${plural(analyzed, "clip")} again.${esc(est)}</p><p class="muted">Current selects stay on screen and are replaced clip by clip as the new scores arrive.</p>`, [
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
