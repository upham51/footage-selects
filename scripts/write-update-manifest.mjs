// Writes latest.json, the small file the app's updater reads to learn about
// the newest release. Each build job leaves an update-<platform>.json with
// its signed file's name and signature; this joins them.
//
//   node scripts/write-update-manifest.mjs DIST_DIR TAG OWNER/REPO
//
// With no update-*.json files (update signing not set up) it writes nothing.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export function buildManifest(entries, tag, repo, now = new Date()) {
  const version = tag.replace(/^v/, "");
  const platforms = {};
  for (const e of entries) {
    if (!e.platform || !e.file || !e.signature) throw new Error(`Incomplete update entry: ${JSON.stringify(e)}`);
    platforms[e.platform] = {
      signature: e.signature,
      url: `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(e.file)}`,
    };
  }
  return {
    version,
    notes: `Version ${version}. See the release page for details.`,
    pub_date: now.toISOString(),
    platforms,
  };
}

if (process.argv[1] && process.argv[1].endsWith("write-update-manifest.mjs")) {
  const [dir, tag, repo] = process.argv.slice(2);
  if (!dir || !tag || !repo) {
    console.error("Usage: write-update-manifest.mjs DIST_DIR TAG OWNER/REPO");
    process.exit(2);
  }
  const entries = readdirSync(dir)
    .filter((f) => /^update-.*\.json$/.test(f))
    .map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")));
  if (entries.length === 0) {
    console.log("No signed update files, so no latest.json. The installers are still published.");
  } else {
    const manifest = buildManifest(entries, tag, repo);
    writeFileSync(join(dir, "latest.json"), JSON.stringify(manifest, null, 2));
    console.log(`Wrote latest.json for ${Object.keys(manifest.platforms).join(", ")}`);
  }
}
