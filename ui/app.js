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
/** A running total, as a clock: 26:11 or 1:04:09. */
function fmtTotal(s) {
  const t = Math.round(Math.max(0, s || 0)), h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), r = t % 60;
  return h ? `${h}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}` : `${m}:${String(r).padStart(2, "0")}`;
}
const fmtLen = (s) => `${s < 10 ? s.toFixed(1) : Math.round(s)} s`;
const pct = (v) => Math.round((v ?? 0) * 100);
const plural = (n, one, many) => `${n} ${n === 1 ? one : many || one + "s"}`;
const range = (a, b) => `${fmtClock(a)} – ${fmtClock(b)}`;
const sharePct = (v) => (v < 0.001 ? "under 0.1%" : v < 0.1 ? `${(v * 100).toFixed(1)}%` : `${Math.round(v * 100)}%`);

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
  pause: '<path d="M4.6 3v8h1.6V3zM7.8 3v8h1.6V3z"/>',
  prevSel: '<path d="M3 2.8v8.4M10.6 2.8L6.4 7l4.2 4.2"/>',
  nextSel: '<path d="M11 2.8v8.4M3.4 2.8L7.6 7l-4.2 4.2"/>',
  down: '<path d="M7 2v7M4 6l3 3 3-3M2.5 11.5h9"/>',
  aside: '<path d="M2.5 7h6.5M6.5 4.2L9.3 7 6.5 9.8M11.5 3v8"/>',
  left: '<path d="M8.8 2.8L4.6 7l4.2 4.2"/>',
  right: '<path d="M5.2 2.8L9.4 7l-4.2 4.2"/>',
  arrow: '<path d="M2 7h9.5M8 3.5L11.5 7 8 10.5"/>',
  film: '<rect x="2" y="2.5" width="10" height="9" rx="1.5"/><path d="M4.6 2.5v9M9.4 2.5v9M2 5.5h2.6M2 8.5h2.6M9.4 5.5H12M9.4 8.5H12"/>',
  star: '<path d="M7 1.9l1.55 3.2 3.5.48-2.55 2.44.63 3.48L7 9.85 3.87 11.5l.63-3.48L1.95 5.58l3.5-.48z"/>',
  cross: '<path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/>',
  hand: '<path d="M4.5 7.5V3.8a.9.9 0 0 1 1.8 0V7M6.3 6.5V2.9a.9.9 0 0 1 1.8 0v3.6M8.1 6.6V3.6a.9.9 0 0 1 1.8 0v4.6c0 2-1.4 3.6-3.4 3.6-1.3 0-2.2-.6-2.9-1.6L2.3 8.1a.9.9 0 0 1 1.4-1.1l.8.9"/>',
  sun: '<circle cx="7" cy="7" r="2.6"/><path d="M7 1.5v1.3M7 11.2v1.3M1.5 7h1.3M11.2 7h1.3M3.1 3.1l.9.9M10 10l.9.9M3.1 10.9l.9-.9M10 4l.9-.9"/>',
  wave: '<path d="M1.8 7h1.4M4.6 4.5v5M7 2.8v8.4M9.4 4.5v5M10.8 7h1.4"/>',
  info: '<circle cx="7" cy="7" r="5.6"/><path d="M7 6.4v3.4M7 4.3v.2"/>',
  chev: '<path d="M3.5 5.2L7 8.8l3.5-3.6"/>',
};
const icon = (name, cls = "icon") => `<svg class="${cls}" viewBox="0 0 14 14" aria-hidden="true">${ICON[name]}</svg>`;
const tipAttr = (text) => (text ? ` data-tip="${esc(text)}"` : "");
/** A status: tone, a shape and a word, never colour alone. */
const pill = (tone, shape, word, tip, cls = "") => `<span class="pill pill-${tone}${cls ? " " + cls : ""}"${tipAttr(tip)}>${icon(shape)}${esc(word)}</span>`;
const tag = (word, ic, tip) => `<span class="tag"${tipAttr(tip)}>${ic ? icon(ic) : ""}${esc(word)}</span>`;
const infoI = (text, label = "More about this") => `<button type="button" class="info-i" aria-label="${esc(label)}"${tipAttr(text)}>${icon("info")}</button>`;
const sw = (on, attrs, label) => `<button type="button" class="switch" role="switch" aria-checked="${!!on}" aria-label="${esc(label)}" ${attrs}></button>`;

const LIGHT_WORDS = { night: "Night", blue_hour: "Blue hour", golden_hour: "Golden hour", daylight: "Daylight", harsh_midday: "Harsh midday" };
const SOUND_WORDS = { wind: "Wind noise", clipped: "Sound clipped", dropout: "Sound drops out" };
const LIGHT_FILTER_WORDS = { any: "any light", golden_hour: "golden hour", blue_hour: "blue hour", daylight: "daylight", night: "night" };
const starText = (n) => (n ? `${n} ${n === 1 ? "star" : "stars"}` : "");

const CLIP_STATUS = {
  analyzed: ["ok", "tick", "Analyzed"],
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
  kept: ["ok", "tick", "Kept"],
  below_threshold: ["red", "triangle", "Below the minimum"],
  too_short: ["grey", "dash", "Too short"],
  over_limit: ["amber", "stack", "Over the limit"],
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
const ISSUE_SHORT = { clipped_highlights: "Clipped", underexposed: "Too dark", overexposed: "Too bright", deep_shadows: "Too dark", flat: "Flat" };
const FAIL_SHORT = { focus: "Soft", stability: "Shaky", exposure: "Exposure" };
const SCORE_WORD = { focus: "Focus", exposure: "Exposure", stability: "Stability" };
/** What a low score means in the picture. */
const SCORE_PROBLEM = { focus: "Too soft", exposure: "Too dark or too bright", stability: "Too shaky" };
const KEYS = ["focus", "exposure", "stability"];

/** Brightness against a normal exposure, in words: "1.1 stops brighter than normal". */
const stopsWords = (v) => (v == null || !isFinite(v) ? "–" : Math.abs(v) < 0.15 ? "normal" : `${Math.abs(v).toFixed(1)} ${Math.abs(v) >= 0.95 && Math.abs(v) < 1.05 ? "stop" : "stops"} ${v > 0 ? "brighter" : "darker"} than normal`);
const profileLabel = (id) => state?.profiles?.find((p) => p.id === id)?.label || id;
const clipName = (id) => state?.clips?.find((c) => c.id === id)?.name || "another clip";

const TIPS = {
  quality: "The lowest score, from 0 to 100, a moment needs to be kept. Raise a slider to be pickier, lower it to keep more.",
  focus: "How much fine detail the picture has. Soft or missed focus scores low.",
  exposure: "Blown highlights, crushed shadows and flat pictures score low. Log footage is judged in stops through the camera's curve, so the flat log look does not count against it.",
  stability: "Locked-off shots and smooth pans score high. Handheld shake scores low.",
  length: "Good stretches shorter than the minimum are dropped. Longer than the maximum are split into equal parts, cut where the camera is still.",
  cap: "The most selects kept from one clip. The best scoring ones win.",
  keep_moves: "One pan, push or drone reveal can run up to twice the longest select instead of being cut in the middle.",
  smoothing: "Evens out the scores over this many seconds, so one soft or shaky frame does not split a good take in two.",
  weights: "When a clip has more good stretches than it keeps, these decide which stay. The minimums are not affected.",
  sliders: "Every change here shows right away. The footage is not analyzed again.",
  handles: "Extra footage before and after each select, for transitions and speed ramps. Markers still sit where each select starts, and scores are for the select itself.",
  layout: "Whole clips on track 1 with each select stacked above its clip, or only the selects, one after another.",
  stills: "A full-size still of the best frame in every select, plus a contact sheet you can print.",
  formats: ".xml for Premiere Pro and DaVinci Resolve, .fcpxml for Final Cut Pro, an .edl, and a .csv spreadsheet, side by side.",
  falsecolor: "Brightness in stops from middle grey. Everything outside these bands stays grey.",
  sample: "This is the only setting that needs the footage analyzed again. More frames per second catches shorter moments but takes longer.",
  updates: "A check asks GitHub whether a newer version exists. No footage, scores or project details are sent, and nothing else in the app uses the internet.",
  reel: "Pick a song and the best selects are cut to its beat, with the song under them, as a timeline for your editor. As shot keeps the order the moments happened. 9:16 and 1:1 are cut from the centre of the picture; check the framing in your editor.",
  taste: "Add videos you already cut from footage in this project. The app finds the shots you used, shows how many these settings would keep, and can fit the sliders to your choices. The videos are only read, on this computer.",
  places: "Phones and DJI drones record where and when, and Sony cameras record the time in UTC. Other cameras often record only the time on their clock, without saying which time zone it was set to. Guess from the clips uses the zone other clips state, or else picks between UTC and the place's local time by which one puts bright pictures in daylight. If times or light look wrong, choose the zone the camera's clock was set to.",
  theme: "Dark suits judging footage. The viewer stays dark either way.",
};

/* ---------- state ---------- */

let state = null;          // last snapshot from Rust, null until the first one arrives
let loadError = null;      // set when the first snapshot fails
let toolError = null;      // set when ffmpeg cannot start
let probe = null;          // {done, total} while files are being read
let view = { mode: "list", clipId: null };
let detail = null;         // clip detail data, null while loading
let detailError = null;
let dragging = false;      // a side slider is held
let ui = { panelTab: "selects", libTab: "all", adv: false, info: false };
try { ui.panelTab = localStorage.getItem("panel-tab") || "selects"; } catch (_) { /* a remembered tab is a convenience */ }

/* ---------- tooltips: every longer explanation sits behind a hover ---------- */

let tipFor = null;
function showTip(el) {
  const text = el.getAttribute("data-tip");
  const t = $("tip");
  if (!text) return;
  tipFor = el;
  t.textContent = text;
  t.hidden = false;
  const r = el.getBoundingClientRect(), tr = t.getBoundingClientRect();
  const below = r.top < tr.height + 16;
  const x = Math.max(8, Math.min(window.innerWidth - tr.width - 8, r.left + r.width / 2 - tr.width / 2));
  const y = below ? r.bottom + 8 : r.top - tr.height - 8;
  t.style.left = `${x}px`;
  t.style.top = `${Math.max(8, y)}px`;
}
function hideTip() { tipFor = null; $("tip").hidden = true; }
function wireTips() {
  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest?.("[data-tip]");
    if (el && el !== tipFor) showTip(el);
    else if (!el && tipFor) hideTip();
  });
  document.addEventListener("focusin", (e) => { const el = e.target.closest?.("[data-tip]"); if (el) showTip(el); });
  document.addEventListener("focusout", () => hideTip());
  document.addEventListener("scroll", () => hideTip(), true);
  document.addEventListener("pointerdown", () => hideTip());
}

/* ---------- theme ---------- */

const theme = () => (document.documentElement.getAttribute("data-theme") === "light" ? "light" : "dark");
function setTheme(t) {
  document.documentElement.setAttribute("data-theme", t);
  try { localStorage.setItem("theme", t); } catch (_) { /* a remembered theme is a convenience */ }
  renderSide();
}

/* ---------- top bar and progress ---------- */

function renderTop() {
  $("project-name").textContent = state?.project_name || "New project";
  const ss = $("save-state");
  if (state?.dirty) { ss.className = "save-state unsaved"; ss.textContent = "Unsaved"; }
  else if (state?.project_path) { ss.className = "save-state saved"; ss.textContent = "Saved"; }
  else { ss.className = "save-state"; ss.textContent = ""; }
  const title = `${state?.project_name || "New project"} · ${appName()}`;
  if (document.title !== title) {
    document.title = title;
    try { tauri.window.getCurrentWindow().setTitle(title); } catch (_) { /* title is cosmetic */ }
  }
  const s = state?.summary;
  $("totals").innerHTML = !s || !s.clips_total
    ? '<span class="word">No footage yet</span>'
    : `<span class="fig">${fmtTotal(s.footage_seconds)}</span><span class="word">of footage</span><span class="arrow">${icon("arrow")}</span><span class="fig out">${fmtTotal(s.selects_seconds)}</span><span class="word">in ${plural(s.selects_count, "select")}</span>`;
}

function renderProgress() {
  const a = state?.analysis;
  const row = $("progress-bar");
  const btn = $("btn-analysis");
  if (!a || (!a.running && a.pending === 0)) { row.hidden = true; return; }
  row.hidden = false;
  if (a.running) {
    const left = a.eta_seconds != null ? `about ${fmtDur(a.eta_seconds)} left` : "working out the time left";
    const speed = a.speed ? ` · ${Math.round(a.speed)}x real time` : "";
    $("progress-text").innerHTML = a.stopping
      ? "<strong>Stopping</strong> after the clips in progress"
      : `<strong>Analyzing ${a.clips_done} of ${plural(a.clips_total, "clip")}</strong> · ${left}${speed}`;
    $("progress-fill").style.width = `${(a.fraction * 100).toFixed(1)}%`;
    $("progress-track").setAttribute("aria-valuenow", String(Math.round(a.fraction * 100)));
    $("progress-track").hidden = false;
    btn.className = "btn sec xs";
    btn.innerHTML = `${icon("stop")}Stop`;
    btn.disabled = a.stopping;
  } else {
    $("progress-text").innerHTML = `<strong>${plural(a.pending, "clip")} not analyzed yet.</strong> Their selects are missing or out of date.`;
    $("progress-track").hidden = true;
    btn.className = "btn xs";
    btn.innerHTML = `${icon("play")}Analyze footage`;
    btn.disabled = false;
  }
}

function openMenu(anchor, items) {
  const m = $("menu");
  m.innerHTML = items.map((it, i) => `<button role="menuitem" data-menu="${i}">${esc(it.label)}${it.kbd ? `<kbd>${esc(it.kbd)}</kbd>` : ""}</button>`).join("");
  m.hidden = false;
  const r = anchor.getBoundingClientRect();
  m.style.left = `${r.left}px`;
  m.style.top = `${r.bottom + 6}px`;
  anchor.setAttribute("aria-expanded", "true");
  m.onclick = (e) => { const b = e.target.closest("[data-menu]"); if (b) { closeMenu(); items[Number(b.dataset.menu)].run(); } };
  m.querySelector("button")?.focus();
}
function closeMenu() {
  $("menu").hidden = true;
  $("project-btn").setAttribute("aria-expanded", "false");
}
const isMac = /Mac/.test(navigator.platform || navigator.userAgent);
const projectMenu = () => [
  { label: "New project", run: newProject },
  { label: "Open project", kbd: isMac ? "⌘O" : "Ctrl O", run: openProject },
  { label: "Save", kbd: isMac ? "⌘S" : "Ctrl S", run: () => saveProject(false) },
  { label: "Save as", kbd: isMac ? "⇧⌘S" : "Shift Ctrl S", run: () => saveProject(true) },
];

/* ---------- shared states ---------- */

function loadingHtml(title) {
  const rows = Array.from({ length: 6 }, () => '<div class="skeleton-row"><span></span><span></span><span></span><span></span></div>').join("");
  return `<div class="state"><div class="loading"><div class="loading-title"><span class="spinner" aria-hidden="true"></span>${esc(title)}</div>${rows}</div></div>`;
}
function errorHtml(title, body, next) {
  return `<div class="state"><div class="error-box" role="alert">${icon("triangle")}<div><strong>${esc(title)}</strong><p>${esc(body)}</p>${next ? `<p>${esc(next)}</p>` : ""}</div></div></div>`;
}
function emptyHtml() {
  return `<div class="state"><div class="empty-drop">
    ${icon("down", "icon icon-lg")}
    <h1>Drag a folder or clips here</h1>
    <p>${esc(AppName())} scores every clip for focus, exposure and steadiness, and picks the best stretches. Nothing leaves your computer.</p>
    <div class="row">
      <button class="btn" data-act="add-folder">${icon("folder")}Add a folder</button>
      <button class="btn sec" data-act="add-files">${icon("plus")}Add clips</button>
    </div></div></div>`;
}

/* ---------- thumbnails: small frames read in the background ---------- */

const thumbCache = new Map();  // key -> data URL, or "" when the frame could not be read
const thumbQueue = [];
let thumbBusy = 0;
const thumbKey = (id, t) => `${id}@${t.toFixed(1)}`;
let thumbObserver = null;

