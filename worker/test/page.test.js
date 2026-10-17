import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { pageValues, PRODUCT_NAME } from "../src/page.js";

const site = (f) => readFileSync(new URL(`../../site/${f}`, import.meta.url), "utf8");
const config = JSON.parse(site("config.json"));
const appName = JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8")).productName;

/** Text a visitor reads: tags, scripts and styles removed. */
function visibleText(html) {
  return html.replace(/<script[\s\S]*?<\/script>/g, " ").replace(/<style[\s\S]*?<\/style>/g, " ").replace(/<[^>]+>/g, " ").replace(/&[a-z]+;/g, " ");
}

test("the product name comes from the app's single name setting", () => {
  assert.equal(PRODUCT_NAME, appName);
  assert.equal(pageValues().productName, appName);
  for (const f of ["index.html", "admin.html", "404.html", "site.js", "admin.js"]) {
    assert.ok(!site(f).includes(appName), `${f} must not spell out the product name`);
  }
});

test("the planned price comes from site/config.json and is not typed into the page", () => {
  const v = pageValues(config, "Name");
  assert.equal(v.price, `$${config.plannedPrice.min} to $${config.plannedPrice.max} a month`);
  assert.equal(pageValues({ ...config, plannedPrice: { ...config.plannedPrice, min: 9, max: 15 } }, "N").price, "$9 to $15 a month");
  assert.doesNotMatch(site("index.html"), /\$\d/);
});

test("the planned price is labelled as planned and not charged, wherever it appears", () => {
  const html = site("index.html");
  const pricing = html.slice(html.indexOf('id="pricing"'), html.indexOf('id="waitlist"'));
  assert.match(pricing, /Planned, not charged/);
  assert.match(pricing, /nothing is charged today/);
  const faqPrice = html.slice(html.indexOf("Is it free?"), html.indexOf("Does it use AI?"));
  assert.match(faqPrice, /planned/);
  assert.match(faqPrice, /nothing is charged today/);
});

test("the page has every section the plan asks for", () => {
  const html = site("index.html");
  for (const needle of ['id="demo-slot"', 'id="who"', "Travel", "Drone", "Real estate", "Events", 'id="how"', 'class="steps"', 'id="today"', "Planned", 'id="pricing"', 'id="waitlist"', 'name="email"', 'name="editor"', 'name="shoots"', 'name="hours_per_month"', 'class="privacy"', 'id="faq"']) {
    assert.ok(html.includes(needle), `missing ${needle}`);
  }
  assert.equal((html.match(/<li class="panel step">/g) || []).length, 3, "three steps");
  assert.match(html, /never uploads footage/);
  assert.match(html, /small still frames/);
  assert.equal((visibleText(html.slice(html.indexOf('class="privacy"'))).split("</p>")[0].match(/\./g) || []).length >= 1, true);
});

test("the copy follows the house rules", () => {
  for (const f of ["index.html", "admin.html", "404.html", "site.js", "admin.js"]) {
    const text = f.endsWith(".html") ? visibleText(site(f)) : site(f);
    assert.ok(!text.includes("—"), `${f}: no em dashes`);
    if (f.endsWith(".html")) assert.ok(!text.includes("!"), `${f}: no exclamation marks`);
    for (const word of ["testimonial", "best-in-class", "revolutionary", "seamless", "powerful", "fastest", "world's", "#1", "game-changing", "loved by", "trusted by"]) {
      assert.ok(!text.toLowerCase().includes(word), `${f}: avoid "${word}"`);
    }
    assert.doesNotMatch(text, />\s*(Submit|OK)\s*</, `${f}: buttons say what happens`);
  }
  // No invented numbers such as user counts or speed claims.
  assert.doesNotMatch(visibleText(site("index.html")), /\d+\s*(x|times) (faster|real ?time)|\d[\d,]*\+? (users|videographers|editors|creators)/i);
});

test("the content security policy only opens what each feature needs", () => {
  const plain = pageValues(config, "N");
  assert.match(plain.csp, /script-src 'self';/);
  assert.match(plain.csp, /frame-src 'none'/);
  assert.equal(plain.turnstileHtml, "");
  const withCheck = pageValues({ ...config, turnstileSiteKey: "0x4AAA-test" }, "N");
  assert.match(withCheck.csp, /script-src 'self' https:\/\/challenges\.cloudflare\.com/);
  assert.match(withCheck.turnstileHtml, /data-sitekey="0x4AAA-test"/);
});

test("the demo video slot shows the video from config, escaped, and allows its host", () => {
  assert.equal(pageValues(config, "N").demoHtml, "");
  const v = pageValues({ ...config, demoVideo: { src: "https://media.example/demo.mp4", poster: "", caption: 'Two "minutes"' } }, "N");
  assert.match(v.demoHtml, /<video class="demo-video" controls preload="metadata" playsinline src="https:\/\/media\.example\/demo\.mp4">/);
  assert.match(v.demoHtml, /Two &quot;minutes&quot;/);
  assert.match(v.csp, /media-src 'self' https:\/\/media\.example/);
});

test("visitors without JavaScript see the result of their signup", async () => {
  const { signupStatusHtml } = await import("../src/page.js");
  assert.match(signupStatusHtml("ok"), /status-done[\s\S]*You are on the list/);
  for (const s of ["invalid", "check", "error"]) assert.match(signupStatusHtml(s), /status-failed[\s\S]*nothing was saved/);
  assert.equal(signupStatusHtml("<script>"), "");
});
