# Developing Footage Selects

## Layout

1. `engine/`: the Rust library that does all the work. Probing (`probe.rs`), sampled decoding through ffmpeg (`analyze.rs`), per-frame measurements (`metrics.rs`), scores (`scores.rs`), selection (`selection.rs`), the project file (`project.rs`), timecode (`rate.rs`) and the four export writers (`export/`). Also: picture profiles and log curves (`tone.rs`), camera sidecar files (`sidecar.rs`), picture fingerprints (`fingerprint.rs`), finished-video decoding (`reference.rs`) and tracing (`matching.rs`), and learning from finished videos (`taste.rs`).
2. `src-tauri/`: the Tauri v2 desktop shell. Commands the window calls (`commands.rs`), the parallel analysis workers (`runner.rs`) and the snapshots sent to the window (`state.rs`).
3. `ui/`: the window itself, plain HTML, CSS and JavaScript with no build step. It talks to Rust through Tauri's global API.
4. `dev/ui-harness/`: opens the window in an ordinary browser with a fake Rust side, for working on the interface. Open `dev/ui-harness/index.html?scene=list` (also `detail`, `frame-error`, `empty`, `loading`, `error`, `probing`). Regenerate its `index.html` from `ui/index.html` if the markup changes.
5. `dev/validate_exports.py` and `dev/validate_matrix.sh`: read exported timelines back with OpenTimelineIO.
6. `scripts/`: download the ffmpeg builds that ship inside the app.

## How analysis works

ffmpeg decodes each clip, skipping non-reference frames at the decoder, keeps the first frame in each sample interval with its real timestamp, downsizes it to about 480 pixels wide in grayscale and pipes raw pixels to the engine. On macOS it first tries VideoToolbox with dropping and downsizing on the GPU, then VideoToolbox with frames copied back, then software. Elsewhere it tries ffmpeg's automatic hardware decoding, then software.

The GPU path (`Decoder::GpuScaled`) downloads small frames with `hwdownload`, whose format must be the one VideoToolbox decoded into. `analyze::videotoolbox_download_format` mirrors ffmpeg's own choice (`videotoolbox_best_pixel_format`): nv12 or p010 for 4:2:0, nv16 or p210 for 4:2:2, nv24 or p410 for 4:4:4, p216 or p416 above 10 bits; formats with alpha or RGB skip the GPU path. Rotated clips use it too: `-noautorotate` is set and `analyze::gpu_filter` downsizes to the unrotated size, then turns the small gray frame with `transpose` the way ffmpeg's automatic rotation would (tested on Linux with ordinary filters standing in for the GPU ones).

Skipping non-reference frames (`-skip_frame noref`) happens in libavcodec's H.264 and HEVC parsers before a frame reaches any hardware decoder, so it applies to VideoToolbox and D3D11 as well. It only helps footage with non-reference B-frames: a 4K 120 fps HEVC clip with a B-frame pyramid decodes 289 of 719 frames, one coded with P-frames only decodes all 719.

All-intra footage (ProRes, DNxHR, MJPEG, intra-only H.264) is detected by reading the key frame flags of the first 32 packets (`probe::is_intra_only`). Every frame of such footage decodes on its own, so an input bitstream filter (`noise=drop=...`) drops the packets between samples before they reach the decoder, and only the sampled frames are decoded: about 4x faster for 24p ProRes and 6x for 120p intra H.264 at 4K in software. Input bitstream filters need ffmpeg 7.1 or later (`ffmpeg::input_bsf_supported`); the shipped build has them, older ffmpeg on a developer's PATH decodes every frame as before. `ClipAnalysis::sampled_only` records which way a clip was decoded. Long-GOP footage cannot do this, since every reference frame is needed to decode the next.

Motion in dark, noisy pictures: when lining two samples up lowers their difference by less than 10%, `metrics::shift_is_global` checks a 4 by 4 grid of tiles and accepts the shift only if most tiles with detail agree, which tells a real camera move hidden by noise from a subject crossing a plain background. Otherwise the sample is left unmeasured rather than called still, and `scores::stability_scores` judges jitter over the measured samples only, giving unmeasured ones the score of their neighbours. Before this (analysis version 3 and earlier), about a quarter of the samples of a flat night scene with gentle handheld sway read as still, doubling the motion error. Focus still rises with sensor noise (the Laplacian measures noise as detail); single-frame noise estimates tried for correcting it also cut daytime focus by about 75%, so it is left as a known limit.

