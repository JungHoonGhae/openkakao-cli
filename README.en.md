<div align="center">
  <h1>OpenKakao</h1>
  <p>Unofficial CLI for KakaoTalk on macOS.</p>
  <p>It works well as a terminal tool for humans and as a local interface for AI or agent workflows through JSON output, watch mode, hooks, and webhooks.</p>
  <p>The executable name is <code>openkakao-cli</code>.</p>
</div>

<p align="center">
  <a href="#quick-start"><strong>Quick Start</strong></a> ·
  <a href="#highlights"><strong>Highlights</strong></a> ·
  <a href="#docs"><strong>Docs</strong></a> ·
  <a href="#claude-code-skill"><strong>Claude Code Skill</strong></a>
</p>

<p align="center">
  <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="MIT License" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.95-orange.svg" alt="Rust" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/status-active-brightgreen" alt="Status Active" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/docs-fumadocs-black" alt="Docs" /></a>
</p>

[한국어](README.md) | **English**

> [!TIP]
> **No login, 0.4 s per send, none of the known account-sanction triggers.** `local-send`/`ax-read` drive the **official KakaoTalk app** you are already signed in to, through the macOS Accessibility API. There is no server login, no LOCO protocol traffic and no database decryption, so the paths with reported sanctions (unregistered-device logins, unofficial-client traffic) are never used. See [Quick Start](#quick-start).

### ⚡ Fast

| Task (real device: Mac Studio, KakaoTalk 26.8) | v1.9 | **v1.11** |
|---|---|---|
| Read an open chat's recent messages (`ax-read`) | 26.8 s | **0.2 s** (~130×) |
| Send a message, **verified by its new bubble** (`local-send`) | 44 s | **0.4 s** (~100×) |
| Find and open a closed chat (`--open-if-needed`) | 6.5 s (v1.10) | **3.2 s** (worst case measured, 140 matching rows; KakaoTalk's own search) |
| Touch ID when sending to an allowlisted chat | every send | **none** (allowlisting is the approval) |

- **Verified, not just sent.** Within those 0.4 s the composer clears and an additional exact-text outgoing bubble appears. When that cannot be proven the result is `delivery_uncertain`, and it is never retried automatically.
- **Your focus stays.** An open chat window is used without raising KakaoTalk or posting keyboard or mouse events.
- **Runs from your phone.** If a login on another device signs this Mac out, `ax-login` signs it back in from the keychain.

> [!NOTE]
> Server login (`login --save`/`login --manual`) is broken on most recent KakaoTalk macOS builds ([#15](https://github.com/JungHoonGhae/openkakao-cli/issues/15), [#20](https://github.com/JungHoonGhae/openkakao-cli/issues/20), [#22](https://github.com/JungHoonGhae/openkakao-cli/issues/22)). **Do NOT repeatedly retry login from an unregistered device** — Kakao may block your account's "sub-device login" or restrict the account (this has actually been reported). The local SQLCipher DB path (`local-chats`/`local-read`/`local-search`) is also currently unreliable on recent builds — use `ax-read` for reads, and `notif-watch` (notification stream, window-independent, self-sends excluded) or `ax-watch` for receive detection.

> [!WARNING]
> This project is an unofficial CLI and is not affiliated with or endorsed by Kakao Corp. It is built for research, automation, and local workflows around the macOS KakaoTalk app.
> Depending on how you use it, Kakao may interpret that use as a violation of its Terms of Service or operating policies, and your account may be suspended or permanently deleted.
> Review the relevant policies yourself before using it and proceed only if you accept full responsibility for that risk.

<div align="center">
<table>
  <tr>
    <td align="center"><strong>Works with</strong></td>
    <td align="center"><img src="docs/assets/logos/openclaw.svg" width="32" alt="OpenClaw" /><br /><sub>OpenClaw</sub></td>
    <td align="center"><img src="docs/assets/logos/claude.svg" width="32" alt="Claude Code" /><br /><sub>Claude Code</sub></td>
    <td align="center"><img src="docs/assets/logos/codex.svg" width="32" alt="Codex" /><br /><sub>Codex</sub></td>
    <td align="center"><img src="docs/assets/logos/cursor.svg" width="32" alt="Cursor" /><br /><sub>Cursor</sub></td>
    <td align="center"><img src="docs/assets/logos/bash.svg" width="32" alt="Bash" /><br /><sub>Bash</sub></td>
    <td align="center"><img src="docs/assets/logos/http.svg" width="32" alt="HTTP" /><br /><sub>HTTP</sub></td>
  </tr>
</table>
</div>

<p align="center">
  <img src="assets/thumbnail-en.png" alt="openkakao" width="720" />
</p>

## Quick Start

### Login-free path (recommended)

No server login needed — just KakaoTalk running and already logged in.

```bash
# Homebrew (Homebrew 7+ must trust a third-party tap formula before installing or upgrading it)
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli
brew install openkakao-cli

# 1. First-run setup: checks Accessibility, sign-in and Spaces, writes the send
#    allowlist, installs the agent skill
openkakao-cli init                                                   # interactive
openkakao-cli init --yes --allow-chat "your memo chat's display name" --skill claude   # scripted
#    Only names you type or pass are allowlisted ([safety] allow_ax_send /
#    allowed_send_chats in ~/.config/openkakao/config.toml). Safe to re-run.

# 2. Send (no server contact; an open chat window is used without raising KakaoTalk)
openkakao-cli local-send "chat display name" "Hello from CLI!" --dry-run   # preview
openkakao-cli local-send "chat display name" "Hello from CLI!" -y         # actually send (no Touch ID for allowlisted chats)
openkakao-cli --open-if-needed local-send "closed chat" "hi" -y            # finds and opens a closed chat via KakaoTalk's search
openkakao-cli --open-if-needed --pick 1 local-send "same-name chat" "hi" -y   # duplicates: choose by number (needs Touch ID)
openkakao-cli local-send-file "chat display name" ./report.docx --dry-run  # any file: docx, pdf, zip, ...
openkakao-cli local-send-file "chat display name" ./report.docx -y        # send through the official app
openkakao-cli local-send-photo "chat display name" ./family.jpg -y        # send a photo

# Signed out by a login on another device: save the password in the keychain once, recover remotely
security add-generic-password -s openkakao.kakaotalk -a 'you@example.com' -w   # prompts without echo
openkakao-cli ax-login --account you@example.com

# 3. Read recent messages — same AX approach, scrapes what's rendered on screen
openkakao-cli ax-read "chat display name" -n 20

# Read-only check of an open chat window (composer, 전송 button, file button)
openkakao-cli doctor --chat "chat display name"

# (experimental) Reply or edit in an open chat window without taking focus.
# The target bubble must match its whole text exactly.
openkakao-cli local-reply "chat display name" "reply text" --to-last --dry-run   # show the target only
openkakao-cli local-reply "chat display name" "reply text" --match "whole original text" -y
openkakao-cli local-edit "chat display name" --match "your whole original text" "fixed text" -y

# 4. Detect incoming messages (AX) — polls the chat list and fires a hook/webhook
#    when a chat's unread count rises (no server contact)
openkakao-cli ax-watch --hook-cmd 'my-script.sh'

# 5. Detect incoming messages (notification stream) — polls the macOS Notification
#    Center DB for new messages. Works with the KakaoTalk window closed, and your
#    own sent messages are excluded (no login, no server contact)
openkakao-cli notif-watch --json
openkakao-cli notif-watch --hook-keyword 'urgent' --hook-cmd 'my-script.sh'
# Also process messages that arrived while the watcher was down and remain in Notification Center
openkakao-cli notif-watch --replay-existing --json
# Recommended for long-running jobs: persist before delivery and retry after restart
openkakao-cli notif-watch --durable --json
```

> [!IMPORTANT]
> **The default paths never take your focus** (verified on KakaoTalk 26.9.0, macOS 26): KakaoTalk is not raised and no synthetic keyboard or mouse input is posted.
> - `ax-read` and `local-send*` use an **already-open chat window** whose title matches exactly; two open windows with that title are refused.
> - Text is inserted through the composer's `AXSelectedText` and sent with `AXPress` on the `전송` button.
> - Files and photos: `파일전송` button → the open panel's sidebar folder → the file row → `열기` → the preview's `1개 전송`, all through Accessibility. A file outside the top level of Desktop, Documents or Downloads is hard-linked (copied across volumes) into `~/Downloads` under its own name for the send and removed once delivery is verified; an existing different file with that name stops with `staging_conflict`.
> - A chat without an open window returns `chat_window_not_open` with a fix by default. `--open-if-needed` opens it from the chat-list row's context menu (`채팅방 열기`), still without raising KakaoTalk, which marks the chat as read. A merely hidden main window is shown again through the app's own `창 > 채팅` menu item.
>
> For builds without the `전송` button, `--fallback-keys` keeps the older raise-and-key routes: Return for text, clipboard plus ⌘V for files, double-click to open a chat. Only these raise KakaoTalk, after the keyboard and mouse have been idle for `--wait-idle` seconds (default 2, `[ax] wait_idle_secs`), or stop with `user_active` after 60 s. They restore the previous app afterwards, error paths included. A background KakaoTalk ignores pid-posted key events, so key-based steps work only while it is frontmost.

> [!TIP]
> **`ax-watch` vs `notif-watch`** — both detect incoming messages without login. `notif-watch` reads the macOS Notification Center DB (a plaintext SQLite), so it **works even when the KakaoTalk window is closed, minimized, or on another Space**, and since notifications only fire for received messages it **excludes your own sends automatically**. Existing notifications only establish a baseline by default; pass `--replay-existing` to process retained notifications that arrived while the watcher was down. For long-running jobs, prefer `--durable`: it persists events to `~/.config/openkakao/receive_inbox.db`, atomically leases each event to one worker, and retries failures with exponential backoff. After eight failures an event is quarantined so later events can proceed; terminal records missing from Notification Center are pruned after a 24-hour grace period. Delivery is **at least once** (use `chat_id` + `log_id` as the downstream idempotency key). The trade-off: **muted / notifications-off chats** and the **chat you're currently focused on** post no notification, so they aren't seen, and messages already removed from Notification Center cannot be replayed. Run `ax-watch` alongside it if you need those chats with the window open. Events are NDJSON with `event_type`, `chat_name`, `chat_id` (room id), `log_id` (message id), `message`, `attachment`, `received_at`.

### Server-login path (mostly broken right now)

```bash
# 1. Save auth data — fails on most recent builds (#15, #20, #22)
openkakao-cli login --manual --save
#    (older builds where cache extraction still works: openkakao-cli login --save)

# 2. List chats
openkakao-cli chats

# 3. Read messages
openkakao-cli read <chat_id> -n 20

# 4. Read from local DB (unreliable on current builds — see ax-read above)
openkakao-cli local-chats
openkakao-cli local-read <chat_id>

# 5. Send a message (requires allow_loco_write = true in config)
openkakao-cli send <chat_id> "Hello from CLI!"
```

Only force the older cache-backed path when you need it:

```bash
openkakao-cli chats --rest
openkakao-cli read <chat_id> --rest
openkakao-cli members <chat_id> --rest
```

### For Agent

```bash
# Login-free read (no server contact, AX-based)
openkakao-cli ax-read "chat display name" -n 20 --json

# Agents persist a proposal; this never sends
openkakao-cli --no-prefix safe-send propose "chat display name" "message" \
  --reply-chat-id 42 --reply-log-id 99 \
  --idempotency-key 'reply:42:99:policy-v1' --json
openkakao-cli safe-send list --json

# Structured output
openkakao-cli --json chats
openkakao-cli --json read <chat_id> -n 20

# Preview before executing
openkakao-cli send <chat_id> "message" --dry-run --json

# Real-time event stream
openkakao-cli watch --json

# Connect to local hooks or webhooks
openkakao-cli --unattended --allow-watch-side-effects watch \
  --hook-cmd 'jq . > /tmp/openkakao-event.json'
```

To use it directly from Claude Code or Codex:

```bash
openkakao-cli init --skill all   # or: npx skills add JungHoonGhae/skills@openkakao-cli
```

## Highlights

- **Fast**: 0.4 s per verified send, 0.2 s per read, about 100× faster than v1.9
- **None of the known sanction triggers**: no server login or LOCO traffic, only the official app is driven
- **Human approval model**: allowlisted chats send directly; any other chat is flagged ⚠ and needs Touch ID; same-name chats are never guessed (`--pick`)
- **Remote recovery**: `ax-login` (keychain), and `disposition` in every `--json` error (whether a retry could send twice)
- Send and read real messages **without logging in**, via `local-send`/`ax-read` (drives the KakaoTalk UI directly through the macOS Accessibility API, no server contact)
- Extracts auth data from the macOS KakaoTalk app
- Reads chats, messages, members, friends, and profiles
- Sends messages, watches real-time events, and handles media over LOCO
- Fits well into `jq`, `cron`, SQLite, and LLM workflows through `--json`
- Connects to local automation and agent flows through `watch`, hooks, and webhooks
- Can recover some reads with `friends --local`, `profile --local`, and `profile --chat-id`
- Local DB reads via `local-chats`, `local-read`, `local-search` (unreliable on current builds — prefer `ax-read`)
- Preview any write with `--dry-run` before executing
- Send to memo chat with `send --me` for quick testing
- LOCO write ops disabled by default — opt in with `safety.allow_loco_write = true`
- `local-send`/`local-send-photo` also disabled by default — opt in with `safety.allow_ax_send = true` plus a `safety.allowed_send_chats` allowlist
- Prefer `safe-send` for agent-driven sends — proposals only touch a local outbox and actual approval requires a human at an interactive terminal; a chat outside the allowlist is flagged ⚠ and also needs macOS Touch ID/login-password authentication

## Where It Fits

- when you want chat history as JSON for downstream tools
- when KakaoTalk should become an input channel for local scripts or operator tools
- when you want to trigger follow-up actions from watch events through hooks or webhooks
- when you want one CLI that works for both direct terminal use and AI-driven local workflows

## Safety Mode

Since v1.1.0, LOCO write operations (send, delete, edit, react) are **disabled by default**.
To protect your account, commands that write to the server require explicit opt-in.

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_loco_write = true
```

`local-send`, `local-send-file` and `local-send-photo` (AX-based real sending) are also disabled by default (`local-send` since v1.4.0), and need their own opt-in. A chat listed by its exact name in the **chat allowlist** (`openkakao-cli init --allow-chat "<name>"`) counts as approved in advance, so sends to it ask for no authentication. Any other chat is flagged `⚠ NOT in [safety] allowed_send_chats` and needs macOS user authentication (Touch ID or login password) immediately before sending. There is no chat-id left to cross-check the target against, so a real send also requires exactly one open chat window titled exactly like the chat (or, when `--open-if-needed` opens it, one unique exact-name chat-list row) and KakaoTalk code-signature verification:

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_ax_send = true
allowed_send_chats = ["your memo chat's display name", "another allowed chat"]
```

For agents and automation, prefer `safe-send` over a direct `local-send -y`:

```bash
# 1. Persist a proposal only — this command can never send
openkakao-cli safe-send propose "chat display name" "message to review"

# 2. Inspect proposed and uncertain intents
openkakao-cli safe-send list

# 3. A human reviews the exact target/message and types the 12-character code
#    (a chat outside the allowlist also needs Touch ID or the login password)
openkakao-cli safe-send approve <intent_id>

# Discard an intent
openkakao-cli safe-send cancel <intent_id>
```

Proposals expire after 15 minutes and live in `~/.config/openkakao/safe_send_outbox.db` with `0600` permissions. Approval has no unattended bypass: it requires the interactive 12-character code. A chat outside the allowlist also needs macOS device-owner authentication, as a direct `local-send*`, `local-reply` or `local-edit` to it does. The allowlist is a config file your account can write, so any local process (a shell, an agent) can send to the chats in it without authentication; list only chats that may receive such sends. With `[safety] device_owner_auth_grace_secs` (default 0) one approval stays valid for that many seconds, sudo-style (max 900). The approval time is a file your account can write, so during the window any local process (a shell, an agent) can send to allowlisted chats or extend the window; enable it only on a Mac where every local process is yours. The outbox enforces at least 10 seconds between claims, 3 sends per chat per hour, 10 globally per hour, and 20 globally per day. A failure proven to occur before the commit (the `전송` press, or Return with `--fallback-keys`) is classified as `not_sent`, so the proposal can be reviewed again; an ambiguous result after the commit may have been delivered, or an interrupted execution, becomes `uncertain` and is never retried automatically. The running process must have KakaoTalk's official bundle/team code signature, duplicate exact-title windows or chat-list rows are refused, and a non-empty composer is refused. The target window and composer are read back immediately before the commit, and completion is verified only when the composer clears and an additional exact-text **outgoing** bubble appears.

This path is implemented directly with macOS Accessibility/CoreGraphics inside `openkakao-cli`. Orca and agent `$computer-use` skills are not installation or runtime dependencies; agents use the domain-level `safe-send propose` plus human approval instead of generic UI click or keyboard actions.

Read-only operations are always available:

| Command | Description | Server Contact |
|---------|-------------|----------------|
| `ax-read <chat_name>` | Scrape recent messages from an open chat window (AX) | None |
| `ax-watch` | Poll the chat list, fire a hook/webhook when unread count rises (AX, no login) | None |
| `notif-watch` | Poll the macOS Notification Center DB for new incoming messages — window-independent, self-sends excluded (no login) | None |
| `local-chats` | List chats from local DB (unreliable on current builds) | None |
| `local-read <id>` | Read messages from local DB (unreliable on current builds) | None |
| `local-search "keyword"` | Search local DB (unreliable on current builds) | None |
| `chats --rest` | List chats via REST | REST |
| `read <id> --rest` | Read messages via REST | REST |
| `send ... --dry-run` | Preview send without executing | None |
| `local-send ... --dry-run` | Preview an AX send without executing | None |
| `local-send-file ... --dry-run` | Preview an AX file send (path and size validated) | None |
| `local-send-photo ... --dry-run` | Preview an AX photo send without executing | None |
| `doctor --chat <chat_name>` | Inspect an open chat window (composer, `전송` and file buttons); presses nothing | None |
| `local-reply/local-edit ... --dry-run` | (experimental) Find and show the bubble a reply/edit would target | None |
| `safe-send propose/list` | Persist or inspect send proposals (never sends) | None |

> [!NOTE]
> `ax-read`/`local-send*` on an already-open chat window do not need the main window. `ax-watch` and `--open-if-needed` need to find KakaoTalk's **main chat-list window** via the macOS Accessibility API. If that window is **minimized**, or on a different **macOS Space** (virtual desktop) than the one you're currently viewing, it won't be found (a merely hidden one is shown again focus-free through `창 > 채팅`; otherwise you get a clear error and fix). If this keeps happening, run `openkakao-cli init --all-spaces` once (the same as Dock → Options → Assign To → All Desktops).
>
> **`notif-watch` is not subject to this** — it reads the macOS Notification Center DB rather than an AX window, so it works regardless of window state (but muted / focused chats post no notification and so aren't seen).

## Troubleshooting

With `--json`, failures print `{"error": {"code", "message", "fix", "disposition"}}` on stdout. Only `disposition.retry: "safe"` (`delivery: "not_delivered"`) can be retried as-is without a duplicate send; on `uncertain` or `unknown`, check KakaoTalk first. `doctor` (read-only) checks the same things up front.

| code | Cause | Fix |
|------|-------|-----|
| `accessibility_not_granted` | The app running the CLI lacks Accessibility | System Settings → Privacy & Security → Accessibility: enable your terminal (or Orca, ...). Over SSH add `/usr/libexec/sshd-keygen-wrapper` |
| `kakaotalk_not_running` | KakaoTalk is not running | `open -g -b com.kakao.KakaoTalkMac` (background launch) or `openkakao-cli init` |
| `kakaotalk_signed_out` | Only the `로그인` window is showing | `openkakao-cli ax-login` (keychain password) or sign in on this Mac |
| `login_rejected` | Wrong account or password | **Do not retry** (lockout risk). Check the login id (usually an email) and the keychain password |
| `login_needs_human`, `login_secret_missing` | Device check or unknown dialog / no keychain password | Finish on the Mac / `security add-generic-password -s openkakao.kakaotalk -a <id> -w` |
| `chat_ambiguous` | Several chats share the exact name | `--open-if-needed` numbers them (time, preview, 📌); choose with `--pick N` (needs Touch ID) |
| `chat_not_allowed` | Touch ID was not given for a chat outside the allowlist | Approve the prompt, or allowlist it with `init --allow-chat` |
| `main_window_not_visible`, `kakaotalk_not_on_all_spaces` | Main window closed or on another Space | `openkakao-cli init --all-spaces` (records Dock → Options → Assign To → All Desktops and restarts the Dock once) |
| `chat_window_not_open` | No open window titled exactly like the chat | Open the chat window once, or pass `--open-if-needed` |
| `chat_not_found`, `chat_list_filtered` | Row not loaded, or the chat list is filtered by a search term | Clear the search field, then scroll the list or switch tabs so it repaints; the list can stay stale until the main window repaints |
| `send_button_missing` | This build exposes no `전송` button | `--fallback-keys` |
| `menu_not_found` | A context menu was not found (nothing else was done) | Retry or open the chat yourself; dismiss a visible menu by clicking elsewhere. **Never press Esc**: KakaoTalk closes its windows on Esc |
| `composer_busy` | The composer holds a draft or a pending reply/edit | Inspect it with `doctor --chat`, then `--discard-draft` (prints it before clearing) or tidy it up in KakaoTalk |
| `message_not_found`, `message_ambiguous` | No bubble matched, or several share the exact text | Only the newest ~40 rows are searched; scroll the bubble into view, or use `--to-last` |
| `action_unverified` | KakaoTalk did not show the expected reply/edit state, so nothing more was done | Check the chat window and close a leftover banner with its X (never Esc) |
| `staging_conflict` | `~/Downloads` already has a different file with this name | Send it from the top level of Downloads, Desktop or Documents, or rename it |
| `user_active` | You kept typing during a `--fallback-keys` raise | Retry when idle, or `--wait-idle 0` |
| `delivery_uncertain` | The send may have happened but was not proven | Check KakaoTalk before retrying; never retry blindly |
| `file_type_refused` | KakaoTalk's file picker greys out this file type (seen with `.bin`) | Zip it, or send a document/image type |
| Homebrew `untrusted tap` | Homebrew 7+ trust policy | `brew trust --formula junghoonghae/openkakao/openkakao-cli` |

## Requirements

| Requirement | Notes |
|-------------|-------|
| macOS | KakaoTalk desktop app must be installed and logged in |
| Rust 1.95.0 | Source builds (`rust-toolchain.toml` selects it automatically) |

## Installation

### Homebrew

```bash
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli   # Homebrew 7+
brew install openkakao-cli
openkakao-cli init
```

If installing or upgrading is refused because the tap is untrusted, run the `brew trust` line; `doctor` shows the same fix.

### From source

```bash
git clone https://github.com/JungHoonGhae/openkakao-cli.git
cd openkakao-cli
cargo install --path .
```

## Docs

- Documentation site: https://openkakao.vercel.app/
- Quick start: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI reference: https://openkakao.vercel.app/docs/cli/overview/
- Automation overview: https://openkakao.vercel.app/docs/automation/overview/
- LLM / agent workflows: https://openkakao.vercel.app/docs/automation/llm-agent-workflows/
- Watch patterns: https://openkakao.vercel.app/docs/automation/watch-patterns/
- Protocol docs: https://openkakao.vercel.app/docs/protocol/overview/
- Android 26.7.1 LOCO static analysis: [docs/research/android-apk-26.7.1.md](docs/research/android-apk-26.7.1.md)

Reverse engineering / local app-state diff:

```bash
openkakao-cli profile-hints --local-graph --json
openkakao-cli profile-hints --app-state --json > /tmp/profile-before.json
openkakao-cli profile-hints --app-state --app-state-diff /tmp/profile-before.json --json
```

## Claude Code Skill

```bash
openkakao-cli init --skill claude   # or codex / all
# manual install: npx skills add JungHoonGhae/skills@openkakao-cli
```

`init` uses `npx skills add` when `npx` is available and otherwise downloads `SKILL.md` into `~/.claude/skills/openkakao-cli/` (or `~/.codex/skills/openkakao-cli/`). An installed skill is left as is and its location is shown.

## Development

```bash
cd openkakao-cli
cargo build --release
```

Detailed usage, operational notes, and protocol details live in the docs site.

## Support

If this tool helps you, consider supporting its maintenance:

<a href="https://www.buymeacoffee.com/lucas.ghae">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="50">
</a>

## Contributing

Bug reports and PRs are welcome.

## Acknowledgments

- [kakaocli](https://github.com/silver-flight-group/kakaocli) (MIT) — `local-send`'s macOS Accessibility API automation (selecting chat rows, locating/driving the message input field) was ported to Rust from this project (`src/ax_send.rs`).
- [Peekaboo](https://github.com/steipete/Peekaboo) (MIT) — `local-send` posts events directly to the target process via `CGEventPostToPid`, an approach borrowed from Peekaboo, to avoid the foreground-activation timing race that kakaocli's `send` hits ([silver-flight-group/kakaocli#9](https://github.com/silver-flight-group/kakaocli/issues/9)).

### Contributors

- [@twoimo](https://github.com/twoimo) — `local-send` fast path: reuses an already-open chat window to avoid a full chat-list AX snapshot that could take tens of seconds ([#35](https://github.com/JungHoonGhae/openkakao-cli/pull/35) → [#36](https://github.com/JungHoonGhae/openkakao-cli/pull/36)).
- [@e-jung](https://github.com/e-jung) — KakaoTalk 26.7 saved-user-ID recovery with active-account-marker verification and regression coverage ([#51](https://github.com/JungHoonGhae/openkakao-cli/pull/51)).

## License

MIT
