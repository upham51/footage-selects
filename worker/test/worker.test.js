import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import worker, { csvCell, handleSignup } from "../src/index.js";
import { validateSignup } from "../src/validate.js";
import { fakeD1 } from "./fake-d1.js";

const ORIGIN = "https://waitlist.example";
const good = { email: "Maya@Example.com", editor: "resolve", shoots: ["travel", "drone"], hours_per_month: "5_to_15", started: String(Date.now() - 10_000) };

let env;
let realFetch;
beforeEach(() => {
  env = { DB: fakeD1(), ASSETS: { fetch: async () => new Response("asset") } };
  realFetch = globalThis.fetch;
});
afterEach(() => { globalThis.fetch = realFetch; });

function post(body, { type = "application/json", origin = ORIGIN, ip = "203.0.113.9" } = {}) {
  const headers = { "Content-Type": type, "CF-Connecting-IP": ip };
  if (origin) headers.Origin = origin;
  return new Request(`${ORIGIN}/api/signup`, { method: "POST", headers, body: type.includes("json") ? JSON.stringify(body) : body });
}
const rows = () => env.DB.raw.prepare("SELECT * FROM waitlist ORDER BY id").all();

test("a valid signup is stored once, with nothing that identifies the visitor's connection", async () => {
  const res = await worker.fetch(post(good), env);
  assert.equal(res.status, 200);
  assert.deepEqual(await res.json(), { ok: true });
  const r = rows();
  assert.equal(r.length, 1);
  assert.equal(r[0].email_normalized, "maya@example.com");
  assert.deepEqual(JSON.parse(r[0].shoots), ["travel", "drone"]);
  assert.ok(!Object.keys(r[0]).some((k) => /ip|agent|country/i.test(k)), "no tracking columns");
});

test("signing up twice gives the same answer and keeps one row", async () => {
  await worker.fetch(post(good), env);
  const again = await worker.fetch(post({ ...good, email: " maya@EXAMPLE.com " }), env);
  assert.equal(again.status, 200);
  assert.equal(rows().length, 1);
});

test("missing or unknown answers are refused with the field and a plain message", async () => {
  const cases = [
    [{ ...good, email: "maya" }, "email"],
    [{ ...good, editor: "imovie" }, "editor"],
    [{ ...good, shoots: [] }, "shoots"],
    [{ ...good, shoots: ["cats"] }, "shoots"],
    [{ ...good, hours_per_month: "lots" }, "hours_per_month"],
    [{ ...good, shoots: ["other"], shoots_other: "x".repeat(101) }, "shoots_other"],
  ];
  for (const [body, field] of cases) {
    const res = await worker.fetch(post(body), env);
    assert.equal(res.status, 400, field);
    const out = await res.json();
    assert.equal(out.field, field);
    assert.match(out.message, /\.$/);
    assert.doesNotMatch(out.message, /!/);
  }
  assert.equal(rows().length, 0);
});

test("bots that fill the hidden field or answer too fast are told it worked but nothing is stored", async () => {
  const a = await worker.fetch(post({ ...good, website: "http://spam.example" }), env);
  const b = await worker.fetch(post({ ...good, started: String(Date.now() - 300) }), env);
  assert.equal(a.status, 200);
  assert.equal(b.status, 200);
  assert.equal(rows().length, 0);
});

test("the rate limit stops a flood from one connection", async () => {
  const seen = [];
  env.SIGNUP_LIMITER = { limit: async ({ key }) => { seen.push(key); return { success: seen.length <= 5 }; } };
  for (let i = 0; i < 5; i++) assert.equal((await worker.fetch(post({ ...good, email: `p${i}@example.com` }), env)).status, 200);
  const blocked = await worker.fetch(post({ ...good, email: "p6@example.com" }), env);
  assert.equal(blocked.status, 429);
  assert.match((await blocked.json()).message, /Nothing was saved/);
  assert.equal(seen[0], "signup:203.0.113.9");
  assert.equal(rows().length, 5);
});

test("posts from another site, oversized bodies and unknown formats are refused", async () => {
  assert.equal((await worker.fetch(post(good, { origin: "https://evil.example" }), env)).status, 403);
  assert.equal((await worker.fetch(post({ ...good, shoots_other: "x".repeat(5000) }), env)).status, 413);
  assert.equal((await worker.fetch(post("hello", { type: "text/plain" }), env)).status, 400);
  assert.equal((await worker.fetch(new Request(`${ORIGIN}/api/signup`), env)).status, 405);
  assert.equal(rows().length, 0);
});

test("the form also works without JavaScript and returns to the page", async () => {
  const body = "email=lee%40example.com&editor=premiere&shoots=events&shoots=weddings&hours_per_month=over_40&website=";
  const res = await worker.fetch(post(body, { type: "application/x-www-form-urlencoded" }), env);
  assert.equal(res.status, 303);
  assert.equal(res.headers.get("Location"), `${ORIGIN}/?signup=ok#waitlist`);
  assert.deepEqual(JSON.parse(rows()[0].shoots), ["events", "weddings"]);
  const bad = await worker.fetch(post("email=lee", { type: "application/x-www-form-urlencoded" }), env);
  assert.equal(bad.headers.get("Location"), `${ORIGIN}/?signup=invalid#waitlist`);
});