ffmpeg runs at `-loglevel verbose` so it reports the pixel format frames enter the filters in. `analyze::decoder_used` turns that into the label stored in `ClipAnalysis::decoder` and shown in the clip view: hardware copy-back gives nv12 or p010 where a software decoder gives planar yuv420p, so a request for hardware decoding that ffmpeg quietly served in software is recorded as software.

Per frame the engine measures the variance of the Laplacian (focus), a luma histogram (clipped highlights, crushed shadows, the 2nd to 98th percentile spread, mean level), and global translation from the previous sample by coarse-to-fine block matching on an image pyramid. Stability compares each sample's motion with the average of its neighbours, so steady movement scores well and jitter does not. Raw measurements are cached in the project; scores and selection are recomputed from them on demand. Block matching sums absolute differences row by row in a form the compiler vectorizes (`metrics::row_difference`); per sampled frame the engine's own work is about 0.15 ms Laplacian, 0.12 ms histogram, 0.55 ms block matching and 0.12 ms fingerprint on one core of the Linux test machine, so ffmpeg's decoding is nearly all of the time.

When an analysed clip has no selects, `selection::no_select_reason` names the score whose clip average misses its threshold by the most points (average rounded down, so a near miss never reads as passing). If all averages pass, it reports the score that is under its threshold for the largest share of samples, or that the passing stretches were all shorter than the minimum. The list row shows it.

Each frame also stores a ladder of luma percentiles (1, 2, 5, 10, 25, 50, 75, 90, 95, 98 and 99%), the brightest value and the share of pixels within one level of it, and three 64-bit difference hashes (whole frame, centred 2.39:1, centred 9:16). The analysis picture is grayscale with video black at 0 and white at 255, so a luma maps exactly back to a 10-bit code value. Because tone curves are monotonic, the ladder can be mapped through any log curve later, which is how a profile change rescores without decoding: `tone::StopsTable` turns luma into stops from middle grey with the maker's published formula, and `tone::log_exposure` scores clipping (a pile-up at the top of the curve), underexposure, deep shadows and very flat light. The profile is resolved per clip in `ProjectClip::resolved_profile`: chosen for the clip, then named by the camera (`sidecar.rs` reads Sony `M01.XML` and DJI `.SRT`, `probe.rs` reads HLG tags), then the project's fallback, then `tone::looks_like_log`, then standard.

Selection cuts long runs with a small dynamic program (`selection::split_at_rest`): cutting where the camera moves costs much more than cutting where it is still, uneven parts cost a little, and with `keep_moves_whole` a part up to twice the maximum is allowed when the camera moves through most of it.

Camera direction: `scores::camera_direction` turns each sample's shift into pan and tilt speeds (picture content moving left means the camera turned right, content moving down means it turned up), stored in `ScoreSeries::pan` and `tilt`. `selection::camera_move_of` names a segment's movement: still unless the camera moves for most of it, then a pan or a tilt when the mean direction carries at least half the mean speed and one axis is at least twice the other, otherwise `Other`. With a camera filter, `selection::camera_pieces` cuts each good run where the camera starts or stops moving (pieces under a second join a neighbour), and non-matching segments get `SegmentStatus::SetAside` with `AsideReason::Camera`, before the per-clip limit. Push-ins and zooms are not measured, since only translation is.

Ranking weights (`focus_weight`, `exposure_weight`, `stability_weight`) only change `Segment::score`, which orders segments for the per-clip limit and the repeated-take check; thresholds ignore them.

Repeated takes: `Project::selections` runs selection without the limit for every clip (`selection::select_candidates`), then compares kept segments across different clips best first, using the whole-frame difference hash at a quarter, half and three quarters of each segment (`ProjectClip::fingerprints`). A mean distance of `project::REPEAT_DISTANCE` (10 of 64 bits) or less marks `similar_to`; with `set_aside_repeats` the segment is set aside with `AsideReason::Repeat`. The limit is applied last (`selection::apply_cap`), so a clip can promote its next best segment. `ProjectClip::select` is still per clip, and `taste.rs` uses per-clip selection, so fitting to finished videos ignores the repeated-take filter. The export, the state snapshot and `clip_detail` all use `Project::selections`.

Frame viewer: `frame::grab_still` runs ffmpeg with input seeking for one frame at `analysis_size(info, 960)`, display-rotated and RGBA, and `clip_frame` returns it as raw bytes (8 byte header of width and height, then pixels) via `tauri::ipc::Response`, which the window receives as an `ArrayBuffer`. Nothing is cached on disk; the window keeps the last 24 stills. `frame::tone_guide` gives the window a 256-entry luma to stops table through the clip's resolved profile and the clip point (250 for standard, the brightest analysed value for log when it sits at least 4 stops over grey), so false colour agrees with the exposure score. False colour, focus peaking (central differences, the top 6% of edges above a floor of 56) and clipping stripes are drawn in `ui/app.js`.

