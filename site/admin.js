"use strict";
// Private waitlist page: count, breakdown and a CSV download.

(function () {
  const login = document.getElementById("login");
  const out = document.getElementById("out");
  const tokenInput = document.getElementById("token");
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const tri = '<svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><path d="M7 1.8l5.6 9.9H1.4z"/><path d="M7 5.6v2.6M7 10.1v.1"/></svg>';

  let token = "";
  try { token = sessionStorage.getItem("waitlist-admin") || ""; } catch (_) { /* private mode */ }

  function failed(message) {
    out.innerHTML = `<div class="status status-failed" role="alert">${tri}<div><strong>Signups could not be shown</strong><p>${esc(message)}</p></div></div>`;
  }

  function table(title, counts, labels) {
    const rows = Object.entries(labels)
      .map(([k, label]) => [label, counts[k] || 0])
      .sort((a, b) => b[1] - a[1]);
    return `<div class="panel breakdown"><table><thead><tr><th>${esc(title)}</th><th class="num">People</th></tr></thead><tbody>${rows
      .map(([label, n]) => `<tr><td>${esc(label)}</td><td class="num">${n}</td></tr>`)
      .join("")}</tbody></table></div>`;
  }

  async function load() {
    out.innerHTML = '<div class="status status-sending"><span class="spinner" aria-hidden="true"></span><p>Loading signups</p></div>';
    let res, data = {};
    try {
      res = await fetch("/api/admin/stats", { headers: { Authorization: `Bearer ${token}` } });
      data = await res.json();
    } catch (_) {
      failed("The waitlist could not be reached. Check your internet connection and try again.");
      return;
    }
    if (!res.ok || !data.ok) {
      if (res.status === 401) { try { sessionStorage.removeItem("waitlist-admin"); } catch (_) {} login.hidden = false; }
      failed(data.message || "Something went wrong reading the waitlist. Nothing was changed.");
      return;
    }
    login.hidden = true;
    if (data.total === 0) {
      out.innerHTML = '<div class="panel empty-state"><p><strong>No signups yet.</strong></p><p>When people join from the page, they appear here.</p></div>';
      return;
    }
    const latest = data.latest ? new Date(data.latest).toLocaleString() : "";
    out.innerHTML = `
      <div class="stat-row">
        <div class="panel stat"><div class="n">${data.total}</div><div class="k">${data.total === 1 ? "person" : "people"} on the waitlist</div></div>
        <div class="panel stat"><div class="n">${data.last_7_days}</div><div class="k">joined in the last 7 days</div></div>
        <div class="panel stat"><div class="n n-small">${esc(latest)}</div><div class="k">latest signup</div></div>
      </div>
      <button class="btn btn-primary" id="download" type="button">Download the list as a spreadsheet</button>
      ${table("Editor", data.editor, data.labels.editor)}
      ${table("What they shoot (people can pick several)", data.shoots, data.labels.shoots)}
      ${table("Hours of footage a month", data.hours_per_month, data.labels.hours_per_month)}`;
    document.getElementById("download").addEventListener("click", download);
  }

  async function download() {
    try {
      const res = await fetch("/api/admin/export.csv", { headers: { Authorization: `Bearer ${token}` } });
      if (!res.ok) { const d = await res.json().catch(() => ({})); failed(d.message || "The download did not start. Nothing was changed."); return; }
      const blob = await res.blob();
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `waitlist-${new Date().toISOString().slice(0, 10)}.csv`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(a.href), 2000);
    } catch (_) {
      failed("The download could not reach the waitlist. Check your internet connection and try again.");
    }
  }

  login.addEventListener("submit", (e) => {
    e.preventDefault();
    token = tokenInput.value.trim();
    try { sessionStorage.setItem("waitlist-admin", token); } catch (_) {}
    load();
  });
  if (token) load();
})();