test("when the spam check is set up, a failed check saves nothing and a passed one saves (recorded responses)", async () => {
  env.TURNSTILE_SECRET_KEY = "test-secret-not-real";
  const calls = [];
  globalThis.fetch = async (url, init) => {
    calls.push(url);
    const token = init.body.get("response");
    // Shapes recorded from Cloudflare's documented siteverify responses.
    const reply = token === "good-token"
      ? { success: true, "error-codes": [], challenge_ts: "2026-10-17T19:40:00.000Z", hostname: "waitlist.example" }
      : { success: false, "error-codes": ["invalid-input-response"] };
    return new Response(JSON.stringify(reply), { headers: { "Content-Type": "application/json" } });
  };
  const noToken = await worker.fetch(post(good), env);
  assert.equal(noToken.status, 400);
  assert.equal(calls.length, 0, "no token means no call out");
  const badToken = await worker.fetch(post({ ...good, "cf-turnstile-response": "bad-token" }), env);
  assert.equal((await badToken.json()).code, "check_failed");
  const goodToken = await worker.fetch(post({ ...good, "cf-turnstile-response": "good-token" }), env);
  assert.equal(goodToken.status, 200);
  assert.equal(calls[0], "https://challenges.cloudflare.com/turnstile/v0/siteverify");
  assert.equal(rows().length, 1);
});

test("if the database is down, the visitor is told nothing was saved", async () => {
  env.DB = fakeD1({ failing: true });
  const res = await worker.fetch(post(good), env);
  assert.equal(res.status, 503);
  assert.match((await res.json()).message, /not saved/);
});

test("the admin pages need the admin password", async () => {
  const get = (path, token) => new Request(`${ORIGIN}${path}`, { headers: token ? { Authorization: `Bearer ${token}` } : {} });
  assert.equal((await worker.fetch(get("/api/admin/stats", "x"), env)).status, 503, "not set up yet");
  env.ADMIN_TOKEN = "correct horse battery staple";
  assert.equal((await worker.fetch(get("/api/admin/stats"), env)).status, 401);
  assert.equal((await worker.fetch(get("/api/admin/stats", "wrong"), env)).status, 401);
  assert.equal((await worker.fetch(get("/api/admin/export.csv", "correct horse battery staple x"), env)).status, 401);
});

test("the admin count, breakdown and spreadsheet match what was stored", async () => {
  env.ADMIN_TOKEN = "t0ken";
  await worker.fetch(post(good), env);
  await worker.fetch(post({ ...good, email: "=cmd@example.com", editor: "final_cut", shoots: ["drone", "other"], shoots_other: 'Boats, "mostly"', hours_per_month: "under_2" }), env);
  const auth = { headers: { Authorization: "Bearer t0ken" } };
  const stats = await (await worker.fetch(new Request(`${ORIGIN}/api/admin/stats`, auth), env)).json();
  assert.equal(stats.total, 2);
  assert.equal(stats.last_7_days, 2);
  assert.deepEqual(stats.editor, { final_cut: 1, resolve: 1 });
  assert.equal(stats.shoots.drone, 2);
  assert.equal(stats.labels.hours_per_month.under_2, "Under 2 hours");

  const res = await worker.fetch(new Request(`${ORIGIN}/api/admin/export.csv`, auth), env);
  assert.match(res.headers.get("Content-Disposition"), /attachment; filename="waitlist-\d{4}-\d{2}-\d{2}\.csv"/);
  const lines = (await res.text()).trim().split("\r\n");
  assert.equal(lines.length, 3);
  assert.equal(lines[0], "Signed up (UTC),Email,Editor,Shoots,Shoots (other),Hours of footage a month");
  assert.match(lines[1], /Maya@Example.com,DaVinci Resolve,Travel; Drone,,5 to 15 hours$/);
  assert.match(lines[2], /,'=cmd@example.com,Final Cut Pro,Drone; Something else,"Boats, ""mostly""",Under 2 hours$/);
});

test("spreadsheet cells that would run as formulas are made plain text", () => {
  assert.equal(csvCell("=SUM(A1)"), "'=SUM(A1)");
  assert.equal(csvCell("+1"), "'+1");
  assert.equal(csvCell("@x"), "'@x");
  assert.equal(csvCell("a,b"), '"a,b"');
  assert.equal(csvCell(null), "");
});

test("unknown API paths are 404 and other paths go to the site files", async () => {
  assert.equal((await worker.fetch(new Request(`${ORIGIN}/api/nope`), env)).status, 404);
  assert.equal(await (await worker.fetch(new Request(`${ORIGIN}/styles.css`), env)).text(), "asset");
});

test("validation trims, drops a stray 'other' description and removes duplicates", () => {
  const v = validateSignup({ email: "  a@b.co ", editor: "none", shoots: ["travel", "travel"], shoots_other: "ignored", hours_per_month: "over_40" });
  assert.ok(v.ok);
  assert.deepEqual(v.value, { email: "a@b.co", editor: "none", shoots: ["travel"], shoots_other: null, hours_per_month: "over_40" });
});

test("a Turnstile secret without a site key on the page never blocks signups", async () => {
  env.TURNSTILE_SECRET_KEY = "test-secret-not-real";
  globalThis.fetch = async () => { throw new Error("must not call Turnstile"); };
  const res = await handleSignup(post(good), env, "");
  assert.equal(res.status, 200);
  assert.equal(rows().length, 1);
});
