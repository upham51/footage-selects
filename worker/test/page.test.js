import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { pageValues, PRODUCT_NAME, APP_VERSION } from "../src/page.js";
import { existsSync } from "node:fs";

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
  const plain = pageValues({ ...config, turnstileSiteKey: "" }, "N");
  assert.match(plain.csp, /script-src 'self';/);
  assert.match(plain.csp, /frame-src 'none'/);
  assert.equal(plain.turnstileHtml, "");
  const withCheck = pageValues({ ...config, turnstileSiteKey: "0x4AAA-test" }, "N");
  assert.match(withCheck.csp, /script-src 'self' https:\/\/challenges\.cloudflare\.com/);
  assert.match(withCheck.turnstileHtml, /data-sitekey="0x4AAA-test"/);
  assert.match(withCheck.turnstileHtml, /<noscript>[\s\S]*needs JavaScript/, "people without JavaScript are told before they fill in the form");
});

test("the demo video slot shows the video from config, escaped, and allows its host", () => {
  assert.equal(pageValues(config, "N").demoHtml, "");
  const v = pageValues({ ...config, demoVideo: { src: "https://media.example/demo.mp4", poster: "", caption: 'Two "minutes"' } }, "N");
  assert.match(v.demoHtml, /<video class="demo-video" controls preload="metadata" playsinline src="https:\/\/media\.example\/demo\.mp4">/);
  assert.match(v.demoHtml, /Two &quot;minutes&quot;/);
  assert.match(v.csp, /media-src 'self' https:\/\/media\.example/);
});

test("the page's own clips and photos come only from the hosts the policy allows", () => {
  const csp = pageValues(config, "N").csp;
  // Links people click (href) are not loaded by the page, so the policy does not cover them.
  const html = (site("index.html") + site("landing.js")).replace(/\shref="[^"]*"/g, "");
  for (const url of html.match(/https:\/\/[a-z0-9.-]+/g) || []) {
    const origin = new URL(url).origin;
    if (origin === "https://challenges.cloudflare.com") continue;
    assert.ok(csp.includes(origin), `${origin} is used by the page but not allowed by the policy`);
  }
  assert.match(csp, /style-src 'self';/, "no inline styles are allowed");
  assert.doesNotMatch(site("index.html"), /\sstyle="/, "index.html has no inline style attributes");
});

test("visitors without JavaScript see the result of their signup", async () => {
  const { signupStatusHtml } = await import("../src/page.js");
  assert.match(signupStatusHtml("ok"), /status-done[\s\S]*You are on the list/);
  assert.match(signupStatusHtml("nojs"), /needs JavaScript/);
  for (const s of ["invalid", "check", "nojs", "error"]) assert.match(signupStatusHtml(s), /status-failed[\s\S]*nothing was saved/);
  assert.equal(signupStatusHtml("<script>"), "");
});

test("shared links get a picture, a description and a full address", () => {
  const html = site("index.html");
  for (const tag of ['property="og:description"', 'property="og:image" content="media/og.jpg"', 'name="twitter:card" content="summary_large_image"']) {
    assert.ok(html.includes(tag), `missing ${tag}`);
  }
  assert.ok(existsSync(new URL("../../site/media/og.jpg", import.meta.url)), "the share picture exists");
  const v = pageValues({ ...config, pageUrl: "https://selects.example/" }, "N");
  assert.equal(v.ogImage, "https://selects.example/media/og.jpg");
  assert.equal(pageValues({ ...config, pageUrl: "" }, "N").ogImage, "");
});

test("search engines read the app's name, free price, platforms and version", () => {
  assert.equal(APP_VERSION, JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8")).version);
  const v = pageValues({ ...config, pageUrl: "https://selects.example/" }, "Some <b> Name", new Date(), "9.9.9");
  const m = v.jsonLd.match(/^<script type="application\/ld\+json">([\s\S]*)<\/script>$/);
  assert.ok(m, "one JSON-LD script");
  assert.doesNotMatch(m[1], /</, "nothing in it can close the script tag");
  const data = JSON.parse(m[1]);
  assert.equal(data["@type"], "SoftwareApplication");
  assert.equal(data.name, "Some <b> Name");
  assert.equal(data.softwareVersion, "9.9.9");
  assert.equal(data.offers.price, "0");
  assert.equal(data.url, "https://selects.example/");
  assert.match(data.downloadUrl, /\/releases\/latest$/);
});

test("the sitemap and robots.txt name the same address as the page", () => {
  assert.ok(config.pageUrl, "site/config.json has pageUrl");
  assert.match(site("sitemap.xml"), new RegExp(`<loc>${config.pageUrl.replace(/[.]/g, "\\.")}</loc>`), "update site/sitemap.xml when pageUrl changes");
  assert.ok(site("robots.txt").includes(`Sitemap: ${new URL("sitemap.xml", config.pageUrl).href}`), "update the Sitemap line in site/robots.txt when pageUrl changes");
});

test("the download buttons go to the latest release", () => {
  const html = site("index.html");
  const downloads = html.match(/<a class="btn btn-primary[^"]*" href="([^"]+)">Download free<\/a>/g) || [];
  assert.equal(downloads.length, 2, "one in the top bar and one in the hero");
  for (const d of downloads) assert.match(d, /href="https:\/\/github\.com\/upham51\/footage-selects\/releases\/latest"/);
});

test("every photo has a smaller WebP copy beside it", () => {
  const html = site("index.html");
  for (const [, name] of html.matchAll(/src="media\/(who_[a-z]+)\.jpg"/g)) {
    assert.ok(html.includes(`srcset="media/${name}.webp"`), `${name} has a WebP source`);
    assert.ok(existsSync(new URL(`../../site/media/${name}.webp`, import.meta.url)), `${name}.webp exists`);
  }
});
