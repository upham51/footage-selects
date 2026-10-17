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
    };
  });
  const sel = { focus_min: .5, exposure_min: .5, stability_min: .5, smoothing: 1, min_length: 2, max_length: 10, max_per_clip: 3 };
  const rates = [["24000", 1001, "23.976"], [24, 1, "24"], [25, 1, "25"], [30000, 1001, "29.97"], [30, 1, "30"], [50, 1, "50"], [60000, 1001, "59.94"], [60, 1, "60"]].map(([n, d, l]) => ({ num: +n, den: d, label: l, drop_frame_capable: l === "29.97" || l === "59.94" }));
  const totalKept = clips.reduce((a, c) => a + c.kept.length, 0);
  const keptSecs = clips.reduce((a, c) => a + c.kept.reduce((b, k) => b + k.end - k.start, 0), 0);
  const state = {
    clips: scene === "empty" || scene === "probing" ? [] : clips,
    summary: { clips_total: 12, clips_analyzed: 6, clips_failed: 1, footage_seconds: clips.reduce((a, c) => a + c.duration, 0), analyzed_seconds: 400, selects_count: totalKept, selects_seconds: keptSecs },
    selection: sel, sample_rate: 4,
    export: { timeline_rate: rates[0], rate_is_auto: true, auto_rate_label: "23.976", drop_frame: false, layout: "full_clips_below", timeline_name: "Day 1 selects", rates, resolution: [1920, 1080] },
    analysis: { running: true, stopping: false, clips_total: 8, clips_done: 4, active: 2, fraction: .58, elapsed: 30, eta_seconds: 22, speed: 61, pending: 0, probing: false },
    project_name: "Day 1", project_path: "/Users/me/Day 1.selects", dirty: true,
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
    const raw = { t, focus: f, exposure: e, stability: s };
    const sm = (v) => v.map((_, i) => { let x = 0, k = 0; for (let j = Math.max(0, i - 2); j <= Math.min(v.length - 1, i + 2); j++) { x += v[j]; k++; } return x / k; });
    const smoothed = { t, focus: sm(f), exposure: sm(e), stability: sm(s) };
    const segs = []; let i = 0;
    while (i < n) {
      const pass = smoothed.focus[i] >= .5 && smoothed.exposure[i] >= .5 && smoothed.stability[i] >= .5;
      let j = i; while (j + 1 < n && (smoothed.focus[j + 1] >= .5 && smoothed.exposure[j + 1] >= .5 && smoothed.stability[j + 1] >= .5) === pass) j++;
      const start = i / 4, end = Math.min(c.duration, (j + 1) / 4);
      const failed = pass ? [] : ["focus", "exposure", "stability"].filter((k) => smoothed[k].slice(i, j + 1).some((v) => v < .5));
      segs.push({ start, end, status: pass ? (end - start < 2 ? "too_short" : "kept") : "below_threshold", focus: .7, exposure: .8, stability: .6, score: .7, failed });
      i = j + 1;
    }
    let keptSeen = 0; for (const g of segs) if (g.status === "kept") { keptSeen++; if (keptSeen > 3) g.status = "over_limit"; }
    return { id, name: c.name, path: c.path, duration: c.duration, measured_until: c.status === "analyzing" ? c.duration * .45 : c.duration,
      info: { duration: c.duration, width: c.width, height: c.height, rotation: 0, rate: { num: 24000, den: 1001 }, variable_frame_rate: false, codec: "h264", has_audio: true, audio_channels: 2, timecode: "01:02:10:04" },
      raw, smoothed, segments: segs, selection: sel, sample_rate: 4, decoder: "hardware when available", analysis_seconds: 1.8 };
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
        if (cmd === "set_selection") { Object.assign(sel, args.selection); return state; }
        return state;
      },
    },
    window: { getCurrentWindow: () => ({ setTitle: () => {} }) },
    event: { listen: async (name, fn) => { (listeners[name] = listeners[name] || []).push(fn); return () => {}; } },
  };
  window.__mockEmit = (name, payload) => (listeners[name] || []).forEach((fn) => fn({ payload }));
  if (scene === "probing") setTimeout(() => window.__mockEmit("probe-progress", { done: 40, total: 170 }), 50);
  if (scene === "detail") setTimeout(() => document.querySelector('tr[data-clip="2"]')?.click(), 300);
})();
