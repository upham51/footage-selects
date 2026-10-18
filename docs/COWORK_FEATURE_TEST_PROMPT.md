You are testing new features in a desktop app called Footage Selects. The code is in my private GitHub repo upham51/footage-selects, on the branch claude/zealous-johnson-qkfy8j (pull request 7). The README on that branch describes every feature; read it first, especially Keep or drop by hand, Sound, Light, scenes and places, Picture checks, Export the timeline, Stills and contact sheet, Make a reel, and Shoot report. I am not technical. Do everything yourself, never ask me to run commands, and write the report in plain language. Do not change any code, do not commit or push anything, do not merge the pull request, and do not spend money.

Ask me for these before you start, all at once:
1. A folder of my real footage (ideally 20 or more clips, from more than one camera). It helps if some clips have someone talking, some are drone clips, some are from a phone, and some were shot around sunrise or sunset.
2. A song file I own (mp3, m4a or wav) with a clear beat.
3. A finished video I already cut from that footage, if I have one.
4. Which video editors are installed (DaVinci Resolve, Premiere Pro, Final Cut Pro).

PART A: BUILD THE APP FROM THE BRANCH

There is no release with these features yet, so build it locally.
1. On a Mac with Apple silicon: install Homebrew, Rust, Node.js 22 and ffmpeg (brew install ffmpeg) if missing. Clone the repo, switch to the branch claude/zealous-johnson-qkfy8j, and follow DEVELOPMENT.md. For a local test build, copy Homebrew's ffmpeg and ffprobe into src-tauri/binaries named ffmpeg-aarch64-apple-darwin and ffprobe-aarch64-apple-darwin. This copy is only for testing on this computer; never ship it.
2. Run the engine tests (cargo test -p selects_engine --release) and record how many passed. Then build and run the app with npm ci followed by npx tauri dev. On Windows, do the same with the x86_64-pc-windows-msvc names, or say you could not.
3. Record every step that failed, with the full error.

PART B: MAKE TEST CLIPS THAT PROVE EACH CHECK