function fillThumb(img, url) {
  if (url) { img.src = url; img.classList.remove("blank"); }
}
function hydrateThumbs(root) {
  if (!thumbObserver) {
    thumbObserver = new IntersectionObserver((entries) => {
      for (const en of entries) if (en.isIntersecting) { thumbObserver.unobserve(en.target); queueThumb(en.target); }
    }, { rootMargin: "200px" });
  }
  for (const img of root.querySelectorAll("img[data-thumb-id]")) {
    const key = thumbKey(Number(img.dataset.thumbId), Number(img.dataset.thumbT));
    if (thumbCache.has(key)) fillThumb(img, thumbCache.get(key));
    else thumbObserver.observe(img);
  }
}
function queueThumb(img) {
  thumbQueue.push(img);
  pumpThumbs();
}
async function pumpThumbs() {
  while (thumbBusy < 2 && thumbQueue.length) {
    const img = thumbQueue.shift();
    if (!img.isConnected) continue;
    const id = Number(img.dataset.thumbId), t = Number(img.dataset.thumbT), key = thumbKey(id, t);
    if (thumbCache.has(key)) { fillThumb(img, thumbCache.get(key)); continue; }
    thumbBusy++;
    (async () => {
      let url = "";
      try { url = scaledUrl(parseStill(await invoke("clip_frame", { id, t })), 384, 216); } catch (_) { /* a blank thumbnail says enough */ }
      thumbCache.set(key, url);
      if (thumbCache.size > 400) thumbCache.delete(thumbCache.keys().next().value);
      for (const el of document.querySelectorAll(`img[data-thumb-id="${id}"][data-thumb-t="${img.dataset.thumbT}"]`)) fillThumb(el, url);
      thumbBusy--;
      pumpThumbs();
    })();
  }
}
function scaledCanvas(f, W, H) {
  const src = document.createElement("canvas");
  src.width = f.w; src.height = f.h;
  src.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(f.rgba), f.w, f.h), 0, 0);
  const c = document.createElement("canvas");
  c.width = W; c.height = H;
  const r = Math.max(W / f.w, H / f.h), w = f.w * r, h = f.h * r;
  c.getContext("2d").drawImage(src, (W - w) / 2, (H - h) / 2, w, h);
  return c;
}
const scaledUrl = (f, W, H) => scaledCanvas(f, W, H).toDataURL("image/jpeg", 0.82);
/** Where a clip's thumbnail is read: the middle of its first select, or a quarter in. */
function thumbTime(c) {
  const k = c.kept?.[0];
  const t = k ? (k.start + k.end) / 2 : (c.duration || 0) * 0.25;
  return Math.round(t * 10) / 10;
}
const canThumb = (c) => c.duration > 0 && !["unreadable", "missing"].includes(c.status);

/* ---------- main panel: all clips ---------- */

/** Why an analyzed clip has no selects: a short pill, and the full sentence behind it. */
function noSelectReason(r) {
  if (r.kind === "too_short") return ["grey", "dash", "Too short", `Good stretches last up to ${fmtDur(r.longest)}, and the shortest select is ${fmtDur(r.needs)}.`];
  if (r.kind === "set_aside") {
    const bits = [];
    if (r.camera) bits.push(`${plural(r.camera, "good stretch", "good stretches")} without ${FILTER_WORDS[state?.selection?.camera] || "the chosen movement"}`);
    if (r.repeats) bits.push(`${plural(r.repeats, "repeated take")} of a better select in another clip`);
    if (r.light) bits.push(`${plural(r.light, "good stretch", "good stretches")} not in ${LIGHT_FILTER_WORDS[state?.selection?.light] || "the chosen light"}`);
    if (r.sound) bits.push(`${plural(r.sound, "good stretch", "good stretches")} with sound problems`);
    if (r.dropped) bits.push(`${plural(r.dropped, "stretch", "stretches")} dropped by hand`);
    return ["grey", "aside", "Set aside", `Set aside: ${bits.join(", ")}.`];
  }
  const word = (SCORE_WORD[r.score] || r.score).toLowerCase();
  const text = r.kind === "average"
    ? `${word[0].toUpperCase()}${word.slice(1)} averaged ${r.average}, and selects need ${r.needs}.`
    : `${word[0].toUpperCase()}${word.slice(1)} is under ${r.needs} for ${r.share}% of the clip.`;
  return ["red", "triangle", SCORE_PROBLEM[r.score] || "Below the minimum", text];
}

function warnPills(c, cls = "sm") {
  const out = [];
  if (c.choppy_shutter) out.push(pill("amber", "triangle", "Fast shutter", "The shutter was much faster than the frame rate suits, so movement looks choppy.", cls));
  if (c.crooked) out.push(pill("amber", "triangle", "Crooked horizon", "At least one good stretch has a horizon a degree or more off level.", cls));
  if (c.flicker) out.push(pill("amber", "triangle", "Flicker", "Bands of light roll through the picture in at least one good stretch.", cls));
  if (c.jello) out.push(pill("amber", "triangle", "Jello", "The picture wobbles within the frame in at least one good stretch.", cls));
  for (const k of c.sound || []) out.push(pill("amber", "triangle", SOUND_WORDS[k] || k, null, cls));
  return out;
}

function clipTagsSmall(c) {
  const bits = [];
  if (c.light) bits.push(`<span class="tag sm" data-tip="From the sun's height when and where the clip was recorded">${icon("sun")}${esc(LIGHT_WORDS[c.light] || c.light)}</span>`);
  if (c.profile_is_log) bits.push(`<span class="tag sm" data-tip="Exposure is judged through this profile's curve">${esc(c.profile_label)}</span>`);
  if (c.talking) bits.push(`<span class="tag sm" data-tip="Someone talks in this clip">${icon("wave")}Talking</span>`);
  if (c.marks) bits.push(`<span class="tag sm">${icon("hand")}${plural(c.marks, "call")} by you</span>`);
  if (c.wb_note) bits.push(`<span class="tag sm"${tipAttr(c.wb_note)}>White balance differs</span>`);
  return bits.concat(warnPills(c)).join("");
}

function barHtml(c) {
  const d = c.duration || 1;
  let s = "";
  if (c.status === "analyzing" || c.status === "partial") s += `<span class="prog" style="width:${(c.progress * 100).toFixed(1)}%"></span>`;
  for (const k of c.kept) s += `<span style="left:${((k.start / d) * 100).toFixed(2)}%;width:${(((k.end - k.start) / d) * 100).toFixed(2)}%"></span>`;
  for (const [a, b] of c.used || []) s += `<span class="used" style="left:${((a / d) * 100).toFixed(2)}%;width:${(((b - a) / d) * 100).toFixed(2)}%"></span>`;
  return `<div class="bar" aria-hidden="true">${s}</div>`;
}

function selCell(c) {
  const kept = c.kept.length;
  const secs = c.kept.reduce((a, k) => a + (k.end - k.start), 0);
  const [tone, shape, word] = CLIP_STATUS[c.status] || CLIP_STATUS.not_analyzed;
  if (c.status === "unreadable" || c.status === "failed") return pill(tone, shape, word, `${c.status_detail || "It could not be read."} It is left out of the selects.`);
  if (!kept && (c.status === "missing" || c.status === "changed")) return pill(tone, shape, word, c.status_detail);
  if (!kept && (c.status === "waiting" || c.status === "not_analyzed")) return pill(tone, shape, c.status === "waiting" ? "Waiting" : "Not scored yet");
  if (!kept && (c.status === "analyzing" || c.status === "partial")) return `${barHtml(c)}${pill(tone, shape, c.status === "analyzing" ? `Analyzing ${pct(c.progress)}%` : word, c.status === "partial" ? "Analysis stopped before the end of this clip. Analyze footage picks it up again." : null)}`;
  if (!kept && c.no_select) { const [t, sh, w, why] = noSelectReason(c.no_select); return `${barHtml(c)}${pill(t, sh, w, why)}`; }
  if (!kept) return `${barHtml(c)}${pill("red", "triangle", "Nothing kept", "No stretch meets every minimum.")}`;
  const extra = [];
  if (c.over_limit) extra.push(`${c.over_limit} more over the limit`);
  if (c.set_aside) extra.push(`${c.set_aside} set aside`);
  if (c.used?.length) extra.push(`${plural(c.used.length, "shot")} in your edits`);
  const status = c.status === "analyzing" ? ` · analyzing ${pct(c.progress)}%` : c.status === "missing" ? " · file missing" : c.status === "changed" ? " · file changed" : "";
  return `${barHtml(c)}<span class="sel-text"${tipAttr(extra.length ? extra.join(". ") + "." : (c.status_detail || ""))}><strong>${kept}</strong> ${kept === 1 ? "select" : "selects"} · <span class="num">${fmtLen(secs)}</span>${status}</span>`;
}

const hasNone = (c) => !c.kept.length && ["analyzed", "missing", "changed", "unreadable", "failed"].includes(c.status);
function libRows() {
  const all = state.clips;
  if (ui.libTab === "with") return all.filter((c) => c.kept.length);
  if (ui.libTab === "none") return all.filter(hasNone);
  return all;
}

function listHtml() {
  const clips = state.clips;
  const analyzed = clips.filter((c) => c.status === "analyzed").length;
  const failed = state.summary?.clips_failed || 0;
  const tabs = [["all", "All", clips.length], ["with", "With selects", clips.filter((c) => c.kept.length).length], ["none", "Nothing kept", clips.filter(hasNone).length]]
    .map(([id, word, n]) => `<button role="tab" aria-selected="${ui.libTab === id}" class="${ui.libTab === id ? "on" : ""}" data-libtab="${id}">${word}<span class="n">${n}</span></button>`).join("");
  const rows = libRows().map((c) => {
    const th = canThumb(c) ? `<img class="thumb" alt="" data-thumb-id="${c.id}" data-thumb-t="${thumbTime(c)}">` : `<div class="thumb"></div>`;
    return `<div class="lib-row" role="button" tabindex="0" data-clip="${c.id}">
      ${th}
      <div style="min-width:0"><div class="clip-name">${esc(c.name)}</div><div class="clip-sub"><span class="where"${tipAttr(c.folder)}>${esc(c.group || c.folder)}</span>${clipTagsSmall(c)}</div></div>
      <span class="num">${c.duration ? fmtClock(c.duration) : "–"}</span>
      <span class="num rate">${c.rate ? esc(c.rate) : "–"}${c.variable_frame_rate ? "<small>Variable</small>" : ""}</span>
      <div class="sel-col">${selCell(c)}</div>
    </div>`;
  }).join("");
  const none = !rows ? `<div class="state"><div class="empty-small" style="max-width:420px"><strong>${ui.libTab === "with" ? "No clip has a select yet" : "Every clip has at least one select"}</strong>${ui.libTab === "with" ? "Lower a minimum on the Selects tab, or wait for analysis to finish." : "Nothing is left out at these settings."}</div></div>` : "";
  const banner = probe ? `<div class="banner-row"><span class="spinner" aria-hidden="true"></span>Reading new clips: <span class="num">${probe.done}</span> of <span class="num">${probe.total}</span></div>` : "";
  return `<div class="lib-head">
      <div class="lib-title"><h1>All clips</h1><span class="count">${plural(clips.length, "clip")} · ${analyzed} analyzed${failed ? ` · ${failed} cannot be read` : ""}</span>
        <div class="right"><button class="btn sec sm" data-act="add-folder">${icon("folder")}Add folder</button><button class="btn sec sm" data-act="add-files">${icon("plus")}Add clips</button></div></div>
      <div class="tabs" role="tablist" aria-label="Which clips">${tabs}</div>
    </div>
    ${banner}
    <div class="lib-cols"><span></span><span>Clip</span><span>Length</span><span>Rate</span><span>Selects</span></div>
    <div class="scroll" id="lib-scroll">${rows}${none}</div>`;
}

/* ---------- main panel: shoot report ---------- */

let report = null, reportError = null;

async function openReport() {
  stopPlay();
  view = { mode: "report", clipId: null };
  report = null; reportError = null;
  renderMain();
  try { report = await invoke("shoot_report"); } catch (e) { reportError = String(e); }
  if (view.mode === "report") renderMain();
}

const TONE_PILL = { good: ["ok", "tick", "Good"], warn: ["amber", "triangle", "Worth fixing"], info: ["grey", "dash", "Note"] };

function renderReport() {
  const panel = $("main-panel");
  const head = (inner) => `<div class="report-head"><button class="back" data-act="back">${icon("back")}All clips</button><h1>Shoot report</h1>${inner || ""}</div>`;
  if (reportError) { panel.innerHTML = head() + errorHtml("The report could not be made", reportError, "Nothing was changed. Go back and try again."); return; }
  if (!report) { panel.innerHTML = head() + loadingHtml("Going through the shoot"); return; }
  if (!report.clips_analyzed) {
    panel.innerHTML = head() + `<div class="state"><div class="empty-small" style="max-width:460px"><strong>Nothing to report yet</strong>The report is made from analyzed clips. Add footage and let it analyze, then open the report again.</div></div>`;
    return;
  }
  const cards = report.findings.map((f) => {
    const [tone, shape, word] = TONE_PILL[f.tone] || TONE_PILL.info;
    const rows = f.rows?.length ? `<table class="fit-table"><thead><tr><th>Group</th><th class="num">Clips</th><th>Result</th></tr></thead><tbody>${f.rows.map(([a, n, v]) => `<tr><td>${esc(a)}</td><td class="num">${n}</td><td>${esc(v)}</td></tr>`).join("")}</tbody></table>` : "";
    return `<article class="finding">${pill(tone, shape, word)}<h3>${esc(f.title)}</h3><p>${esc(f.body)}</p>${rows}</article>`;
  }).join("");
  panel.innerHTML = head(`<div class="meta"><span><span class="num">${report.clips_analyzed}</span> of ${plural(report.clips, "clip")} analyzed</span><span><span class="num">${fmtDur(report.footage_seconds)}</span> of footage</span><span><span class="num">${report.selects}</span> selects, <span class="num">${fmtDur(report.selects_seconds)}</span></span></div>
    <div class="right"><button class="btn sec sm" data-act="save-report">${icon("down")}Save as a page</button></div>`) + `<div class="scroll"><div class="findings">${cards}</div></div>`;
}

async function saveReport() {
  try { const path = await invoke("save_report"); if (path) toast(`${icon("tick")} Saved to ${esc(path)}`); }
  catch (e) { showError("The report was not saved", String(e)); }
}

function renderMain() {
  const panel = $("main-panel");
  if (view.mode === "detail") { renderDetail(); return; }
  if (view.mode === "report") { renderReport(); return; }
  if (toolError) { panel.innerHTML = errorHtml("The video tools could not start", toolError, `Nothing was analyzed. Reinstall ${appName()} from the download page, then open it again.`); return; }
  if (loadError) { panel.innerHTML = errorHtml(`${AppName()} could not load its state`, loadError, "Nothing was lost. Close the app and open it again."); return; }
  if (state === null) { panel.innerHTML = loadingHtml(`Opening ${appName()}`); return; }
  if (state.clips.length === 0) {
    panel.innerHTML = probe ? loadingHtml(probe.total ? `Reading clips: ${probe.done} of ${probe.total}` : "Looking for video files") : emptyHtml();
    return;
  }
  const scroll = $("lib-scroll")?.scrollTop || 0;
  panel.innerHTML = listHtml();
  const sc = $("lib-scroll");
  if (sc) sc.scrollTop = scroll;
  hydrateThumbs(panel);
  if (tipFor && !tipFor.isConnected) hideTip();
}

/* ---------- one clip: words for each stretch ---------- */

