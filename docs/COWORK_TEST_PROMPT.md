You are testing two things I own: a desktop app called Footage Selects (Phase 1) and its landing page with a waitlist (Phase 2). The code is in my private GitHub repo upham51/footage-selects. Read docs/BUILD_PLAN.md there first; it says what each phase must do. I am not technical. Do everything yourself, never ask me to run commands, and write the report in plain language. Do not change any code or settings, do not commit or push anything, and do not spend money.

Ask me for these before you start, all at once:
1. The waitlist page address (it ends in workers.dev).
2. The admin password for the waitlist (I will type it into the page myself when you get there, or paste it into the field for you; never write it into the report).
3. A folder of my real footage to test with (ideally 20 or more clips, mixed cameras or frame rates).
4. Which video editors are installed on this computer (DaVinci Resolve, Premiere Pro, Final Cut Pro).

PART A: THE LANDING PAGE AND WAITLIST

1. Open the page on desktop width and at phone width (about 390 pixels wide). Check that every section is there and reads correctly: two plain sentences on what it does, the demo video slot, who it is for (travel, drone, real estate, events), how it works in three steps, "today" and "planned" kept separate, the planned price for smart features clearly marked as planned and not charged, the waitlist form (email, editor, what they shoot, hours a month), a one sentence privacy note, and the questions section (including that the base app never uploads footage and that smart features send only small still frames). Flag any exclamation marks, em dashes, testimonials, invented numbers, superlatives, comparisons with other products, or text that is not in sentence case. Check that nothing scrolls sideways on the phone and that every button and link works.
2. Sign-up tests. Use only email addresses ending in @example.com so I can remove them later, and wait at least 3 seconds after loading the page before sending each form:
   a. Send the form empty, then with a broken email (for example "maya@"), then with no footage type ticked. Each time a clear message should appear next to the right question, and it should disappear when you fix that answer.
   b. Send a complete, valid form. You should see a green "You are on the list" message with a tick.
   c. Send the same email again with different capital letters. It should look exactly the same to you, and should not create a second entry.
   d. Tick "Something else" and check that a text box appears for describing it.
   e. Turn JavaScript off in the browser if you can, send a valid form, and check you land back on the page with the same green message. Turn JavaScript back on.
   f. Send 7 or more valid forms within one minute. After about 5, you should get a plain message saying too many signups came from your connection. That is the protection working.
3. Open the page address followed by /admin.html. Enter a wrong password first: it should be refused with a plain message. Then the right one. Check the count matches the unique people you signed up, the "last 7 days" number, and the three breakdown tables. Click the download button and open the spreadsheet: one row per person, readable column names, and nothing that looks like a formula running.
4. Privacy: with the browser's developer tools (network tab), confirm the page loads nothing from other websites, apart from Cloudflare's Turnstile check if a check box is shown above the button.

PART B: THE DESKTOP APP

Getting the app:
1. Look for a release at https://github.com/upham51/footage-selects/releases. If there is one, download the file for this computer (Mac with Apple silicon: the file ending in mac-apple-silicon.dmg; Windows: windows-setup.exe) and install it following the README's steps, including the first-open approval on Mac or the SmartScreen "More info, Run anyway" step on Windows. Note whether the README's steps matched what you saw, screen by screen.
2. If there is no release and this is a Mac with Apple silicon, build it yourself from the repo following DEVELOPMENT.md: install Rust and Node.js 22 if missing, run scripts/build-ffmpeg.sh macos into a folder, copy ffmpeg and ffprobe into src-tauri/binaries named ffmpeg-aarch64-apple-darwin and ffprobe-aarch64-apple-darwin, run the engine tests with that ffmpeg first on the PATH, then build the app with the Tauri command line tool. Report how long each step took and any errors in full. Record that it was built locally, not from a release. If there is no release and this is Windows, say so and skip to Part C.

Testing the app with my footage folder:
3. Note the total length of footage and number of clips. Drag the folder onto the window. Check the progress bar, the time estimate, and that selects start appearing while analysis is still running. Time the whole analysis with a stopwatch and work out how many times faster than real time it ran (total footage minutes divided by analysis minutes). The goal is about 50 times on an Apple M-series chip.
4. Check that clips with odd properties were handled: different frame rates, phone clips shot vertically, clips with no sound, proxies. List any clip that failed and the reason the app gave.
5. Move each slider and confirm the selects change immediately without analyzing again. Check that "Reset to defaults" works. Check the summary line (footage in, selects out, clips analyzed) updates.
6. Open three clips in the detail view: one the app kept a lot from, one it dropped entirely, one in between. Check the three graphs, the threshold lines, and the kept and dropped stretches, and that each status has a shape and a word, not only a color. Say whether the reasons make sense when you watch the actual clip.
7. Save the project, quit the app, reopen it and open the project. Nothing should need analyzing again. Then change "Frames scored per second": the app should ask before analyzing again.
8. Export the timeline. Check four files appear (.xml, .fcpxml, .edl, .csv). Import into each installed editor following the README: .xml into Premiere Pro and DaVinci Resolve, .fcpxml into Final Cut Pro. In each, check: full clips on video track 1, each select directly above its source on video track 2, markers on the selects whose names include strong, good or fair and whose notes list focus, exposure and stability scores, marker colors, the timeline frame rate, and that the selects line up with the right moments in the source (compare 3 selects against the CSV times). If any footage is 29.97 or 59.94, also check drop frame timecode reads correctly.
9. Updates panel: click "Check for updates". Report exactly what it says. (An "this copy cannot update itself" message is expected until update signing is set up.)
10. Offline: turn off wifi, start a new project, analyze a few clips and export. Everything should work.

PART C: REPORT

Write the report for a non-technical reader:
1. Start with one line: does it work well enough to show people, yes or no, and the single biggest problem.
2. A numbered list of every check with pass, fail or could not test, and a one-line reason for each fail.
3. The speed result from Part B step 3, with the numbers you used.
4. Every problem in order of how much it would bother a real videographer, each with exact steps to reproduce and a screenshot.
5. Anything in the README or on the page that was wrong, unclear or did not match what you saw.
6. The @example.com addresses you used, so they can be removed from the waitlist.
Save the report and screenshots in a folder called footage-selects-test-report on my desktop.
