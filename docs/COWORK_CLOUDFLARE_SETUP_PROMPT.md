You are setting up the live waitlist page for my app, Footage Selects, in my own Cloudflare account. The code is already finished in my private GitHub repo upham51/footage-selects (the page is in the site folder, the Cloudflare Worker in the worker folder, and docs/BUILD_PLAN.md explains the project). A Cloudflare D1 database called footage-selects-waitlist already exists in my account and is already connected in the code, so do not create another one. I am not technical: do every click yourself in the browser, and only stop to ask me when you need me to sign in or approve something.

Rules:
1. Stay on Cloudflare's free plan. If any screen asks for a payment method or an upgrade, stop and tell me.
2. Do not change any code, files or settings in the GitHub repo, and do not create, delete or rename any other Worker, database or project in my Cloudflare account.
3. Secrets: never write the admin password or the Turnstile secret key into your report, a chat message, a note or a file. The Turnstile site key is public and is fine to report.

STEP 1: CONNECT THE REPO AND DEPLOY
1. Open dash.cloudflare.com and sign in (ask me if needed). Go to Workers & Pages, then Create, then choose the option to import a repository from GitHub (Workers Builds).
2. If asked, connect GitHub and give Cloudflare access to the repository upham51/footage-selects only. Ask me to approve the GitHub screen if it needs my sign-in.
3. Pick upham51/footage-selects. Use exactly these settings:
   Project or Worker name: footage-selects-waitlist (must match exactly)
   Production branch: main
   Root directory (under advanced settings): worker
   Build command: leave empty
   Deploy command: npx wrangler deploy
4. Click create and deploy, and wait for the build to finish. If it fails, open the build log, copy the error lines exactly into your report, and try nothing else on your own except this one fix: if the error mentions permissions for D1 or the API token, look in the build settings for the API token and give it permission to edit D1, then retry once.
5. When it succeeds, write down the page address. It ends in workers.dev.

STEP 2: THE ADMIN PASSWORD
1. Open the Worker footage-selects-waitlist, then Settings, then Variables and Secrets, then Add.
2. Type: Secret. Name: ADMIN_TOKEN (exactly). For the value, ask me to type a long password of my own choosing into the field myself and save it in my password manager. If I ask you to make one, create a random password of at least 24 characters, put it straight into the field, and show it to me once on screen so I can save it, but never write it anywhere else.
3. Save and deploy.

STEP 3: THE SPAM CHECK (TURNSTILE)
1. In the Cloudflare sidebar open Turnstile, then add a widget.
2. Name: Footage Selects waitlist. Hostname: the workers.dev address from step 1, without https:// and without any path. Widget mode: Managed. Create it.
3. Write down the Site Key for the report.
4. Copy the Secret Key and add it to the Worker the same way as step 2: Settings, Variables and Secrets, Add, type Secret, name TURNSTILE_SECRET_KEY (exactly), paste the value, save and deploy. Do not write the secret key anywhere else.
5. Note: the human check box only appears on the page after the site key is added to the page's config, which my developer will do. Until then the page still works and is protected by the other spam checks.

STEP 4: CHECK IT WORKS
1. Open the page address. Check it loads, shows the product name at the top, the planned price in the pricing section, and the waitlist form.
2. Wait 3 seconds, then sign up once with the email cowork-setup-test@example.com (editor: DaVinci Resolve, footage: Drone, hours: Under 2 hours). You should see a green "You are on the list" message.
3. Open the page address followed by /admin.html and ask me to type the admin password. Check it shows 1 person, and that the download button gives a spreadsheet with that one test row.
4. Open the Worker's Settings in Cloudflare and confirm it lists the bindings DB (the footage-selects-waitlist database), ASSETS and SIGNUP_LIMITER, and the two secrets ADMIN_TOKEN and TURNSTILE_SECRET_KEY (names only).

REPORT
Write a short plain-language report:
1. One line first: is the waitlist page live and working, yes or no.
2. The page address and the admin page address.
3. The Turnstile site key (the public one only).
4. Each step above with done, failed or skipped, and the exact error text for anything that failed, with a screenshot.
5. Anything on screen that did not match these instructions, and what you did instead.
6. A reminder that the test signup cowork-setup-test@example.com should be removed from the list.