function whyText(seg, sel, cap) {
  switch (seg.status) {
    case "kept":
      if (seg.manual) return `Kept by you${seg.stars ? `, ${starText(seg.stars)}` : ""}. It stays whatever the settings say.`;
      if (seg.camera_move && seg.end - seg.start > sel.max_length + 0.01) return "Kept whole: one camera move, longer than the longest select.";
      return seg.camera_move ? "Every score is at or above its minimum. Camera move." : "Every score is at or above its minimum.";
    case "below_threshold": {
      const names = seg.failed.map((f) => `${f} (${pct(sel[f + "_min"])})`);
      const list = names.length > 1 ? names.slice(0, -1).join(", ") + " and " + names[names.length - 1] : names[0];
      const issue = seg.failed.includes("exposure") && seg.exposure_issue ? ` ${ISSUE_WORDS[seg.exposure_issue] || ""}` : "";
      return `Left out: ${list} under the minimum.${issue}`;
    }
    case "too_short": return `Good, but shorter than the ${fmtDur(sel.min_length)} minimum.`;
    case "over_limit": return `Good, but ${plural(cap, "better select is", "better selects are")} already kept in this clip.`;
    case "set_aside":
      if (seg.aside === "repeat" && seg.similar_to) return `Set aside: looks like the select in ${clipName(seg.similar_to.clip)} at ${fmtClock(seg.similar_to.start)}, which scored higher.`;
      if (seg.aside === "dropped") return "Dropped by you. The clip's next best stretch can take its place.";
      if (seg.aside === "light") return `Set aside: the light filter asks for ${LIGHT_FILTER_WORDS[sel.light] || "another light"}, and this clip ${detail?.scene?.light ? `was recorded in ${(LIGHT_WORDS[detail.scene.light] || "").toLowerCase()}` : "has no known time and place to judge the light by"}.`;
      if (seg.aside === "sound") return "Set aside: its sound has a problem, and the sound filter sets those aside.";
      return `Set aside: the camera filter asks for ${FILTER_WORDS[sel.camera] || "another movement"}, and this stretch is ${(CAMERA_WORDS[seg.camera] || "moving").toLowerCase()}.`;
  }
  return "";
}

/** A short word for a stretch, with its tone and shape. */
function segLabel(seg, num) {
  if (seg.status === "kept") return seg.manual ? ["ok", "hand", `Select ${num}, kept by you`] : ["ok", "tick", `Select ${num}`];
  if (seg.status === "below_threshold") return ["red", "triangle", seg.failed.map((k) => (k === "exposure" ? ISSUE_SHORT[seg.exposure_issue] || FAIL_SHORT.exposure : FAIL_SHORT[k])).join(" · ")];
  if (seg.status === "too_short") return ["grey", "dash", "Too short"];
  if (seg.status === "over_limit") return ["amber", "stack", "Over the limit"];
  if (seg.aside === "dropped") return ["red", "cross", "Dropped by you"];
  if (seg.aside === "repeat") return ["grey", "aside", "Repeat take"];
  if (seg.aside === "light") return ["grey", "aside", "Light filter"];
  if (seg.aside === "sound") return ["grey", "aside", "Sound filter"];
  return ["grey", "aside", "Camera filter"];
}

/** A note for a kept or over-limit stretch that looks like a better select elsewhere. */
function repeatNote(seg) {
  if (!seg.similar_to || seg.status === "set_aside") return "";
  return ` Looks like the select in ${clipName(seg.similar_to.clip)} at ${fmtClock(seg.similar_to.start)}.`;
}

/** Problems found in a stretch that passed the minimums: [short word, full sentence]. */
function checkNotes(seg) {
  if (seg.status === "below_threshold") return [];
  const out = [];
  if (seg.roll != null && Math.abs(seg.roll) >= 1) out.push([`Horizon ${Math.abs(seg.roll).toFixed(1)}°`, `The horizon tilts down to the ${seg.roll > 0 ? "right" : "left"} by ${Math.abs(seg.roll).toFixed(1)}°; rotate ${seg.roll > 0 ? "counter-clockwise" : "clockwise"} to level it.`]);
  if (seg.flicker != null && seg.flicker >= 0.012) out.push(["Flicker", "Light flicker: bands roll through the picture."]);
  if (seg.wobble != null && seg.wobble >= 0.35) out.push(["Jello", "Jello: the picture wobbles within the frame."]);
  for (const k of seg.sound || []) out.push([SOUND_WORDS[k] || k, `${SOUND_WORDS[k] || k} in this stretch.`]);
  if (seg.status === "kept" && seg.speech_cuts) {
    const why = localSel?.snap_to_pauses === false ? ", since Cut on pauses in talking is off." : ": no pause was close enough.";
    out.push(["Cuts mid-sentence", (seg.speech_cuts === 2 ? "Starts and ends mid-sentence" : "Cuts mid-sentence") + why]);
  }
  return out;
}

/** A drop call that covers most of this stretch, when the stretch itself shows no sign of it. */
function idleDrop(s, marks) {
  if (s.manual || s.aside === "dropped") return null;
  return (marks || []).find((m) => m.call === "drop" && Math.min(m.end, s.end) - Math.max(m.start, s.start) >= 0.5 * Math.min(s.end - s.start, m.end - m.start)) || null;
}

function usedIn(d, s) {
  const names = (d.used || []).filter(([a, b]) => Math.min(b, s.end) - Math.max(a, s.start) > 0.25).map(([, , n]) => n);
  return [...new Set(names)];
}

/** Everything known about a stretch, as sentences, for its tooltip. */
function stretchTip(d, s, cap) {
  const bits = [whyText(s, d.selection, cap) + repeatNote(s)];
  for (const [, full] of checkNotes(s)) bits.push(full);
  if (idleDrop(s, d.marks)) bits.push("You dropped it by hand too; that changes nothing while it is not a select, and keeps it out if new settings would choose it.");
  const used = usedIn(d, s);
  if (used.length) bits.push(`Used in ${used.join(", ")}.`);
  const isLog = d.profile && d.profile.profile !== "standard";
  bits.push(`Focus ${pct(s.focus)} · Exposure ${pct(s.exposure)} · Stability ${pct(s.stability)}${isLog && s.key_stops != null ? ` · ${stopsWords(s.key_stops)}` : ""} · ${CAMERA_WORDS[s.camera] || "Still"}`);
  return bits.join("\n");
}

const fmtWhen = (utc, offset) => {
  if (utc == null) return "";
  const d = new Date((utc + (offset || 0) * 60) * 1000);
  const p2 = (n) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${p2(d.getUTCMonth() + 1)}-${p2(d.getUTCDate())} ${p2(d.getUTCHours())}:${p2(d.getUTCMinutes())}`;
};
const resLabel = (w, h) => (Math.max(w, h) >= 3800 ? "4K" : Math.max(w, h) >= 2600 ? "2.7K" : Math.min(w, h) >= 1060 ? "1080p" : Math.min(w, h) >= 700 ? "720p" : `${w}×${h}`);
const rateText = (r) => (r.den === 1 ? String(r.num) : (r.num / r.den).toFixed(3));

/* ---------- one clip: page ---------- */

function detailTags(d, row) {
  const info = d.info, bits = [];
  if (info) {
    bits.push(tag(fmtClock(info.duration)));
    bits.push(tag(resLabel(info.width, info.height), null, `${info.width}×${info.height}${info.rotation ? `, rotated ${info.rotation}°` : ""}`));
    bits.push(tag(`${row.rate || rateText(info.rate)} fps`, null, info.variable_frame_rate ? "Variable frame rate" : null));
  }
  if (row.profile_is_log) bits.push(tag(row.profile_label, null, "Exposure is judged through this profile's curve"));
  if (d.scene?.group) bits.push(tag(d.scene.group));
  if (d.scene?.light) bits.push(tag(LIGHT_WORDS[d.scene.light], "sun", `Sun ${Math.abs(d.scene.sun).toFixed(0)}° ${d.scene.sun >= 0 ? "above" : "below"} the horizon`));
  if ((d.talking || []).length) bits.push(tag("Talking", "wave"));
  if (row.status !== "analyzed") {
    const [tone, shape, word] = CLIP_STATUS[row.status] || CLIP_STATUS.not_analyzed;
    bits.push(pill(tone, shape, row.status === "analyzing" ? `${word} ${pct(row.progress)}%` : word, row.status_detail));
  }
  return bits.concat(warnPills(row, "")).join("");
}

function infoPopover(d) {
  const info = d.info, rows = [];
  const add = (k, v) => { if (v != null && v !== "") rows.push(`<dt>${esc(k)}</dt><dd>${v}</dd>`); };
  if (info) {
    add("Length", `<span class="num">${fmtClock(info.duration)}</span>`);
    add("Resolution", `<span class="num">${info.width} × ${info.height}</span>${info.rotation ? `, rotated ${info.rotation}°` : ""}`);
    add("Frame rate", `<span class="num">${esc(rateText(info.rate))}</span> fps${info.variable_frame_rate ? ", variable" : ""}`);
    add("Codec", esc(info.codec.toUpperCase()));
    add("Audio", info.has_audio ? plural(info.audio_channels, "channel") : "None");
    add("Timecode", info.timecode ? `<span class="num">${esc(info.timecode)}</span>` : "None, starts at 00:00:00:00");
  }
  const sc = d.scene;
  if (sc) {
    if (sc.utc != null) add("Recorded", `<span class="num">${fmtWhen(sc.utc, sc.offset_minutes)}</span> local${sc.zone_estimated ? ", time zone estimated" : ""}`);
    else if (sc.wall != null) add("Recorded", `<span class="num">${fmtWhen(sc.wall, 0)}</span> by the camera's clock, time zone unknown`);
    add("Scene", sc.group ? esc(sc.group) : null);
    add("Light", sc.light ? `${esc(LIGHT_WORDS[sc.light])}, sun <span class="num">${Math.abs(sc.sun).toFixed(0)}°</span> ${sc.sun >= 0 ? "above" : "below"} the horizon` : sc.utc == null && sc.wall == null ? "Unknown, no recording time" : "Unknown, no place recorded");
    const src = [];
    if (sc.time_source) src.push(`time from ${esc(sc.time_source)}`);
    if (sc.place_source) src.push(`place from ${esc(sc.place_source)}`);
    add("From", src.join(", "));
  }
  const n = d.camera;
  if (n) {
    add("Camera", [n.camera, n.lens].filter(Boolean).map(esc).join(", "));
    const set = [];
    if (n.iso) set.push(`ISO <span class="num">${Math.round(n.iso)}</span>`);
    if (n.shutter) set.push(`<span class="num">1/${Math.round(1 / n.shutter)}</span>`);
    if (n.aperture) set.push(`<span class="num">f/${n.aperture.toFixed(1)}</span>`);
    add("Settings", set.join(" · "));
    if (n.max_height != null) add("Height", `Up to <span class="num">${Math.round(n.max_height)}</span> m above take-off`);
    if (n.latitude != null && n.longitude != null) add("Location", `<span class="num">${n.latitude.toFixed(5)}, ${n.longitude.toFixed(5)}</span>`);
  }
  const sm = d.sound;
  if (sm) {
    if (sm.silent) add("Sound", "Silent");
    else {
      add("Talking", sm.has_speech ? `<span class="num">${Math.round(sm.talk_share * 100)}%</span> of the clip` : "None found");
      add("Sound", `Around <span class="num">${Math.round(sm.median_db)}</span> dB, background <span class="num">${Math.round(sm.noise_floor_db)}</span> dB`);
    }
  }
  const ks = (d.raw.key_stops || []).filter((v) => v != null && isFinite(v));
  if (ks.length && d.profile?.profile !== "standard") {
    const cl = d.raw.clipped || [], clipped = cl.length ? cl.reduce((a, b) => a + b, 0) / cl.length : 0;
    add("Brightness", `On average ${stopsWords(ks.reduce((a, b) => a + b, 0) / ks.length)}${clipped > 0.0005 ? `, ${sharePct(clipped)} clipped` : ""}`);
  }
  if (d.profile) add("Profile", `${esc(profileLabel(d.profile.profile))}. <span class="muted">${esc(d.profile.explanation)}</span>`);
  if (d.sample_rate) add("Scored", `${plural(d.sample_rate, "frame")} per second${d.analysis_seconds ? ` in ${fmtDur(d.analysis_seconds)}` : ""}`);
  if (d.decoder) add("Decoded", esc(d.decoder[0].toUpperCase() + d.decoder.slice(1)));
  add("File", `<span class="selectable">${esc(d.path)}</span>`);

  const notes = [];
  if (d.shutter_warning) notes.push([pill("amber", "triangle", "Fast shutter"), d.shutter_warning]);
  if (sc?.wb_note) notes.push([pill("grey", "dash", "White balance"), sc.wb_note]);
  if (sc && !sc.light && (sc.utc != null || sc.wall != null)) notes.push([pill("grey", "dash", "Light unknown"), "No place found, so the light cannot be worked out. Add where you shot on the Project tab."]);
  if (d.audio_error) notes.push([pill("amber", "triangle", "Sound not checked"), d.audio_error]);
  if (sm && !sm.silent) {
    if (sm.wind_share >= 0.2) notes.push([pill("amber", "triangle", "Wind noise"), `Wind for ${Math.round(sm.wind_share * 100)}% of the clip.`]);
    if (sm.clipped_events >= 3) notes.push([pill("amber", "triangle", "Sound clipped"), `The sound distorts ${sm.clipped_events} times: it was recorded too loud.`]);
    if (sm.dropouts?.length) notes.push([pill("amber", "triangle", "Sound drops out"), `The sound cuts out ${plural(sm.dropouts.length, "time")}, first at ${fmtClock(sm.dropouts[0][0])}.`]);
  }
  const notesHtml = notes.length ? `<div class="notes">${notes.map(([p, t]) => `<div class="note">${p}<span>${esc(t)}</span></div>`).join("")}</div>` : "";
  return `<div class="popover" id="info-pop" role="dialog" aria-label="Clip details">
    <div class="popover-head"><span class="eyebrow">Clip details</span><button data-act="toggle-info">Close</button></div>
    <dl class="kv">${rows.join("")}</dl>${notesHtml}</div>`;
}

function profileSelect(d) {
  if (!d.profile || !state?.profiles) return "";
  const p = d.profile;
  const opts = [`<option value="auto">Automatic${d.chosen_profile ? "" : ` (${esc(profileLabel(p.profile))})`}</option>`].concat(state.profiles.map((o) => `<option value="${o.id}"${d.chosen_profile === o.id ? " selected" : ""}>${esc(o.label)}</option>`)).join("");
  let note = "";
  if (p.profile === "d_log_m") note = " DJI has not published the D-Log M curve, so D-Log's curve stands in and readings in stops are approximate.";
  else if (p.profile === "generic_log") note = " An average of four published log curves stands in, so readings in stops are approximate.";
  const how = p.profile === "standard"
    ? "Judged on how the picture looks: blown highlights, crushed shadows and flat contrast count against it."
    : "Judged in stops through the camera's curve: clipped highlights and underexposure count against it, the flat log look does not.";
  return `<div class="t-profile"><label for="clip-profile">Profile</label><select class="input sm" id="clip-profile">${opts}</select>${infoI(`Exposure judged as ${profileLabel(p.profile)}. ${p.explanation}${note}\n${how}`, "About the profile")}</div>`;
}

function axisHtml(dur) {
  const steps = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
  const step = steps.find((s) => dur / s <= 10) || 3600;
  let s = "";
  for (let t = 0; t <= dur + 1e-6; t += step) {
    const p = (t / dur) * 100;
    if (p > 96 && t > 0) break;
    s += `<span style="left:${p.toFixed(2)}%;${t ? "transform:translateX(-50%)" : ""}">${fmtClock(t).replace(/\.0$/, "")}</span>`;
  }
  return `<div class="axis" aria-hidden="true">${s}</div>`;
}

/** One score as a thin curve: red where it falls under its minimum, hatched where not measured yet. */
function laneSvg(key, d) {
  const dur = d.duration || 1, ts = d.smoothed.t, vals = d.smoothed[key], thr = d.selection[key + "_min"];
  const x = (t) => ((t / dur) * 1000).toFixed(1), y = (v) => (96 - Math.max(0, Math.min(1, v)) * 92).toFixed(1);
  let line = "", area = "", bands = "", red = "";
  vals.forEach((v, i) => { line += `${i ? "L" : "M"}${x(ts[i])},${y(v)}`; });
  if (ts.length) area = `M${x(ts[0])},100${line.replace(/^M/, "L")}L${x(ts[ts.length - 1])},100Z`;
  let i = 0;
  const n = vals.length;
  while (i < n) {
    const low = vals[i] < thr;
    let j = i;
    while (j + 1 < n && (vals[j + 1] < thr) === low) j++;
    if (low) {
      const a = i ? (ts[i - 1] + ts[i]) / 2 : 0, b = j + 1 < n ? (ts[j] + ts[j + 1]) / 2 : Math.min(dur, d.measured_until);
      bands += `M${x(a)},0H${x(b)}V100H${x(a)}Z`;
      let p = "";
      for (let q = Math.max(0, i - 1); q <= Math.min(n - 1, j + 1); q++) p += `${p ? "L" : "M"}${x(ts[q])},${y(vals[q])}`;
      red += p;
    }
    i = j + 1;
  }
  const later = d.measured_until < dur - 0.01 ? `<path class="later" d="M${x(d.measured_until)},0H1000V100H${x(d.measured_until)}Z"/>` : "";
  return `<svg viewBox="0 0 1000 100" preserveAspectRatio="none" aria-hidden="true">${later}${bands ? `<path class="band" d="${bands}"/>` : ""}${area ? `<path class="area" d="${area}"/>` : ""}<path class="thr" d="M0,${y(thr)}H1000"/>${line ? `<path class="line" d="${line}"/>` : ""}${red ? `<path class="red" d="${red}"/>` : ""}</svg>`;
}

