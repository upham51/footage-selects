import { test } from "node:test";
import assert from "node:assert/strict";
import { buildManifest } from "./write-update-manifest.mjs";

test("joins each platform's signed file into one manifest", () => {
  const m = buildManifest(
    [
      { platform: "darwin-aarch64", file: "Footage-Selects-0.1.0-mac-apple-silicon.app.tar.gz", signature: "SIGMAC" },
      { platform: "darwin-x86_64", file: "Footage-Selects-0.1.0-mac-intel.app.tar.gz", signature: "SIGINTEL" },
      { platform: "windows-x86_64", file: "Footage-Selects-0.1.0-windows-setup.exe", signature: "SIGWIN" },
    ],
    "v0.1.0",
    "owner/repo",
    new Date("2026-10-17T00:00:00Z"),
  );
  assert.equal(m.version, "0.1.0");
  assert.equal(m.pub_date, "2026-10-17T00:00:00.000Z");
  assert.equal(m.platforms["darwin-aarch64"].signature, "SIGMAC");
  assert.equal(m.platforms["darwin-x86_64"].signature, "SIGINTEL");
  assert.equal(
    m.platforms["windows-x86_64"].url,
    "https://github.com/owner/repo/releases/download/v0.1.0/Footage-Selects-0.1.0-windows-setup.exe",
  );
});

test("refuses an entry without a signature", () => {
  assert.throws(() => buildManifest([{ platform: "windows-x86_64", file: "x.exe" }], "v1.0.0", "o/r"));
});
