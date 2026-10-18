# Footage Selects

Footage Selects looks through your raw footage and picks the usable moments for you. It checks every clip for focus, exposure and camera shake, keeps the best stretches, and hands you a timeline you can open in DaVinci Resolve, Premiere Pro or Final Cut Pro.

It understands log footage. S-Log3, V-Log, Canon Log 3, LogC3, F-Log, D-Log, D-Log M, Apple Log, N-Log and HLG are judged in stops through the camera's own curve, so a flat-looking log shot is not marked down, while clipped highlights and underexposure are caught. It can also learn from videos you have already finished: it finds which shots you used and tunes its settings toward your choices.

It is free, it works on Mac (Apple silicon) and Windows, and it runs entirely on your computer. Nothing is uploaded, there is no account, and it works without an internet connection. It uses plain image math, not artificial intelligence.

On an Apple silicon Mac, 4K footage is analyzed at roughly 8 times real time, so two hours of 4K takes about 15 minutes. HD footage and proxies are usually quicker, and 120 fps footage slower, because the computer has to unpack every frame it uses to rebuild the next. You then tune the result with sliders, which update instantly.

## What it does

1. Reads every clip you give it: any mix of cameras, formats, sizes and frame rates, including phone footage and proxies.
2. Scores a few frames from every second of each clip for:
   1. Focus: how much fine detail the picture has. Soft or missed focus scores low.
   2. Exposure: blown highlights, crushed shadows and flat, washed out pictures score low. Log footage is judged differently, as described under Log footage below.
   3. Stability: smooth pans and locked off shots score high, handheld shake scores low.
3. Keeps the stretches where all three scores are good enough, up to the number you choose per clip.
4. Exports a timeline with every clip on video track 1 and each select directly above it on video track 2, with a colored marker on each select that lists its three scores.

## Download