Finished videos are decoded at 6 samples per second with letterbox bars trimmed and flat frames skipped (`reference.rs`), split into shots at hash jumps, and each shot is fitted to a clip as source time = a + b times finished time, which covers slow motion and speed-ups (`matching.rs`). `taste.rs` counts how many traced shots the settings keep and fits the sliders by coordinate search, checking on each video with settings learned from the others when there are two or more.

## Running it

You need Rust (stable), Node.js 20 or later, and ffmpeg on your PATH for development and tests.

Linux also needs the WebKitGTK development packages:

```
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev librsvg2-dev
```

For a development build the bundler wants sidecar binaries named for your platform in `src-tauri/binaries/`, for example `ffmpeg-aarch64-apple-darwin`. Build them with `scripts/build-ffmpeg.sh macos out` on a Mac (or `linux`), then copy `out/ffmpeg` and `out/ffprobe` there under those names.

## ffmpeg and licensing

The app ships ffmpeg and ffprobe built from FFmpeg's source release by `scripts/build-ffmpeg.sh`, under the LGPL 2.1 only: no external libraries apart from the operating system's video decoding and zlib, and the script fails if GPL, version 3 or non-free parts are switched on. macOS builds natively on the Apple silicon runner; Windows is cross-compiled on Linux with mingw-w64. The release workflow caches both builds keyed on the script, and checks the finished program's `-buildconf` again before bundling.

Never replace these with a downloaded GPL build (anything containing x264, x265 and so on). The app talks to ffmpeg as a separate program, which keeps its own code free of LGPL obligations; a GPL build would not.

`THIRD_PARTY_LICENSES` is generated by `node scripts/third-party-licenses.mjs` from Cargo's view of the libraries compiled into the Mac and Windows apps. CI runs it with `--check`, which fails if the file is stale or if any shipped library has a license that needs review (GPL, AGPL, LGPL, missing).

The product name lives only in `productName` in `src-tauri/tauri.conf.json`. The engine reads it at build time (`engine/build.rs`, `PRODUCT_NAME`), the window gets it from the `tool_check` command, and the release workflow reads it with node.

```
npm ci
npx tauri dev          # run the app
npx tauri build        # build an installer for this computer
```

Paths given on the command line are added on start, which is handy for testing: `target/debug/footage-selects /path/to/footage`.

## Tests

```
cargo test -p selects_engine --release
```

The tests generate synthetic clips with ffmpeg (sharp and blurred, well exposed, dark and blown out, steady, panning and shaky, variable frame rate, rotated, with and without audio and timecode) and check that scores rank correctly, that selection returns the expected segments, that projects save and reload, and that the export writers get frame rates, timecode, track placement and markers right.

To read every export back with an independent parser:

```
python3 -m venv .venv
.venv/bin/pip install opentimelineio otio-fcp-adapter otio-fcpx-xml-adapter otio-cmx3600-adapter
dev/validate_matrix.sh /tmp/footage-selects-matrix .venv/bin/python
```

OpenTimelineIO's FCPXML reader truncates frame rates to whole numbers (23.976 becomes 23), so FCPXML is only read back at whole-number rates, and only when every clip is at the timeline rate. With mixed rates (for example 119.88 fps clips in a 24 fps timeline) its reader places connected clips differently from Final Cut Pro, because FCPXML gives a connected clip's offset in its parent clip's own time; a disagreement there is the reader's limit, not a broken file. Only Final Cut Pro itself can confirm those files. The engine's own FCPXML tests cover the NTSC rates.

Measuring speed on your own footage:

```
cargo run --release -p selects_engine --example bench -- /path/to/footage 3      # whole folder, 3 clips at once, decoders used
cargo run --release -p selects_engine --example stages -- clip.mov               # ffmpeg alone, then each measurement, then the full analysis
cargo run --release -p selects_engine --example inspect -- clip.mov              # every sample's measurements and scores
cargo run --release -p selects_engine --example inspect -- --summary *.mov       # one line of average scores per clip
dev/speed-clips.sh /tmp/speed-clips 5                                            # synthetic 4K H.264 and HEVC test clips
```

