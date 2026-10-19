// Illustrative score data for the landing page. Seeded, so every visit shows the same clips and scores.
(function () {
  function prng(seed) { let s = seed >>> 0; return () => { s = (s + 0x6D2B79F5) >>> 0; let t = s; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }
  function bump(t, a, b, depth, soft) { soft = soft || 0.8; if (t < a - soft || t > b + soft) return 0; if (t >= a && t <= b) return depth; const d = t < a ? (a - t) / soft : (t - b) / soft; return depth * (0.5 + 0.5 * Math.cos(Math.PI * d)); }
  function smooth(arr, w) { const out = []; for (let i = 0; i < arr.length; i++) { let s = 0, c = 0; for (let j = i - w; j <= i + w; j++) if (j >= 0 && j < arr.length) { s += arr[j]; c++; } out.push(s / c); } return out; }
  function clip(o) {
    o = o || {}; const dur = o.dur || 48, rate = o.rate || 5, n = Math.round(dur * rate), r = prng(o.seed || 7);
    const thr = o.thr || { focus: 50, exposure: 50, stability: 50 };
    const spec = o.spec || {
      focus: { base: 79, dips: [[14, 19, 47]] },
      exposure: { base: 72, dips: [[39, 44, 42]] },
      stability: { base: 83, dips: [[5.5, 9.5, 58], [27, 28.6, 40]] }
    };
    const m = {};
    Object.keys(spec).forEach(k => {
      const raw = []; let drift = 0;
      for (let i = 0; i < n; i++) {
        const t = i / rate; drift += (r() - 0.5) * 1.6; drift *= 0.93;
        let v = spec[k].base + drift + Math.sin(t * 0.7 + k.length) * 3;
        spec[k].dips.forEach(d => { v -= bump(t, d[0], d[1], d[2]); });
        v += (r() - 0.5) * (k === 'stability' ? 18 : 12);
        raw.push(Math.max(2, Math.min(99, v)));
      }
      m[k] = { raw, smooth: smooth(raw, Math.round(rate * 0.6)) };
    });
    const ok = i => m.focus.smooth[i] >= thr.focus && m.exposure.smooth[i] >= thr.exposure && m.stability.smooth[i] >= thr.stability;
    const runs = []; let st = null;
    for (let i = 0; i <= n; i++) { const g = i < n && ok(i); if (g && st === null) st = i; if (!g && st !== null) { runs.push([st, i]); st = null; } }
    const segs = []; const minLen = (o.minLen || 2) * rate, maxLen = (o.maxLen || 12) * rate;
    runs.forEach(([a, b]) => {
      if (b - a < minLen) { segs.push({ a: a / n, b: b / n, status: 'short' }); return; }
      const parts = Math.ceil((b - a) / maxLen), len = (b - a) / parts;
      for (let p = 0; p < parts; p++) segs.push({ a: (a + p * len) / n, b: (a + (p + 1) * len) / n, status: 'kept' });
    });
    const below = {};
    Object.keys(m).forEach(k => { const out = []; let s = null; for (let i = 0; i <= n; i++) { const lo = i < n && m[k].smooth[i] < thr[k]; if (lo && s === null) s = i; if (!lo && s !== null) { out.push({ a: s / n, b: i / n }); s = null; } } below[k] = out; });
    return { dur, rate, n, m, thr, segs, below };
  }
  function path(arr, w, h, area) {
    const n = arr.length; let d = '';
    for (let i = 0; i < n; i++) { const x = (i / (n - 1)) * w, y = h - (arr[i] / 100) * h; d += (i ? 'L' : 'M') + x.toFixed(1) + ' ' + y.toFixed(1); }
    if (area) d += 'L' + w + ' ' + h + 'L0 ' + h + 'Z';
    return d;
  }
  function spark(arr, w, h, step) { step = step || 4; const s = []; for (let i = 0; i < arr.length; i += step) s.push(arr[i]); return path(s, w, h); }
  function tc(sec, fps) { fps = fps || 24; const f = Math.floor((sec % 1) * fps), s = Math.floor(sec) % 60, mi = Math.floor(sec / 60) % 60, hr = Math.floor(sec / 3600); const p = x => String(x).padStart(2, '0'); return p(hr) + ':' + p(mi) + ':' + p(s) + ':' + p(f); }
  function hist(seed, center, spread, clipPct) {
    const r = prng(seed || 3), bins = []; for (let s = -7; s <= 5; s += 0.5) { let v = Math.exp(-Math.pow((s - center) / spread, 2)) * 100 + r() * 6; if (s >= 4.5) v = clipPct; bins.push({ stop: s, v: Math.max(1.5, v) }); }
    return bins;
  }
  window.FSSignal = { prng, clip, path, spark, tc, hist, smooth };
})();
