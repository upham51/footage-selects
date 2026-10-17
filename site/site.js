"use strict";
// Waitlist form. Without JavaScript the form still posts to /api/signup, and the
// Worker shows the result when the page comes back with ?signup=... in the address.

(function () {
  const form = document.getElementById("signup-form");
  if (!form) return;
  const statusBox = document.getElementById("form-status");
  const button = document.getElementById("signup-button");
  const otherBox = document.getElementById("shoots-other-box");
  const otherText = document.getElementById("shoots_other");
  document.getElementById("started").value = String(Date.now());

  const ICON = {
    tick: '<svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><path d="M2.5 7.5l3 3 6-7"/></svg>',
    triangle: '<svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><path d="M7 1.8l5.6 9.9H1.4z"/><path d="M7 5.6v2.6M7 10.1v.1"/></svg>',
  };
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  function show(kind, title, body) {
    if (kind === "sending") {
      statusBox.innerHTML = `<div class="status status-sending"><span class="spinner" aria-hidden="true"></span><p>${esc(title)}</p></div>`;
    } else {
      const icon = kind === "done" ? ICON.tick : ICON.triangle;
      statusBox.innerHTML = `<div class="status status-${kind}" role="${kind === "failed" ? "alert" : "status"}">${icon}<div><strong>${esc(title)}</strong><p>${esc(body)}</p></div></div>`;
    }
  }

  function syncOther() {
    otherText.hidden = !otherBox.checked;
    if (!otherBox.checked) otherText.value = "";
  }
  otherBox.addEventListener("change", syncOther);
  syncOther();

  const FIELDS = ["email", "editor", "shoots", "hours_per_month"];
  // The inputs a question's message belongs to. "What do you shoot?" has the boxes and the text box.
  function inputsFor(name) {
    return name === "shoots" ? [...form.querySelectorAll('[name="shoots"]'), otherText] : [document.getElementById(name)].filter(Boolean);
  }
  function clearField(name) {
    const el = document.getElementById(`${name}-error`);
    if (!el) return;
    el.hidden = true;
    el.textContent = "";
    for (const input of inputsFor(name)) input.removeAttribute("aria-invalid");
  }
  function clearErrors() {
    for (const f of FIELDS) clearField(f);
  }
  // An error goes away as soon as that answer is changed.
  const changed = (e) => clearField(e.target.name === "shoots_other" ? "shoots" : e.target.name);
  form.addEventListener("input", changed);
  form.addEventListener("change", changed);

  // Shows a message under the question. The first problem also gets the focus.
  function fieldError(field, message, focus = true) {
    const key = field === "shoots_other" ? "shoots" : field;
    const el = document.getElementById(`${key}-error`);
    if (!el) return false;
    el.textContent = message;
    el.hidden = false;
    const input = document.getElementById(field) || document.getElementById(key) || form.querySelector(`[name="${key}"]`);
    if (input) {
      input.setAttribute("aria-invalid", "true");
      if (focus) input.focus();
    }
    return true;
  }

  function collect() {
    const fd = new FormData(form);
    const data = {};
    for (const [k, v] of fd.entries()) {
      if (k === "shoots") (data.shoots = data.shoots || []).push(v);
      else data[k] = v;
    }
    data.shoots = data.shoots || [];
    return data;
  }

  // Quick checks before sending, returning every problem at once. The server checks everything again.
  function check(d) {
    const problems = [];
    const email = (d.email || "").trim();
    if (!email) problems.push(["email", "Enter your email address."]);
    else if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) problems.push(["email", "That email address does not look complete. Check for a missing @ or ending such as .com."]);
    if (!d.editor) problems.push(["editor", "Choose the editor you use, or that you don't edit yet."]);
    if (d.shoots.length === 0) problems.push(["shoots", "Choose at least one kind of footage you shoot."]);
    else if (d.shoots.includes("other") && !(d.shoots_other || "").trim()) problems.push(["shoots_other", "Say what else you shoot, or untick Something else."]);
    if (!d.hours_per_month) problems.push(["hours_per_month", "Choose roughly how many hours of footage you shoot a month."]);
    return problems;
  }

  function done() {
    form.hidden = true;
    show("done", "You are on the list", "Thank you. You will get an email when there is something to try. Nothing else will be sent.");
  }

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    clearErrors();
    statusBox.innerHTML = "";
    const data = collect();
    const problems = check(data);
    if (problems.length) {
      problems.forEach(([field, message], i) => fieldError(field, message, i === 0));
      return;
    }
    button.disabled = true;
    show("sending", "Adding you to the waitlist");
    try {
      const res = await fetch(form.action, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(data) });
      let out = {};
      try { out = await res.json(); } catch (_) { /* handled below */ }
      if (res.ok && out.ok) { done(); return; }
      if (out.field && fieldError(out.field, out.message)) { statusBox.innerHTML = ""; return; }
      show("failed", "You were not added yet", out.message || "Something went wrong on our side and nothing was saved. Try again in a few minutes.");
    } catch (_) {
      show("failed", "You were not added yet", "The waitlist could not be reached, so nothing was saved. Check your internet connection and try again.");
    } finally {
      button.disabled = false;
      if (window.turnstile && typeof window.turnstile.reset === "function") window.turnstile.reset();
    }
  });

})();
