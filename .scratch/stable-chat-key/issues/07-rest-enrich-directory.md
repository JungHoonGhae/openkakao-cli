# 07: Enrich the directory from REST

**What to build:** When a saved REST token exists and `room_id == chat_id` has
been proven, fill the stable chat key directory from REST `chats`
(`chat_id` ↔ title), so rooms that have never fired a notification are still
targetable by key.

**Blocked by:** 05.

**Status:** ready-for-agent

- [ ] With a token and the proof in place, the directory is enriched from REST `chats`; rows are marked `source = rest`.
- [ ] Coverage extends to rooms that have not notified since watching.
- [ ] Enrichment is inactive before the proof; the two key spaces are never mixed otherwise.
