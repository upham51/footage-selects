// The landing page and waitlist, on one Cloudflare Worker.
//
//   GET  /                     the page (site/index.html with name, price and video filled in)
//   POST /api/signup           add someone to the waitlist
//   GET  /api/admin/stats      signup count and breakdown        (needs ADMIN_TOKEN)
//   GET  /api/admin/export.csv every signup as a spreadsheet     (needs ADMIN_TOKEN)
//   anything else              files from site/, served by Cloudflare directly
//
// Bindings (worker/wrangler.toml): DB (D1), ASSETS (site/), SIGNUP_LIMITER (rate limit).
// Secrets, set in the Cloudflare dashboard: ADMIN_TOKEN (required for the admin
// page), TURNSTILE_SECRET_KEY (optional spam check).
// No IP address or other tracking data is stored with a signup.

import { validateSignup, EDITORS, SHOOTS, HOURS } from "./validate.js";
import { fillPage } from "./page.js";

const MAX_BODY = 4096;
const MIN_FILL_MS = 2500;

const SECURITY_HEADERS = {
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "X-Frame-Options": "DENY",
};

function json(data, status = 200, extra = {}) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store", ...SECURITY_HEADERS, ...extra },
  });
}

function fail(status, code, message, field) {
  return json({ ok: false, code, message, ...(field ? { field } : {}) }, status);
}

/** Form posts without JavaScript get sent back to the page with a status. */
function redirectBack(request, status) {
  const url = new URL(request.url);
  return new Response(null, { status: 303, headers: { Location: `${url.origin}/?signup=${status}#waitlist`, ...SECURITY_HEADERS } });
}

async function readBody(request) {
  const type = (request.headers.get("Content-Type") || "").toLowerCase();
  const length = Number(request.headers.get("Content-Length") || 0);
  if (length > MAX_BODY) return { error: "too_large" };
  const text = await request.text();
  if (text.length > MAX_BODY) return { error: "too_large" };
  if (type.startsWith("application/json")) {
    try {
      const data = JSON.parse(text);
      return { data: data && typeof data === "object" ? data : {}, form: false };
    } catch {
      return { error: "bad_json" };
    }
  }
  if (type.startsWith("application/x-www-form-urlencoded")) {
    const p = new URLSearchParams(text);
    const data = {};
    for (const key of new Set(p.keys())) data[key] = key === "shoots" ? p.getAll(key) : p.get(key);
    return { data, form: true };
  }
  return { error: "bad_type" };
}

function sameOrigin(request) {
  const origin = request.headers.get("Origin");
  if (!origin) return true; // Non-browser clients send none; the rate limit still applies.
  return origin === new URL(request.url).origin;
}

async function verifyTurnstile(env, token, ip) {
  if (!env.TURNSTILE_SECRET_KEY) return true;
  if (!token) return false;
  const body = new FormData();
  body.append("secret", env.TURNSTILE_SECRET_KEY);
  body.append("response", token);
  if (ip) body.append("remoteip", ip);
  try {
    const r = await fetch("https://challenges.cloudflare.com/turnstile/v0/siteverify", { method: "POST", body });
    const out = await r.json();
    return out.success === true;
  } catch {
    return false;
  }
}

export async function handleSignup(request, env) {
  if (!sameOrigin(request)) return fail(403, "wrong_origin", "Signups are only accepted from the waitlist page.");

  const ip = request.headers.get("CF-Connecting-IP") || "unknown";
  if (env.SIGNUP_LIMITER) {
    const { success } = await env.SIGNUP_LIMITER.limit({ key: `signup:${ip}` });
    if (!success) return fail(429, "rate_limited", "Too many signups came from your connection in the last minute. Nothing was saved. Wait a minute and try again.");
  }

  const body = await readBody(request);
  if (body.error) {
    const msg = body.error === "too_large" ? "That form was larger than expected, so it was not saved. Reload the page and try again." : "The form could not be read, so nothing was saved. Reload the page and try again.";
    return fail(body.error === "too_large" ? 413 : 400, body.error, msg);
  }
  const { data, form } = body;

  // Spam checks. Bots that trip them are told it worked, and nothing is stored.
  const honeypot = typeof data.website === "string" && data.website.trim() !== "";
  const started = Number(data.started);
  const tooFast = Number.isFinite(started) && started > 0 && Date.now() - started < MIN_FILL_MS;
  if (honeypot || tooFast) return form ? redirectBack(request, "ok") : json({ ok: true });

  const v = validateSignup(data);
  if (!v.ok) return form ? redirectBack(request, "invalid") : fail(400, "invalid", v.message, v.field);

  if (!(await verifyTurnstile(env, data["cf-turnstile-response"], ip))) {
    return form ? redirectBack(request, "check") : fail(400, "check_failed", "The spam check did not pass, so nothing was saved. Complete the check above the button and try again.");
  }

  const s = v.value;
  try {
    // A repeat signup is ignored quietly, so the form never reveals who is on the list.
    await env.DB.prepare(
      "INSERT INTO waitlist (email, editor, shoots, shoots_other, hours_per_month) VALUES (?, ?, ?, ?, ?) ON CONFLICT(email_normalized) DO NOTHING",
    )
      .bind(s.email, s.editor, JSON.stringify(s.shoots), s.shoots_other, s.hours_per_month)
      .run();
  } catch (e) {
    console.error("waitlist insert failed", e && e.message);
    return form ? redirectBack(request, "error") : fail(503, "store_failed", "The waitlist could not be reached just now, so your signup was not saved. Try again in a few minutes.");
  }
  return form ? redirectBack(request, "ok") : json({ ok: true });
}

