New in 0.2.6:

1. The website moves. Headlines rise word by word, the footage frame opens like a shutter, the navy band floods in, and a gold playhead under the header shows where you are on the page. The app itself is unchanged.

New in 0.2.5:

1. A version for Macs with an Intel processor: the file ending in `mac-intel.dmg`. It needs macOS Big Sur (11) or later.

New in 0.2.4:

1. The app updates itself. Open the Project tab and choose Check for updates, or turn on Check when the app opens. Download this version by hand once; every version after it installs from inside the app.

New in 0.2.3:

1. A new look. The window opens dark, with light one switch away on the Project tab. The viewer stays dark in both, so footage is judged the same way.
2. Less to read. Explanations sit behind the small i beside each setting; hover it to read them.
3. One clip at a time: a large viewer first, then one timeline with the selects numbered on a frame strip and thin Focus, Exposure and Stability lines that turn red under their minimum.
4. Play flips through the clip as fast as frames can be read, without sound. Space plays, [ and ] jump between selects, K keeps, X drops, 1 to 5 give stars.
5. Selects are cards with a picture, length and three small score bars. Stretches that were not used are small chips; hover one to see why.
6. All clips shows a picture of each clip, and tabs for clips with selects and clips with nothing kept.
7. Settings are in four tabs: Selects, Filters, Export and Project. The two length sliders are now one, and smoothing and ranking are under Advanced.

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
2. Mac with an Intel processor: the file ending in `mac-intel.dmg`
3. Windows (64-bit): the file ending in `windows-setup.exe`

The first time you open it, your computer will ask you to confirm, because the app is not signed with a paid developer certificate. The README explains each click: https://github.com/upham51/footage-selects#readme
