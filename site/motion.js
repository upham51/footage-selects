"use strict";
// The landing page's choreography, borrowed from launch-film grammar:
// 1. Text rises out of a mask line on a closed-form spring (about 10% overshoot), word by word.
// 2. Panels grow out of the line or row they belong to; nothing simply fades in.
// 3. The navy band floods in from its first step number and clears the farthest corner.
// 4. One persistent actor: a gold playhead under the header scrubs through the page as you scroll.
// 5. Scroll-linked moves (a slow camera push on the hero, parallax on photos) are pure
//    functions of the scroll position, so the same position always draws the same frame.
// Every entrance is a spring sampled into keyframes and played by the browser, with no CSS
// transitions or timers. Under reduced motion, or without JavaScript, nothing here runs and
// the page shows its final state.

(function () {
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce || !("IntersectionObserver" in window) || !Element.prototype.animate) return;
  document.documentElement.classList.add("motion");

  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));

  // Seeded, so the small timing differences between lines are the same on every visit.
  let seed = 20261019 >>> 0;
  function rnd() {
    seed = (seed + 0x6D2B79F5) >>> 0;
    let t = seed;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  const jit = (ms) => Math.round((rnd() - 0.5) * 2 * ms);

  /* ---------- Springs ---------- */

  // Step response of a damped spring, sampled until it has settled to 0.2%.
  // z is the damping ratio: 0.59 overshoots by about 10%, 0.85 by under 1%.
  const curves = {};
  function curve(z) {
    if (curves[z]) return curves[z];
    const n = 64, wd = Math.sqrt(1 - z * z), T = Math.log(500) / z, out = [];
    for (let i = 0; i <= n; i++) {
      const s = (i / n) * T;
      out.push(i === 0 ? 0 : i === n ? 1 : 1 - Math.exp(-z * s) * (Math.cos(wd * s) + (z / wd) * Math.sin(wd * s)));
    }
    return (curves[z] = out);
  }
  function keys(z, frame) {
    const c = curve(z);
    return c.map((v, i) => Object.assign({ offset: i / (c.length - 1) }, frame(v)));
  }

  // The looks things start from. v runs 0 to 1, overshooting a little past 1.
  const F = {
    rise: (v) => ({ transform: `translateY(${((1 - v) * 112).toFixed(2)}%) rotate(${((1 - v) * 3).toFixed(2)}deg)` }),
    wipe: (v) => ({ transform: `translateY(${((1 - v) * 22).toFixed(2)}px)`, clipPath: `inset(-2px -2px ${clamp((1 - v) * 100, 0, 100).toFixed(2)}% -2px)` }),
    fromLeft: (v) => ({ transform: `translateX(${((1 - v) * -28).toFixed(2)}px)`, clipPath: `inset(0 ${clamp((1 - v) * 100, 0, 100).toFixed(2)}% 0 0)` }),
    growX: (v) => ({ transform: `scaleX(${Math.max(0, v).toFixed(4)})` }),
    growY: (v) => ({ transform: `scaleY(${Math.max(0, v).toFixed(4)})` }),
    pop: (v) => ({ transform: `scale(${Math.max(0, v).toFixed(4)})` }),
    open: (v) => ({ transform: `scale(${(0.82 + 0.18 * v).toFixed(4)})`, clipPath: `inset(0 ${clamp((1 - v) * 50, 0, 50).toFixed(2)}% 0 ${clamp((1 - v) * 50, 0, 50).toFixed(2)}% round 12px)` }),
    grow: (r) => (v) => ({ clipPath: `inset(0 0 ${clamp((1 - v) * 100, 0, 100).toFixed(2)}% 0 round ${r}px)` }),
  };

  /* ---------- Triggers ---------- */

  // Each trigger element holds paused animations; they play when it scrolls into view.
  const groups = new Map();
  const onPlay = new Map();
  function add(trigger, el, frame, o) {
    if (!el) return null;
    o = o || {};
    const a = el.animate(keys(o.z || 0.59, frame), {
      duration: o.dur || 1000,
      delay: Math.max(0, o.delay || 0),
      fill: "both",
      pseudoElement: o.pseudo,
    });
    // Hand the element back to the stylesheet once it has landed, so hover and press still work.
    a.onfinish = () => a.cancel();
    push(trigger, a);
    return a;
  }
  function push(trigger, a) {
    a.pause();
    if (!groups.has(trigger)) groups.set(trigger, []);
    groups.get(trigger).push(a);
  }
  function onStart(trigger, fn) {
    if (!onPlay.has(trigger)) onPlay.set(trigger, []);
    onPlay.get(trigger).push(fn);
  }
  // Anything already scrolled past, as on a reload part way down, is left as it is.
  const past = (n) => n.getBoundingClientRect().bottom < 0;

  /* ---------- Building blocks ---------- */

  // Wraps each word in a mask so it can rise out of its own line.
  function words(node) {
    const out = [];
    (function walk(n) {
      Array.from(n.childNodes).forEach((c) => {
        if (c.nodeType === 1) { walk(c); return; }
        if (c.nodeType !== 3 || !c.textContent.trim()) return;
        const frag = document.createDocumentFragment();
        c.textContent.split(/(\s+)/).forEach((p) => {
          if (!p) return;
          if (/^\s+$/.test(p)) { frag.appendChild(document.createTextNode(p)); return; }
          const w = document.createElement("span"), i = document.createElement("span");
          w.className = "w";
          i.className = "wi";
          i.textContent = p;
          w.appendChild(i);
          frag.appendChild(w);
          out.push(i);
        });
        n.replaceChild(frag, c);
      });
    })(node);
    return out;
  }
  // Wraps a whole line (a heading or a number) in one mask.
  function line(node) {
    const m = document.createElement("span"), i = document.createElement("span");
    m.className = "ml";
    i.className = "ml-in";
    while (node.firstChild) i.appendChild(node.firstChild);
    m.appendChild(i);
    node.appendChild(m);
    return i;
  }
  // A rule that draws itself in place of a border.
  function rule(host, cls) {
    const r = document.createElement("span");
    r.className = "draw-line " + cls;
    r.setAttribute("aria-hidden", "true");
    host.appendChild(r);
    return r;
  }
  function riseWords(trigger, list, start, gap, z) {
    list.forEach((w, i) => add(trigger, w, F.rise, { z: z || 0.59, dur: 1050 + jit(140), delay: start + i * gap + jit(12) }));
    return start + list.length * gap;
  }
  // Counts the leading number of a stat up from zero, keeping the rest of the text as written.
  function countUp(node, delay) {
    const text = node.textContent, m = text.match(/^(\d+)(.*)$/s);
    if (!m || +m[1] < 2) return;
    const end = +m[1], rest = m[2];
    node.textContent = "0" + rest;
    setTimeout(() => {
      const t0 = performance.now();
      (function step(now) {
        const p = clamp((now - t0) / 1300, 0, 1), e = p === 1 ? 1 : 1 - Math.pow(2, -10 * p);
        node.textContent = Math.round(end * e) + rest;
        if (p < 1) requestAnimationFrame(step);
      })(t0);
    }, delay);
  }
  // One press on a call to action: a 1px dip and a ring that leaves it.
  function pressPulse(btn, delay) {
    setTimeout(() => {
      btn.animate([
        { transform: "none", boxShadow: "0 0 0 0 rgba(44, 74, 110, .38)" },
        { transform: "translateY(1px)", boxShadow: "0 0 0 2px rgba(44, 74, 110, .3)", offset: 0.18 },
        { transform: "none", boxShadow: "0 0 0 14px rgba(44, 74, 110, 0)" },
      ], { duration: 900, easing: "cubic-bezier(.2,.7,.4,1)" });
    }, delay);
  }

  /* ---------- Opening: header and hero, played at load ---------- */

  const top = $(".topbar");
  const heroT = $(".hero");
  if (top) {
    add(top, $(".wordmark"), F.wipe, { z: 0.75, dur: 900, delay: 0 });
    add(top, $(".badge"), F.open, { z: 0.62, dur: 900, delay: 140 });
    $$(".topnav a").forEach((a, i) => add(top, a, F.rise, { z: 0.66, dur: 900, delay: 90 + i * 55 + jit(10) }));
    add(top, $(".topbar-right .btn"), F.open, { z: 0.58, dur: 1000, delay: 330 });
  }

  if (heroT && !past(heroT)) {
    // Headline: word by word, the serif words land last and a little looser.
    const hw = $$(".hero h1 [data-word]");
    hw.forEach((w, i) => {
      const serif = w.classList.contains("serif");
      add(heroT, w, F.rise, { z: serif ? 0.55 : 0.6, dur: (serif ? 1250 : 1050) + jit(120), delay: 90 + i * 52 + (serif ? 70 : 0) + jit(10) });
    });

    // The frame opens from a gold line, like a shutter.
    const frame = $(".hero-frame");
    if (frame) {
      const slit = document.createElement("div");
      slit.className = "hero-slit";
      slit.setAttribute("aria-hidden", "true");
      frame.appendChild(slit);
      // The clip runs past the frame's edges at the end, so its shadow is not cut off.
      const h = frame.offsetHeight || 600, half = h / 2 - 1, spill = 160;
      add(heroT, slit, F.growX, { z: 0.9, dur: 700, delay: 260 });
      const open = frame.animate(keys(0.8, (v) => {
        const k = clamp((1 - v) * (half + spill) - spill, -spill, half).toFixed(1);
        return { clipPath: `inset(${k}px -${spill}px ${k}px -${spill}px round 24px)` };
      }), { duration: 1300, delay: 620, fill: "both" });
      open.onfinish = () => open.cancel();
      push(heroT, open);
      const fade = slit.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 420, delay: 900, fill: "forwards", easing: "cubic-bezier(.2,.7,.4,1)" });
      fade.onfinish = () => slit.remove();
      push(heroT, fade);

      // Viewfinder: the corners spring out to the edges and focus is found.
      const k = 46;
      [["tl", k, k], ["tr", -k, k], ["bl", k, -k], ["br", -k, -k]].forEach(([c, x, y], i) => {
        add(heroT, $(".corner." + c, frame), (v) => ({ transform: `translate(${((1 - v) * x).toFixed(2)}px, ${((1 - v) * y).toFixed(2)}px)` }), { z: 0.58, dur: 1200, delay: 980 + i * 40 });
      });
      add(heroT, $(".focus-box", frame), (v) => ({ transform: `translate(-50%, -50%) scale(${(1.7 - 0.7 * v).toFixed(4)})` }), { z: 0.5, dur: 1300, delay: 1060 });
      add(heroT, $(".focus-dot", frame), F.pop, { z: 0.45, dur: 900, delay: 1240 });
      add(heroT, $(".hero-tags", frame), F.wipe, { z: 0.72, dur: 900, delay: 1140 });
      add(heroT, $(".tag-tc", frame), F.wipe, { z: 0.72, dur: 900, delay: 1200 });
      $$(".readout", frame).forEach((r, i) => add(heroT, r, F.wipe, { z: 0.66, dur: 900, delay: 1260 + i * 75 + jit(10) }));
      add(heroT, $(".hero-status", frame), F.open, { z: 0.52, dur: 1000, delay: 1420 });

      // The strip: clip names rise, kept stretches are laid down left to right, score lines draw.
      $$(".bound", frame).forEach((b, i) => add(heroT, b, F.wipe, { z: 0.7, dur: 800, delay: 1300 + i * 45 }));
      $$(".seg.kept", frame).forEach((s, i) => add(heroT, s, F.growX, { z: 0.7, dur: 800, delay: 1380 + i * 38 + jit(8) }));
      add(heroT, $(".hero-lines", frame), (v) => ({ clipPath: `inset(0 ${clamp((1 - v) * 100, 0, 100).toFixed(2)}% 0 0)` }), { z: 0.95, dur: 1700, delay: 1350 });
      add(heroT, $(".strip-foot", frame), F.wipe, { z: 0.8, dur: 900, delay: 1700 });
    }

    const side = $(".hero-side");
    if (side) {
      add(heroT, $(".lede", side), F.wipe, { z: 0.8, dur: 1100, delay: 520 });
      $$(".btn", side).forEach((b, i) => add(heroT, b, F.open, { z: 0.56, dur: 1050, delay: 700 + i * 90 }));
      const cta = $(".btn-primary", side);
      if (cta) onStart(heroT, () => pressPulse(cta, 1900));
    }
    add(heroT, $(".credit"), F.wipe, { z: 0.85, dur: 900, delay: 1800 });
  }

  // Each time the reel moves to the next clip, focus is found again.
  document.addEventListener("fs:clip", () => {
    const box = $(".focus-box"), dot = $(".focus-dot"), name = $("#hero-name");
    if (box) box.animate(keys(0.5, (v) => ({ transform: `translate(-50%, -50%) scale(${(1.28 - 0.28 * v).toFixed(4)})` })), { duration: 1000 });
    if (dot) dot.animate(keys(0.45, F.pop), { duration: 800, delay: 160 });
    if (name) name.animate(keys(0.66, F.wipe), { duration: 700 });
  });

  /* ---------- Sections, played as each comes into view ---------- */

  // Stats: the numbers rise and count, the notes follow.
  $$(".stat").forEach((s, i) => {
    if (past(s)) return;
    const n = $(".stat-n", s);
    const inner = line(n);
    add(s, inner, F.rise, { z: 0.58, dur: 1150, delay: i * 110 });
    add(s, $("p", s), F.wipe, { z: 0.82, dur: 1000, delay: 220 + i * 110 });
    onStart(s, () => countUp(inner, i * 110 + 60));
  });

  // Section heads: the rule draws, the eyebrow ticks in, the headline rises word by word.
  $$(".sec-head").forEach((h) => {
    if (past(h)) return;
    add(h, rule(h, "draw-top"), F.growX, { z: 0.92, dur: 1400, delay: 0 });
    const eb = $(".sec-eyebrow", h);
    if (eb) {
      add(h, $(".sec-n", eb), F.wipe, { z: 0.66, dur: 800, delay: 120 });
      add(h, $(".sec-rule", eb), F.growX, { z: 0.8, dur: 900, delay: 220 });
      add(h, eb.lastElementChild, F.wipe, { z: 0.72, dur: 900, delay: 300 });
    }
    const h2 = $("h2", h);
    if (h2) riseWords(h, words(h2), 260, 48);
    const p = h.querySelector(":scope > p");
    if (p) add(h, p, F.wipe, { z: 0.82, dur: 1100, delay: 560 });
  });

  // Clip detail: the panel grows down out of its section rule, the clip list cascades in.
  const inst = $(".inst");
  if (inst && !past(inst)) {
    add(inst, inst, F.grow(20), { z: 0.86, dur: 1400, delay: 0 });
    add(inst, $(".inst-list-head", inst), F.wipe, { z: 0.75, dur: 800, delay: 260 });
    $$(".clip-btn", inst).forEach((b, i) => add(inst, b, F.fromLeft, { z: 0.72, dur: 900, delay: 320 + i * 65 + jit(10) }));
    $$(".inst-bar > *", inst).forEach((b, i) => add(inst, b, F.wipe, { z: 0.7, dur: 850, delay: 360 + i * 70 }));
    add(inst, $(".inst-meta", inst), F.wipe, { z: 0.8, dur: 900, delay: 520 });
    $$(".slider", inst).forEach((s, i) => add(inst, s, F.wipe, { z: 0.78, dur: 900, delay: 700 + i * 80 }));
    $$(".mini-card", inst).forEach((c, i) => add(inst, c, F.grow(14), { z: 0.84, dur: 1100, delay: 820 + i * 120 }));
  }

  // Who it is for: each photo opens from the bottom and settles from a slow zoom.
  $$(".who").forEach((w, i) => {
    if (past(w)) return;
    const d = i * 95 + jit(15);
    add(w, $(".who-img", w), F.grow(16), { z: 0.84, dur: 1300, delay: d });
    add(w, $(".who-img img", w), (v) => ({ transform: `scale(${(1.3 - 0.3 * v).toFixed(4)})` }), { z: 0.95, dur: 1900, delay: d });
    add(w, $(".who-n", w), F.wipe, { z: 0.7, dur: 800, delay: d + 380 });
    add(w, line($("h3", w)), F.rise, { z: 0.6, dur: 1000, delay: d + 420 });
    add(w, w.querySelector(":scope > p"), F.wipe, { z: 0.82, dur: 1000, delay: d + 540 });
  });

  // Specification: each row draws its rule, then its label and text.
  $$(".spec > div").forEach((r, i) => {
    if (past(r)) return;
    const d = (i % 2) * 90;
    add(r, rule(r, "draw-top"), F.growX, { z: 0.92, dur: 1200, delay: d });
    add(r, line($("dt", r)), F.rise, { z: 0.62, dur: 950, delay: d + 140 });
    add(r, $("dd", r), F.wipe, { z: 0.82, dur: 1000, delay: d + 220 });
  });
  const prof = $(".profiles");
  if (prof && !past(prof)) {
    add(prof, $(".eyebrow-navy", prof), F.wipe, { z: 0.72, dur: 800, delay: 0 });
    $$(".chip-static", prof).forEach((c, i) => add(prof, c, F.open, { z: 0.55, dur: 900, delay: 120 + i * 45 + jit(10) }));
  }

  // How it works: the navy floods in from the first step number and clears the farthest corner.
  const how = $(".how");
  if (how && !past(how)) {
    how.classList.add("flood-wait");
    const origin = $(".step-n", how);
    onStart(how, () => {
      const r = how.getBoundingClientRect(), o = origin ? origin.getBoundingClientRect() : r;
      const x = o.left - r.left + 6, y = o.top - r.top + 6;
      const R = Math.max(Math.hypot(x, y), Math.hypot(r.width - x, y), Math.hypot(x, r.height - y), Math.hypot(r.width - x, r.height - y)) * 1.05;
      const a = how.animate([
        { clipPath: `circle(0px at ${x}px ${y}px)` },
        { clipPath: `circle(${R.toFixed(0)}px at ${x}px ${y}px)` },
      ], { duration: 820, easing: "cubic-bezier(.7,0,.2,1)", fill: "both" });
      how.classList.remove("flood-wait");
      a.onfinish = () => a.cancel();
    });
    $$(".step > div", how).forEach((s, i) => {
      const d = 520 + i * 150 + jit(15);
      add(how, $(".step-n", s), F.wipe, { z: 0.7, dur: 850, delay: d });
      riseWords(how, words($("h3", s)), d + 80, 46);
      add(how, $("p:last-child", s), F.wipe, { z: 0.82, dur: 1000, delay: d + 300 });
    });
  }

  // Today and planned: the heads underline themselves, every tick draws, every clock turns.
  $$(".list-head").forEach((h) => {
    if (past(h)) return;
    add(h, rule(h, "draw-bottom"), F.growX, { z: 0.9, dur: 1300, delay: 0 });
    add(h, $(".list-title", h), F.wipe, { z: 0.7, dur: 800, delay: 120 });
    add(h, h.lastElementChild, F.wipe, { z: 0.8, dur: 900, delay: 200 });
  });
  let pseudoOk = true;
  try { const t = document.body.animate([{ opacity: 1 }, { opacity: 1 }], { duration: 1, pseudoElement: "::before" }); t.cancel(); } catch (_) { pseudoOk = false; }
  $$(".list li").forEach((li) => {
    if (past(li)) return;
    const clock = li.parentElement.classList.contains("list-clocks");
    add(li, li, F.wipe, { z: 0.8, dur: 900, delay: 60 + jit(20) });
    if (pseudoOk) {
      add(li, li, clock
        ? (v) => ({ transform: `rotate(${((1 - v) * -140).toFixed(2)}deg) scale(${Math.max(0, v).toFixed(4)})` })
        : (v) => ({ transform: `scale(${(0.6 + 0.4 * v).toFixed(4)})`, clipPath: `inset(0 ${clamp((1 - v) * 100, 0, 100).toFixed(2)}% 0 0)` }),
      { z: clock ? 0.6 : 0.7, dur: 900, delay: 220, pseudo: "::before" });
    }
  });
  $$(".price-card").forEach((c, i) => {
    if (past(c)) return;
    add(c, c, F.grow(14), { z: 0.84, dur: 1100, delay: i * 120 });
    add(c, line(c.children[1]), F.rise, { z: 0.58, dur: 1100, delay: i * 120 + 260 });
  });

  // Waitlist: the panel grows, the fields follow, the button gets its own beat and one press.
  const form = $(".form-panel");
  if (form && !past(form)) {
    add(form, form, F.grow(20), { z: 0.86, dur: 1300, delay: 0 });
    $$(".signup > .field, .signup > fieldset", form).forEach((f, i) => add(form, f, F.wipe, { z: 0.76, dur: 900, delay: 260 + i * 80 + jit(10) }));
    $$(".chip", form).forEach((c, i) => add(form, c, F.open, { z: 0.56, dur: 900, delay: 540 + i * 45 }));
    add(form, $(".privacy", form), F.wipe, { z: 0.82, dur: 900, delay: 760 });
    const btn = $("#signup-button");
    if (btn) {
      add(form, btn, F.open, { z: 0.52, dur: 1150, delay: 880 });
      onStart(form, () => pressPulse(btn, 2100));
    }
  }

  // Questions: rule, question, answer.
  $$(".faq-item").forEach((f, i) => {
    if (past(f)) return;
    const d = (i % 2) * 90;
    add(f, rule(f, "draw-top"), F.growX, { z: 0.92, dur: 1200, delay: d });
    add(f, line($("h3", f)), F.rise, { z: 0.62, dur: 1000, delay: d + 120 });
    add(f, $("p", f), F.wipe, { z: 0.82, dur: 1000, delay: d + 240 });
  });

  const foot = $(".footer-inner");
  if (foot && !past(foot)) $$(":scope > *", foot).forEach((c, i) => add(foot, c, F.wipe, { z: 0.75, dur: 900, delay: i * 90 }));

  // Start each group as its trigger comes into view.
  const io = new IntersectionObserver((es) => es.forEach((e) => {
    if (!e.isIntersecting) return;
    io.unobserve(e.target);
    (groups.get(e.target) || []).forEach((a) => a.play());
    (onPlay.get(e.target) || []).forEach((fn) => fn());
  }), { rootMargin: "0px 0px -8% 0px" });
  new Set([...groups.keys(), ...onPlay.keys()]).forEach((t) => io.observe(t));

  /* ---------- Scroll-linked: the playhead, the camera push, parallax ---------- */

  // The persistent actor: the page is a clip and the gold playhead is where you are in it.
  let rail = null, fill = null, head = null, ticks = [];
  const marks = ["instrument", "who", "spec", "how", "today", "waitlist", "faq"].map((id) => document.getElementById(id)).filter(Boolean);
  if (top) {
    rail = document.createElement("div");
    rail.className = "page-rail";
    rail.setAttribute("aria-hidden", "true");
    fill = document.createElement("span");
    fill.className = "page-rail-fill";
    head = document.createElement("span");
    head.className = "page-rail-head";
    ticks = marks.map(() => { const t = document.createElement("span"); t.className = "page-rail-tick"; rail.appendChild(t); return t; });
    rail.append(fill, head);
    top.appendChild(rail);
  }

  const heroHead = $(".hero-head"), heroMedia = $("#hero-media");
  const photos = $$(".who-img img");
  let geo = null;
  function measure() {
    const y = window.scrollY;
    geo = {
      vh: window.innerHeight,
      max: Math.max(1, document.documentElement.scrollHeight - window.innerHeight),
      railW: rail ? rail.offsetWidth : 0,
      hero: heroT ? heroT.offsetHeight : 0,
      marks: marks.map((m) => m.getBoundingClientRect().top + y),
      photos: photos.map((p) => { const r = p.parentElement.getBoundingClientRect(); return { mid: r.top + y + r.height / 2 }; }),
    };
    if (rail) ticks.forEach((t, i) => { t.style.left = (clamp((geo.marks[i] - 64) / geo.max, 0, 1) * 100).toFixed(2) + "%"; });
  }

  let queued = false;
  function draw() {
    queued = false;
    if (!geo) measure();
    const y = window.scrollY, g = geo;
    if (rail) {
      const p = clamp(y / g.max, 0, 1);
      fill.style.transform = `scaleX(${p.toFixed(4)})`;
      head.style.transform = `translateX(${(p * g.railW).toFixed(1)}px)`;
      ticks.forEach((t, i) => t.classList.toggle("on", y + 64 >= g.marks[i] - 1));
    }
    // Clamped rather than skipped off screen, so a fast jump never leaves a stale value behind.
    const hy = Math.min(y, g.hero + g.vh), p = clamp(hy / Math.max(1, g.hero), 0, 1.2);
    // The headline lifts away a little faster than the page, the footage pushes in.
    if (heroHead) heroHead.style.translate = `0 ${(hy * -0.1).toFixed(1)}px`;
    if (heroMedia) $$(".hero-vid", heroMedia).forEach((v) => { v.style.scale = (1 + 0.09 * p).toFixed(4); });
    if (heroT) heroT.style.setProperty("--lattice-y", (hy * 0.3).toFixed(1) + "px");
    photos.forEach((ph, i) => {
      const d = (g.photos[i].mid - (y + g.vh / 2)) / g.vh;
      if (Math.abs(d) < 1.4) ph.style.translate = `0 ${(d * -34).toFixed(1)}px`;
    });
  }
  const ask = () => { if (!queued) { queued = true; requestAnimationFrame(draw); } };
  window.addEventListener("scroll", ask, { passive: true });
  window.addEventListener("resize", () => { geo = null; ask(); });
  if ("ResizeObserver" in window) new ResizeObserver(() => { geo = null; ask(); }).observe(document.body);
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(() => { geo = null; ask(); });
  window.addEventListener("load", () => { geo = null; ask(); });
  ask();
})();
