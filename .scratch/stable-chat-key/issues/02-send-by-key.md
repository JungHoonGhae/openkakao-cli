# 02: Send by stable chat key

**What to build:** `local-send --chat-id <key> "msg"` sends to the room named by
the stable chat key: resolve the key to the room's *current* display name,
require **exactly one** matching chat window/row, otherwise refuse — never a
namesake. This is the never-wrong-send core.

**Blocked by:** 01.

**Status:** done (2026-11-16)

- [ ] `--chat-id <key>` resolves via the directory to the current display name, then sends only when exactly one window/row matches that name.
- [ ] Zero match → refuse `chat_not_found`; several → `chat_ambiguous`; key absent from the directory → a distinct "unknown key" code. Never sends to a namesake.
- [ ] `--dry-run` shows key, resolved name, and would-send/refuse; `--json` returns the typed result via the existing `code`/`fix`/`disposition` shape.
- [ ] Existing name-based authorization / device-owner-auth applies to the resolved target (allowlist-by-key is ticket 04).
- [ ] The decision is a pure function `(key, directory snapshot, candidate names) → target | refuse(reason)`, unit-tested like `resolve_chat_id` with no live AX.

**Amended (maintainer-approved, ADR 0002):** a group chat's notification title is
its sender, so a key resolves only to the name a person bound with
`local-keys --bind <key> "<name>"` (binding an allowlisted name needs
device-owner auth). An unbound key is `chat_key_unbound`. The decision counts
KakaoTalk's chat-search rows as well as windows: no row → `chat_not_found`, one
row without a window → `chat_window_not_open` (same as a send by name; kept
instead of `chat_not_found`), several rows/windows or a name bound to two keys →
`chat_ambiguous`.