const STRIP_FRAMES = 12;

function timelineHtml(d) {
  const dur = d.duration || 1, P = (t) => `${((t / dur) * 100).toFixed(3)}%`, W = (a, b) => `${(((b - a) / dur) * 100).toFixed(3)}%`;
  let n = 0, overlays = "";
  for (const s of d.segments) {
    if (s.status === "kept") {
      n++;
      const narrow = (s.end - s.start) / dur < 0.04 ? " narrow" : "";
      overlays += `<div class="kept${narrow}" style="left:${P(s.start)};width:${W(s.start, s.end)}"><span>${icon(s.manual ? "hand" : "tick")}${n}</span></div>`;
    } else overlays += `<div class="dim" style="left:${P(s.start)};width:${W(s.start, s.end)}"></div>`;
  }
  if (d.measured_until < dur - 0.01) overlays += `<div class="unmeasured" style="left:${P(d.measured_until)}"></div>`;
  for (const [a, b] of d.used || []) overlays += `<div class="used" style="left:${P(a)};width:${W(a, b)}"></div>`;
  const frames = Array.from({ length: STRIP_FRAMES }, (_, j) => `<canvas width="192" height="108" data-a="${((j / STRIP_FRAMES) * dur).toFixed(3)}" data-b="${(((j + 1) / STRIP_FRAMES) * dur).toFixed(3)}"></canvas>`).join("");
  const talking = d.talking || [];
  const talk = talking.length ? `<div class="talk" aria-hidden="true">${talking.map(([a, b]) => `<span style="left:${P(a)};width:${W(a, b)}"></span>`).join("")}</div>` : "";
  const mean = (k) => { const v = d.smoothed[k]; return v.length ? pct(v.reduce((a, b) => a + b, 0) / v.length) : 0; };
  const usedNote = (d.used || []).length ? " Amber marks along the bottom are shots used in your finished videos." : "";
  const labels = `<div class="l-strip"><span${tipAttr(`Green outlines are the selects, numbered in order. Dimmed parts are not used; hover a chip below the selects to see why.${usedNote}`)}>Selects</span></div>
    ${talk ? `<div class="l-talk"><span data-tip="Where someone talks. Selects start and end in the gaps.">Talking</span></div>` : ""}
    ${KEYS.map((k) => `<div class="l-lane"><span${tipAttr(`${TIPS[k]}\nAverage ${mean(k)}, minimum ${pct(d.selection[k + "_min"])}. The dashed line is the minimum.`)}>${SCORE_WORD[k]}</span><span class="num" data-lane-value="${k}">–</span></div>`).join("")}
    <div class="l-axis"></div>`;
  return `<div class="timeline">
    <div class="tl-labels">${labels}</div>
    <div class="track" id="track">
      <div class="strip" id="strip">${frames}${overlays}</div>
      ${talk}
      ${KEYS.map((k) => `<div class="lane">${laneSvg(k, d)}</div>`).join("")}
      ${axisHtml(dur)}
      <div class="hover-line" id="hover-line" hidden></div>
      <div class="playhead" id="playhead"></div>
    </div></div>`;
}

function cardsHtml(d) {
  const kept = d.segments.filter((s) => s.status === "kept");
  const secs = kept.reduce((a, s) => a + (s.end - s.start), 0);
  const cap = kept.length;
  let cards = "";
  kept.forEach((s, i) => {
    const m = (s.start + s.end) / 2, mark = (d.marks || []).find((k) => k.call === "keep" && m >= k.start && m < k.end);
    const stars = s.stars || mark?.stars || 0;
    const meters = KEYS.map((k) => `<div class="meter"><span class="l">${SCORE_WORD[k]}</span><div class="t"><i class="${s[k] < d.selection[k + "_min"] ? "low" : ""}" style="width:${pct(s[k])}%"></i></div><span class="v num">${pct(s[k])}</span></div>`).join("");
    const notes = checkNotes(s).map(([w, full]) => pill("amber", "triangle", w, full, "sm"));
    if (s.similar_to) notes.push(`<span class="tag sm"${tipAttr(repeatNote(s).trim())}>${icon("stack")}Similar take</span>`);
    const used = usedIn(d, s);
    if (used.length) notes.push(`<span class="tag sm"${tipAttr(`Used in ${used.join(", ")}`)}>${icon("film")}In your edits</span>`);
    if (s.camera_move && s.end - s.start > d.selection.max_length + 0.01) notes.push(`<span class="tag sm" data-tip="One camera move, kept whole although it is longer than the longest select.">Whole move</span>`);
    if (s.manual) notes.push(`<span class="by-you">${icon("hand")}Kept by you</span>`);
    cards += `<button class="card-sel" data-seek="${m}" data-a="${s.start}" data-b="${s.end}" aria-label="Select ${i + 1}, ${range(s.start, s.end)}">
      <div class="card-thumb"><img alt="" data-thumb-id="${d.id}" data-thumb-t="${Math.round(m * 10) / 10}">
        <span class="badge">${icon("tick")}Select ${i + 1}</span>
        ${stars ? `<span class="rating">${icon("star")}${stars}</span>` : ""}
        <span class="len">${fmtLen(s.end - s.start)}</span></div>
      <div class="card-body">
        <div class="card-line"><span class="num">${range(s.start, s.end)}</span><span class="cam">${esc(CAMERA_WORDS[s.camera] || "Still")}</span></div>
        <div class="meters">${meters}</div>
        ${notes.length ? `<div class="card-notes">${notes.join("")}</div>` : ""}
      </div></button>`;
  });
  const sel = d.selection;
  const noneHint = sel.light && sel.light !== "any" && !d.scene?.light ? "This clip has no known light, so the light filter sets it aside."
    : d.segments.some((s) => s.status === "too_short") ? "Some stretches are good but short. Lower the shortest select length to keep them."
      : "Lower a minimum on the Selects tab to keep more.";
  const others = d.segments.filter((s) => s.status !== "kept");
  const chips = others.map((s) => {
    const [tone, shape, word] = segLabel(s);
    return `<button class="unused" data-seek="${(s.start + s.end) / 2}"${tipAttr(stretchTip(d, s, cap))}><span class="num">${range(s.start, s.end)}</span><span class="word tone-${tone}">${icon(shape)}${esc(word)}</span></button>`;
  });
  if (d.measured_until < d.duration - 0.01) chips.push(`<span class="unused" style="cursor:default"><span class="num">${range(d.measured_until, d.duration)}</span><span class="word tone-grey">${icon("clock")}Not scored yet</span></span>`);
  return `<div class="selects">
    <div class="selects-head"><h2>Selects</h2><span class="num">${kept.length ? `${kept.length} · ${fmtLen(secs)} kept` : "none"}</span></div>
    ${kept.length ? `<div class="cards">${cards}</div>` : `<div class="empty-small"><strong>Nothing kept at these settings</strong>${esc(noneHint)}</div>`}
    ${chips.length ? `<div class="unused-wrap"><span class="eyebrow">Not used · ${others.length}</span><div class="unused-list">${chips.join("")}</div></div>` : ""}
  </div>`;
}

function clipNeighbours() {
  const ids = state?.clips?.map((c) => c.id) || [];
  const i = ids.indexOf(view.clipId);
  return { i, n: ids.length, prev: i > 0 ? ids[i - 1] : null, next: i >= 0 && i < ids.length - 1 ? ids[i + 1] : null };
}

function renderDetail() {
  const panel = $("main-panel");
  const row = state?.clips.find((c) => c.id === view.clipId);
  const backBtn = `<button class="back" data-act="back">${icon("back")}All clips</button>`;
  if (detailError) { panel.innerHTML = `<div class="detail-head">${backBtn}</div>` + errorHtml("This clip could not be shown", detailError, "Nothing was changed. Go back to the list and try again."); return; }
  if (!detail || !row) { panel.innerHTML = `<div class="detail-head">${backBtn}</div>` + loadingHtml("Loading scores"); return; }

  const d = detail, nb = clipNeighbours();
  const head = `<div class="detail-head">${backBtn}
      <div class="detail-title"><h1 class="selectable"${tipAttr(d.name)}>${esc(d.name)}</h1><button class="info-i lg${ui.info ? " on" : ""}" data-act="toggle-info" aria-expanded="${ui.info}" aria-label="Clip details" data-tip="Clip details">${icon("info")}</button></div>
      <div class="detail-tags">${detailTags(d, row)}</div>
      <div class="clip-nav"><button class="iconbtn" data-act="prev-clip" aria-label="Previous clip" data-tip="Previous clip"${nb.prev == null ? " disabled" : ""}>${icon("left")}</button><span class="num">${nb.i + 1} of ${nb.n}</span><button class="iconbtn" data-act="next-clip" aria-label="Next clip" data-tip="Next clip"${nb.next == null ? " disabled" : ""}>${icon("right")}</button></div>
      ${ui.info ? infoPopover(d) : ""}
    </div>`;

  let body;
  if (!d.raw.t.length) {
    body = row.status === "unreadable" || row.status === "failed"
      ? errorHtml("This clip has no scores", row.status_detail || "It could not be read.", "Other clips are not affected. Check the file, then add it again.")
      : row.status === "waiting" || row.status === "analyzing"
        ? loadingHtml("Waiting for the first scores from this clip")
        : `<div class="state"><div class="empty-small" style="max-width:420px"><strong>No scores yet</strong>This clip has not been analyzed. Use Analyze footage above.</div></div>`;
  } else {
    const modes = VIEW_MODES.map(([id, word]) => `<button role="radio" aria-checked="${viewer.mode === id}" data-viewmode="${id}">${word}</button>`).join("");
    const kept = d.segments.some((s) => s.status === "kept");
    body = `<div class="viewer" id="viewer">
        <canvas id="v-canvas" aria-label="The clip at the playhead"></canvas>
        <div class="viewer-state" id="v-state"></div>
        <div class="v-top-left"><span class="v-tc" id="v-tc"></span><span id="v-cur"></span></div>
        <div class="mode-ctl" role="radiogroup" aria-label="How the frame is shown">${modes}</div>
        <div class="v-legend" id="v-legend"></div>
      </div>
      <div class="transport">
        <div class="t-group">
          <button class="iconbtn lg" data-act="prev-select" aria-label="Previous select" data-tip="Previous select   ["${kept ? "" : " disabled"}>${icon("prevSel")}</button>
          <button class="iconbtn play" id="t-play" data-act="play" aria-label="Play or pause" data-tip="Play or pause   Space\nShows stills as fast as they can be read, without sound.">${icon(viewer.playing ? "pause" : "play")}</button>
          <button class="iconbtn lg" data-act="next-select" aria-label="Next select" data-tip="Next select   ]"${kept ? "" : " disabled"}>${icon("nextSel")}</button>
          <span class="t-time num"><b id="t-tc">${fmtClock(viewer.pin)}</b><span> / ${fmtClock(d.duration)}</span></span>
        </div>
        <span class="rule"></span>
        <div class="calls" id="calls"></div>
        ${profileSelect(d)}
      </div>
      ${timelineHtml(d)}
      ${cardsHtml(d)}`;
  }
  const scroll = $("detail-scroll")?.scrollTop || 0;
  panel.innerHTML = `<div class="scroll" id="detail-scroll">${head}${body}</div>`;
  $("detail-scroll").scrollTop = scroll;
  if (tipFor && !tipFor.isConnected) hideTip();
  if (!d.raw.t.length) return;
  attachTrack(d);
  paintViewer();
  paintPlayhead();
  paintPoint(viewer.pin, false);
  loadStrip();
  hydrateThumbs(panel);
}

/* ---------- one clip: the playhead, live values and the frame strip ---------- */

function nearestIndex(ts, t) {
  let lo = 0, hi = ts.length - 1;
  while (lo < hi) { const m = (lo + hi) >> 1; if (ts[m] < t) lo = m + 1; else hi = m; }
  if (lo > 0 && Math.abs(ts[lo - 1] - t) < Math.abs(ts[lo] - t)) lo--;
  return lo;
}

/** The timecode, result and scores at moment t: on the viewer and beside each lane. */
function paintPoint(t, live) {
  const d = detail, tc = $("v-tc");
  if (!d || !tc || !d.smoothed.t.length) return;
  const i = nearestIndex(d.smoothed.t, t);
  const kept = d.segments.filter((s) => s.status === "kept");
  const seg = d.segments.find((s) => t >= s.start && t < s.end);
  tc.textContent = fmtClock(t);
  const cur = $("v-cur");
  if (seg && t <= d.measured_until) {
    const [tone, shape, word] = segLabel(seg, kept.indexOf(seg) + 1);
    cur.innerHTML = pill(tone, shape, word, stretchTip(d, seg, kept.length));
  } else {
    cur.innerHTML = pill("grey", "clock", "Not scored yet", "This part of the clip has not been scored yet.");
  }
  if (tipFor && !tipFor.isConnected) hideTip();
  for (const key of KEYS) {
    const v = d.smoothed[key][i], low = v < d.selection[key + "_min"];
    const out = document.querySelector(`[data-lane-value="${key}"]`);
    if (out) { out.textContent = String(pct(v)); out.classList.toggle("low", low); }
  }
  for (const cv of document.querySelectorAll("#strip canvas")) cv.classList.toggle("cur", t >= +cv.dataset.a && t < +cv.dataset.b);
  void live;
}

/** Moves the playhead and everything tied to it, without drawing the page again. */
function paintPlayhead() {
  const d = detail;
  if (!d) return;
  const ph = $("playhead");
  if (ph) ph.style.left = `${((viewer.pin / (d.duration || 1)) * 100).toFixed(3)}%`;
  const tc = $("t-tc");
  if (tc) tc.textContent = fmtClock(viewer.pin);
  for (const c of document.querySelectorAll(".card-sel")) c.classList.toggle("active", viewer.pin >= +c.dataset.a && viewer.pin < +c.dataset.b);
  const calls = $("calls");
  if (calls) { const h = callsHtml(); if (calls.dataset.html !== h) { calls.innerHTML = h; calls.dataset.html = h; } }
}

function attachTrack(d) {
  const track = $("track");
  if (!track) return;
  const timeAt = (e) => { const r = track.getBoundingClientRect(); return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * (d.duration || 1); };
  track.addEventListener("mousemove", (e) => {
    const t = timeAt(e), hl = $("hover-line");
    hl.hidden = false;
    hl.style.left = `${((t / (d.duration || 1)) * 100).toFixed(3)}%`;
    paintPoint(t, true);
    if (!viewer.playing) showFrameAt(t);
  });
  track.addEventListener("mouseleave", () => {
    $("hover-line").hidden = true;
    paintPoint(viewer.pin, false);
    if (!viewer.playing) showFrameAt(viewer.pin);
  });
  track.addEventListener("click", (e) => pinAt(timeAt(e)));
}

// Small frames across each clip, read once per clip and kept for the last few clips opened.
const stripCache = new Map();
function paintStrip() {
  const entry = stripCache.get(view.clipId), box = $("strip");
  if (!box || !entry) return;
  box.querySelectorAll("canvas").forEach((cv, j) => { if (entry.frames[j]) cv.getContext("2d").drawImage(entry.frames[j], 0, 0, cv.width, cv.height); });
}
async function loadStrip() {
  const d = detail, id = view.clipId;
  if (!d || !d.raw.t.length || !$("strip")) return;
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
      try { entry.frames[j] = scaledCanvas(parseStill(await invoke("clip_frame", { id, t: ((j + 0.5) / STRIP_FRAMES) * d.duration })), 192, 108); }
      catch (_) { entry.failed = true; break; }
      if (view.clipId === id) paintStrip();
    }
  } finally {
    entry.busy = false;
  }
  if (view.clipId === id) paintStrip();
}

