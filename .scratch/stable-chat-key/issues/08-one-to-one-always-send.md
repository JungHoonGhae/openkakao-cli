# 08: 1:1 always-send via `user_id` URL

**What to build:** Target a 1:1 chat by opening it *exactly* via the peer's
`user_id` URL, then send into the window that just opened — always-send certainty
for direct chats, on top of the never-wrong-send floor.

**Blocked by:** 02, 06.

**Status:** ready-for-agent

- [ ] A 1:1 target with a known `user_id` opens the exact room via `kakaotalk://friend?action=chat&user_id=<id>` and sends into the opened window.
- [ ] The send binds to the just-opened window (not by namesake), preserving never-wrong-send.
- [ ] `user_id` is sourced from REST `friends` (token) or supplied by the user; the path stays gated on the focus-free verification (06).
