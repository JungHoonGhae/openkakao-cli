# 06: Verify `user_id` URL opens focus-free

**What to build:** On the signed-in Mac, confirm that
`open -g "kakaotalk://friend?action=chat&user_id=<id>"` opens the 1:1 chat
without stealing focus. This gates the 1:1 always-send path.

**Blocked by:** None (needs a signed-in Mac — human-in-loop, not agent-only).

**Status:** ready-for-agent

- [ ] Confirm the URL opens the correct 1:1 chat window and does not raise or steal focus (consistent with the focus-free model).
- [ ] Record the behavior and any focus/activation side-effects.