What the measurements say (October 2026). On the Linux test machine and on GitHub's Apple silicon runner (a 3-core M1 virtual machine), ffmpeg's decoding is over 95% of the time per clip; choosing samples, downsizing on the GPU or with `scale`, and the pipe add almost nothing, and the engine's own work is under 1 ms per sample. The runner's VideoToolbox decoded HEVC (4:2:0 and 4:2:2 10-bit, landscape and rotated) on the GPU path, while H.264 fell back to software there; on a real Mac VideoToolbox also decodes 8-bit 4:2:0 H.264, but not 10-bit 4:2:2 H.264, which is always software. Clips at once: on the runner 2 to 3 at once were fastest and more made no difference, which matches `runner::worker_count` (half the cores, 2 to 6). The 50 times real time goal in the build plan is not reachable for 4K long-GOP footage at 4 samples per second, because the decoder has to decode every reference frame whatever the sample rate: a 4K 120 fps clip encoded with P-frames only decodes all of its frames. Getting near it would mean decoding only keyframes (`-skip_frame nokey`), which gives one or two samples a second and makes the stability measure meaningless, so it has not been done.

Set `FOOTAGE_SELECTS_FFMPEG_DIR` to time the ffmpeg that ships (`scripts/build-ffmpeg.sh linux out`) rather than the one on your PATH. The "Speed check (Apple silicon)" workflow, run by hand from the Actions tab, does all of this on GitHub's Apple silicon runner with the shipped macOS ffmpeg and compares against an earlier version.

## Updates

The app uses Tauri's updater plugin against `latest.json` on the newest GitHub release. Checks happen only when the person clicks Check for updates or has turned on Check when the app opens (off by default); the setting lives in the app's config folder as `settings.json`.

Signing: run the "Create update signing keys" workflow once from the Actions tab, download its artifact, and store `private.key` as the repository secret `TAURI_SIGNING_PRIVATE_KEY` and `private.key.pub` as the repository variable `UPDATER_PUBLIC_KEY`. The release workflow then builds the update files, re-signs them with the version bound in (the app sets `requireSignedVersion`), and publishes `latest.json`. Without the key, installers still build, and the app says it cannot update itself.

The updater fetches without credentials, so it only works when the release files are publicly downloadable.

## Landing page and waitlist (site/ and worker/)

`site/` is plain HTML, CSS and JavaScript with no build step. The landing page is `index.html` with `landing.css`, `landing.js` (the hero reel and clip detail panel) and `fs-signal.js` (the seeded, illustrative scores they show); `site.js` handles the waitlist form, and `admin.html` and `404.html` keep using `styles.css`. The page's styles must stay in `landing.css`, because the content security policy allows no inline styles. The hero clips stream from Pexels and the photos come from Unsplash; those two hosts are listed in `PAGE_MEDIA_HOSTS` and `PAGE_IMAGE_HOSTS` in `worker/src/page.js`, and a test fails if the page uses a host the policy does not allow. `worker/` is a Cloudflare Worker (`wrangler.toml`) that serves `site/` as static assets and handles `/api/signup` and `/api/admin/*`; for `/` it fills the page with the product name (from `src-tauri/tauri.conf.json`), the planned price, demo video and Turnstile key (from `site/config.json`) and sets the content security policy. Signups go to D1 (`footage-selects-waitlist`, schema in `worker/migrations/`). Spam protection: honeypot field, minimum fill time, the Workers rate limiting binding (5 a minute per connection per location), optional Turnstile (`TURNSTILE_SECRET_KEY` secret plus `turnstileSiteKey` in config). The admin endpoints need the `ADMIN_TOKEN` secret.

Deployment is Cloudflare Workers Builds: the repository is connected in the Cloudflare dashboard with root directory `worker` and deploy command `npx wrangler deploy`, so every push to main redeploys. Schema changes: add a numbered file to `worker/migrations/` and apply it with `npx wrangler d1 migrations apply footage-selects-waitlist --remote`.

```
cd worker
npm ci
npm test                                   # unit tests, real SQLite via node:sqlite
npx wrangler d1 migrations apply footage-selects-waitlist --local
echo 'ADMIN_TOKEN="local-test-only"' > .dev.vars
npx wrangler dev                           # http://localhost:8787
node e2e/browser-check.mjs http://127.0.0.1:8787 local-test-only /tmp   # needs Playwright
```

## Releases

Update the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, then either push a tag starting with `v` (for example `v0.1.1`), or run the Release workflow from the Actions tab with "publish" ticked, which creates the tag `v<version>` on that commit itself (useful where tags cannot be pushed). The Release workflow builds ffmpeg (or reuses its cache), runs the engine tests against it on each platform, builds the macOS (Apple silicon) disk image and the Windows installer, and attaches both to the GitHub release for the tag. The Mac app is ad-hoc signed, not notarized, so the README explains how to approve it on first open.
