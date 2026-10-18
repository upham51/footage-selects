// A fake Tauri bridge for looking at the interface in a plain browser.
// Load with ?scene=empty|loading|error|probing|list|detail
(function () {
  const scene = new URLSearchParams(location.search).get("scene") || "list";
  const listeners = {};
  let seed = 3;
  const rnd = () => { seed = (seed * 1103515245 + 12345) % 2147483648; return seed / 2147483648; };
  const names = ["A001_C003_0612.mov", "A001_C004_0612.mov", "DJI_0042.MP4", "IMG_4410.MOV", "A002_C001_0613.mov", "GX010233.MP4", "A002_C002_0613.mov", "proxy_A002_C003.mov", "IMG_4412.MOV", "C0031.MP4", "A003_C007_0614.mov", "DJI_0051.MP4"];
  const statuses = ["analyzed", "analyzed", "analyzed", "analyzed", "analyzing", "analyzing", "waiting", "waiting", "unreadable", "analyzed", "missing", "analyzed"];
  const clips = names.map((n, i) => {
    const dur = 20 + rnd() * 90;
    const st = statuses[i];
    const kept = [];
    if (["analyzed", "analyzing", "missing"].includes(st)) {
      let t = rnd() * 6;
      while (t < dur * (st === "analyzing" ? 0.45 : 1) - 4 && kept.length < 3) { const l = 2 + rnd() * 8; kept.push({ start: t, end: Math.min(dur, t + l), focus: .7, exposure: .8, stability: .75, score: .75 }); t += l + 3 + rnd() * 20; }
    }
    return {
      id: i + 1, name: n, path: "/Volumes/Shoot/Day 1/" + n, folder: "/Volumes/Shoot/Day 1", duration: st === "unreadable" ? 0 : dur,
      width: st === "unreadable" ? 0 : (i % 3 === 0 ? 3840 : 1920), height: st === "unreadable" ? 0 : (i % 3 === 0 ? 2160 : 1080),
      rate: st === "unreadable" ? "" : ["23.976", "23.976", "29.97", "29.97", "23.976", "59.94"][i % 6], variable_frame_rate: n.startsWith("IMG"),
      codec: "h264", has_audio: true, rotation: 0, timecode: null, status: st,
      status_detail: st === "unreadable" ? "This file could not be read as video. The file looks damaged or incomplete, for example a recording that stopped early." : st === "missing" ? "The file is no longer where it was. Its scores are kept, but exports will point to the old location." : null,
      progress: st === "analyzing" ? 0.45 : st === "analyzed" || st === "missing" ? 1 : 0, kept, too_short: 1, over_limit: i % 4 === 0 ? 2 : 0,
      profile_label: n.startsWith("DJI") ? "DJI D-Log M" : n.startsWith("C0") || n.startsWith("A0") ? "Sony S-Log3" : n.startsWith("IMG") ? "Apple Log" : "Standard (Rec. 709)",
      profile_is_log: !n.startsWith("GX") && !n.startsWith("proxy"),
      profile_source: n.startsWith("DJI") || n.startsWith("C0") ? "camera" : n.startsWith("IMG") ? "picture" : n.startsWith("A0") ? "project" : "default",
      used: i === 1 ? [[dur * 0.3, dur * 0.3 + 4]] : i === 2 ? [[4, 9], [dur * 0.6, dur * 0.6 + 3]] : [],
      choppy_shutter: n === "DJI_0042.MP4",
      no_select: null,
    };
  });
  // Two analysed clips with nothing kept, to show the reason in the list.
  Object.assign(clips[3], { kept: [], no_select: { kind: "average", score: "stability", average: 17, needs: 50 } });
  Object.assign(clips[9], { kept: [], no_select: { kind: "too_short", longest: 1.4, needs: 2 } });
  Object.assign(clips[11], { kept: [], no_select: { kind: "set_aside", camera: 0, repeats: 2 } });
  clips[0].set_aside = 1;
  const sel = { focus_min: .5, exposure_min: .5, stability_min: .5, smoothing: 1, min_length: 2, max_length: 10, max_per_clip: 3, keep_moves_whole: true, camera: "any", focus_weight: 1, exposure_weight: 1, stability_weight: 1, set_aside_repeats: false };
  const profiles = [["standard", "Standard (Rec. 709)"], ["s_log3", "Sony S-Log3"], ["v_log", "Panasonic V-Log"], ["c_log3", "Canon Log 3"], ["log_c3", "ARRI LogC3"], ["f_log", "Fujifilm F-Log"], ["f_log2", "Fujifilm F-Log2"], ["d_log", "DJI D-Log"], ["d_log_m", "DJI D-Log M"], ["apple_log", "Apple Log"], ["n_log", "Nikon N-Log"], ["hlg", "HLG"], ["generic_log", "Log, curve unknown"]].map(([id, label]) => ({ id, label, approximate: id === "d_log_m" || id === "generic_log" }));
  const rates = [["24000", 1001, "23.976"], [24, 1, "24"], [25, 1, "25"], [30000, 1001, "29.97"], [30, 1, "30"], [50, 1, "50"], [60000, 1001, "59.94"], [60, 1, "60"]].map(([n, d, l]) => ({ num: +n, den: d, label: l, drop_frame_capable: l === "29.97" || l === "59.94" }));
  const totalKept = clips.reduce((a, c) => a + c.kept.length, 0);
  const keptSecs = clips.reduce((a, c) => a + c.kept.reduce((b, k) => b + k.end - k.start, 0), 0);
  const state = {
    clips: scene === "empty" || scene === "probing" ? [] : clips,
    summary: { clips_total: 12, clips_analyzed: 6, clips_failed: 1, footage_seconds: clips.reduce((a, c) => a + c.duration, 0), analyzed_seconds: 400, selects_count: totalKept, selects_seconds: keptSecs },
    selection: sel, sample_rate: 4,
    aside: { camera: 0, repeats: 0, repeats_found: 7 },
    export: { timeline_rate: rates[0], rate_is_auto: true, auto_rate_label: "23.976", drop_frame: false, layout: "full_clips_below", timeline_name: "Day 1 selects", rates, resolution: [1920, 1080] },
    analysis: { running: true, stopping: false, clips_total: 8, clips_done: 4, active: 2, fraction: .58, elapsed: 30, eta_seconds: 22, speed: 61, pending: 0, probing: false },
    project_name: "Day 1", project_path: "/Users/me/Day 1.selects", dirty: true,
    fallback_profile: "s_log3", profiles,
    finished_videos: scene === "empty" ? [] : [
      { name: "Lisbon in 60 seconds.mp4", path: "/Users/me/Exports/Lisbon in 60 seconds.mp4", duration: 61, shots: 24, traced: 21, stale: false },
      { name: "Lisbon vertical.mov", path: "/Users/me/Exports/Lisbon vertical.mov", duration: 30, shots: 11, traced: 9, stale: true },
    ],
    taste: scene === "empty" ? null : { report: { videos: 2, used: 30, kept: 19, clips_used: 17, misses: [["stability", 6], ["too_short", 3], ["over_limit", 2]], selects_seconds: 1100, footage_seconds: 7400 }, miss_words: ["stability under the threshold", "shorter than the shortest select", "over the selects-per-clip limit"] },
    tracing: scene === "tracing" ? { name: "Porto teaser.mp4", done: 0, total: 2 } : null,
  };
  if (scene === "empty") { state.summary = { clips_total: 0, clips_analyzed: 0, clips_failed: 0, footage_seconds: 0, analyzed_seconds: 0, selects_count: 0, selects_seconds: 0 }; state.analysis.running = false; state.project_name = null; state.project_path = null; state.dirty = false; }
  function detail(id) {
    const c = clips.find((x) => x.id === id);
    const n = Math.floor(c.duration * 4);
    const t = [], f = [], e = [], s = [];
    let a = .8, b = .8, d = .9;
    for (let i = 0; i < n; i++) {
      t.push(i / 4);
      a = Math.min(1, Math.max(0, a + (rnd() - .5) * .12 + (i > n * .3 && i < n * .45 ? -.05 : .01)));
      b = Math.min(1, Math.max(0, b + (rnd() - .5) * .06));
      d = Math.min(1, Math.max(0, d + (rnd() - .5) * .2 + (i > n * .7 && i < n * .8 ? -.12 : .04)));
      f.push(a); e.push(b); s.push(d);
    }
    const raw = { t, focus: f, exposure: e, stability: s, key_stops: t.map((_, i) => -0.6 + Math.sin(i / 9) * 0.5), clipped: t.map((_, i) => (i > n * .55 && i < n * .6 ? 0.06 : 0)) };
    const sm = (v) => v.map((_, i) => { let x = 0, k = 0; for (let j = Math.max(0, i - 2); j <= Math.min(v.length - 1, i + 2); j++) { x += v[j]; k++; } return x / k; });
    const smoothed = { t, focus: sm(f), exposure: sm(e), stability: sm(s) };
    const segs = []; let i = 0;
    while (i < n) {
      const pass = smoothed.focus[i] >= .5 && smoothed.exposure[i] >= .5 && smoothed.stability[i] >= .5;
      let j = i; while (j + 1 < n && (smoothed.focus[j + 1] >= .5 && smoothed.exposure[j + 1] >= .5 && smoothed.stability[j + 1] >= .5) === pass) j++;
      const start = i / 4, end = Math.min(c.duration, (j + 1) / 4);
      const failed = pass ? [] : ["focus", "exposure", "stability"].filter((k) => smoothed[k].slice(i, j + 1).some((v) => v < .5));
      segs.push({ start, end, status: pass ? (end - start < 2 ? "too_short" : "kept") : "below_threshold", focus: .7, exposure: .8, stability: .6, score: .7, failed,
        exposure_issue: failed.includes("exposure") ? "clipped_highlights" : null, key_stops: -0.4 + rnd() * 0.8, camera_move: segs.length === 1,
        camera: ["still", "pan_right", "still", "tilt_up", "other"][segs.length % 5], aside: null, similar_to: null });
      i = j + 1;
    }
    let keptSeen = 0; for (const g of segs) if (g.status === "kept") { keptSeen++; if (keptSeen > 3) g.status = "over_limit"; }
    const firstKept = segs.find((g) => g.status === "kept");
    if (firstKept) firstKept.similar_to = { clip: 2, start: 14.2, end: 20.1 };
    const lastKept = [...segs].reverse().find((g) => g.status === "kept" && g !== firstKept);
    if (lastKept) Object.assign(lastKept, { status: "set_aside", aside: "repeat", similar_to: { clip: 5, start: 3.5, end: 9 } });
    return { id, name: c.name, path: c.path, duration: c.duration, measured_until: c.status === "analyzing" ? c.duration * .45 : c.duration,
      info: { duration: c.duration, width: c.width, height: c.height, rotation: 0, rate: { num: 24000, den: 1001 }, variable_frame_rate: false, codec: "h264", has_audio: true, audio_channels: 2, timecode: "01:02:10:04" },
      raw, smoothed, segments: segs, selection: sel, sample_rate: 4, decoder: "hardware, downsized on the GPU", analysis_seconds: 1.8,
      profile: { profile: "d_log_m", source: "camera", explanation: "Named by the camera's file DJI_0042.SRT." }, chosen_profile: null,
      camera_profile: "DJI D-Log M, named by the camera's file DJI_0042.SRT",
      camera: { source: "DJI_0042.SRT", camera: "DJI", lens: null, picture_profile: "dlog_m", iso: 100, shutter: 1 / 2000, aperture: 1.7, latitude: 38.71223, longitude: -9.13934, max_height: 84 },
      shutter_warning: "Shutter 1/2000 at 24 fps (about 4 degrees). Movement in this clip will look choppy; 1/48 is the usual choice. An ND filter lets you keep the slower shutter in bright light.",
      used: c.used.map(([a, b]) => [a, b, "Lisbon in 60 seconds.mp4"]),
      tone: { profile: "standard", clip_luma: 250, stops: Array.from({ length: 256 }, (_, v) => Math.round(Math.log2(Math.max(1e-5, Math.pow(v / 255, 2.4) * 0.9) / 0.18) * 100) / 100) } };
  }
  // A made-up landscape: sky, a clipped sun, a tree line and a textured field.
  function still(t) {
    const w = 960, h = 540, buf = new ArrayBuffer(8 + w * h * 4), dv = new DataView(buf);
    dv.setUint32(0, w, true); dv.setUint32(4, h, true);
    const px = new Uint8ClampedArray(buf, 8);
    const sunX = 620 + t * 8, horizon = 250;
    let r = 7;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const j = (y * w + x) * 4;
      r = (r * 1103515245 + 12345) & 0x7fffffff;
      const noise = (r / 0x7fffffff - 0.5) * 22;
      let c;
      const ridge = horizon - 30 - 26 * Math.abs(Math.sin(x * 0.045)) - 14 * Math.abs(Math.sin(x * 0.13 + 1));
      if (Math.hypot(x - sunX, y - 110) < 34) c = [255, 255, 250];
      else if (y < ridge) { const k = y / horizon; c = [150 + 70 * k, 185 + 50 * k, 230 + 15 * k]; }
      else if (y < horizon) c = [32 + noise, 52 + noise, 34 + noise];
      else { const k = (y - horizon) / (h - horizon); c = [96 - 70 * k + noise * 2, 118 - 80 * k + noise * 2, 54 - 40 * k + noise]; }
      if (x > 400 && x < 470 && y > 300 && y < 460) c = [120 + noise, 60 + noise, 40 + noise];
      px[j] = c[0]; px[j + 1] = c[1]; px[j + 2] = c[2]; px[j + 3] = 255;
    }
    return buf;
  }
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        if (scene === "error" && cmd === "get_state") throw "The app's background process did not answer.";
        if (scene === "loading" && cmd === "get_state") return new Promise(() => {});
        if (cmd === "tool_check") return { ok: true, version: "ffmpeg version 9.0.2", product_name: "Footage Selects", product_version: "0.1.0" };
        if (cmd === "update_settings") return { configured: scene !== "noupdates", check_updates_on_open: false };
        if (cmd === "check_for_update") { await new Promise((r) => setTimeout(r, 600)); if (scene === "error") throw "The app could not reach GitHub to check for a newer version. Nothing was changed. Check your internet connection and try again."; return { configured: true, current_version: "0.1.0", available: scene === "update" ? "0.2.0" : null, notes: null }; }
        if (cmd === "clip_detail") return detail(args.id);
        if (cmd === "clip_frame") { await new Promise((r) => setTimeout(r, 120)); if (scene === "frame-error") throw "The file is no longer where it was, so its picture cannot be shown. Its scores are kept."; return still(args.t); }
        if (cmd === "set_selection") { Object.assign(sel, args.selection); return state; }
        if (cmd === "fit_to_finished_videos") { await new Promise((r) => setTimeout(r, 400)); return { settings: { ...sel, stability_min: 0.35, min_length: 1.5 }, before: state.taste.report, after: { ...state.taste.report, kept: 27, selects_seconds: 1290 }, held_out_kept_share: 0.82, held_out_before_share: 0.63 }; }
        return state;
      },
    },
    window: { getCurrentWindow: () => ({ setTitle: () => {} }) },
    event: { listen: async (name, fn) => { (listeners[name] = listeners[name] || []).push(fn); return () => {}; } },
  };
  window.__mockEmit = (name, payload) => (listeners[name] || []).forEach((fn) => fn({ payload }));
  if (scene === "probing") setTimeout(() => window.__mockEmit("probe-progress", { done: 40, total: 170 }), 50);
  if (scene === "detail" || scene === "frame-error") setTimeout(() => document.querySelector('tr[data-clip="3"]')?.click(), 300);
  if (scene === "fit") setTimeout(() => document.querySelector('[data-act="fit"]')?.click(), 300);
})();