function timingSafeEqual(a, b) {
  const enc = new TextEncoder();
  const x = enc.encode(a), y = enc.encode(b);
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) diff |= (x[i] || 0) ^ (y[i] || 0);
  return diff === 0;
}

function authorized(request, env) {
  if (!env.ADMIN_TOKEN) return { ok: false, res: fail(503, "admin_not_set_up", "The admin page is not set up yet. Add a secret called ADMIN_TOKEN to this Worker in the Cloudflare dashboard.") };
  const header = request.headers.get("Authorization") || "";
  const token = header.startsWith("Bearer ") ? header.slice(7) : "";
  if (!token || !timingSafeEqual(token, env.ADMIN_TOKEN)) return { ok: false, res: fail(401, "wrong_token", "That admin password is not right. Nothing was shown.") };
  return { ok: true };
}

export async function handleStats(env) {
  const total = (await env.DB.prepare("SELECT count(*) AS n FROM waitlist").first())?.n ?? 0;
  const week = (await env.DB.prepare("SELECT count(*) AS n FROM waitlist WHERE created_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-7 days')").first())?.n ?? 0;
  const group = async (col) => {
    const { results } = await env.DB.prepare(`SELECT ${col} AS k, count(*) AS n FROM waitlist GROUP BY ${col}`).all();
    return Object.fromEntries((results || []).map((r) => [r.k, r.n]));
  };
  const { results: shootRows } = await env.DB.prepare(
    "SELECT j.value AS k, count(*) AS n FROM waitlist, json_each(waitlist.shoots) AS j GROUP BY j.value",
  ).all();
  const latest = (await env.DB.prepare("SELECT max(created_at) AS t FROM waitlist").first())?.t ?? null;
  return json({
    ok: true,
    total,
    last_7_days: week,
    latest,
    editor: await group("editor"),
    hours_per_month: await group("hours_per_month"),
    shoots: Object.fromEntries((shootRows || []).map((r) => [r.k, r.n])),
    labels: { editor: EDITORS, shoots: SHOOTS, hours_per_month: HOURS },
  });
}

/** One CSV cell. Cells that a spreadsheet would run as a formula are made plain text. */
export function csvCell(v) {
  let s = v == null ? "" : String(v);
  if (/^[=+\-@\t\r]/.test(s)) s = `'${s}`;
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export async function handleExport(env) {
  const { results } = await env.DB.prepare(
    "SELECT created_at, email, editor, shoots, shoots_other, hours_per_month FROM waitlist ORDER BY created_at",
  ).all();
  const lines = [["Signed up (UTC)", "Email", "Editor", "Shoots", "Shoots (other)", "Hours of footage a month"].map(csvCell).join(",")];
  for (const r of results || []) {
    let shoots = [];
    try { shoots = JSON.parse(r.shoots); } catch { /* stored by this Worker, so always valid */ }
    lines.push([
      r.created_at,
      r.email,
      EDITORS[r.editor] || r.editor,
      shoots.map((s) => SHOOTS[s] || s).join("; "),
      r.shoots_other,
      HOURS[r.hours_per_month] || r.hours_per_month,
    ].map(csvCell).join(","));
  }
  const day = new Date().toISOString().slice(0, 10);
  return new Response(lines.join("\r\n") + "\r\n", {
    headers: {
      "Content-Type": "text/csv; charset=utf-8",
      "Content-Disposition": `attachment; filename="waitlist-${day}.csv"`,
      "Cache-Control": "no-store",
      ...SECURITY_HEADERS,
    },
  });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;

    if (path === "/api/signup") {
      if (request.method !== "POST") return fail(405, "method", "Send the form with POST.");
      return handleSignup(request, env);
    }
    if (path === "/api/admin/stats" || path === "/api/admin/export.csv") {
      if (request.method !== "GET") return fail(405, "method", "Use GET.");
      const auth = authorized(request, env);
      if (!auth.ok) return auth.res;
      try {
        return path.endsWith("stats") ? await handleStats(env) : await handleExport(env);
      } catch (e) {
        console.error("admin query failed", e && e.message);
        return fail(503, "store_failed", "The waitlist database could not be read just now. Nothing was changed. Try again in a minute.");
      }
    }
    if (path.startsWith("/api/")) return fail(404, "not_found", "There is nothing at this address.");

    if (path === "/index.html") return Response.redirect(`${url.origin}/${url.search}`, 301);
    if (path === "/") {
      const page = await env.ASSETS.fetch(new Request(url.origin + "/", { headers: request.headers }));
      return fillPage(page, env, undefined, url.searchParams.get("signup") || "");
    }
    return env.ASSETS.fetch(request);
  },
};
