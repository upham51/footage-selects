# Notes for Claude

## Releases after every pull request

The owner wants a new release every time a pull request is merged. After each merge into main:

1. Bump the patch version (0.2.1 to 0.2.2 and so on) in all five places: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `package.json`, `package-lock.json` (the two top entries) and `Cargo.lock` (run `cargo update -p footage-selects --offline`).
2. Add a short "Fixed in" or "New in" section at the top of `.github/release-notes.md`, in plain words, no em dashes.
3. When Claude opens the pull request, put the bump in that same pull request so the owner merges once. For a pull request someone else opened, put the bump in a small follow-up pull request. Once the bump is on main, run the Release workflow (`.github/workflows/release.yml`) on main with `publish` ticked. It tags `v<version>` and attaches the Mac and Windows installers to the release.
4. Give the owner the release link: https://github.com/upham51/footage-selects/releases/latest

## Writing

No em dashes anywhere: code comments, user-facing text, docs, commit messages.
