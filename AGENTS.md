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
adapter verifies the KakaoTalk bundle/team code signature, requires one unique
exact-name chat-list row, an empty composer, and verified row/text read-back
immediately before Return. After Return may have been pressed, any result
lacking both a cleared composer and an additional exact-text outgoing bubble is
`uncertain`.

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
openkakao-cli doctor --json        # Check installation, credentials, local DB access
openkakao-cli auth-status --json   # Check auth recovery state
```
