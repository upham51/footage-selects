"use strict";
/* Runs before the page paints, so the window opens in the remembered theme. Dark unless chosen otherwise. */
(function () {
  let t = "dark";
  try { t = localStorage.getItem("theme") === "light" ? "light" : "dark"; } catch (_) { /* a remembered theme is a convenience */ }
  document.documentElement.setAttribute("data-theme", t);
})();
