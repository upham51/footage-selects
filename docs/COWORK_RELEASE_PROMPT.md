You are publishing the first release of a free Mac and Windows app called Footage Selects. The code is in my GitHub repository upham51/footage-selects. GitHub Actions cannot run while the repository is private (its build jobs stop after 2 seconds with no log), so we are making the repository public, which makes Actions free. I am not technical. Do everything in the browser yourself, never ask me to run commands, and write your report in plain language. Do not change any code, do not spend money, and do not change any setting I do not list here.

Stop and ask me before going on if any of these happen:
1. GitHub asks for my password, a two-factor code or a passkey. I will enter it myself.
2. Anything asks for payment or a billing change.
3. A step below fails twice in a row.

STEP 1: CHECK THE SIGNING KEY BEFORE GOING PUBLIC

1. Open github.com/upham51/footage-selects, then Settings, then Secrets and variables, then Actions.
2. Note whether a secret called TAURI_SIGNING_PRIVATE_KEY exists, and whether the Variables tab has UPDATER_PUBLIC_KEY. Do not open or copy them.
3. If they are missing, that is fine: the app is built without self-updating this time. Never run the workflow called "Create update signing keys" while the repository is public. It saves the private signing key as a download on the run page, and on a public repository anyone can download it.

STEP 2: MAKE THE REPOSITORY PUBLIC

1. Settings, then General, scroll to Danger Zone, then Change repository visibility, then Change to public.
2. Read each warning GitHub shows and confirm. If it asks you to type the repository name, type upham51/footage-selects.
3. Check that the repository page now shows the word Public next to its name.

STEP 3: GET THE OPEN PULL REQUEST GREEN AND MERGE IT

1. Open Pull requests and then pull request number 1, "Fix the problems from the October test report".
2. Open the Checks tab. For the workflow called CI, click Re-run all jobs (or Re-run failed jobs).
3. Wait until the three CI checks finish: App compiles (Linux), Engine tests (Linux), Waitlist page and Worker tests. This takes about 10 to 20 minutes. Check back every few minutes and do not click anything else while you wait.
4. If all three pass, go to the Conversation tab. If the pull request says Draft, click Ready for review. Then click Merge pull request and Confirm merge.
5. A check called "Workers Builds: footage-selects-waitlist" may still be red. Ignore it for the merge. Open its View logs link only if it is visible to you, and copy the last 30 lines of the log into your report.
6. If any of the three CI checks fail, do not merge. Open the failed check, copy the last 40 lines of its log into your report, then go straight to step 4 anyway.

STEP 4: RUN THE RELEASE

1. Open the Actions tab. In the left list, click Release.
2. Click Run workflow. Leave the branch as main. Tick "Create the tag and release for the version in tauri.conf.json". Click the green Run workflow button.
3. Open the new run. It builds the Windows installer and the Mac disk image and takes about 30 to 60 minutes the first time, because it also builds the video reader. Check back every 5 minutes.
4. When it finishes, every job should have a green tick. If one fails, open it, copy the last 40 lines of the failed step's log into your report and stop.

STEP 5: CHECK THE RELEASE

1. Open the repository's Releases page (github.com/upham51/footage-selects/releases).
2. There should be a release called v0.1.0 with at least two files: one ending in mac-apple-silicon.dmg and one ending in windows-setup.exe.
3. Sign out of GitHub in a private browser window, or use a different browser, and open the releases page. Check that both files can be downloaded without signing in.

STEP 6: INSTALL ON THIS MAC

1. Download the .dmg file. Open it and drag Footage Selects into Applications.
2. Open Footage Selects from Applications. The Mac will warn that Apple could not verify it. That is expected. Follow the README's "Install on a Mac" steps for this macOS version (System Settings, then Privacy & Security, then Open Anyway). If it asks for this Mac's password, stop and ask me to type it.
3. Confirm the app window opens, and take a screenshot.

REPORT

Write a short report in plain language, with no em dashes. Use a numbered list for each step with PASS, FAIL or SKIPPED and one sentence on why. Include:
1. Whether the signing key existed (yes or no only).
2. The time the repository became public.
3. Whether pull request 1 was merged.
4. The link to the release, and the names and sizes of its files.
5. Whether the app opened on this Mac, with the screenshot.
6. Any log lines you copied.

After that, if the app opened, ask me whether to go on with Part B of docs/COWORK_TEST_PROMPT.md in the repository. That part tests the app itself on my footage.
