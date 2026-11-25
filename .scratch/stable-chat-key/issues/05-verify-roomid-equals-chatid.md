# 05: Verify `room_id == chat_id` on device

**What to build:** On the signed-in Mac with a saved REST token, confirm whether a
notification's `room_id` equals the server `chat_id` for the same rooms, and
record the verdict. This is the gate that lets REST data be merged into the
stable-key space.

**Blocked by:** 01. (Runtime prerequisites: a saved REST token and a signed-in
Mac — a human-in-loop task, not agent-only.)

**Status:** ready-for-agent

- [ ] Compare notification `room_id`s against REST `chats` `chat_id`s for the same rooms on a real install.
- [ ] Record the verdict (equal / not). If equal, upgrade the claim from "stable chat key" to verified `chat_id` in the GLOSSARY and user-facing text.
- [ ] If not equal, keep the two key spaces separate and note the finding; tickets 07 stay gated.
