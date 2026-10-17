// Fills the landing page with values that live in config files, as it is served:
// the product name (productName in src-tauri/tauri.conf.json, the app's single
// name setting) and the planned price, demo video and spam check key
// (site/config.json).

import siteConfig from "../../site/config.json" with { type: "json" };
import appConfig from "../../src-tauri/tauri.conf.json" with { type: "json" };

export const PRODUCT_NAME = appConfig.productName;

/** The public Turnstile site key from site/config.json, or "" when the check is not set up. */
export const TURNSTILE_SITE_KEY = (siteConfig.turnstileSiteKey || "").trim();

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function originOf(src) {
  try {
    return /^https?:\/\//.test(src) ? new URL(src).origin : "";
  } catch {
    return "";
  }
}

/** Everything the page needs, worked out from the config files. Pure, so it can be tested. */
export function pageValues(config = siteConfig, productName = PRODUCT_NAME, now = new Date()) {
  const p = config.plannedPrice;
  const price = `${p.currency}${p.min} to ${p.currency}${p.max} a ${p.period}`;
  const video = config.demoVideo || {};
  const turnstile = (config.turnstileSiteKey || "").trim();

  let demoHtml = "";
  if (video.src) {
    demoHtml = `<video class="demo-video" controls preload="metadata" playsinline src="${esc(video.src)}"${video.poster ? ` poster="${esc(video.poster)}"` : ""}></video>`;
    if (video.caption) demoHtml += `<p class="demo-caption">${esc(video.caption)}</p>`;
  }

  const media = ["'self'", originOf(video.src)].filter(Boolean);
  const images = ["'self'", "data:", originOf(video.poster)].filter(Boolean);
  const scripts = ["'self'", turnstile ? "https://challenges.cloudflare.com" : ""].filter(Boolean);
  const csp = [
    "default-src 'self'",
    `script-src ${scripts.join(" ")}`,
    "style-src 'self'",
    `img-src ${images.join(" ")}`,
    `media-src ${media.join(" ")}`,
    "font-src 'self'",
    "connect-src 'self'",
    turnstile ? "frame-src https://challenges.cloudflare.com" : "frame-src 'none'",
    "form-action 'self'",
    "base-uri 'none'",
    "frame-ancestors 'none'",
  ].join("; ");

  return {
    productName,
    title: `${productName}: pull the usable shots from raw footage`,
    price,
    priceIncludes: p.includes,
    year: String(now.getUTCFullYear()),
    demoHtml,
    turnstileHtml: turnstile ? `<div class="cf-turnstile" data-sitekey="${esc(turnstile)}"></div>${NOSCRIPT_NOTICE}` : "",
    turnstileScript: turnstile ? `<script src="https://challenges.cloudflare.com/turnstile/v0/api.js" async defer></script>` : "",
    pageUrl: config.pageUrl || "",
    csp,
  };
}

const TICK = '<svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><path d="M2.5 7.5l3 3 6-7"/></svg>';
const TRIANGLE = '<svg class="icon" viewBox="0 0 14 14" aria-hidden="true"><path d="M7 1.8l5.6 9.9H1.4z"/><path d="M7 5.6v2.6M7 10.1v.1"/></svg>';
const NOT_ADDED = "You were not added yet";

// Shown before anyone fills in the form, when the spam check is on and JavaScript is off.
const NOSCRIPT_NOTICE = `<noscript><div class="status status-failed" role="note">${TRIANGLE}<div><strong>This form needs JavaScript</strong><p>The spam check above the button only runs with JavaScript, and it is turned off in your browser. Turn it on for this page and reload, or signups from this browser cannot be saved.</p></div></div></noscript>`;

/** The message shown after a form post made without JavaScript (?signup=...). */
export function signupStatusHtml(status) {
  const box = (kind, title, body) =>
    `<div class="status status-${kind}" role="${kind === "failed" ? "alert" : "status"}">${kind === "done" ? TICK : TRIANGLE}<div><strong>${title}</strong><p>${body}</p></div></div>`;
  switch (status) {
    case "ok": return box("done", "You are on the list", "Thank you. You will get an email when there is something to try. Nothing else will be sent.");
    case "invalid": return box("failed", NOT_ADDED, "One of the answers was missing or not recognised, so nothing was saved. Check each question and try again.");
    case "check": return box("failed", NOT_ADDED, "The spam check did not pass, so nothing was saved. Wait for the check above the button to show Success, then send the form again.");
    case "nojs": return box("failed", NOT_ADDED, "The spam check needs JavaScript, and it looks turned off in your browser, so nothing was saved. Turn JavaScript on for this page, reload it and send the form again.");
    case "error": return box("failed", NOT_ADDED, "The waitlist could not be reached just now, so nothing was saved. Try again in a few minutes.");
    default: return "";
  }
}

/** Apply pageValues to the page as it streams out. Needs Cloudflare's HTMLRewriter. */
export function fillPage(response, env, values = pageValues(), signupStatus = "") {
  if (!response.ok) return response;
  const fill = (key) => ({ element: (el) => el.setInnerContent(values[key]) });
  let rewriter = new HTMLRewriter()
    .on("title", fill("title"))
    .on('[data-fill="product-name"]', fill("productName"))
    .on('[data-fill="price"]', fill("price"))
    .on('[data-fill="price-includes"]', fill("priceIncludes"))
    .on('[data-fill="year"]', fill("year"))
    .on('meta[property="og:title"]', { element: (el) => el.setAttribute("content", values.title) })
    .on('meta[property="og:site_name"]', { element: (el) => el.setAttribute("content", values.productName) });
  if (values.demoHtml) {
    rewriter = rewriter.on("#demo-slot", {
      element: (el) => {
        el.setInnerContent(values.demoHtml, { html: true });
        el.setAttribute("class", "demo has-video");
      },
    });
  }
  if (values.turnstileHtml) {
    rewriter = rewriter
      .on("#turnstile-slot", { element: (el) => el.setInnerContent(values.turnstileHtml, { html: true }) })
      .on("head", { element: (el) => el.append(values.turnstileScript, { html: true }) });
  }
  const statusHtml = signupStatusHtml(signupStatus);
  if (statusHtml) {
    rewriter = rewriter.on("#form-status", { element: (el) => el.setInnerContent(statusHtml, { html: true }) });
    if (signupStatus === "ok") rewriter = rewriter.on("#signup-form", { element: (el) => el.setAttribute("hidden", "") });
  }
  if (values.pageUrl) {
    rewriter = rewriter.on("head", { element: (el) => el.append(`<link rel="canonical" href="${esc(values.pageUrl)}">`, { html: true }) });
  }
  const out = rewriter.transform(response);
  const headers = new Headers(out.headers);
  headers.set("Content-Security-Policy", values.csp);
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  headers.set("X-Frame-Options", "DENY");
  // Checked with the server on every visit, so a new version reaches visitors at once.
  headers.set("Cache-Control", "no-cache");
  return new Response(out.body, { status: out.status, headers });
}