async function openDetail(id) {
  stopPlay();
  view = { mode: "detail", clipId: id };
  detail = null; detailError = null;
  viewer.clipId = null;
  ui.info = false;
  renderMain();
  renderFoot();
  await refreshDetail();
}

async function refreshDetail() {
  if (view.mode !== "detail") return;
  const id = view.clipId;
  try {
    const d = await invoke("clip_detail", { id });
    if (view.mode !== "detail" || view.clipId !== id) return;
    detail = d;
    detailError = null;
  } catch (e) {
    detailError = String(e);
  }
  if (detail && viewer.clipId !== view.clipId) resetViewer(detail);
  renderDetail();
  renderFoot();
}

/* ---------- frame viewer ---------- */

/*
 * One still of the clip, read by ffmpeg at the moment under the pointer, or
 * at the playhead. Overlays are drawn here from the still's pixels: false
 * colour reads brightness in stops from middle grey through the clip's
 * picture profile (detail.tone), the same way the exposure score does.
 */
const VIEW_MODES = [["picture", "Picture"], ["false_color", "False color"], ["peaking", "Focus peaking"], ["clipping", "Clipping"]];
const FALSE_BANDS = [
  // [lowest stops, rgb, words]; checked top down after the clip point.
  [2.5, [240, 146, 36], "+2.5"],
  [1, [236, 214, 120], "+1"],
  [0.5, null, ""],
  [-0.5, [64, 170, 98], "Middle grey"],
  [-2.5, null, ""],
  [-5, [58, 100, 206], "−2.5"],
  [-Infinity, [116, 52, 150], "−5"],
];
const CLIP_RGB = [222, 52, 40];
const PEAK_RGB = [255, 214, 10];

let viewer = { clipId: null, pin: 0, want: null, frame: null, loading: false, error: null, mode: "picture", playing: false };
try { viewer.mode = localStorage.getItem("viewer-mode") || "picture"; } catch (_) { /* a remembered view is a convenience */ }
const frameCache = new Map();
const FRAME_CACHE_SIZE = 24;
const frameKey = (id, t) => `${id}@${(Math.round(t * 10) / 10).toFixed(1)}`;

function resetViewer(d) {
  const kept = d.segments.find((s) => s.status === "kept");
  const pin = kept ? (kept.start + kept.end) / 2 : (d.duration || 0) * 0.25;
  viewer = { clipId: view.clipId, pin, want: null, frame: null, loading: false, error: null, mode: viewer.mode, playing: false };
  showFrameAt(pin);
}

function parseStill(buf) {
  const u8 = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf);
  const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
  const w = dv.getUint32(0, true), h = dv.getUint32(4, true);
  return { w, h, rgba: new Uint8ClampedArray(u8.buffer, u8.byteOffset + 8, w * h * 4), modes: {} };
}

async function getFrame(id, t) {
  const key = frameKey(id, t), hit = frameCache.get(key);
  if (hit) return hit;
  const f = parseStill(await invoke("clip_frame", { id, t }));
  frameCache.set(key, f);
  if (frameCache.size > FRAME_CACHE_SIZE) frameCache.delete(frameCache.keys().next().value);
  return f;
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
    const f = await getFrame(id, t);
    if (view.clipId === id) { viewer.frame = f; viewer.frameT = t; viewer.error = null; }
  } catch (e) {
    if (view.clipId === id) viewer.error = String(e);
  } finally {
    viewer.loading = false;
  }
  if (viewer.playing) return;
  if (view.mode === "detail" && view.clipId === id && viewer.want !== t) showFrameAt(viewer.want);
  else paintViewer();
}

function pinAt(t) {
  if (!detail) return;
  viewer.pin = Math.max(0, Math.min(Math.max(0, (detail.duration || 0) - 0.05), t));
  if (viewer.playing) { play.p0 = viewer.pin; play.t0 = performance.now(); }
  paintPlayhead();
  paintPoint(viewer.pin, false);
  if (!viewer.playing) showFrameAt(viewer.pin);
}

function stepSelect(dir) {
  if (!detail) return;
  const mids = detail.segments.filter((s) => s.status === "kept").map((s) => (s.start + s.end) / 2);
  const next = dir > 0 ? mids.find((m) => m > viewer.pin + 0.05) : [...mids].reverse().find((m) => m < viewer.pin - 0.05);
  if (next != null) pinAt(next);
}

/* Play: the playhead runs in real time and the viewer shows stills as fast as ffmpeg reads them. */
let play = { t0: 0, p0: 0, raf: 0 };
function setPlayIcon() { const b = $("t-play"); if (b) b.innerHTML = icon(viewer.playing ? "pause" : "play"); }
function togglePlay() {
  if (!detail || !detail.raw.t.length) return;
  if (viewer.playing) { stopPlay(); return; }
  if (viewer.pin >= detail.duration - 0.15) viewer.pin = 0;
  viewer.playing = true;
  play.t0 = performance.now(); play.p0 = viewer.pin;
  setPlayIcon();
  const tick = () => {
    if (!viewer.playing || view.mode !== "detail" || !detail) return;
    const t = play.p0 + (performance.now() - play.t0) / 1000;
    if (t >= detail.duration - 0.05) { viewer.pin = Math.max(0, detail.duration - 0.05); stopPlay(); paintPlayhead(); paintPoint(viewer.pin, false); showFrameAt(viewer.pin); return; }
    viewer.pin = t;
    paintPlayhead();
    if ($("hover-line")?.hidden !== false) paintPoint(t, false);
    play.raf = requestAnimationFrame(tick);
  };
  play.raf = requestAnimationFrame(tick);
  playFrames(view.clipId);
}
async function playFrames(id) {
  let shown = null;
  while (viewer.playing && view.mode === "detail" && view.clipId === id) {
    const t = viewer.pin;
    try {
      const f = await getFrame(id, t);
      if (!viewer.playing || view.clipId !== id) break;
      if (f !== shown) { shown = f; viewer.frame = f; viewer.frameT = t; viewer.error = null; paintViewer(); }
      // A cached frame comes back at once; wait for the next paint so the playhead can move.
      await new Promise((r) => requestAnimationFrame(r));
    } catch (e) {
      viewer.error = String(e);
      stopPlay();
      paintViewer();
      break;
    }
  }
}
function stopPlay() {
  if (!viewer.playing) return;
  viewer.playing = false;
  cancelAnimationFrame(play.raf);
  setPlayIcon();
}

/** The stretch a call made now would cover: the hand call at the playhead, the stretch under it, or for a long left-out stretch, a select-length piece around it. */
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
  const starBtns = [1, 2, 3, 4, 5].map((n) => `<button class="star-btn" data-act="star" data-stars="${n}" aria-pressed="${stars >= n}" aria-label="${n} ${n === 1 ? "star" : "stars"}" data-tip="${n} ${n === 1 ? "star" : "stars"}   ${n}">${icon("star")}</button>`).join("");
  return `<button class="call-btn keep" data-act="keep" aria-pressed="${!!isKeep}" data-tip="Keep this stretch whatever the settings say   K">${icon("tick")}Keep<kbd>K</kbd></button>
    <button class="call-btn drop" data-act="drop" aria-pressed="${!!isDrop}" data-tip="Leave this stretch out   X">${icon("cross")}Drop<kbd>X</kbd></button>
    <span class="stars" role="group" aria-label="Stars">${starBtns}</span>
    <span class="call-range num">${range(tg.start, tg.end)}</span>
    ${m ? `<button class="link" data-act="clear-call" data-tip="Go back to what the settings choose   0">Undo</button>` : ""}`;
}