Go to the [latest release](https://github.com/upham51/footage-selects/releases/latest) and download the file for your computer. No GitHub account is needed.

1. Mac with Apple silicon (M1, M2, M3, M4 or later): the file ending in `mac-apple-silicon.dmg`
2. Windows: the file ending in `windows-setup.exe`

Not sure which Mac you have? Open the Apple menu, choose About This Mac, and look at the line called Chip. If it starts with Apple, this download is for you.

## Install on a Mac

1. Open the `.dmg` file you downloaded.
2. Drag Footage Selects into the Applications folder shown in the window.
3. Open your Applications folder and double-click Footage Selects.

The first time, your Mac will stop and show a warning. This is expected: the app is free and is not registered with Apple's paid developer program, so your Mac cannot check who made it. You only need to approve it once.

On macOS Sequoia (15) and later:

1. When the warning says Apple could not verify Footage Selects, click Done.
2. Open the Apple menu and choose System Settings.
3. Click Privacy & Security in the sidebar.
4. Scroll down to the Security section. You will see a line saying Footage Selects was blocked. Click Open Anyway.
5. Enter your Mac's password if asked, then click Open Anyway again.

On macOS Sonoma (14) and earlier:

1. In your Applications folder, hold the Control key and click Footage Selects (or right-click it).
2. Choose Open from the menu.
3. In the message that appears, click Open.

From then on, Footage Selects opens normally with a double-click.

## Install on Windows

1. Double-click the `windows-setup.exe` file you downloaded.
2. If a blue box says Windows protected your PC, click More info, then click Run anyway. This appears because the app is not signed with a paid certificate.
3. Follow the installer. It does not need administrator rights.
4. Start Footage Selects from the Start menu.

## Add your footage

Drag a folder of footage, or a handful of clips, onto the Footage Selects window. Folders inside the folder are included. You can also click Add folder or Add clips.

Analysis starts right away. Several clips are analyzed at once, and the bar at the top shows how many are done, roughly how long is left, and how fast it is going. You can keep working while it runs: selects appear in the list as each clip is scored.

If you need to stop, click Stop analysis. Clips that are finished keep their scores. Click Analyze footage later to finish the rest.

Clips that cannot be read are listed with the reason, and left out of the selects. Everything else carries on.

## Adjust the selects

The sliders on the right change the selects immediately. Footage is never analyzed again when you move them.

1. Focus at least, Exposure at least, Stability at least: the lowest score, from 0 to 100, a moment needs to be kept. Raise a slider to be pickier, lower it to keep more.
2. Smoothing: evens out the scores over this many seconds, so a single soft or shaky frame does not split a good take in two. Off uses the raw scores.
3. Shortest select: good stretches shorter than this are dropped.
4. Longest select: good stretches longer than this are split into equal parts.
5. Selects per clip: the most selects kept from one clip. The best scoring ones win. Slide all the way right for no limit.
6. Keep camera moves whole: when a good stretch is longer than the longest select, it is always cut where the camera is still, never in the middle of a move if that can be avoided. With this ticked, a single pan, push or drone reveal can run up to twice the longest select instead of being cut in two. It is on to start with.

Reset to defaults puts every slider back where it started.

Frames scored per second, under Analysis, is the one setting that means analyzing everything again. More frames per second catches shorter moments but takes longer. The app asks before it starts.

## See why a clip was kept or dropped

When an analyzed clip has no selects, its row in the list already says why, with a triangle and the words Below threshold, for example "Stability averaged 17, needs 50". It names the score that missed its threshold by the most. If every average is high enough but the scores take turns dipping, it says how much of the clip one score spent under its threshold. If good stretches exist but are all short, a dash and the words Too short say how long the longest one was.

Click any clip in the list. You will see three graphs, one each for focus, exposure and stability, across the length of the clip.

1. The navy line is the smoothed score used for selection. The grey line behind it is the raw score of each frame.
2. The dashed line is your threshold.
3. Green stretches were kept. A red stretch on a graph was dropped because that score was under its threshold. Grey stretches were good but too short. Amber stretches were good, but the clip already had its maximum number of better selects.
4. Every result also has a shape and a word: a tick for kept, a triangle for below threshold, a dash for too short, and three lines for over the per-clip limit.
5. Move the pointer over the graphs to read the exact scores at any moment.
6. Below the graphs, a table lists every stretch of the clip with the reason it was kept or dropped. When exposure is the reason, it says what was wrong: highlights clipped, underexposed, overexposed, large areas lost in the shadows, or very flat light.
7. For log footage, the exposure graph's heading and the readout under the pointer give the middle tones in stops above or below middle grey, and how much of the picture is clipped. The table has a Stops column.
8. Above the graphs you see which picture profile the clip was judged with and why, with a menu to choose another for this clip. Camera details read from the camera's own files (camera, lens, ISO, shutter, aperture, height and position for drones) are listed too.
9. Shots you used in a finished video are outlined with a dashed line on the strip above the graphs, and the table says which video used them.

The line under the clip's name also says how it was read: by the graphics chip, by other video hardware, or by the processor alone (software, which is slower). If hardware decoding was asked for but the computer could not do it for this clip's format, it says so.

Click Back to all clips to return to the list.

## Log footage

Log footage is meant to look flat and grey until it is graded, so judging it on how it looks would mark down every good log shot. Instead, each clip's tones are turned back into light with the curve the camera maker published for that profile, and exposure is judged in stops from middle grey:

1. Clipped highlights cost the most, because a grade cannot bring them back. Log footage clips well below pure white, at the top of the camera's curve, and that is where the app looks for them.
2. Middle tones more than about one and a half stops under middle grey count as underexposed. Log footage gets noisy when it is brought up in the grade.
3. Large areas more than about five stops under middle grey count as lost in the shadows.
4. Being a stop or two bright costs little, because exposing log a little bright is a common way to keep shadows clean.
5. Very flat light, such as fog, costs a little.

How the app knows which profile a clip used, in this order:

1. A profile you choose for that clip in its detail view.
2. What the camera says. Sony cameras write a small file beside each clip (C0001M01.XML beside C0001.MP4) that names the profile. DJI drones and cameras write a caption file (DJI_0001.SRT beside DJI_0001.MP4) when video captions are turned on in the camera settings; turn them on to get the profile, ISO, shutter, aperture, GPS position and height for every clip. HLG is also recognised from the video's own colour tags.
3. The choice under Picture profile, on the right, for clips whose camera does not say. The app remembers it for new projects, so if you always shoot one profile you only choose it once.
4. Otherwise, the clip's tones: footage whose blacks never reach the bottom and whose whites never reach the top is treated as log, with a stand-in curve averaged from four published ones. Choose the exact profile for exact readings.

Changing a profile updates the selects right away. Nothing is analyzed again.

Two profiles are approximate. DJI has not published the D-Log M curve, so D-Log's curve stands in for it. "Log, curve unknown" uses the averaged curve. Readings in stops for those are close but not exact.

If the camera recorded a shutter much faster than the frame rate suits (for example 1/2000 at 24 frames per second), the clip is tagged Fast shutter. Movement in such clips looks choppy and cannot be fixed in the edit; an ND filter lets you keep the slower shutter in bright light.

## Learn from your edits

Under Learn from your edits, on the right, click Add finished videos and choose videos you have already cut and exported from footage in this project. Each one is read on this computer and its shots are traced back to the clips they came from. This works through colour grading, slow motion, speed changes, letterboxing to 2.39:1 and vertical 9:16 versions cut from the centre of the frame. Title cards, black frames and footage that is not in the project are left out rather than guessed.

You then see how many of the shots you used are kept by the current settings, and why the others are left out (for example, 6 under the stability threshold). Click Fit settings to my edits and the app tries many combinations of the same sliders to find the ones that keep more of your shots without making the selects much longer. It shows the settings side by side with yours, and you choose Use these settings or Keep my settings. With two or more finished videos, the fit is also checked fairly: each video is tested with settings learned only from the others, so you see how well the settings carry over to an edit they have not seen.

When more clips are analyzed after you add a finished video, the app looks for its shots in the new clips by itself. Finished videos are saved with the project.

What it cannot trace yet: punch-ins and reframes that are not centred, zoom added by stabilising in the edit, shots played backwards or mirrored, speed ramps inside one shot, shots under about half a second, and very plain pictures such as blank sky or fog.

## Save your work

Click Save project to save a `.selects` file. It holds every score, so when you open it again with Open project, nothing needs analyzing. The project file does not contain your footage, only where it is. If you move or rename footage, those clips are marked as missing; their scores still count, and you can relink them in your editor after export. The save window opens in your Documents folder the first time, and beside the saved project after that, so project files do not end up among your footage.

If a clip has changed since it was analyzed, it is marked as changed and analyzed again when you click Analyze footage.

Projects saved by version 0.1 open normally, but each clip needs analyzing once more, because version 0.2 measures more about each frame (what log exposure and finished-video tracing need). Click Analyze footage after opening one.

## Export the timeline

Choose your settings under Export:

1. Timeline name: what the timeline will be called in your editor.
2. Timeline frame rate: starts as the most common frame rate in your footage. Pick another if your project uses a different one.
3. Drop frame timecode: appears for 29.97 and 59.94. Use it if your project uses drop frame timecode. The app turns it on when most of your 29.97 or 59.94 footage was recorded that way.
4. Layout: either every full clip on track 1 with each select above it on track 2, or only the selects back to back on track 2.

Click Export timeline, choose where to save and a name, and four files are written side by side. The save window starts in the same place as for projects: Documents, or the saved project's folder. The four files are:

1. `.xml` for Premiere Pro and DaVinci Resolve
2. `.fcpxml` for Final Cut Pro (DaVinci Resolve can read it too)
3. `.edl`, an edit list for other editing systems
4. `.csv`, a spreadsheet of every select that opens in Excel, Numbers or Google Sheets

The spreadsheet gives each select's source timecode twice. The timeline rate columns match the `.edl`. The clip rate columns match what your editor shows for the clip itself, so a 120 fps clip can show frame numbers up to 119 there. Both point to the same moment.

## Import into DaVinci Resolve

1. Open your project in Resolve.
2. Choose File, then Import, then Timeline.
3. Pick the `.xml` file you exported.
4. In the window that opens, leave Automatically import source clips into media pool ticked, and click OK.
5. The timeline appears in the media pool. Double-click it to open it.

If clips show as offline, select them in the media pool, right-click, choose Relink Selected Clips, and pick the folder with your footage.

## Import into Premiere Pro

1. Open your project in Premiere Pro.
2. Choose File, then Import, and pick the `.xml` file.
3. A new bin appears with the sequence inside. Double-click the sequence to open it.

If clips show as offline, right-click them, choose Link Media, and point Premiere to your footage.

## Import into Final Cut Pro

1. Choose File, then Import, then XML.
2. Pick the `.fcpxml` file.
3. A new event called Footage Selects appears with the project inside. Double-click the project to open it.

## Reading the markers

Each select has a marker named Select 1, Select 2 and so on, followed by how strong it is: strong (overall score 80 or more), good (65 to 79) or fair (under 65). The marker's note lists the select's focus, exposure and stability scores. For log footage it adds the profile and where the middle tones sit, for example "Sony S-Log3, middle tones -0.4 stops", and it says "Camera move" when the camera moves through most of the select. The strength word is always in the name, so you never have to rely on color.

The exported files also color the selects by strength: green for strong, blue for good, yellow for fair. How much of that color your editor shows depends on the editor. Final Cut Pro has no yellow markers, so there strong selects get a green completed to-do marker, good selects a blue marker, and fair selects a red to-do marker. In Premiere Pro the selects' clip labels are colored too.

## Get new versions

Under Updates, at the bottom of the panel on the right:

1. Click Check for updates. The app asks GitHub whether a newer version exists.
2. If one does, the button changes to Install version, followed by the number, and restart. Click it. If your project has been saved before, it is saved first; footage is never touched.
3. The app downloads the new version, checks its signature, installs it and opens again.

Tick Check when the app opens if you would like it to look every time it starts. This is off until you turn it on.

A check sends GitHub a request for one small file that describes the newest version. No footage, scores or project details are sent. If the app says this copy cannot update itself, download the newest version from the release page instead, the same way you installed it.

## Tips

1. If you shoot log, set Picture profile on the right once. Sony and DJI clips name their own profile, so this matters for other cameras.
2. Clear blue sky, plain walls and other shots with little detail score lower on focus even when sharp. Look at the clip's graph before writing it off.
3. Intentional camera moves, like a slow pan or a push in, count as stable. Fast whip pans count as a little less stable.
4. Night and other low-light footage: a low stability score means the camera really moved. In very dark, flat, noisy shots, earlier versions sometimes read small camera moves as no movement at all, which made gentle handheld footage look shakier than it was; this is fixed, and projects scored before the fix are analyzed again when you open them. Dark scenes score lower on exposure, because the app cannot tell a night scene from an underexposed day scene; lower Exposure at least for a night shoot. Sensor noise also reads as fine detail, so night shots can score higher on focus than they deserve. Check focus by eye on night clips.
5. Footage Selects judges picture quality, not content. It cannot tell a great moment from a dull one, so treat its selects as a first pass.

## Good to know

1. Nothing leaves your computer. Analysis, selection, export and reading finished videos make no network connections at all. The only time the app goes online is when you check for updates, as described above.
2. Your footage, the camera's files beside it and your finished videos are only read, never changed.
3. Sound is not analyzed. In the exported timeline the full clips carry their sound; the selects above them are picture only, so you do not hear everything twice.
4. Camera RAW formats such as Blackmagic RAW and RED R3D cannot be read. Use proxies or transcodes of that footage instead; Footage Selects handles proxies well.

## Landing page and waitlist

The project also has a one-page website that explains the app and collects a waitlist, to test whether people want it. It lives in the `site` folder, and a small Cloudflare Worker in the `worker` folder serves it and stores signups in a Cloudflare D1 database called footage-selects-waitlist. Everything runs on Cloudflare's free plan, and Cloudflare publishes it straight from GitHub, so it uses no GitHub Actions minutes.

What visitors see: what the app does, a place for a demo video, who it is for, how it works, what exists today kept separate from what is planned, the planned price for smart features (marked as planned and not charged), the waitlist form, a one sentence privacy note, and common questions.

What is stored for each signup: email, editor, what they shoot, roughly how many hours they shoot a month, and the time they joined. No IP address or tracking information is kept. Signing up twice with the same email keeps one entry.

Spam protection: a hidden field that only bots fill in, a check that the form was not filled in faster than a person can, a limit of 5 signups a minute from one connection, and Cloudflare Turnstile, an "are you human" check. Turnstile is on: a small box appears above the button and normally passes by itself without a click. It needs JavaScript, so a visitor with JavaScript turned off is told so above the form and cannot sign up. Turning Turnstile off (removing the key from `site/config.json`) lets those visitors sign up again, with the other three protections still in place.

The page is served with `Cache-Control: no-cache`, so browsers check for a new version on every visit and a change reaches visitors as soon as it is deployed.

To see the count and download the list, open the page's address followed by `/admin.html` and enter your admin password. It shows the total, signups in the last 7 days, a breakdown by editor, footage type and hours, and a button that downloads every signup as a spreadsheet.

What lives in config: the planned price, the demo video and the Turnstile key are in `site/config.json`. The product name comes from the same single setting the app uses.

## Credits

Footage Selects uses FFmpeg (ffmpeg.org) to read video. The copy inside the app is built from FFmpeg's own source under the GNU Lesser General Public License, version 2.1, with no parts under the GPL. It uses the Inter, Inter Tight and Averia Serif Libre typefaces under the SIL Open Font License. Every bundled library and its license is listed in [THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES), which is also inside the installed app.

The app's name lives in one setting, `productName` in `src-tauri/tauri.conf.json`. The app, its installers and its exported timelines all read it from there. This README and the build plan mention the name in their text and would need updating by hand.

People who want to build or change the app will find instructions in [DEVELOPMENT.md](DEVELOPMENT.md).