Put these in a new folder called feature-test-clips on the desktop. Make them with ffmpeg (and the Mac's say command for speech). Record the exact command you used for each.
1. talking.mov: 40 seconds of a test pattern picture with speech made by say. Use 5 or 6 sentences and a 1 second pause between each, and keep talking right up to the 10, 20 and 30 second marks.
2. wind.mov: 20 seconds of picture with stereo low-frequency noise that gusts, different in each channel. Low-pass brown noise below 150 Hz, varying in level, works.
3. clipped.mov: 15 seconds of picture with a sine tone boosted far past full scale, so it distorts.
4. dropout.mov: 20 seconds of picture with steady speech or a tone that drops to silence for half a second at 10 seconds.
5. crooked.mov: 15 seconds of a picture full of straight lines (a grid or the testsrc2 pattern) rotated 3 degrees clockwise.
6. flicker.mov: 15 seconds of a still textured picture with horizontal dark bands of about 4% that move from frame to frame.
7. jello.mov: 15 seconds of a textured picture whose rows shift sideways by a fast sine wave, like propeller vibration.
8. Optional: two clips of the same shot, to check repeated takes.

PART C: TEST EVERY FEATURE

Add my footage folder and the test clips folder to one project, let analysis finish, and time it. Then check each item below. For each one, write pass, fail or could not test, with a screenshot.

1. Hand calls. Open a clip and hold a moment on a kept select. Press X: it should become Dropped by hand, the clip's next best stretch should take its place, and a red outline should appear on the strip. Hold a moment in a dropped (red) stretch and press K: a navy-outlined Kept by hand stretch should appear, no longer than Longest select. Press 4: four gold stars should show. Press 0: your call should be removed. Now move every slider: the hand calls must stay. Save, quit, reopen: they must still be there. Check the list row says how many calls by hand.
2. Learn from hand calls. Make 5 or more keep and drop calls across clips, then click Fit settings to my edits under Learn from your edits. It should offer settings and explain the change. Repeat with my finished video added, if I gave you one.
3. Speech pauses. In talking.mov, the clip view should say someone talks, and a Talking lane should show the sentences. Set Longest select to 8 seconds. Every select in that clip should start and end in a gap between sentences. Play each select's start and end in a player to confirm. Untick Start and end in a pause when someone talks: some selects should now cut mid-sentence, and the table should say so. Tick it again.
4. Sound problems. wind.mov should show Wind noise, clipped.mov Sound clipped and dropout.mov Sound drops out, both in the list and in the clip view. A steady music or tone clip should show none of these. Tick Set aside selects with sound problems: those selects should be set aside, with the reason given.
5. Picture checks. crooked.mov should show Crooked horizon, saying it tilts down to the right by about 3 degrees and to rotate counter-clockwise. flicker.mov should show Flicker. jello.mov should show Jello. A clean steady clip should show none of them. Also note any of my own clips flagged. Watch them and say whether each flag looks right.
6. White balance. If any of my clips shows White balance differs, open it next to another clip from the same scene. Say whether it really looks warmer, cooler, greener or more magenta.
7. Light and scenes. Check which of my clips got a light tag and a scene name, such as "Day 1, evening". For three clips, check the light is right for when they were shot, using the recording time in the clip view against sunrise and sunset for that place and day. If any clip says no place was found, type the shoot location into Places and time (latitude, longitude from a map app). Confirm the light then appears. Set Camera clock time zone to the right zone and check the times. Try each Light filter chip and confirm only matching clips keep selects.
8. Export extras. Set Extra time around each select to 2 seconds, tick Also save a still of every select and a contact sheet, and export. Import into each installed editor. Check that selects are 2 seconds longer on each side and that the markers still sit where each select starts. Check selects from the same clip never overlap on track 2. Check the keywords: in Final Cut Pro, keyword collections such as Golden hour or Pan in the event, and favorites on starred or hand-kept selects. In Premiere Pro, the Scene, Log Note and Good columns. In Resolve, the clip metadata. Open the CSV and check the new columns: Stars, Kept by hand, Scene, Light, Camera, Keywords, Problems, Extra before, Extra after.
9. Stills and contact sheet. A folder ending in "stills" should hold one JPEG per select, a small folder and Contact sheet.html. Open the sheet in a browser. Check it shows every select grouped by scene, with readable details, and that clicking a picture opens the full-size still. Check each still is a sharp, steady moment. Print it to PDF to confirm it prints cleanly. Also try Save stills and contact sheet only, and the Stop writing stills button part way.
10. Reel. Under Make a reel, choose my song. Check the tempo it reports against a beats-per-minute tool or by tapping along. Try each Cut, Length, Order and Shape option. Open See the shot list and check the list makes sense: no two shots in a row from the same clip where avoidable, and As shot in recording order. Export reel timeline for 30 seconds in 9:16 and 16:9, and import into each editor. Check the song is on the audio track, the shots are back to back with no sound of their own, and the cuts land on the beat (play it through). Check the 9:16 version fills the vertical frame from the centre.
11. Shoot report. Click Shoot report. Check every finding against what you saw: shutter and shake, cameras, clipped highlights, light, horizons, flicker, jello, sound and white balance. Flag anything that is wrong or not backed by the footage. Click Save as a page and open the file.
12. Old projects. If an older .selects project exists, open it. It should ask to analyze clips again (expected) and then work normally.
13. Speed. Compare analysis time with the old version if you know it. These features add sound reading and about half a millisecond per sampled frame, so it should be only a little slower.
14. Offline. Turn off wifi and repeat a reel export and a stills export.

PART D: REPORT

Write it for a non-technical reader:
1. Start with one line: are the new features ready to ship, yes or no, and the single biggest problem.
2. A numbered list of every check in Part C with pass, fail or could not test, and a one-line reason for each fail.
3. False alarms and misses for the automatic checks (talking, wind, clipping, dropouts, horizon, flicker, jello, white balance, light), counted on my real footage: how many clips each flagged, and how many of those flags you agreed with after watching.
4. Every problem in order of how much it would bother a real videographer, each with exact steps to reproduce and a screenshot.
5. Anything in the README that was wrong, unclear or did not match what you saw.
6. Anything in the app's wording that has an exclamation mark, an em dash, jargon, or text not in sentence case.
Save the report, screenshots and the feature-test-clips commands in a folder called footage-selects-feature-report on my desktop.
