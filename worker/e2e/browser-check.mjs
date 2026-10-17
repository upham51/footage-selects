// End-to-end check of the page and Worker in a real browser, against
// `npx wrangler dev` (local D1, test admin password in .dev.vars).
//
//   node e2e/browser-check.mjs http://127.0.0.1:8787 <admin password> <screenshot folder>
//
// Needs Playwright. Not part of the automatic tests; run by hand before publishing.
// Clear the local table and wait a minute between runs, or the rate limit (5 a
// minute) will refuse the signups, which is the protection working.
import { chromium } from "playwright";

const [base, adminToken, shots = "."] = process.argv.slice(2);
const results = [];
const check = (name, ok, detail = "") => { results.push({ name, ok, detail }); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? `: ${detail}` : ""}`); };

const browser = await chromium.launch();
const errors = [];
const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await ctx.newPage();
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
page.on("pageerror", (e) => errors.push(String(e)));

await page.goto(base + "/");
check("page title carries the product name", /: pull the usable shots/.test(await page.title()), await page.title());
check("price filled from config", (await page.textContent('#pricing [data-fill="price"]')).trim().startsWith("$"));
await page.screenshot({ path: `${shots}/site-desktop.png`, fullPage: true });

await page.click("#signup-button");
check("empty form shows a field error", await page.isVisible("#email-error"));
await page.fill("#email", "tester+e2e@example.com");
await page.selectOption("#editor", "resolve");
await page.check('input[name="shoots"][value="drone"]');
await page.check("#shoots-other-box");
check("other description appears when chosen", await page.isVisible("#shoots_other"));
await page.fill("#shoots_other", "Boats");
await page.selectOption("#hours_per_month", "5_to_15");
await page.waitForTimeout(2700);
await page.screenshot({ path: `${shots}/site-form-filled.png`, clip: await (await page.$(".form-panel")).boundingBox() });
await page.click("#signup-button");
await page.waitForSelector(".status-done", { timeout: 8000 });
check("signup shows the done state", true);
await page.screenshot({ path: `${shots}/site-form-done.png`, clip: await (await page.$(".form-panel")).boundingBox() });

// Same email again, different case: still a done message, still one row.
await page.goto(base + "/");
await page.fill("#email", "TESTER+e2e@example.com");
await page.selectOption("#editor", "premiere");
await page.check('input[name="shoots"][value="travel"]');
await page.selectOption("#hours_per_month", "under_2");
await page.waitForTimeout(2700);
await page.click("#signup-button");
await page.waitForSelector(".status-done", { timeout: 8000 });
check("repeat signup looks the same to the visitor", true);

// Without JavaScript.
const nojs = await browser.newContext({ javaScriptEnabled: false, reducedMotion: "reduce" });
const p2 = await nojs.newPage();
await p2.goto(base + "/");
await p2.fill("#email", "nojs@example.com");
await p2.selectOption("#editor", "final_cut");
await p2.check('input[name="shoots"][value="events"]');
await p2.selectOption("#hours_per_month", "over_40");
await Promise.all([p2.waitForNavigation(), p2.click("#signup-button")]);
check("no-JavaScript signup comes back to the page with a done message", (await p2.isVisible(".status-done")) && p2.url().includes("signup=ok"), p2.url());
await nojs.close();

// Admin page.
await page.goto(base + "/admin.html");
await page.fill("#token", "wrong");
await page.click('#login button[type="submit"]');
await page.waitForSelector(".status-failed");
check("wrong admin password is refused plainly", /not right/.test(await page.textContent(".status-failed")));
await page.fill("#token", adminToken);
await page.click('#login button[type="submit"]');
await page.waitForSelector(".stat .n");
const total = (await page.textContent(".stat .n")).trim();
check("admin count shows the two people (repeat not counted)", total === "2", `count ${total}`);
await page.screenshot({ path: `${shots}/site-admin.png`, fullPage: true });
const [download] = await Promise.all([page.waitForEvent("download"), page.click("#download")]);
const csv = await (await download.createReadStream()).toArray().then((c) => Buffer.concat(c).toString());
check("CSV export has a header and two rows", csv.trim().split(/\r?\n/).length === 3, csv.split("\r\n")[0]);

// Rate limit: hammer the API from one connection.
const statuses = [];
for (let i = 0; i < 12; i++) {
  const r = await page.request.post(base + "/api/signup", { data: { email: `flood${i}@example.com`, editor: "resolve", shoots: ["drone"], hours_per_month: "under_2" }, headers: { Origin: base } });
  statuses.push(r.status());
}
check("rate limit answers 429 once a connection floods", statuses.includes(429), statuses.join(","));

// Phone width.
const phone = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 2 });
const p3 = await phone.newPage();
await p3.goto(base + "/");
const overflow = await p3.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
check("no sideways scrolling on a phone", overflow <= 0, `overflow ${overflow}px`);
await p3.screenshot({ path: `${shots}/site-phone.png`, fullPage: true });
await phone.close();

// The deliberate wrong-password attempt above logs one 401; anything else is a real problem.
const unexpected = errors.filter((e) => !/status of 401/.test(e));
check("no browser console errors (including blocked content)", unexpected.length === 0, unexpected.join(" | "));
await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length} of ${results.length} checks passed`);
process.exit(failed.length ? 1 : 0);
