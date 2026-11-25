# 01: Stable chat key directory + discovery

**What to build:** As notifications arrive, `notif-watch` records each room's
`room_id` as a **stable chat key** together with the room's current display name,
type, and last-seen, into a local login-free store. A read surfaces the known
keys so a user can find the key to target a room.

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

- [ ] `notif-watch` upserts a directory row per decoded notification: stable key (`room_id`), display name (notification title), type, last-seen; an empty title never overwrites an existing name.
- [ ] A read command/flag lists known keys with current name, type (`direct`/`group`/`unknown`), and last-seen.
- [ ] Store is local SQLite, login-free, no server contact and no DB decryption — a sibling of the existing inbox/outbox stores.
- [ ] Unit tests in the existing store-test style: synthetic notifications → directory lookups; name refresh on re-notify; empty-name no-clobber.
