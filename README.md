# Footage Selects

Footage Selects looks through your raw footage and picks the usable moments for you. It checks every clip for focus, exposure and camera shake, keeps the best stretches, and hands you a timeline you can open in DaVinci Resolve, Premiere Pro or Final Cut Pro.

It is free, it works on Mac (Apple silicon) and Windows, and it runs entirely on your computer. Nothing is uploaded, there is no account, and it works without an internet connection. It uses plain image math, not artificial intelligence.

A first pass over about two hours of footage takes a few minutes. You then tune the result with sliders, which update instantly.

## What it does

1. Reads every clip you give it: any mix of cameras, formats, sizes and frame rates, including phone footage and proxies.
2. Scores a few frames from every second of each clip for:
   1. Focus: how much fine detail the picture has. Soft or missed focus scores low.
   2. Exposure: blown highlights, crushed shadows and flat, washed out pictures score low.
   3. Stability: smooth pans and locked off shots score high, handheld shake scores low.
3. Keeps the stretches where all three scores are good enough, up to the number you choose per clip.
4. Exports a timeline with every clip on video track 1 and each select directly above it on video track 2, with a colored marker on each select that lists its three scores.

## Download

Go to the [latest release](https://github.com/upham51/footage-selects/releases/latest) and download the file for your computer:

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

Reset to defaults puts every slider back where it started.

Frames scored per second, under Analysis, is the one setting that means analyzing everything again. More frames per second catches shorter moments but takes longer. The app asks before it starts.

## See why a clip was kept or dropped

Click any clip in the list. You will see three graphs, one each for focus, exposure and stability, across the length of the clip.

1. The navy line is the smoothed score used for selection. The grey line behind it is the raw score of each frame.
2. The dashed line is your threshold.
3. Green stretches were kept. A red stretch on a graph was dropped because that score was under its threshold. Grey stretches were good but too short. Amber stretches were good, but the clip already had its maximum number of better selects.
4. Every result also has a shape and a word: a tick for kept, a triangle for below threshold, a dash for too short, and three lines for over the per-clip limit.
5. Move the pointer over the graphs to read the exact scores at any moment.
6. Below the graphs, a table lists every stretch of the clip with the reason it was kept or dropped.

Click Back to all clips to return to the list.

## Save your work

Click Save project to save a `.selects` file. It holds every score, so when you open it again with Open project, nothing needs analyzing. The project file does not contain your footage, only where it is. If you move or rename footage, those clips are marked as missing; their scores still count, and you can relink them in your editor after export.

If a clip has changed since it was analyzed, it is marked as changed and analyzed again when you click Analyze footage.

## Export the timeline

Choose your settings under Export:

1. Timeline name: what the timeline will be called in your editor.
2. Timeline frame rate: starts as the most common frame rate in your footage. Pick another if your project uses a different one.
3. Drop frame timecode: appears for 29.97 and 59.94. Use it if your project uses drop frame timecode. The app turns it on when most of your 29.97 or 59.94 footage was recorded that way.
4. Layout: either every full clip on track 1 with each select above it on track 2, or only the selects back to back on track 2.

Click Export timeline, choose where to save and a name, and four files are written side by side:

1. `.xml` for Premiere Pro and DaVinci Resolve
2. `.fcpxml` for Final Cut Pro (DaVinci Resolve can read it too)
3. `.edl`, an edit list for other editing systems
4. `.csv`, a spreadsheet of every select that opens in Excel, Numbers or Google Sheets

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

Each select has a marker named Select 1, Select 2 and so on, followed by how strong it is: strong (overall score 80 or more), good (65 to 79) or fair (under 65). The marker's note lists the select's focus, exposure and stability scores. The strength word is always in the name, so you never have to rely on color.

The exported files also color the selects by strength: green for strong, blue for good, yellow for fair. How much of that color your editor shows depends on the editor. Final Cut Pro has no yellow markers, so there strong selects get a green completed to-do marker, good selects a blue marker, and fair selects a red to-do marker. In Premiere Pro the selects' clip labels are colored too.

## Tips

1. Log or flat picture profiles (such as S-Log, V-Log or C-Log) look washed out until graded, so they score low on exposure. Lower the exposure slider for that footage.
2. Clear blue sky, plain walls and other shots with little detail score lower on focus even when sharp. Look at the clip's graph before writing it off.
3. Intentional camera moves, like a slow pan or a push in, count as stable. Fast whip pans count as a little less stable.
4. Footage Selects judges picture quality, not content. It cannot tell a great moment from a dull one, so treat its selects as a first pass.

## Good to know

1. Nothing leaves your computer. The app makes no network connections.
2. Your footage is only read, never changed.
3. Sound is not analyzed. In the exported timeline the full clips carry their sound; the selects above them are picture only, so you do not hear everything twice.
4. Camera RAW formats such as Blackmagic RAW and RED R3D cannot be read. Use proxies or transcodes of that footage instead; Footage Selects handles proxies well.

## Credits

Footage Selects is released under the MIT license. It includes FFmpeg (ffmpeg.org) to read video, which is licensed under the LGPL or GPL depending on the build; its source code is available from ffmpeg.org. It uses the Inter, Inter Tight and Averia Serif Libre typefaces under the SIL Open Font License.

People who want to build or change the app will find instructions in [DEVELOPMENT.md](DEVELOPMENT.md).