async function makeCall(call, stars) {
  const tg = callTarget();
  if (!tg || !detail) return;
  try {
    const wasSelect = tg.seg ? tg.seg.status === "kept" : tg.mark?.call === "keep";
    state = await invoke("mark_stretch", { id: view.clipId, start: tg.start, end: tg.end, call, stars: stars || 0 });
    renderTop();
    renderProgress();
    renderSide();
    await refreshDetail();
    if (call === "drop" && !wasSelect) toast(`Dropped <span class="num">${range(tg.start, tg.end)}</span>. It was not a select, so nothing changes now; it stays out if new settings would choose it.`);
    else if (call === "keep" && !wasSelect) toast(`Kept <span class="num">${range(tg.start, tg.end)}</span>. It is now a select whatever its scores, and goes into the export.`);
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

function legendHtml(mode, stats, tone) {
  const swatch = (c, w, cls = "") => `<span class="sw"><i class="${cls}"${c ? ` style="background:rgb(${c.join(",")})"` : ""}></i>${esc(w)}</span>`;
  if (mode === "false_color") {
    const how = tone.profile === "standard" ? "Stops from middle grey, as the picture looks." : `Stops from middle grey, read through the ${profileLabel(tone.profile)} curve.`;
    return [swatch(CLIP_RGB, "Clipped"), ...FALSE_BANDS.filter((b) => b[1]).map((b) => swatch(b[1], b[2]))].join("") + infoI(`${TIPS.falsecolor} ${how}`, "About false color");
  }
  if (mode === "peaking") {
    return `<span class="text">${stats.peaks < 0.004 ? "Almost nothing is marked, so nothing in this frame is crisp" : "Yellow marks the sharpest edges"}</span>`;
  }
  if (mode === "clipping") {
    const c = stats.clipped, k = stats.crushed;
    const said = c > 0.0005 || k > 0.0005
      ? `${c > 0.0005 ? `${sharePct(c)} of this frame is clipped and cannot be recovered in the grade.` : "Nothing is clipped."} ${k > 0.0005 ? `${sharePct(k)} is ${tone.profile === "standard" ? "crushed to black" : "more than 5 stops under, mostly noise"}.` : ""}`
      : "Nothing in this frame is clipped or crushed.";
    return `${swatch(null, "Clipped", "zebra")}${swatch([58, 100, 206], tone.profile === "standard" ? "Crushed" : "−5")}<span class="text">${c > 0.0005 ? `${sharePct(c)} clipped` : "No clipping"}</span>${infoI(said, "About clipping")}`;
  }
  return "";
}

function paintViewerState() {
  const box = $("v-state");
  if (!box) return;
  const canvas = $("v-canvas");
  if (viewer.error) {
    canvas.hidden = true;
    box.className = "viewer-state on";
    box.innerHTML = `<div class="error-box" role="alert">${icon("triangle")}<div><strong>This frame could not be shown</strong><p>${esc(viewer.error)}</p><p>Try another moment on the timeline. If every moment fails, check that the file opens in another player.</p></div></div>`;
    return;
  }
  canvas.hidden = !viewer.frame;
  if (viewer.loading && !viewer.frame) {
    box.className = "viewer-state on";
    box.innerHTML = `<div class="loading-title"><span class="spinner" aria-hidden="true"></span>Reading the frame at <span class="num">${fmtClock(viewer.want || 0)}</span></div>`;
  } else if (viewer.loading && !viewer.playing) {
    box.className = "viewer-state on busy";
    box.innerHTML = `<span class="spinner" aria-hidden="true"></span>`;
  } else if (!viewer.frame) {
    box.className = "viewer-state on";
    box.innerHTML = `<div class="empty-small"><strong>No frame shown</strong>Point at the timeline or a select to see that moment.</div>`;
  } else {
    box.className = "viewer-state";
    box.innerHTML = "";
  }
}

function paintViewer() {
  const canvas = $("v-canvas");
  if (!canvas || !detail) return;
  paintViewerState();
  const f = viewer.frame;
  const legend = $("v-legend");
  if (f && !viewer.error) {
    const { img, stats } = overlay(f, viewer.mode, detail.tone);
    if (canvas.width !== f.w || canvas.height !== f.h) { canvas.width = f.w; canvas.height = f.h; }
    canvas.getContext("2d").putImageData(img, 0, 0);
    const h = legendHtml(viewer.mode, stats, detail.tone);
    if (legend.dataset.html !== h) { legend.innerHTML = h; legend.dataset.html = h; }
  } else {
    legend.innerHTML = ""; legend.dataset.html = "";
  }
}

function setViewMode(mode) {
  viewer.mode = mode;
  try { localStorage.setItem("viewer-mode", mode); } catch (_) { /* a remembered view is a convenience */ }
  for (const b of document.querySelectorAll("[data-viewmode]")) b.setAttribute("aria-checked", String(b.dataset.viewmode === mode));
  paintViewer();
}

/* ---------- side panel ---------- */

const DEFAULT_SELECTION = { focus_min: 0.5, exposure_min: 0.5, stability_min: 0.5, smoothing: 1, min_length: 2, max_length: 10, max_per_clip: 3, keep_moves_whole: true, focus_weight: 1, exposure_weight: 1, stability_weight: 1 };
/* Presets set the sliders only; the filters stay as they are. */
const PRESETS = [
  { id: "balanced", label: "Balanced", hint: "The starting settings. Suits most footage, up to three selects from each clip.", v: DEFAULT_SELECTION },
  { id: "long", label: "Long takes", hint: "Interviews, talks and locked-off shots: selects of 5 to 30 seconds, with focus and steadiness deciding which win.",
    v: { focus_min: 0.6, exposure_min: 0.5, stability_min: 0.65, smoothing: 1.5, min_length: 5, max_length: 30, max_per_clip: 2, keep_moves_whole: false, focus_weight: 1, exposure_weight: 0.6, stability_weight: 0.8 } },
  { id: "short", label: "Short clips", hint: "Reels and social edits: clean selects of 1.5 to 5 seconds, up to five from each clip.",
    v: { focus_min: 0.55, exposure_min: 0.55, stability_min: 0.55, smoothing: 0.5, min_length: 1.5, max_length: 5, max_per_clip: 5, keep_moves_whole: false, focus_weight: 1, exposure_weight: 1, stability_weight: 1 } },
  { id: "handheld", label: "Handheld", hint: "Run-and-gun, sport and walking shots: more shake is allowed, and sharpness decides which selects win.",
    v: { focus_min: 0.45, exposure_min: 0.45, stability_min: 0.3, smoothing: 1.5, min_length: 1.5, max_length: 8, max_per_clip: 4, keep_moves_whole: true, focus_weight: 1, exposure_weight: 0.7, stability_weight: 0.4 } },
  { id: "strict", label: "Cleanest", hint: "The single best moment of each clip, and only if it is clearly good. Expect some clips to have none.",
    v: { focus_min: 0.7, exposure_min: 0.65, stability_min: 0.7, smoothing: 1, min_length: 2.5, max_length: 10, max_per_clip: 1, keep_moves_whole: true, focus_weight: 1, exposure_weight: 1, stability_weight: 1 } },
];
const presetMatching = (sel) => PRESETS.find((p) => Object.entries(p.v).every(([k, v]) => (typeof v === "boolean" ? !!sel[k] === v : Math.abs((sel[k] ?? DEFAULT_SELECTION[k]) - v) < 1e-4)));

/* Single sliders: [min, max, step] in slider units, and how a value reads. */
const SLIDER = {
  focus_min: [0, 100, 1], exposure_min: [0, 100, 1], stability_min: [0, 100, 1],
  smoothing: [0, 5, 0.25],
  focus_weight: [0, 100, 5], exposure_weight: [0, 100, 5], stability_weight: [0, 100, 5],
};
const toSlider = (key, v) => (key === "smoothing" ? v : Math.round((v ?? 1) * 100));
const fromSlider = (key, raw) => (key === "smoothing" ? Number(raw) : Number(raw) / 100);
const sliderLabel = (key, v) => (key === "smoothing" ? (v === 0 ? "Off" : `${v} s`) : String(Math.round((v ?? 1) * 100)));
const fillPct = (key, raw) => { const [a, b] = SLIDER[key]; return `${(((raw - a) / (b - a)) * 100).toFixed(1)}%`; };

/* The length slider is squared so short lengths get most of the track: 0.5 s to 60 s. */
const LEN_MIN = 0.5, LEN_MAX = 60;
const lenToPos = (v) => Math.round(Math.sqrt(Math.max(0, (Math.min(LEN_MAX, v) - LEN_MIN) / (LEN_MAX - LEN_MIN))) * 1000);
const posToLen = (p) => { const v = LEN_MIN + (LEN_MAX - LEN_MIN) * (p / 1000) ** 2; return v < 10 ? Math.round(v * 2) / 2 : Math.round(v); };
const lenText = (v) => (v % 1 ? v.toFixed(1) : String(v));

let localSel = null;

function sliderHtml(key, label, tip) {
  const raw = toSlider(key, localSel[key]);
  const [a, b, st] = SLIDER[key];
  return `<div class="field">
    <div class="field-top"><label class="lbl" for="sl-${key}">${esc(label)}</label>${tip ? infoI(tip) : ""}<output class="val" data-out="${key}">${sliderLabel(key, localSel[key])}</output></div>
    <input type="range" id="sl-${key}" data-sel="${key}" data-fk="sl-${key}" min="${a}" max="${b}" step="${st}" value="${raw}" style="--fill:${fillPct(key, raw)}">
  </div>`;
}

function switchRow(label, on, attrs, tip, sub) {
  return `<div class="field-top"><span class="lbl">${esc(label)}</span>${tip ? infoI(tip) : ""}${sub ? `<span class="sub">${esc(sub)}</span>` : ""}${sw(on, attrs, label)}</div>`;
}

function snapTip() {
  const talk = state?.talking_clips || 0;
  return talk
    ? `Someone talks in ${plural(talk, "clip")}. With this on, a select there starts just before a sentence and ends just after one, moving up to 2 seconds to find a pause.`
    : "When someone talks, selects start just before a sentence and end just after one. No talking has been found yet.";
}

function selectsTab() {
  const on = presetMatching(localSel);
  const presets = PRESETS.map((p) => `<button class="chip" aria-pressed="${on?.id === p.id}" data-preset="${p.id}" data-fk="pr-${p.id}"${tipAttr(p.hint)}>${esc(p.label)}</button>`).join("")
    + (on ? "" : `<span class="chip custom" data-tip="Your own settings. Pick a preset to start again from one.">Your own</span>`);
  const lo = lenToPos(localSel.min_length), hi = lenToPos(localSel.max_length);
  const cap = localSel.max_per_clip;
  return `<div class="side-stack">
    <div class="group">
      <div class="head-row"><span class="eyebrow">Start from</span>${infoI(TIPS.sliders)}<button class="link" data-act="reset" data-fk="reset">Reset</button></div>
      <div class="chips">${presets}</div>
    </div>
    <div class="group loose">
      <div class="head-row"><span class="eyebrow">Minimum quality</span>${infoI(TIPS.quality)}</div>
      ${sliderHtml("focus_min", "Focus", TIPS.focus)}
      ${sliderHtml("exposure_min", "Exposure", TIPS.exposure)}
      ${sliderHtml("stability_min", "Stability", TIPS.stability)}
    </div>
    <div class="group loose">
      <span class="eyebrow">Length</span>
      <div class="field">
        <div class="field-top"><span class="lbl">Select length</span>${infoI(TIPS.length)}<output class="val" id="len-out">${lenText(localSel.min_length)} – ${lenText(localSel.max_length)} s</output></div>
        <div class="dual"><div class="dual-track"></div><div class="dual-fill" id="len-fill" style="left:${lo / 10}%;width:${(hi - lo) / 10}%"></div>
          <input type="range" data-len="min" data-fk="len-min" aria-label="Shortest select" min="0" max="1000" step="1" value="${lo}" style="z-index:${lo > 900 ? 3 : 2}">
          <input type="range" data-len="max" data-fk="len-max" aria-label="Longest select" min="0" max="1000" step="1" value="${hi}">
        </div>
      </div>
      <div class="field-top"><span class="lbl">Selects per clip</span>${infoI(TIPS.cap)}
        <div class="stepper"><button data-act="cap-dec" data-fk="cap-dec" aria-label="Fewer"${cap === 1 ? " disabled" : ""}>−</button><output class="num">${cap === 0 ? "All" : cap}</output><button data-act="cap-inc" data-fk="cap-inc" aria-label="More"${cap === 0 ? " disabled" : ""}>+</button></div></div>
    </div>
    <div class="group" style="gap:12px">
      ${switchRow("Keep camera moves whole", localSel.keep_moves_whole, 'data-act="toggle-moves" data-fk="sw-moves"', TIPS.keep_moves)}
      ${switchRow("Cut on pauses in talking", localSel.snap_to_pauses !== false, 'data-act="toggle-snap" data-fk="sw-snap"', snapTip())}
    </div>
    <div class="group loose ruled">
      <button class="adv-toggle" data-act="toggle-adv" data-fk="adv" aria-expanded="${ui.adv}"><span class="eyebrow">Advanced</span>${icon("chev")}</button>
      ${ui.adv ? `${sliderHtml("smoothing", "Smoothing", TIPS.smoothing)}
        <div class="head-row" style="margin-top:4px"><span class="sub-head">Which selects win</span>${infoI(TIPS.weights)}</div>
        ${sliderHtml("focus_weight", "Focus")}${sliderHtml("exposure_weight", "Exposure")}${sliderHtml("stability_weight", "Stability")}` : ""}
    </div>
  </div>`;
}

/** Selects kept by hand pass every filter; say so, or the counts look stuck. */
function handNote(a) {
  return a.hand_kept ? ` ${plural(a.hand_kept, "select")} you kept by hand ${a.hand_kept === 1 ? "stays" : "stay"} whatever the filter says.` : "";
}

function filtersTab() {
  const a = state.aside || { camera: 0, repeats: 0, repeats_found: 0 };
  const cam = localSel.camera || "any";
  const camTip = cam === "any"
    ? "Keep only stretches where the camera is still, or moving a certain way. Movement means sliding sideways or up and down, so a push-in or zoom reads as still. The rest are set aside, not deleted."
    : a.camera
      ? `${plural(a.camera, "good stretch is", "good stretches are")} set aside because the camera does something else. Good stretches are cut where the camera starts or stops moving.${handNote(a)}`
      : `Every good stretch so far shows ${FILTER_WORDS[cam]}. Good stretches are cut where the camera starts or stops moving.`;
  const light = localSel.light || "any";
  const known = state.light_known || 0, total = (state.clips || []).filter((c) => c.status !== "unreadable").length;
  const unknown = Math.max(0, total - known);
  const lightTip = light === "any"
    ? known
      ? `Light is known for ${known} of ${plural(total, "clip")}, from the sun's height when and where each was recorded.${unknown ? ` ${plural(unknown, "clip")} ${unknown === 1 ? "has" : "have"} no time or place; add where you shot on the Project tab.` : ""}`
      : "Keep only footage shot in a certain light. It needs each clip's recording time and place; phones and DJI drones record both, and you can add the place for other cameras on the Project tab."
    : `${a.light ? `${plural(a.light, "good stretch is", "good stretches are")} set aside.` : "Nothing is set aside."} Clips with no known light are set aside too${unknown ? ` (${unknown} now)` : ""}.${handNote(a)}`;
  const repTip = localSel.set_aside_repeats
    ? a.repeats ? `${plural(a.repeats, "repeated take is", "repeated takes are")} set aside. The best take of each shot stays, and each clip's next best stretch can take a freed place.` : "No select looks like a better one in another clip, so nothing is set aside."
    : "Finds selects that look like a better select in another clip, such as a second take from the same spot, and keeps only the best one.";
  const repSub = localSel.set_aside_repeats ? (a.repeats ? `${a.repeats} set aside` : "") : (a.repeats_found ? `${a.repeats_found} found` : "");
  const soundTip = localSel.set_aside_sound_problems
    ? (a.sound ? `${plural(a.sound, "good stretch is", "good stretches are")} set aside for strong wind, clipped sound or a dropout. A light breeze is kept.` : "No select has a sound problem, so nothing is set aside.") + handNote(a)
    : "Sets aside selects with distorted sound, dropouts or strong wind. A light breeze is kept. Problems are always listed on each clip either way.";
  const soundSub = localSel.set_aside_sound_problems && a.sound ? `${a.sound} set aside` : "";
  const chips = (list, cur, attr) => list.map(([id, w]) => `<button class="chip" role="radio" aria-checked="${cur === id}" data-${attr}="${id}" data-fk="${attr}-${id}">${w}</button>`).join("");
  return `<div class="side-stack">
    <div class="group">
      <div class="head-row"><span class="eyebrow">Camera movement</span>${infoI(camTip)}${cam !== "any" && a.camera ? `<span class="sub" style="margin-left:auto;font-size:12px;color:var(--ink-3)">${a.camera} set aside</span>` : ""}</div>
      <div class="chips" role="radiogroup" aria-label="Camera movement">${chips([["any", "Any"], ["still", "Still"], ["moving", "Moving"], ["pans", "Pans"], ["tilts", "Tilts"]], cam, "camera")}</div>
    </div>
    <div class="group">
      <div class="head-row"><span class="eyebrow">Light</span>${infoI(lightTip)}${light !== "any" && a.light ? `<span class="sub" style="margin-left:auto;font-size:12px;color:var(--ink-3)">${a.light} set aside</span>` : ""}</div>
      <div class="chips" role="radiogroup" aria-label="Light">${chips([["any", "Any"], ["golden_hour", "Golden hour"], ["blue_hour", "Blue hour"], ["daylight", "Daylight"], ["night", "Night"]], light, "light")}</div>
    </div>
    <div class="group" style="gap:12px">
      ${switchRow("Set aside repeated takes", localSel.set_aside_repeats, 'data-act="toggle-repeats" data-fk="sw-rep"', repTip, repSub)}
      ${switchRow("Set aside sound problems", localSel.set_aside_sound_problems, 'data-act="toggle-sound" data-fk="sw-sound"', soundTip, soundSub)}
    </div>
  </div>`;
}

function exportTab() {
  const ex = state.export;
  const rateOpts = [`<option value="auto"${ex.rate_is_auto ? " selected" : ""}>Most common (${esc(ex.auto_rate_label)} fps)</option>`]
    .concat(ex.rates.map((r) => `<option value="${r.num}/${r.den}"${!ex.rate_is_auto && r.num === ex.timeline_rate.num && r.den === ex.timeline_rate.den ? " selected" : ""}>${esc(r.label)} fps</option>`)).join("");
  const handles = [[0, "None"], [0.5, "0.5 s"], [1, "1 s"], [2, "2 s"], [3, "3 s"]].map(([v, w]) => `<button role="radio" aria-checked="${(ex.handles || 0) === v}" data-handles="${v}" data-fk="h-${v}">${w}</button>`).join("");
  const layouts = [["full_clips_below", "Whole clips", "Selects stacked above"], ["selects_only", "Selects only", "Back to back"]]
    .map(([id, t, s]) => `<button class="layout-opt" role="radio" aria-checked="${ex.layout === id}" data-layout="${id}" data-fk="lay-${id}"><strong>${t}</strong><span>${s}</span></button>`).join("");
  const has = state.summary.selects_count > 0;
  const df = ex.timeline_rate.drop_frame_capable;
  const p = state.stills_progress;
  const stillsStatus = p ? `<div class="status-line"><span class="spinner" aria-hidden="true"></span>Writing stills: <span class="num">${p[0]}</span> of <span class="num">${p[1] || "…"}</span><button class="link" data-act="stop-stills">Stop</button></div>` : "";
  return `<div class="side-stack" style="gap:18px">
    <div class="field"><label class="field-label" for="timeline-name">Timeline name</label><input id="timeline-name" class="input" type="text" maxlength="80" spellcheck="false" value="${esc(ex.timeline_name)}"></div>
    <div class="field"><label class="field-label" for="timeline-rate">Frame rate</label><select id="timeline-rate" class="input">${rateOpts}</select></div>
    ${df ? switchRow("Drop frame timecode", ex.drop_frame, 'data-act="toggle-df" data-fk="sw-df"', "Drop frame keeps timecode in step with the clock at 29.97 and 59.94 fps.") : ""}
    <div class="group"><div class="head-row"><span class="field-label">Extra time around each select</span>${infoI(TIPS.handles)}</div><div class="seg-ctl" role="radiogroup" aria-label="Extra time around each select">${handles}</div></div>
    <div class="group"><div class="head-row"><span class="field-label">Layout</span>${infoI(TIPS.layout)}</div><div class="layout-opts" role="radiogroup" aria-label="Layout">${layouts}</div></div>
    ${switchRow("Stills and contact sheet", ex.stills, 'data-act="toggle-stills" data-fk="sw-stills"', TIPS.stills)}
    <div class="group" style="gap:10px">
      <button class="btn block" data-act="export" data-fk="export"${has ? "" : " disabled"}>Export timeline</button>
      ${has
        ? `<div class="formats">Writes<span class="tag sm">.xml</span><span class="tag sm">.fcpxml</span><span class="tag sm">.edl</span><span class="tag sm">.csv</span>${infoI(TIPS.formats)}</div>
           <div class="small">Timeline <span class="num">${esc(ex.timeline_rate.label)}</span> fps${df ? (ex.drop_frame ? " drop frame" : " non drop frame") : ""}, <span class="num">${ex.resolution[0]}×${ex.resolution[1]}</span>, ${plural(state.summary.selects_count, "select")}</div>`
        : `<div class="small">There are no selects to export yet. Add footage, or lower a minimum once every clip is analyzed.</div>`}
    </div>
    <div class="group ruled" style="gap:10px">
      ${stillsStatus}
      <button class="btn sec sm block" data-act="stills-only" data-fk="stills-only"${p || !has ? " disabled" : ""}>Save stills and contact sheet only</button>
    </div>
  </div>`;
}

/** Familiar places for common time zones, so the list reads without knowing UTC offsets. */
const ZONE_PLACES = {
  "-600": "Hawaii", "-540": "Alaska", "-480": "Los Angeles in winter", "-420": "Los Angeles in summer, Denver in winter",
  "-360": "Denver in summer, Chicago in winter", "-300": "Chicago in summer, New York in winter", "-240": "New York in summer",
  "-180": "Rio de Janeiro", "0": "the clock is on UTC, London in winter", "60": "London in summer, Paris in winter",
  "120": "Paris in summer, Cape Town", "180": "Istanbul, Nairobi", "240": "Dubai", "330": "India", "420": "Bangkok",
  "480": "Singapore, Perth", "540": "Tokyo", "600": "Sydney in winter", "660": "Sydney in summer", "720": "Auckland in winter", "780": "Auckland in summer",
};
let zoneOpts = null;
function zoneOptions(cur) {
  if (!zoneOpts) {
    zoneOpts = [["auto", "Guess from the clips"]];
    for (let m = -12 * 60; m <= 14 * 60; m += 30) {
      if (m % 60 !== 0 && ![-570, -210, 210, 270, 330, 345, 390, 525, 570, 630].includes(m) && ![-150, 150, 330, 570, 630].includes(m)) continue;
      const h = Math.floor(Math.abs(m) / 60), mm = Math.abs(m) % 60;
      zoneOpts.push([String(m), `UTC${m < 0 ? "−" : "+"}${h}${mm ? ":" + String(mm).padStart(2, "0") : ""}${ZONE_PLACES[m] ? ` · ${ZONE_PLACES[m]}` : ""}`]);
    }
  }
  return zoneOpts.map(([v, w]) => `<option value="${v}"${v === cur ? " selected" : ""}>${esc(w)}</option>`).join("");
}

function profileTip() {
  const rows = (state?.clips || []).filter((c) => c.status !== "unreadable");
  const log = rows.filter((c) => c.profile_is_log);
  const base = "Sony and DJI cameras name their profile in a file beside each clip, and that comes first. This choice is for clips whose camera does not say. Log is judged in stops through the camera's curve, so clipped highlights and underexposure count against a shot and the flat look does not.";
  if (!rows.length) return base;
  const by = (src) => log.filter((c) => c.profile_source === src).length;
  const parts = [];
  if (by("camera")) parts.push(`${by("camera")} named by the camera`);
  if (by("project")) parts.push(`${by("project")} from this choice`);
  if (by("picture")) parts.push(`${by("picture")} recognised from their tones`);
  if (by("chosen")) parts.push(`${by("chosen")} chosen by hand`);
  const std = rows.length - log.length;
  const line = log.length ? `${log.length} of ${plural(rows.length, "clip")} judged as log (${parts.join(", ")}).${std ? ` ${plural(std, "clip")} judged as standard.` : ""}` : "Every clip is judged as standard footage.";
  return `${line}\n${base}`;
}

const PACES = [["auto", "About 2 s"], ["every_beat", "Every beat"], ["every_two_beats", "Every 2 beats"], ["every_bar", "Every bar"], ["every_two_bars", "Every 2 bars"]];
const SHAPES = [["landscape", "16:9"], ["vertical", "9:16"], ["square", "1:1"]];
const ORDERS = [["as_shot", "As shot"], ["best_first", "Best first"]];
const LENGTHS = [["", "Whole song"], ["15", "15 s"], ["30", "30 s"], ["60", "60 s"], ["90", "90 s"]];

function reelHtml() {
  const r = state.reel;
  if (r.reading) return `<div class="status-line"><span class="spinner" aria-hidden="true"></span>Finding the beats in the song</div>`;
  if (!r.music_name) return `<button class="btn sec sm block" data-act="choose-music" data-fk="music">${icon("plus")}Choose a song</button>`;
  const steady = r.beat_confidence != null && r.beat_confidence < 0.35 ? " The beat is not very steady, so some cuts may land a little off." : "";
  const chips = (list, cur, attr) => list.map(([id, w]) => `<button class="chip" role="radio" aria-checked="${String(cur) === id}" data-${attr}="${id}" data-fk="${attr}-${id}">${esc(w)}</button>`).join("");
  const none = !state.summary.selects_count;
  return `<div class="fv-row"><div class="fv-text"><div class="fv-name">${esc(r.music_name)}</div><div class="fv-sub"${tipAttr(steady.trim())}>${r.tempo ? `<span class="num">${r.tempo}</span> bpm · ` : ""}<span class="num">${fmtTotal(r.music_seconds || 0)}</span>${steady ? " · beat unsteady" : ""}</div></div><button class="link" data-act="choose-music">Change</button></div>
    <span class="sub-head">Cut</span><div class="chips" role="radiogroup" aria-label="How often to cut">${chips(PACES, r.pace, "pace")}</div>
    <span class="sub-head">Length</span><div class="chips" role="radiogroup" aria-label="Reel length">${chips(LENGTHS, r.length == null ? "" : String(r.length), "reel-length")}</div>
    <span class="sub-head">Order</span><div class="chips" role="radiogroup" aria-label="Order">${chips(ORDERS, r.order, "order")}</div>
    <span class="sub-head">Shape</span><div class="chips" role="radiogroup" aria-label="Shape">${chips(SHAPES, r.shape, "shape")}</div>
    <button class="btn sec sm block" data-act="preview-reel"${none ? " disabled" : ""}>See the shot list</button>
    <button class="btn sm block" data-act="export-reel"${none ? " disabled" : ""}>Export reel timeline</button>
    ${none ? `<div class="small">There are no selects to cut yet.</div>` : ""}`;
}

function tasteHtml() {
  const fv = state.finished_videos || [];
  const tr = state.tracing;
  let html = "";
  for (const f of fv) {
    html += `<div class="fv-row"><div class="fv-text"><div class="fv-name"${tipAttr(f.path)}>${esc(f.name)}</div>
      <div class="fv-sub"><span class="num">${f.traced}</span> of ${plural(f.shots, "shot")} found${f.stale ? " · more clips analyzed since" : ""}</div></div>
      <button class="link" data-act="remove-finished" data-path="${esc(f.path)}">Remove</button></div>`;
  }
  if (fv.some((f) => f.stale) && !tr) html += `<button class="link" data-act="retrace" style="align-self:flex-start">Look for their shots in the new clips</button>`;
  if (tr) html += `<div class="status-line"><span class="spinner" aria-hidden="true"></span>Reading ${esc(tr.name)}${tr.total > 1 ? ` (${tr.done + 1} of ${tr.total})` : ""}<button class="link" data-act="stop-tracing">Stop</button></div>`;
  const t = state.taste?.report;
  if (t && t.used) {
    const dropped = t.used - t.kept;
    const reasons = t.misses.map(([, n], i) => `${n} ${state.taste.miss_words[i]}`).join("; ");
    html += `<div class="small"${tipAttr(dropped ? `${plural(dropped, "used shot")} would be left out: ${reasons}.` : "")}><strong class="num">${t.kept} of ${t.used}</strong> shots your edits used are kept, from ${plural(t.clips_used, "clip")}.</div>`;
  } else if (fv.length && !tr) {
    html += `<div class="small">None of these videos' shots were found in the clips analyzed so far. They may come from footage that is not in this project, or from clips still waiting to be analyzed.</div>`;
  }
  if (t && (t.hand || t.dropped)) {
    const bits = [];
    if (t.hand) bits.push(`the settings alone would keep ${t.hand_kept} of ${t.hand} stretches you kept by hand`);
    if (t.dropped) bits.push(`${t.dropped_kept} of ${t.dropped} you dropped`);
    html += `<div class="small">Your calls count too: ${esc(bits.join(", and "))}.</div>`;
  }
  if (t && (t.used || t.hand)) html += `<button class="btn sm block" data-act="fit"${tr ? " disabled" : ""}>Fit settings to my edits</button>`;
  else if (t && t.dropped && !fv.length) html += `<div class="small">Keep a few stretches you like (K) and the sliders can be fitted to your calls.</div>`;
  html += `<button class="btn sec sm block" data-act="add-finished"${tr ? " disabled" : ""}>${icon("film")}Add finished videos</button>`;
  return html;
}

function updatesHtml() {
  let st = "";
  if (update.phase === "checking") st = `<div class="status-line"><span class="spinner" aria-hidden="true"></span>Asking GitHub for the newest version</div>`;
  else if (update.phase === "current") st = `<div class="status-line">${pill("ok", "tick", "Up to date")}This is the newest version.</div>`;
  else if (update.phase === "available") st = `<div class="status-line">${pill("amber", "clock", "Update ready", "Your open project is saved first if it has a file. Footage is never touched.")}Version ${esc(update.version)} is available.</div>`;
  else if (update.phase === "installing") st = `<div class="status-line"><span class="spinner" aria-hidden="true"></span>Installing version ${esc(update.version)}. The app restarts when it is done.</div>`;
  else if (update.phase === "failed") st = `<div class="error-box" role="alert" style="padding:10px 12px;font-size:12.5px">${icon("triangle")}<div><p>${esc(update.message)}</p></div></div>`;
  else if (update.phase === "off") st = `<div class="status-line">${pill("grey", "dash", "Not available")}${esc(update.message)}</div>`;
  const busy = update.phase === "checking" || update.phase === "installing" || update.phase === "off";
  return `${product.version ? `<div class="small">You have version <span class="num">${esc(product.version)}</span>.</div>` : ""}${st}
    <button class="btn sec sm block" data-act="update" data-fk="update"${busy ? " disabled" : ""}>${update.phase === "available" ? `Install version ${esc(update.version)} and restart` : "Check for updates"}</button>
    ${switchRow("Check when the app opens", update.auto, `data-act="toggle-auto-update" data-fk="sw-au"${update.phase === "off" ? " disabled" : ""}`)}`;
}

function projectTab() {
  const pl = state.places || {};
  const fp = state.fallback_profile || "auto";
  const popts = [`<option value="auto"${fp === "auto" ? " selected" : ""}>Decide from each clip's tones</option>`].concat((state.profiles || []).map((o) => `<option value="${o.id}"${fp === o.id ? " selected" : ""}>${esc(o.label)}</option>`)).join("");
  const rates = [1, 2, 4, 6, 8, 12].map((r) => `<option value="${r}"${state.sample_rate === r ? " selected" : ""}>${r} per second</option>`).join("");
  const th = theme();
  return `<div class="side-stack">
    <div class="group"><div class="head-row"><span class="eyebrow">Appearance</span>${infoI(TIPS.theme)}</div>
      <div class="seg-ctl" role="radiogroup" aria-label="Theme"><button role="radio" aria-checked="${th === "dark"}" data-theme-pick="dark" data-fk="th-dark">Dark</button><button role="radio" aria-checked="${th === "light"}" data-theme-pick="light" data-fk="th-light">Light</button></div></div>
    <div class="group"><div class="head-row"><span class="eyebrow">Make a reel</span>${infoI(TIPS.reel)}</div>${reelHtml()}</div>
    <div class="group"><div class="head-row"><span class="eyebrow">Learn from your edits</span>${infoI(TIPS.taste)}</div>${tasteHtml()}</div>
    <div class="group"><div class="head-row"><span class="eyebrow">Places and time</span>${infoI(TIPS.places)}</div>
      <div class="field"><label class="field-label" for="place-input">Where you shot</label><input id="place-input" class="input" type="text" spellcheck="false" placeholder="Latitude, longitude" value="${pl.location ? `${pl.location[0]}, ${pl.location[1]}` : ""}" data-tip="For clips that do not record it. For example 38.7223, -9.1393, copied from a map app."></div>
      <div class="field"><label class="field-label" for="clock-zone">Camera clock time zone</label><select id="clock-zone" class="input">${zoneOptions(pl.clock_offset_minutes == null ? "auto" : String(pl.clock_offset_minutes))}</select></div></div>
    <div class="group"><div class="head-row"><span class="eyebrow">Picture profile</span>${infoI(profileTip())}</div>
      <select id="fallback-profile" class="input" aria-label="Picture profile for clips whose camera does not say">${popts}</select></div>
    <div class="group"><div class="head-row"><span class="eyebrow">Analysis</span>${infoI(TIPS.sample)}</div>
      <select id="sample-rate" class="input" aria-label="Frames scored per second">${rates}</select></div>
    <div class="group"><div class="head-row"><span class="eyebrow">Updates</span>${infoI(TIPS.updates)}</div>${updatesHtml()}</div>
  </div>`;
}

const PANEL_TABS = [["selects", "Selects"], ["filters", "Filters"], ["export", "Export"], ["project", "Project"]];

function renderSide() {
  const tabs = PANEL_TABS.map(([id, w]) => `<button role="tab" aria-selected="${ui.panelTab === id}" class="${ui.panelTab === id ? "on" : ""}" data-paneltab="${id}">${w}</button>`).join("");
  const tb = $("side-tabs");
  if (tb.dataset.html !== tabs) { tb.innerHTML = tabs; tb.dataset.html = tabs; }
  renderFoot();
  const body = $("side-body");
  if (!state) { body.innerHTML = ""; return; }
  if (dragging) return;
  if (!selInFlight && !selQueued) localSel = { ...state.selection };
  const act = document.activeElement;
  // Never redraw under a field someone is typing in; it redraws when they leave it.
  if (act && body.contains(act) && (act.tagName === "SELECT" || (act.tagName === "INPUT" && act.type !== "range"))) { body.dataset.stale = "1"; return; }
  const html = ui.panelTab === "filters" ? filtersTab() : ui.panelTab === "export" ? exportTab() : ui.panelTab === "project" ? projectTab() : selectsTab();
  if (body.dataset.html === html) return;
  const fk = act && body.contains(act) ? act.getAttribute("data-fk") : null;
  body.innerHTML = html;
  body.dataset.html = html;
  body.dataset.stale = "";
  if (fk) body.querySelector(`[data-fk="${fk}"]`)?.focus();
  if (tipFor && !tipFor.isConnected) hideTip();
}

function renderFoot() {
  const f = $("side-foot");
  const s = state?.summary;
  let html = "";
  if (view.mode === "detail" && detail) {
    const kept = detail.segments.filter((x) => x.status === "kept");
    html += `<span>This clip</span><span>${plural(kept.length, "select")} · ${fmtLen(kept.reduce((a, x) => a + x.end - x.start, 0))}</span>`;
  }
  html += `<span>All clips</span><span>${s ? `${plural(s.selects_count, "select")} · ${fmtTotal(s.selects_seconds)}` : "–"}</span>`;
  if (f.dataset.html !== html) { f.innerHTML = html; f.dataset.html = html; }
}

/* While a change is on its way to the Rust side, localSel stays the source of
   truth: a reply to an older change must not undo a newer one. */
let selInFlight = false, selQueued = false;
async function pushSelection() {
  if (selInFlight) { selQueued = true; return; }
  selInFlight = true; selQueued = false;
  try {
    state = await invoke("set_selection", { selection: { ...localSel } });
    renderTop();
    if (view.mode === "list") renderMain();
    renderSide();
    if (view.mode === "detail") refreshDetail();
  } catch (e) {
    showError("The selects could not be updated", String(e));
  } finally {
    selInFlight = false;
    if (selQueued) pushSelection();
    else renderSide();
  }
}
function patchSel(patch) {
  if (!localSel) return;
  Object.assign(localSel, patch);
  renderSideNow();
  pushSelection();
}
/** Redraws the side panel from localSel at once, before the Rust side answers. */
function renderSideNow() {
  if (!state) return;
  const keep = state.selection;
  state = { ...state, selection: { ...localSel } };
  renderSide();
  state = { ...state, selection: keep };
}

function wireSide() {
  const body = $("side-body");
  body.addEventListener("pointerdown", (e) => { if (e.target.matches('input[type="range"]')) dragging = true; });
  window.addEventListener("pointerup", () => { if (dragging) { dragging = false; renderSideNow(); } });
  body.addEventListener("input", (e) => {
    const el = e.target;
    if (!localSel) return;
    if (el.dataset.sel) {
      const key = el.dataset.sel;
      localSel[key] = fromSlider(key, el.value);
      el.style.setProperty("--fill", fillPct(key, Number(el.value)));
      const out = body.querySelector(`[data-out="${key}"]`);
      if (out) out.textContent = sliderLabel(key, localSel[key]);
      pushSelection();
    } else if (el.dataset.len) {
      const v = posToLen(Number(el.value));
      if (el.dataset.len === "min") localSel.min_length = Math.min(v, localSel.max_length - 0.5);
      else localSel.max_length = Math.max(v, localSel.min_length + 0.5, 1);
      const lo = lenToPos(localSel.min_length), hi = lenToPos(localSel.max_length);
      const [mn, mx] = body.querySelectorAll("[data-len]");
      if (el === mn && Number(mn.value) !== lo) mn.value = lo;
      if (el === mx && Number(mx.value) !== hi) mx.value = hi;
      mn.style.zIndex = lo > 900 ? 3 : 2;
      const fill = $("len-fill");
      fill.style.left = `${lo / 10}%`; fill.style.width = `${(hi - lo) / 10}%`;
      $("len-out").textContent = `${lenText(localSel.min_length)} – ${lenText(localSel.max_length)} s`;
      pushSelection();
    }
  });
  body.addEventListener("change", async (e) => {
    const id = e.target.id;
    if (id === "timeline-name") pushExport({ timeline_name: e.target.value });
    else if (id === "timeline-rate") { const v = e.target.value; pushExport({ rate: v === "auto" ? null : { num: Number(v.split("/")[0]), den: Number(v.split("/")[1]) }, drop_frame: null }); }
    else if (id === "place-input" || id === "clock-zone") pushPlaces();
    else if (id === "sample-rate") changeSampleRate(Number(e.target.value));
    else if (id === "fallback-profile") {
      const v = e.target.value;
      try { state = await invoke("set_fallback_profile", { profile: v === "auto" ? null : v }); renderAll(); if (view.mode === "detail") refreshDetail(); }
      catch (err) { showError("The profile was not changed", String(err)); }
    }
  });
  body.addEventListener("focusout", () => { setTimeout(() => { if (body.dataset.stale === "1" && !body.contains(document.activeElement)) renderSide(); }, 0); });
}

function sideClick(e) {
  const t = e.target;
  const act = t.closest("[data-act]")?.dataset.act;
  if (act === "reset") { patchSel({ ...DEFAULT_SELECTION, camera: "any", set_aside_repeats: false, light: "any", set_aside_sound_problems: false, snap_to_pauses: true }); return true; }
  if (act === "cap-dec") { const c = localSel.max_per_clip; patchSel({ max_per_clip: c === 0 ? 20 : Math.max(1, c - 1) }); return true; }
  if (act === "cap-inc") { const c = localSel.max_per_clip; if (c !== 0) patchSel({ max_per_clip: c >= 20 ? 0 : c + 1 }); return true; }
  if (act === "toggle-moves") { patchSel({ keep_moves_whole: !localSel.keep_moves_whole }); return true; }
  if (act === "toggle-snap") { patchSel({ snap_to_pauses: localSel.snap_to_pauses === false }); return true; }
  if (act === "toggle-repeats") { patchSel({ set_aside_repeats: !localSel.set_aside_repeats }); return true; }
  if (act === "toggle-sound") { patchSel({ set_aside_sound_problems: !localSel.set_aside_sound_problems }); return true; }
  if (act === "toggle-adv") { ui.adv = !ui.adv; renderSide(); return true; }
  if (act === "toggle-df") { pushExport({ drop_frame: !state.export.drop_frame }); return true; }
  if (act === "toggle-stills") { pushExport({ stills: !state.export.stills }); return true; }
  if (act === "export") { exportTimeline(); return true; }
  if (act === "stills-only") { exportStills(); return true; }
  if (act === "update") { if (update.phase === "available") installUpdate(); else checkForUpdate(false); return true; }
  if (act === "toggle-auto-update") { toggleAutoUpdate(); return true; }
  const pick = (attr) => t.closest(`[data-${attr}]`)?.getAttribute(`data-${attr}`);
  let v;
  if ((v = pick("paneltab")) != null) { ui.panelTab = v; try { localStorage.setItem("panel-tab", v); } catch (_) { /* a convenience */ } renderSide(); $("side-body").scrollTop = 0; return true; }
  if ((v = pick("preset")) != null) { const p = PRESETS.find((x) => x.id === v); if (p) patchSel({ ...p.v }); return true; }
  if ((v = pick("camera")) != null) { patchSel({ camera: v }); return true; }
  if ((v = pick("light")) != null) { patchSel({ light: v }); return true; }
  if ((v = pick("handles")) != null) { pushExport({ handles: Number(v) }); return true; }
  if ((v = pick("layout")) != null) { pushExport({ layout: v }); return true; }
  if ((v = pick("theme-pick")) != null) { setTheme(v); return true; }
  if ((v = pick("pace")) != null) { pushReel({ pace: v }); return true; }
  if ((v = pick("reel-length")) != null) { pushReel({ length: v ? Number(v) : null }); return true; }
  if ((v = pick("order")) != null) { pushReel({ order: v }); return true; }
  if ((v = pick("shape")) != null) { pushReel({ shape: v }); return true; }
  return false;
}

/* ---------- side actions ---------- */

async function pushExport(patch) {
  const ex = state.export;
  const base = {
    rate: ex.rate_is_auto ? null : { num: ex.timeline_rate.num, den: ex.timeline_rate.den },
    drop_frame: ex.timeline_rate.drop_frame_capable ? ex.drop_frame : null,
    layout: ex.layout, timeline_name: ex.timeline_name, handles: ex.handles || 0, stills: !!ex.stills,
  };
  try { state = await invoke("set_export", { export: { ...base, ...patch } }); renderAll(); }
  catch (e) { showError("The export settings could not be changed", String(e)); renderSide(); }
}

async function pushPlaces() {
  const raw = ($("place-input")?.value || "").trim();
  let location = null;
  if (raw) {
    const m = raw.match(/^\s*(-?\d+(?:\.\d+)?)\s*[,;\s]\s*(-?\d+(?:\.\d+)?)\s*$/);
    if (!m) { showError("That place was not understood", "Type a latitude and longitude separated by a comma, for example 38.7223, -9.1393. You can copy them from a map app. Nothing was changed."); return; }
    location = [Number(m[1]), Number(m[2])];
  }
  const z = $("clock-zone")?.value || "auto";
  try { state = await invoke("set_places", { location, clockOffsetMinutes: z === "auto" ? null : Number(z) }); renderAll(); if (view.mode === "detail") refreshDetail(); }
  catch (e) { showError("The place was not changed", String(e)); renderSide(); }
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
  const rows = plan.shots.map((s, i) => `<tr><td class="num">${i + 1}</td><td class="num">${fmtClock(s.at)}</td><td class="num">${s.length.toFixed(1)} s</td><td>${esc(s.clip_name)}</td><td class="num">${fmtClock(s.source_start)}</td><td>${esc(CAMERA_WORDS[s.camera] || "Still")}${s.stretched ? " · stretched" : ""}${s.manual ? " · kept by you" : ""}</td></tr>`).join("");
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

async function addFinished() {
  try {
    const r = await invoke("add_finished_videos");
    if (!r) return;
    await refresh();
    const bits = [];
    if (r.added.length) bits.push(`Read ${plural(r.added.length, "finished video")}`);
    if (r.cancelled) bits.push("Stopped before the rest were read; nothing already read was lost");
    if (r.failed.length) await showError("Some finished videos could not be read", r.failed.map(([n, why]) => `${n}: ${why}`).join(" ") + " The others were read normally.");
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
    return `<tr class="${changed ? "changed" : ""}"><td>${name}</td><td class="num">${f(a[k])}</td><td class="num">${changed ? "<strong>" + f(b[k]) + "</strong>" : f(b[k])}</td></tr>`;
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
  const vb = { used: b.used - (b.hand || 0), kept: b.kept - (b.hand_kept || 0) }, va = { used: a.used - (a.hand || 0), kept: a.kept - (a.hand_kept || 0) };
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

/* ---------- modal and toast ---------- */

function modal(title, bodyHtml, actions) {
  return new Promise((resolve) => {
    hideTip();
    $("modal-title").textContent = title;
    $("modal-body").innerHTML = bodyHtml;
    const box = $("modal-actions");
    box.innerHTML = "";
    for (const a of actions) {
      const b = document.createElement("button");
      b.className = `btn sm${a.primary ? "" : " sec"}`;
      b.textContent = a.label;
      b.addEventListener("click", () => { $("modal").hidden = true; resolve(a.value); });
      box.appendChild(b);
    }
    $("modal").hidden = false;
    (box.querySelector(".btn:not(.sec)") || box.querySelector(".btn"))?.focus();
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

/* ---------- project actions ---------- */

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
  stopPlay();
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
    if (s) { stopPlay(); state = s; view = { mode: "list", clipId: null }; renderAll(); notePending(); }
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

async function changeSampleRate(rate) {
  if (!state || rate === state.sample_rate) return;
  const analyzed = state.clips.filter((c) => c.status === "analyzed" || c.status === "partial").length;
  if (!analyzed) { state = await invoke("set_sample_rate", { rate, reanalyze: true }); renderAll(); return; }
  const speed = state.analysis.speed;
  const est = speed ? ` That took about ${fmtDur(state.summary.analyzed_seconds / speed)} last time at ${state.sample_rate} per second.` : "";
  const choice = await modal("Analyze the footage again?", `<p>Scoring ${plural(rate, "frame")} per second instead of ${state.sample_rate} means analyzing ${plural(analyzed, "clip")} again.${esc(est)}</p><p class="muted">Current selects stay on screen and are replaced clip by clip as the new scores arrive.</p>`, [
    { label: `Keep ${state.sample_rate} per second`, value: false },
    { label: `Analyze again at ${rate} per second`, value: true, primary: true },
  ]);
  if (!choice) { renderSide(); const sr = $("sample-rate"); if (sr) sr.value = String(state.sample_rate); return; }
  state = await invoke("set_sample_rate", { rate, reanalyze: true });
  renderAll();
}

/* ---------- updates ---------- */

let update = { phase: "idle", version: null, message: "", auto: false }; // idle | checking | current | available | installing | failed | off

function setUpdate(patch) { update = { ...update, ...patch }; if (ui.panelTab === "project") renderSide(); }

async function checkForUpdate(quiet) {
  setUpdate({ phase: "checking", version: null, message: "" });
  try {
    const r = await invoke("check_for_update");
    setUpdate(r.available ? { phase: "available", version: r.available, message: "" } : { phase: quiet ? "idle" : "current", version: null, message: "" });
  } catch (e) {
    setUpdate(quiet ? { phase: "idle", version: null, message: "" } : { phase: "failed", version: null, message: String(e) });
  }
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
  setUpdate({ phase: "installing" });
  try { await invoke("install_update"); }
  catch (e) { setUpdate({ phase: "failed", version: null, message: String(e) }); }
}

async function toggleAutoUpdate() {
  const on = !update.auto;
  setUpdate({ auto: on });
  try { await invoke("set_check_updates_on_open", { on }); }
  catch (err) { setUpdate({ auto: !on }); showError("The setting was not changed", String(err)); }
}

async function startUpdates() {
  try {
    const s = await invoke("update_settings");
    update.auto = !!s.check_updates_on_open;
    if (!s.configured) setUpdate({ phase: "off", message: "This copy cannot update itself. Download newer versions from the release page." });
    else if (s.check_updates_on_open) { checkForUpdate(true); return; }
  } catch (_) { /* updates are optional; the rest of the app carries on */ }
  setUpdate({});
}

/* ---------- refresh ---------- */

function renderAll() { renderTop(); renderProgress(); renderMain(); renderSide(); }

let refreshTimer = null, lastRefresh = 0;
function scheduleRefresh(immediate) {
  const wait = immediate ? 0 : Math.max(0, 400 - (Date.now() - lastRefresh));
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(refresh, wait);
}
async function refresh() {
  lastRefresh = Date.now();
  const before = view.mode === "detail" ? state?.clips.find((c) => c.id === view.clipId) : null;
  try {
    state = await invoke("get_state");
    loadError = null;
  } catch (e) {
    if (!state) loadError = String(e);
  }
  renderTop();
  renderProgress();
  renderSide();
  if (view.mode === "detail") {
    const row = state?.clips.find((c) => c.id === view.clipId);
    if (!row) { stopPlay(); view = { mode: "list", clipId: null }; renderMain(); }
    else if (row.status === "analyzing" || !detail || row.status !== before?.status) refreshDetail();
  } else renderMain();
}

/* ---------- wiring ---------- */

function back() { stopPlay(); view = { mode: "list", clipId: null }; detail = null; ui.info = false; renderMain(); renderFoot(); }

function wire() {
  wireTips();
  wireSide();
  document.addEventListener("click", (e) => {
    if (!$("menu").hidden && !e.target.closest("#menu") && !e.target.closest("#project-btn")) closeMenu();
    if (e.target.closest("#side") && sideClick(e)) return;
    const el = e.target.closest("[data-act]");
    const act = el?.dataset.act;
    if (el?.disabled) return;
    if (act === "add-files") addFiles(false);
    else if (act === "add-folder") addFiles(true);
    else if (act === "back") back();
    else if (act === "toggle-info") { ui.info = !ui.info; hideTip(); renderDetail(); }
    else if (act === "prev-clip" || act === "next-clip") { const nb = clipNeighbours(); const id = act === "prev-clip" ? nb.prev : nb.next; if (id != null) openDetail(id); }
    else if (act === "play") togglePlay();
    else if (act === "prev-select") stepSelect(-1);
    else if (act === "next-select") stepSelect(1);
    else if (act === "keep") makeCall("keep", callTarget()?.mark?.stars || 0);
    else if (act === "drop") makeCall("drop", 0);
    else if (act === "clear-call") makeCall(null, 0);
    else if (act === "star") { const n = Number(el.dataset.stars); const cur = callTarget()?.mark; makeCall("keep", cur?.call === "keep" && cur.stars === n ? n - 1 : n); }
    else if (act === "save-report") saveReport();
    else if (act === "add-finished") addFinished();
    else if (act === "stop-tracing") invoke("stop_tracing");
    else if (act === "fit") fitToEdits();
    else if (act === "retrace") invoke("retrace_finished_videos").then((s) => { state = s; renderAll(); });
    else if (act === "remove-finished") invoke("remove_finished_video", { path: el.dataset.path }).then((s) => { state = s; renderAll(); });
    else if (act === "choose-music") chooseMusic();
    else if (act === "preview-reel") previewReel();
    else if (act === "export-reel") exportReel();
    else if (act === "stop-stills") invoke("stop_stills");
    else if (e.target.closest("[data-viewmode]")) setViewMode(e.target.closest("[data-viewmode]").dataset.viewmode);
    else if (e.target.closest("[data-libtab]")) { ui.libTab = e.target.closest("[data-libtab]").dataset.libtab; $("lib-scroll") && ($("lib-scroll").scrollTop = 0); renderMain(); }
    else if (e.target.closest("[data-seek]")) { pinAt(Number(e.target.closest("[data-seek]").dataset.seek)); $("viewer")?.scrollIntoView({ block: "nearest", behavior: "smooth" }); }
    else if (e.target.closest("[data-clip]")) openDetail(Number(e.target.closest("[data-clip]").dataset.clip));
    else if (ui.info && view.mode === "detail" && !e.target.closest("#info-pop")) { ui.info = false; renderDetail(); }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.target.matches?.(".lib-row")) { openDetail(Number(e.target.dataset.clip)); return; }
  });
  document.addEventListener("change", async (e) => {
    if (e.target.id !== "clip-profile") return;
    const v = e.target.value;
    try { state = await invoke("set_clip_profile", { id: view.clipId, profile: v === "auto" ? null : v }); renderTop(); renderSide(); refreshDetail(); }
    catch (err) { showError("The profile was not changed", String(err)); }
  });
  $("project-btn").addEventListener("click", (e) => { if ($("menu").hidden) openMenu(e.currentTarget, projectMenu()); else closeMenu(); });
  $("btn-save").addEventListener("click", (e) => saveProject(e.shiftKey));
  $("btn-export-top").addEventListener("click", () => { ui.panelTab = "export"; try { localStorage.setItem("panel-tab", "export"); } catch (_) { /* a convenience */ } renderSide(); });
  $("btn-report").addEventListener("click", openReport);
  $("btn-analysis").addEventListener("click", async () => {
    if (state?.analysis.running) await invoke("stop_analysis");
    else await invoke("start_analysis");
    scheduleRefresh(true);
  });
  document.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); saveProject(e.shiftKey); return; }
    if (mod && e.key.toLowerCase() === "o") { e.preventDefault(); openProject(); return; }
    if (!$("modal").hidden) return;
    if (e.key === "Escape") {
      if (!$("menu").hidden) { closeMenu(); return; }
      if (ui.info) { ui.info = false; renderDetail(); return; }
      if (view.mode === "detail" || view.mode === "report") { back(); return; }
    }
    const typing = ["INPUT", "SELECT", "TEXTAREA"].includes(document.activeElement?.tagName) && document.activeElement.type !== "range";
    if (view.mode !== "detail" || !detail || typing || mod) return;
    const k = e.key.toLowerCase();
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      if (document.activeElement?.type === "range") return;
      e.preventDefault(); pinAt(viewer.pin + (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 5 : 1));
    } else if (e.key === " ") { if (document.activeElement?.tagName === "BUTTON" && !document.activeElement.closest("#main-panel")) return; e.preventDefault(); togglePlay(); }
    else if (e.key === "[" || e.key === "]") { e.preventDefault(); stepSelect(e.key === "[" ? -1 : 1); }
    else if (k === "k") { e.preventDefault(); makeCall("keep", callTarget()?.mark?.stars || 0); }
    else if (k === "x") { e.preventDefault(); makeCall("drop", 0); }
    else if (k === "0" || k === "u") { e.preventDefault(); makeCall(null, 0); }
    else if (/^[1-5]$/.test(k)) { e.preventDefault(); makeCall("keep", Number(k)); }
  });
  // Space plays; it must not also press whichever button in the clip view has focus.
  document.addEventListener("keyup", (e) => { if (e.key === " " && view.mode === "detail" && e.target.closest?.("#main-panel button")) e.preventDefault(); });
  window.addEventListener("resize", () => { hideTip(); closeMenu(); });

  listen("analysis-changed", () => scheduleRefresh(false));
  listen("state-changed", () => scheduleRefresh(true));
  listen("probe-progress", (e) => {
    const p = e.payload;
    probe = p.done >= p.total ? null : p;
    if (!probe) scheduleRefresh(true); else if (view.mode === "list") renderMain();
  });
  listen("drop-hover", (e) => { $("drop-overlay").hidden = !e.payload; });
  listen("stills-progress", (e) => { if (state) { state.stills_progress = e.payload.done >= e.payload.total ? null : [e.payload.done, e.payload.total]; renderSide(); } });
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
  renderSide();
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
