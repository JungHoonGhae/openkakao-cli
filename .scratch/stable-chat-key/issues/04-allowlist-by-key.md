# 04: Allowlist keyed by stable chat key

**What to build:** `allowed_send_chats` accepts stable-chat-key entries, and a
key-targeted write is authorized by key — so renaming a room or a namesake can no
longer silently break authorization. Existing name entries keep working during
migration.

**Blocked by:** 02.

**Status:** done (2026-11-16)

- [ ] `allowed_send_chats` accepts stable chat key entries; a key-targeted send to an allowlisted key needs no device-owner-auth.
- [ ] A key-targeted send to a non-allowlisted key follows the existing device-owner-auth path, unchanged.
- [ ] Existing name-based allowlist entries remain honored (migration window) and are shown for humans.
- [ ] Renaming an allowlisted room does not drop it from the allowlist when the room is targeted by key.
- [ ] Unit tests for the key-based authorization check: allowed / not-allowed by key; legacy name entry still honored.

**Note from 02:** keys resolve only to a bound name (ADR 0002), and binding an allowlisted *name* already needs device-owner auth. Once keys are allowlisted, (re)binding an allowlisted *key* must need it too, or a rebind silently redirects prompt-free sends.
