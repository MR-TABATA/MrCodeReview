# MrCodeReview Privacy Policy

Last updated: 2026-11-11

MrCodeReview (the "Software") runs on your Mac. It has **no account, no analytics, no telemetry,
and no crash reporting**. The author (TABATA Hitoshi) does not receive your code,
your review results, or your usage data.

## What the Software sends over the network

The Software connects to the internet only in the two cases below. Nothing
else is sent, and nothing is sent in the background.

### 1. AI review requests (when you press a review button)

- **Where:** the AI service you configured. Currently `api.anthropic.com`
  (Anthropic).
- **What:** the full review request: the review instructions, the code
  differences, and related code. **You can see the exact text on screen before
  and after each request** ("What was sent" tab). Your API key is sent only as
  the authentication header.
- **Who sees it:** the AI provider, under its own terms and privacy policy. The author
  does not.
- The Software does not send anything to an AI service until you press the button.

### 2. Update check (only when you choose "Check for Updates…")

- **Where:** `mrktools.app` (the author's site, served through Cloudflare).
- **When:** only when you choose "アップデートを確認…" in the app menu. Never at launch.
- **What:** an ordinary web request. The only identifying header is the name and
  version of the update library (for example `tauri-plugin-updater/2.12.0`); the app
  version, your name, and any ID are not sent. The host sees what any web request
  reveals (for example your IP address and request time). An update is installed
  only if its signature is valid.

## What stays on your Mac

- **Review history, triage, notes, and settings:** in
  `~/Library/Application Support/com.hitoshi.mrcodereview/`. Delete the folder to remove them.
  The `mrcr` command-line tool uses this same folder. Earlier versions (MrkCodeReview 1.x and `mrcr` 0.5.0)
  kept them in `com.hitoshi.mrkcodereview/`; a newer version moves that folder here the first time it starts,
  and leaves a link under the old name so that an earlier version still sees the same data.
- **Your API key:** in the macOS Keychain, never in a file.
- **Hand-off files:** `.mrcodereview/out/` inside the repository you chose. The
  folder ignores itself (`.gitignore`), so it is not committed.
- **Your repositories:** the Software reads them with `git`. It does not upload them
  anywhere except as described in (1).

## Reading agent conversation logs (optional)

If you press "対応内容を取り込む", the Software looks for the report lines that your
coding agent wrote in its own logs (Claude Code: `~/.claude/projects`; agy:
`~/.gemini/antigravity-cli/conversations`; GitHub Copilot CLI: `~/.copilot/session-state`,
or the folder `COPILOT_HOME` points to). It asks your permission the first time and
you can revoke it in Settings. It stores only the matching report lines and the log
name and time, not the conversation. It never reads these logs unless you press the button.

## Changes and contact

If this policy changes, the new version ships with the Software and the date above
is updated. Questions: the [contact page](https://mrktools.app/en/contact) on mrktools.app.
