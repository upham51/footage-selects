Fixed in 0.2.2:

1. Jello: a fast, small wobble is now caught, and calm footage, pans and water no longer come near the line.
2. Flicker: moving things in part of the frame no longer read as light flicker. Real flicker bands are caught more reliably.
3. Wind: a clip needs clearer evidence to be called windy. Set aside selects with sound problems now keeps a light breeze and sets aside only clipped sound, dropouts and strong wind.
4. Talking: synthetic and text to speech voices are recognised.
5. White balance: overcast sky, fog and haze no longer make a clip read as a different camera setting.

Clips are measured once more after opening a project from an earlier version, so the picture checks can use the new measurements.

This app pulls a first pass of selects from raw footage using classical image math. It runs fully offline.

Fixed in 0.2.1, from testing on a real shoot:

1. Sony clips without their XML file no longer read afternoon footage as night. Sony writes its recording time in UTC, and the app now reads it that way. For other cameras that do not say their time zone, the app checks which guess puts bright pictures in daylight.
2. Scenes are named, and times shown, in the local time where you shot.
3. Fit settings to my edits works from hand calls alone, and a call on a weak clip says what it did.
4. The light and sound filters say how many selects kept by hand they leave alone, and selects that cut mid-sentence say so with Start and end in a pause turned off.
5. Reels: the tempo no longer drifts after a fill or a breakdown, the first shot is never a sliver, and vertical and square reels fill the frame in Premiere Pro and DaVinci Resolve.
6. Typing a name that ends in .xml no longer gives the stills folder that ending. Plainer wording throughout.

New in 0.2:

1. Log footage is judged in stops through the camera's own curve (S-Log3, V-Log, Canon Log 3, LogC3, F-Log, F-Log2, D-Log, D-Log M, Apple Log, N-Log, HLG). Clipped highlights and underexposure are caught; the flat log look is no longer marked down.
2. The profile is read from Sony's XML and DJI's caption files beside each clip. DJI captions also give ISO, shutter, aperture, position and height, and clips shot with a shutter far too fast for the frame rate are flagged.
3. Learn from your edits: add videos you already finished, see how many of their shots the settings keep, and fit the settings to your choices.
4. Long selects are cut where the camera is still, and a camera move can stay whole.

Projects saved by 0.1 need analyzing once more after opening.

Download the file for your computer:

1. Mac with Apple silicon (M1 or later): the file ending in `mac-apple-silicon.dmg`
2. Windows (64-bit): the file ending in `windows-setup.exe`

The first time you open it, your computer will ask you to confirm, because the app is not signed with a paid developer certificate. The README explains each click: https://github.com/upham51/footage-selects#readme
