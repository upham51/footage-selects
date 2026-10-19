"use strict";
// The landing page's moving parts: the hero reel of six clips with live scores,
// and the clip detail panel. Entrances and scroll moves live in motion.js. Clips stream from Pexels;
// every file name, camera and score is illustrative and comes from fs-signal.js.
// The waitlist form is handled separately by site.js.

(function () {
  const S = window.FSSignal;
  if (!S) return;

  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const $ = (id) => document.getElementById(id);
  const SVG = "http://www.w3.org/2000/svg";

  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }
  function svg(tag, attrs) {
    const e = document.createElementNS(SVG, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    return e;
  }
  // Every status carries a shape as well as a colour: tick, triangle or dash.
  const SHAPES = {
    kept: ["M2.5 7.5l3 3 6-7"],
    below: ["M7 1.8l5.6 9.9H1.4z", "M7 5.6v2.6M7 10.1v.1"],
    short: ["M3.5 7h7"],
  };
  const PILL = { kept: "pill pill-green", below: "pill pill-red", short: "pill pill-grey" };
  function icon(kind) {
    const s = svg("svg", { class: "icon", viewBox: "0 0 14 14", "aria-hidden": "true" });
    SHAPES[kind].forEach((d) => s.appendChild(svg("path", { d })));
    return s;
  }
  function setPill(node, kind, text) {
    const key = kind + "|" + text;
    if (node._key === key) return;
    node._key = key;
    node.className = PILL[kind];
    node.replaceChildren(icon(kind), document.createTextNode(text));
  }
  function setText(node, text) {
    if (node.textContent !== text) node.textContent = text;
  }

  const METRICS = [["focus", "Focus"], ["exposure", "Exposure"], ["stability", "Stability"]];
  const NAME = { focus: "Focus", exposure: "Exposure", stability: "Stability" };
  const PX = (id, f) => "https://videos.pexels.com/video-files/" + id + "/" + id + "-hd_1280_720_" + f + "fps.mp4";
  const CLIPS = [
    { name: "C0042.MP4", what: "Ceremony, wide", cam: "Sony FX3", profile: "S-Log3", source: "Profile read from C0042M01.XML", vid: PX(8776110, 25), dur: 23, seed: 7, hist: [-0.4, 2.1, 3], mid: "Middle tones −0.4 stops · 0.3% clipped",
      spec: { focus: { base: 80, dips: [[6, 8.5, 42]] }, exposure: { base: 73, dips: [] }, stability: { base: 86, dips: [[15, 16.2, 44]] } } },
    { name: "A001_C014.MOV", what: "Stair reveal, gimbal", cam: "Canon C70", profile: "Canon Log 3", source: "Profile chosen under Picture profile", vid: PX(7578544, 30), dur: 12, seed: 19, hist: [0.3, 1.9, 2], mid: "Middle tones +0.3 stops · 0.2% clipped",
      spec: { focus: { base: 82, dips: [] }, exposure: { base: 70, dips: [[0, 2.2, 36]] }, stability: { base: 88, dips: [] } } },
    { name: "IMG_4410.MOV", what: "Street, handheld", cam: "iPhone 15 Pro", profile: "Apple Log", source: "Profile chosen under Picture profile", vid: PX(3796613, 25), dur: 17, seed: 23, hist: [1.2, 2.2, 9], mid: "Middle tones +1.2 stops · 2.6% clipped",
      spec: { focus: { base: 76, dips: [[9, 10.5, 40]] }, exposure: { base: 68, dips: [] }, stability: { base: 62, dips: [[1, 5, 30], [12, 15, 28]] } } },
    { name: "DJI_0117.MP4", what: "Festival crowd, aerial", cam: "DJI Mavic 3", profile: "D-Log M", source: "Profile read from DJI_0117.SRT", vid: PX(3941289, 30), dur: 24, seed: 11, hist: [-1.6, 2.4, 5], mid: "Middle tones −1.6 stops · 1.1% clipped",
      spec: { focus: { base: 78, dips: [] }, exposure: { base: 60, dips: [[0, 5, 22], [17, 24, 20]] }, stability: { base: 88, dips: [] } } },
    { name: "C0051.MP4", what: "Couple at night, follow", cam: "Sony FX3", profile: "S-Log3", source: "Profile read from C0051M01.XML", vid: PX(5418493, 50), dur: 40, seed: 31, hist: [-2.4, 2.0, 1], mid: "Middle tones −2.4 stops · 0.1% clipped",
      spec: { focus: { base: 66, dips: [[22, 30, 26]] }, exposure: { base: 40, dips: [] }, stability: { base: 80, dips: [] } } },
    { name: "DJI_0122.MP4", what: "River valley, aerial", cam: "DJI Mavic 3", profile: "D-Log M", source: "Profile read from DJI_0122.SRT", vid: PX(3235598, 24), dur: 33, seed: 37, hist: [0.2, 2.3, 4], mid: "Middle tones +0.2 stops · 0.5% clipped",
      spec: { focus: { base: 84, dips: [[25, 27, 40]] }, exposure: { base: 76, dips: [] }, stability: { base: 86, dips: [[3, 6, 50]] } } },
  ];
  const TOTAL = CLIPS.reduce((a, c) => a + c.dur, 0);
  const pct = (f) => (f * 100).toFixed(3) + "%";
  const mmss = (s) => Math.floor(s / 60) + ":" + String(Math.round(s) % 60).padStart(2, "0");
  const keptOf = (d) => d.segs.filter((s) => s.status === "kept");
  const keptSecs = (d, dur) => keptOf(d).reduce((a, s) => a + (s.b - s.a), 0) * dur;
  const avgOf = (arr) => arr.reduce((a, b) => a + b, 0) / arr.length;

  // Scores with the default thresholds (the hero) and with the panel's sliders.
  const baseCache = {};
  const base = (ci) => baseCache[ci] || (baseCache[ci] = S.clip({ seed: CLIPS[ci].seed, dur: CLIPS[ci].dur, spec: CLIPS[ci].spec }));
  const baseClsCache = {};
  const baseCls = (ci) => baseClsCache[ci] || (baseClsCache[ci] = classify(base(ci)));

  // Labels every sample: inside select k, below a threshold on metric m, or good but too short.
  function classify(d) {
    const n = d.n, cls = new Array(n), keptNo = {};
    let ki = 0;
    d.segs.forEach((s, k) => { if (s.status === "kept") keptNo[k] = ++ki; });
    for (let i = 0; i < n; i++) {
      const f = (i + 0.5) / n;
      let c = null;
      for (let k = 0; k < d.segs.length; k++) {
        const s = d.segs[k];
        if (f >= s.a && f < s.b) { c = s.status === "kept" ? "k" + k : "s"; break; }
      }
      if (!c) {
        let w = null, g = 0;
        METRICS.forEach(([m]) => { const gap = d.thr[m] - d.m[m].smooth[i]; if (gap > g) { g = gap; w = m; } });
        c = w ? "b" + w : "s";
      }
      cls[i] = c;
    }
    const rows = [];
    let a = 0;
    for (let i = 1; i <= n; i++) if (i === n || cls[i] !== cls[a]) { rows.push(row(d, cls[a], a, i, keptNo)); a = i; }
    return { cls, rows, keptNo };
  }
  function row(d, c, a, b, keptNo) {
    const avg = (m) => { let s = 0; for (let i = a; i < b; i++) s += d.m[m].smooth[i]; return Math.round(s / (b - a)); };
    const len = (b - a) / d.rate;
    const out = { range: S.tc(a / d.rate) + " – " + S.tc(b / d.rate), len: len.toFixed(1) + " s" };
    if (c[0] === "k") return Object.assign(out, { kind: "kept", label: "Select " + keptNo[+c.slice(1)], why: "Focus " + avg("focus") + " · Exposure " + avg("exposure") + " · Stability " + avg("stability") });
    if (c[0] === "b") { const m = c.slice(1); return Object.assign(out, { kind: "below", label: "Below threshold", why: NAME[m] + " averaged " + avg(m) + ", needs " + d.thr[m] }); }
    return Object.assign(out, { kind: "short", label: "Too short", why: "Good for " + len.toFixed(1) + " s, needs 2.0 s" });
  }
  // What a low score looks like on the picture: soft, shaken or washed out.
  function frameFx(d, i) {
    let blur = 0, f = "", tf = "";
    const fo = d.m.focus.smooth[i], ex = d.m.exposure.smooth[i], st = d.m.stability.smooth[i];
    if (fo < d.thr.focus) blur += Math.min(4, (d.thr.focus - fo) / 7);
    if (st < d.thr.stability) { blur += 1.2; tf = "translate(1.5px,-1px) rotate(.8deg) scale(1.04)"; }
    if (ex < d.thr.exposure) f += " brightness(1.55) contrast(.72) saturate(.5)";
    return { filter: ((blur ? "blur(" + blur.toFixed(1) + "px)" : "") + f).trim() || "none", tf };
  }
  function statusAt(cl, i, offset) {
    const c = cl.cls[i];
    if (c[0] === "k") return { kind: "kept", text: "Kept · Select " + ((offset || 0) + cl.keptNo[+c.slice(1)]) };
    if (c[0] === "b") return { kind: "below", text: "Below threshold", metric: c.slice(1) };
    return { kind: "short", text: "Too short" };
  }
  const safePlay = (v) => { if (reduce) return; const p = v.play(); if (p) p.catch(() => {}); };

  /* ---------- Hero reel ---------- */

  const hero = { hc: 0, ht: 0, fht: 0, hover: false, stall: 0, lastVT: -1, lap: 0, advFrom: "", vids: [], bounds: [], readouts: [] };
  const keptBefore = [];

  function buildHero() {
    const media = $("hero-media");
    CLIPS.forEach((c, q) => {
      const v = document.createElement("video");
      v.className = "hero-vid" + (q === 0 ? " on" : "");
      v.muted = true;
      v.playsInline = true;
      v.setAttribute("muted", "");
      v.setAttribute("playsinline", "");
      v.setAttribute("aria-hidden", "true");
      v.preload = q < 2 ? "auto" : "metadata";
      v.src = c.vid;
      v.addEventListener("ended", () => { if (q === hero.hc && !hero.hover) heroGo((q + 1) % CLIPS.length, 0); });
      hero.vids.push(v);
    });
    media.prepend(...hero.vids); // Behind the viewfinder overlays.

    const lanes = $("hero-lanes");
    METRICS.forEach(([, name]) => {
      const r = el("div", "readout");
      r.append(el("span", "readout-name", name), el("span", "readout-value", "00"));
      lanes.appendChild(r);
      hero.readouts.push(r);
    });

    const bounds = $("hero-bounds"), track = $("hero-track"), L = [[], [], []];
    let x = 0, k = 0;
    CLIPS.forEach((c, ci) => {
      const d = base(ci), a = x / TOTAL, w = c.dur / TOTAL;
      keptBefore[ci] = k;
      const b = el("div", "bound" + (ci === 0 ? " on" : ""));
      b.style.left = pct(a);
      b.style.width = pct(w);
      b.append(el("b", null, c.name), el("span", null, c.dur + " s"));
      bounds.appendChild(b);
      hero.bounds.push(b);
      if (ci) { const dv = el("div", "divider"); dv.style.left = pct(a); track.appendChild(dv); }
      d.segs.forEach((s) => {
        const kept = s.status === "kept";
        if (kept) k++;
        const sg = el("div", "seg" + (kept ? " kept" : ""), kept ? String(k) : "");
        sg.style.left = pct(a + s.a * w);
        sg.style.width = pct((s.b - s.a) * w);
        track.appendChild(sg);
      });
      METRICS.forEach(([m], li) => {
        const sm = d.m[m].smooth;
        for (let i = 0; i < sm.length; i += 2) L[li].push([(a + (i / (sm.length - 1)) * w) * 1000, 100 - sm[i]]);
      });
      x += c.dur;
    });
    const path = (pts) => pts.map((p, i) => (i ? "L" : "M") + p[0].toFixed(1) + " " + p[1].toFixed(1)).join("");
    L.forEach((pts, li) => $("hero-l" + li).setAttribute("d", path(pts)));
    const keptTotal = CLIPS.reduce((a, c, q) => a + keptSecs(base(q), c.dur), 0);
    $("hero-kept").textContent = k + " selects · " + keptTotal.toFixed(1) + " s kept of " + TOTAL + " s";

    const scrub = $("hero-scrub");
    scrub.addEventListener("pointerenter", () => { hero.hover = true; hero.vids[hero.hc].pause(); });
    scrub.addEventListener("pointerleave", () => { hero.hover = false; safePlay(hero.vids[hero.hc]); });
    scrub.addEventListener("pointermove", (e) => {
      const r = scrub.getBoundingClientRect();
      heroSeek(Math.max(0, Math.min(0.999, (e.clientX - r.left) / r.width)));
    });
    scrub.addEventListener("focus", () => { hero.hover = true; hero.vids[hero.hc].pause(); });
    scrub.addEventListener("blur", () => { hero.hover = false; safePlay(hero.vids[hero.hc]); });
    scrub.addEventListener("keydown", (e) => {
      const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!step) return;
      e.preventDefault();
      const before = CLIPS.slice(0, hero.hc).reduce((a, c) => a + c.dur, 0);
      const g = (before + hero.ht * CLIPS[hero.hc].dur) / TOTAL + step * (e.shiftKey ? 0.05 : 0.01);
      heroSeek(Math.max(0, Math.min(0.999, g)));
    });
    updateHero();
  }

  // g is a fraction of the whole reel.
  function heroSeek(g) {
    let s = g * TOTAL, ci = 0;
    while (ci < CLIPS.length - 1 && s >= CLIPS[ci].dur) { s -= CLIPS[ci].dur; ci++; }
    const t = s / CLIPS[ci].dur;
    if (ci !== hero.hc) showClip(ci);
    const v = hero.vids[ci];
    if (v.duration) { try { v.currentTime = t * v.duration; } catch (_) { /* not seekable yet */ } }
    hero.fht = t;
    hero.ht = t;
    updateHero();
  }
  function showClip(ci) {
    // motion.js listens for this to refocus the viewfinder.
    if (ci !== hero.hc) document.dispatchEvent(new CustomEvent("fs:clip", { detail: ci }));
    hero.vids.forEach((v, q) => {
      v.classList.toggle("on", q === ci);
      if (q !== ci && !v.paused) v.pause();
      if (q === ci || q === (ci + 1) % CLIPS.length) v.preload = "auto";
    });
    hero.hc = ci;
  }
  function heroGo(ci, t) {
    hero.lap++;
    hero.stall = 0;
    hero.lastVT = -1;
    showClip(ci);
    hero.ht = hero.fht = t || 0;
    const v = hero.vids[ci];
    try { v.currentTime = (t || 0) * (v.duration || CLIPS[ci].dur); } catch (_) { /* not loaded yet */ }
    if (!hero.hover) safePlay(v);
    updateHero();
  }
  function updateHero() {
    const ci = hero.hc, c = CLIPS[ci], d = base(ci), cl = baseCls(ci);
    const i = Math.min(d.n - 1, Math.floor(hero.ht * d.n));
    const st = statusAt(cl, i, keptBefore[ci]);
    setPill($("hero-pill"), st.kind, st.kind === "below" ? "Below threshold · " + NAME[st.metric] : st.text);
    setText($("hero-name"), c.name);
    setText($("hero-profile"), c.profile);
    setText($("hero-what"), c.what);
    setText($("hero-tc"), S.tc(hero.ht * c.dur));
    METRICS.forEach(([m], li) => {
      const val = Math.round(d.m[m].smooth[i]), r = hero.readouts[li];
      setText(r.lastChild, String(val));
      r.classList.toggle("low", val < d.thr[m]);
    });
    hero.bounds.forEach((b, q) => b.classList.toggle("on", q === ci));
    const before = CLIPS.slice(0, ci).reduce((a, cc) => a + cc.dur, 0), g = (before + hero.ht * c.dur) / TOTAL;
    $("hero-head").style.left = pct(g);
    const scrub = $("hero-scrub");
    scrub.setAttribute("aria-valuenow", String(Math.round(g * 100)));
    scrub.setAttribute("aria-valuetext", c.name + ", " + S.tc(hero.ht * c.dur) + ", " + st.text);
    const fx = frameFx(d, i), v = hero.vids[ci];
    v.style.filter = fx.filter;
    v.style.transform = fx.tf || "scale(1.02)";
  }
  function heroTick(dt, tick) {
    if (reduce) return;
    const v = hero.vids[hero.hc];
    if (v.readyState >= 2 && v.duration) {
      if (tick && !v.paused) { hero.ht = v.currentTime / v.duration; updateHero(); }
      if (!hero.hover && !v.paused && v.currentTime > 0.3) {
        if (Math.abs(v.currentTime - hero.lastVT) < 0.001) hero.stall += dt; else hero.stall = 0;
        hero.lastVT = v.currentTime;
      }
      // Move on at the end, or when the stream has stopped loading for 3 seconds.
      if (!hero.hover && (v.currentTime >= v.duration - 0.15 || hero.stall > 3000) && hero.advFrom !== hero.hc + ":" + hero.lap) {
        hero.advFrom = hero.hc + ":" + hero.lap;
        heroGo((hero.hc + 1) % CLIPS.length, 0);
        return;
      }
      if (v.paused && !hero.hover && !v.ended) safePlay(v);
    } else if (!hero.hover) {
      // Until the video is ready, the scores run on their own clock.
      hero.fht += dt / (CLIPS[hero.hc].dur * 1000);
      if (hero.fht >= 1) heroGo((hero.hc + 1) % CLIPS.length, 0);
      else if (tick) { hero.ht = hero.fht; updateHero(); }
    }
  }

  /* ---------- Clip detail panel ---------- */

  const inst = { clip: 0, t: 0.3, hover: false, visible: false, drawn: false, thr: { focus: 50, exposure: 50, stability: 50 }, cache: {}, cl: null, d: null };
  const ui = { clips: [], frames: [], lanes: [], dots: [], sliders: [] };
  const frames = {};

  function data(ci) {
    const thr = inst.thr, key = ci + "|" + thr.focus + "|" + thr.exposure + "|" + thr.stability;
    if (!inst.cache[key]) { const c = CLIPS[ci]; inst.cache[key] = S.clip({ seed: c.seed, dur: c.dur, spec: c.spec, thr: Object.assign({}, thr) }); }
    return inst.cache[key];
  }

  function buildInst() {
    const list = $("clip-list");
    CLIPS.forEach((c, k) => {
      const b = el("button", "clip-btn");
      b.type = "button";
      const cv = el("canvas", "clip-thumb");
      cv.width = 128; cv.height = 72;
      const body = el("div");
      body.style.minWidth = "0";
      const top = el("div", "clip-top");
      top.append(el("span", "clip-name", c.name), el("span", "clip-dur", mmss(c.dur)));
      const strip = el("div", "clip-strip"), status = el("div", "clip-status");
      body.append(top, strip, status);
      b.append(cv, body);
      b.addEventListener("click", () => pickClip(k));
      list.appendChild(b);
      ui.clips.push({ b, cv, strip, status });
    });

    const fr = $("frames");
    for (let j = 0; j < 12; j++) { const cv = el("canvas"); cv.width = 192; cv.height = 128; fr.appendChild(cv); ui.frames.push(cv); }

    const chart = $("chart"), scrub = $("inst-scrub");
    METRICS.forEach(([m, name], li) => {
      const lab = el("div", "lane-label");
      lab.style.gridRow = String(3 + li);
      const value = el("div", "lane-value", "0");
      lab.append(el("div", "chart-label", name), value);
      const lane = el("div", "lane");
      lane.style.gridRow = String(3 + li);
      const bands = el("div"), thr = el("div", "thr-line");
      const s = svg("svg", { class: "lane-svg", viewBox: "0 0 1000 100", preserveAspectRatio: "none", "aria-hidden": "true" });
      const raw = svg("path", { class: "raw", pathLength: "1" }), line = svg("path", { class: "line", pathLength: "1" });
      s.append(raw, line);
      lane.append(bands, thr, s);
      chart.insertBefore(lab, scrub);
      chart.insertBefore(lane, scrub);
      const dot = el("div", "dot");
      scrub.appendChild(dot);
      ui.lanes.push({ m, value, bands, thr, raw, line });
      ui.dots.push(dot);
    });

    const sliders = $("sliders");
    METRICS.forEach(([m, name]) => {
      const lab = el("label", "slider"), top = el("span", "slider-top"), num = el("b", null, String(inst.thr[m]));
      top.append(el("span", null, name + " at least"), num);
      const input = el("input");
      input.type = "range"; input.min = "0"; input.max = "100"; input.step = "1"; input.value = String(inst.thr[m]);
      input.addEventListener("input", () => {
        inst.thr = Object.assign({}, inst.thr, { [m]: +input.value });
        num.textContent = input.value;
        renderClip(false);
      });
      lab.append(top, input);
      sliders.appendChild(lab);
      ui.sliders.push(input);
    });

    const setT = (t) => { inst.t = t; updateInstT(); };
    scrub.addEventListener("pointerenter", () => { inst.hover = true; });
    scrub.addEventListener("pointerleave", () => { inst.hover = false; });
    scrub.addEventListener("pointermove", (e) => {
      const r = scrub.getBoundingClientRect();
      inst.hover = true;
      setT(Math.max(0, Math.min(0.999, (e.clientX - r.left) / r.width)));
    });
    scrub.addEventListener("focus", () => { inst.hover = true; });
    scrub.addEventListener("blur", () => { inst.hover = false; });
    scrub.addEventListener("keydown", (e) => {
      const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!step) return;
      e.preventDefault();
      setT(Math.max(0, Math.min(0.999, inst.t + step * (e.shiftKey ? 0.05 : 0.01))));
    });

    renderClip(false);
  }

  function pickClip(k) {
    if (k === inst.clip) return;
    inst.clip = k;
    renderClip(true);
    paint();
  }

  // Redraws everything that depends on the clip or the thresholds.
  function renderClip(animate) {
    const ci = inst.clip, c = CLIPS[ci], d = data(ci), cl = classify(d);
    inst.d = d; inst.cl = cl;

    CLIPS.forEach((cc, k) => {
      const dd = data(k), kept = keptOf(dd), u = ui.clips[k];
      u.b.classList.toggle("on", k === ci);
      u.b.setAttribute("aria-pressed", String(k === ci));
      u.strip.replaceChildren(...dd.segs.map((s) => { const sp = el("span", s.status === "kept" ? "kept" : ""); sp.style.left = pct(s.a); sp.style.width = pct(s.b - s.a); return sp; }));
      let text, kind;
      if (kept.length) {
        kind = "kept";
        text = kept.length + (kept.length === 1 ? " select · " : " selects · ") + keptSecs(dd, cc.dur).toFixed(1) + " s";
      } else {
        // No selects: name the score that fell furthest short.
        let w = "focus", g = -1e9;
        METRICS.forEach(([m]) => { const gap = dd.thr[m] - avgOf(dd.m[m].smooth); if (gap > g) { g = gap; w = m; } });
        kind = "below";
        text = "No selects · " + NAME[w] + " averaged " + Math.round(avgOf(dd.m[w].smooth)) + ", needs " + dd.thr[w];
      }
      u.status.className = "clip-status" + (kind === "kept" ? "" : " none");
      u.status.replaceChildren(icon(kind), el("span", null, text));
    });

    let kn = 0;
    $("selects").replaceChildren(...d.segs.map((s) => {
      const kept = s.status === "kept";
      if (kept) kn++;
      const b = el("div", "seg" + (kept ? " kept" : ""), kept ? "Select " + kn : "");
      b.dataset.grow = "";
      b.style.left = pct(s.a);
      b.style.width = pct(s.b - s.a);
      return b;
    }));

    ui.lanes.forEach((ln) => {
      ln.thr.style.top = (100 - d.thr[ln.m]) + "%";
      ln.bands.replaceChildren(...d.below[ln.m].map((r) => { const b = el("div", "band"); b.dataset.grow = ""; b.style.left = pct(r.a); b.style.width = pct(r.b - r.a); return b; }));
      ln.raw.setAttribute("d", S.path(d.m[ln.m].raw, 1000, 100));
      ln.line.setAttribute("d", S.path(d.m[ln.m].smooth, 1000, 100));
    });

    const step = c.dur <= 30 ? 5 : c.dur <= 60 ? 10 : 15, ticks = [];
    for (let s = 0; s <= c.dur; s += step) { const t = el("span", null, mmss(s)); t.style.left = pct(s / c.dur); ticks.push(t); }
    $("ticks").replaceChildren(...ticks);

    setText($("cur-name"), c.name);
    setText($("cur-cam"), c.cam);
    setText($("cur-profile"), c.profile);
    setText($("cur-source"), c.source);
    setText($("cur-mid"), c.mid);
    setText($("cur-kept"), keptSecs(d, c.dur).toFixed(1) + " s kept");

    const hb = S.hist(c.seed, c.hist[0], c.hist[1], c.hist[2]), mx = Math.max.apply(null, hb.map((b) => b.v));
    $("bins").replaceChildren(...hb.map((b) => { const x = el("div", b.stop >= 4.5 ? "clip" : ""); x.dataset.rise = ""; x.style.height = (b.v / mx * 100).toFixed(1) + "%"; return x; }));

    $("rows").replaceChildren(...cl.rows.map((r) => {
      const line = el("div", "stretch"), left = el("div");
      left.style.minWidth = "0";
      const range = el("div", "stretch-range", r.range + " ");
      range.appendChild(el("span", null, "· " + r.len));
      left.append(range, el("div", "stretch-why", r.why));
      const pill = el("span");
      setPill(pill, r.kind, r.label);
      line.append(left, pill);
      return line;
    }));
    setText($("row-count"), cl.rows.length + (cl.rows.length === 1 ? " stretch" : " stretches"));

    ui.frames.forEach((cv, j) => {
      const fx = frameFx(d, Math.min(d.n - 1, Math.floor(((j + 0.5) / 12) * d.n)));
      cv.style.filter = fx.filter;
      cv.style.transform = fx.tf || "none";
    });

    updateInstT();
    if (animate && inst.drawn) playDraw();
  }

  // Moves the playhead: dots, values, the status line and the current frame.
  function updateInstT() {
    const d = inst.d, cl = inst.cl, c = CLIPS[inst.clip];
    if (!d) return;
    const i = Math.min(d.n - 1, Math.floor(inst.t * d.n));
    ui.lanes.forEach((ln, li) => {
      const v = d.m[ln.m].smooth[i], low = v < d.thr[ln.m];
      setText(ln.value, String(Math.round(v)));
      ln.value.classList.toggle("low", low);
      const dot = ui.dots[li];
      dot.style.left = pct(inst.t);
      dot.style.top = (112 + li * 98 + (1 - v / 100) * 88).toFixed(1) + "px";
      dot.classList.toggle("low", low);
    });
    $("ph-head").style.left = pct(inst.t);
    const st = statusAt(cl, i);
    setPill($("ph-pill"), st.kind, st.text);
    setText($("ph-why"), st.kind === "kept" ? "All three scores clear their thresholds." : st.kind === "below" ? NAME[st.metric] + " is " + Math.round(d.m[st.metric].smooth[i]) + " here, needs " + d.thr[st.metric] + "." : "Good here, but the stretch is under 2 seconds.");
    setText($("ph-tc"), S.tc(inst.t * c.dur));
    const cur = Math.floor(inst.t * 12);
    ui.frames.forEach((cv, j) => cv.classList.toggle("cur", j === cur));
    const scrub = $("inst-scrub");
    scrub.setAttribute("aria-valuenow", String(Math.round(inst.t * 100)));
    scrub.setAttribute("aria-valuetext", S.tc(inst.t * c.dur) + ", " + st.text);
  }

  // Line draw, band grow and bar rise, played when the panel first comes into view and on each new clip.
  let draws = [];
  function playDraw() {
    if (reduce) return;
    draws.forEach((a) => a.cancel());
    draws = [];
    const go = (sel, frames, opts, gap, start) => document.querySelectorAll(sel).forEach((p, i) => draws.push(p.animate(frames, Object.assign({ delay: start + i * gap, fill: "both" }, opts))));
    go(".lane-svg path", [{ strokeDasharray: "1", strokeDashoffset: "1" }, { strokeDasharray: "1", strokeDashoffset: "0" }], { duration: 1500, easing: "cubic-bezier(.65,0,.35,1)" }, 110, 200);
    go("#selects [data-grow], .lane [data-grow]", [{ transform: "scaleX(0)" }, { transform: "scaleX(1)" }], { duration: 900, easing: "cubic-bezier(.16,1,.3,1)" }, 60, 900);
    go("#bins [data-rise]", [{ transform: "scaleY(0)" }, { transform: "scaleY(1)" }], { duration: 800, easing: "cubic-bezier(.16,1,.3,1)" }, 22, 600);
  }

  // Reads 12 frames from each clip for the list thumbnails and the frame strip.
  function grabAll() {
    const box = el("div", "grab-box");
    document.body.appendChild(box);
    const one = (q) => new Promise((res) => {
      const v = document.createElement("video");
      v.muted = true; v.preload = "auto"; v.playsInline = true;
      box.appendChild(v);
      const out = [], N = 12;
      let j = 0;
      const done = () => { v.removeAttribute("src"); v.load(); v.remove(); res(out); };
      const tm = setTimeout(done, 20000);
      v.addEventListener("seeked", () => {
        const cv = document.createElement("canvas");
        cv.width = 192; cv.height = 108;
        try { cv.getContext("2d").drawImage(v, 0, 0, 192, 108); } catch (_) { /* leave blank */ }
        out.push(cv);
        j++;
        if (j < N) v.currentTime = ((j + 0.5) / N) * v.duration;
        else { clearTimeout(tm); done(); }
      });
      v.addEventListener("loadeddata", () => { v.currentTime = (0.5 / N) * v.duration; }, { once: true });
      v.addEventListener("error", () => { clearTimeout(tm); done(); }, { once: true });
      v.src = CLIPS[q].vid;
    });
    const order = [inst.clip].concat(CLIPS.map((_, q) => q).filter((q) => q !== inst.clip));
    (async () => {
      for (const q of order) { frames[q] = await one(q); paint(); }
      box.remove();
    })();
  }
  function paint() {
    ui.clips.forEach((u, k) => {
      const f = frames[k];
      if (f && f[4]) u.cv.getContext("2d").drawImage(f[4], 0, 0, u.cv.width, u.cv.height);
    });
    const f = frames[inst.clip];
    ui.frames.forEach((cv, j) => {
      const ctx = cv.getContext("2d"), s = f && f[j];
      if (s) {
        const r = Math.max(cv.width / s.width, cv.height / s.height), w = s.width * r, h = s.height * r;
        ctx.drawImage(s, (cv.width - w) / 2, (cv.height - h) / 2, w, h);
      } else {
        ctx.fillStyle = "#E4E0D6";
        ctx.fillRect(0, 0, cv.width, cv.height);
      }
    });
  }

  /* ---------- Start ---------- */

  buildHero();
  buildInst();

  const panel = $("instrument");
  if ("IntersectionObserver" in window) {
    new IntersectionObserver((es) => es.forEach((e) => {
      inst.visible = e.isIntersecting;
      if (inst.visible && !inst.drawn) { inst.drawn = true; playDraw(); }
    }), { threshold: 0.15 }).observe(panel);
  } else {
    inst.visible = true;
  }

  // A timer rather than requestAnimationFrame, so the clocks also run while the tab is in the background.
  let last = performance.now(), acc = 0;
  setInterval(() => {
    const now = performance.now(), dt = Math.min(250, now - last);
    last = now;
    acc += dt;
    const tick = acc >= 30;
    if (tick) acc = 0;
    if (!reduce && inst.visible && !inst.hover) { inst.t = (inst.t + dt / 16000) % 1; if (tick) updateInstT(); }
    heroTick(dt, tick);
  }, 33);

  safePlay(hero.vids[0]);
  setTimeout(grabAll, 400);
})();
