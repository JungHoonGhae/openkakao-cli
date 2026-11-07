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
`allow_ax_send = true`, an interactive terminal and the proposal's
12-character approval code. An exact `allowed_send_chats` entry is the
human's standing approval: AX writes to that chat (`safe-send approve`,
`local-send*`, `local-reply`, `local-edit`) need no further authentication.
Any other target is flagged with `⚠ NOT in [safety] allowed_send_chats` and
needs macOS device-owner authentication (Touch ID or login password) right
before the commit; a declined prompt is `chat_not_allowed`. Every AX write
first prints which chat it is (members, recent senders, last activity), and
for a chat outside the allowlist its three newest messages. The global
`--dangerously-skip-allowlist` skips that prompt (not `--pick`'s) once the
human sets `[safety] allow_skip_allowlist = true`; agents use the flag only
when the user asked for it and never set that key. `local-react` is approved
like a send; `local-rename-chat` and `ax-friend --rename` always need
device-owner authentication (a new name could match an allowlisted one). The opt-in
`[safety] device_owner_auth_grace_secs` (default 0) reuses one approval for
that many seconds, sudo-style, and only the human sets it. Agents stop after
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
exact-text outgoing bubble is `uncertain` (`delivery_uncertain`). A photo or
file counts only once its upload finished (the bubble shows `공유`); a failed
upload (KakaoTalk's alert, red ✕ on the bubble) is `delivery_uncertain`. An
alert left over the chat window that only acknowledges (one 확인/OK) is
dismissed before a send and printed; any other dialog is `chat_dialog_open`.

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
- Reply/edit (experimental `local-reply`, `local-edit`): `AXShowMenu` on the
  bubble's `AXTextArea`. The menu is found in KakaoTalk's menu-layer window
  (CoreGraphics layer ≥ 100, matched by pid, then hit-tested). Press `답장` or
  `수정`, and confirm the mode by the composer description (`답장/수정 메시지
  입력`) and its banner before inserting text. Mentions cannot be completed by
  AX and stay manual.
- Delete (experimental `local-delete --for-everyone|--for-me`): `AXShowMenu`
  on the bubble, `AXPress` `모든 대화 상대에게서 삭제` or `나에게서만 삭제`
  (KakaoTalk shows only what it allows: an absent item is
  `delete_unavailable`), tick only the target row's `AXCheckBox` in the
  selection mode ("삭제할 메시지를 선택하세요." 취소/확인), `확인`, then `삭제`
  on a confirmation headed by that scope. Any earlier failure leaves the
  selection mode; success needs one fewer bubble with that text.

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
4. **Human approval** happens only through interactive `safe-send approve` and its approval code, plus macOS device-owner authentication for a chat outside `allowed_send_chats`

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
{"error": {"code": "chat_window_not_open", "message": "...", "fix": "...",
           "disposition": {"delivery": "not_delivered", "retry": "safe"}}}
```

`disposition` says whether repeating the command could send twice.
`delivery` is `not_delivered` for every code except `delivery_uncertain`
(all are raised before the commit point), `uncertain` for
`delivery_uncertain`, and `unknown` for uncoded errors. Retry or substitute
an action only when `retry` is `safe`; otherwise inspect KakaoTalk (or
`safe-send list`) and decide from that.

Branch on `code`, which stays stable; uncoded errors use `"unclassified"`.
Show `fix` to the human. Codes: `accessibility_not_granted`,
`kakaotalk_not_running`, `kakaotalk_signed_out`, `main_window_not_visible`,
`main_window_minimized`, `kakaotalk_not_on_all_spaces`, `chat_list_unavailable`,
`chat_window_not_open`, `chat_not_found`, `chat_list_filtered`,
`chat_ambiguous`, `send_button_missing`, `menu_not_found`, `staging_conflict`,
`composer_busy`, `chat_dialog_open`, `delete_unavailable`, `message_not_found`,
`message_ambiguous`, `action_unverified`,
`user_active`, `ax_send_disabled`, `chat_not_allowed`, `invalid_file`,
`attachment_unverified`, `file_type_refused`, `delivery_uncertain`,
`login_rejected`, `login_needs_human`, `login_secret_missing`. Never retry a
`delivery_uncertain` send automatically. On `kakaotalk_signed_out`, run
`openkakao-cli ax-login --json` once; never retry a `login_rejected`. On `chat_ambiguous` with
`--open-if-needed`, the message numbers the same-name chat-list rows (time,
preview, pin); show that list to the human and pass `--pick <N>` only with
their choice. A send to a picked row always needs device-owner
authentication, because the allowlist names every chat with that name.
