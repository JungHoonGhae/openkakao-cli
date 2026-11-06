# AI Agent Integration Guide

openkakao-cli is designed for AI agent integration. All commands support `--json` for structured output.

## Safety Model

LOCO write operations (send, delete, edit, react) are **disabled by default** to prevent account bans.

### Safe commands (always available, no server contact)

```bash
# Read chats from local KakaoTalk database (SQLCipher, no network)
openkakao-cli local-chats --json
openkakao-cli local-read <chat_id> -n 30 --json
openkakao-cli local-search "keyword" --json
openkakao-cli local-schema

# Preview actions without executing
openkakao-cli send 123 "message" --dry-run --json
openkakao-cli delete 123 456 --dry-run --json
openkakao-cli local-send-file "chat name" ./report.docx --dry-run --json

# Read-only AX readiness, optionally for one open chat window
openkakao-cli doctor --json --chat "chat name"

# Queue an AX send proposal. This never sends a KakaoTalk message.
openkakao-cli --no-prefix safe-send propose "chat name" "message" \
  --reply-chat-id 123 --reply-log-id 456 \
  --idempotency-key "reply:123:456:policy-v1" --json
openkakao-cli safe-send list --json
```

### Safe commands (REST API, lower risk)

```bash
openkakao-cli chats --json
openkakao-cli read <chat_id> --rest --json
openkakao-cli friends --json
openkakao-cli me --json
openkakao-cli doctor --json
```

### Risky commands (require opt-in)

These require `allow_loco_write = true` in `~/.config/openkakao/config.toml`:

```bash
openkakao-cli send <chat_id> "message" -y --json
openkakao-cli send --me "test" -y --json    # Send to memo chat
openkakao-cli delete <chat_id> <log_id> -y --json
openkakao-cli edit <chat_id> <log_id> "new" -y --json
openkakao-cli react <chat_id> <log_id> --json
```

`safe-send approve <intent_id>` is the preferred AX write path. It requires
`allow_ax_send = true`, an exact `allowed_send_chats` match, an interactive
terminal, the proposal's 12-character approval code, and macOS device-owner
authentication (Touch ID or login password). Direct real `local-send` and
`local-send-photo` use the same OS authentication. Agents stop after
`safe-send propose`; a human reviews and approves. There is no unattended
approval mode. An `uncertain` result is inspected in KakaoTalk and
never retried automatically.

The AX implementation is native to `openkakao-cli`; Orca and `$computer-use`
are not runtime dependencies. Do not replace the domain-level workflow with
generic desktop `click`, `type`, or `press-key` actions. Before commit, the AX
adapter verifies the KakaoTalk bundle/team code signature, requires exactly one
open chat window titled exactly like the target (or, with `--open-if-needed`,
one unique exact-name chat-list row), an empty composer, and verified
window/text read-back immediately before the commit. After the commit may have
happened, any result lacking both a cleared composer and an additional
exact-text outgoing bubble is `uncertain` (`delivery_uncertain`).

## Focus Model

Default AX paths never raise KakaoTalk or post input events (verified on
KakaoTalk 26.9.0, macOS 26), so an agent does not interfere with the user's
foreground app:

- Text: insert with the composer's `AXSelectedText` (setting `AXValue` does not
  enable `전송`), then `AXPress` the `전송` button.
- Files and photos: `AXPress` the `파일전송 ⌘O` button, select the open panel's
  sidebar folder and file rows via `AXSelected`, `AXPress` `열기`, then the
  preview's `1개 전송` / `Send 1 files`.
- Opening a closed chat (`--open-if-needed`): `AXShowMenu` on the chat-list row
  and `AXPress` `채팅방 열기`. A hidden main window is shown with `AXPress` on
  the menu bar's `창 > 채팅`.

`--fallback-keys` keeps the older raise + Return / ⌘V / double-click routes.
Those wait until the user has been idle for `--wait-idle` seconds and restore
the previously frontmost app afterwards.

### Guardrails

- **Never post Escape to KakaoTalk.** It closes the focused chat window and the
  main window.
- Key events posted to KakaoTalk's pid do nothing while it is in the
  background. Any key-based step needs the app frontmost, so prefer AX actions.
- Never leave a menu or sheet open. A menu that was found is closed with
  `AXCancel`; a sheet is dismissed with its `취소`/`Cancel` button. If a menu
  cannot be found within a bounded wait, do nothing else and report
  `menu_not_found`.
- Never type into or clear a composer you did not verify empty first.

## Setup

```bash
openkakao-cli init --yes --allow-chat "exact chat name" --skill claude --json
```

`init` only allowlists the names it is given, and re-running it changes
nothing that is already set. A non-interactive run assigns KakaoTalk to All
Desktops only with `--all-spaces`, which restarts the Dock once.

## Unattended Mode

For fully non-interactive operation:

```bash
openkakao-cli --unattended --allow-non-interactive-send send <chat_id> "msg" -y --json
```

Or configure in `~/.config/openkakao/config.toml`:

```toml
[mode]
unattended = true

[send]
allow_non_interactive = true

[safety]
allow_loco_write = true
min_unattended_send_interval_secs = 10
```

## Recommended Agent Workflow

1. **Read** with local read commands or `notif-watch` (zero Kakao write risk)
2. **Propose** with `safe-send propose`; use the source `chat_id` + `log_id` in the idempotency key
3. **Stop** and return the intent, exact target, message, and approval code to the human
4. **Human approval** happens only through interactive `safe-send approve`, its approval code, and macOS device-owner authentication

## JSON Output

All commands with `--json` return structured JSON to stdout. Diagnostic messages go to stderr.

```bash
# List chats
openkakao-cli local-chats --json
# Returns: [{"chat_id": 123, "chat_type": 0, "chat_name": "...", ...}]

# Read messages
openkakao-cli local-read 123 --json
# Returns: [{"log_id": 456, "chat_id": 123, "sender_name": "...", "message": "...", ...}]

# Dry-run
openkakao-cli send 123 "hello" --dry-run --json
# Returns: {"dry_run": true, "action": "send", "chat_id": 123, "message": "..."}
```

## Diagnostics

```bash
openkakao-cli doctor --json        # Check installation, credentials, local DB access, AX readiness
openkakao-cli doctor --json --chat "chat name"   # Also inspect one open chat window (read-only)
openkakao-cli auth-status --json   # Check auth recovery state
```

## Errors

With `--json`, a failure prints one JSON object on stdout and exits 1:

```json
{"error": {"code": "chat_window_not_open", "message": "...", "fix": "..."}}
```

Branch on `code`, which stays stable; uncoded errors use `"unclassified"`.
Show `fix` to the human. Codes: `accessibility_not_granted`,
`kakaotalk_not_running`, `kakaotalk_signed_out`, `main_window_not_visible`,
`main_window_minimized`, `kakaotalk_not_on_all_spaces`, `chat_list_unavailable`,
`chat_window_not_open`, `chat_not_found`, `chat_list_filtered`,
`chat_ambiguous`, `send_button_missing`, `menu_not_found`, `staging_conflict`,
`user_active`, `ax_send_disabled`, `chat_not_allowed`, `invalid_file`,
`attachment_unverified`, `delivery_uncertain`. Never retry a
`delivery_uncertain` send automatically.
