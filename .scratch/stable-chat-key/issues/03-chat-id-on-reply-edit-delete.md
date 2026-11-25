# 03: `--chat-id` on reply / edit / delete

**What to build:** The same stable-chat-key targeting on the other write
commands: `local-reply`, `local-edit`, `local-delete` all accept `--chat-id
<key>` and resolve it the same deterministic way.

**Blocked by:** 02.

**Status:** done (2026-11-16)

- [ ] `--chat-id` works on reply, edit, and delete with the same resolve → exactly-one-or-refuse rule as send.
- [ ] `local-delete` keeps its existing irreversibility safeguards (device-owner-auth, scope confirmation) unchanged.
- [ ] Per-command tests reuse the ticket-02 resolution seam; no live AX in tests.

**Note from 02:** reuse `commands::local_send::locate_by_key` (directory → windows → chat-search rows → `chat_directory::resolve_send_target`); only bound keys resolve.
