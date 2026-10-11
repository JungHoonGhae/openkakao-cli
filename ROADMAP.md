# Roadmap

## Receive detection: notification-stream watching (`notif-watch`)

`ax-watch` polls the KakaoTalk chat list via the macOS Accessibility API and
infers "a new message arrived" from list-level signals (unread count, preview).
That is a lossy proxy: it needs the main window open/visible on the active
Space, only sees loaded chat rows, and can't cleanly tell your own sends from
incoming messages.

The landed answer is **`notif-watch`**: poll the macOS Notification Center DB
(`~/Library/Group Containers/group.com.apple.usernoted/db2/db`, plaintext
SQLite, read-only) for KakaoTalk notifications. Each payload carries the message
body, the room/sender name, a `<room_id>_<msg_id>` identity, a timestamp, and
(for images) the local attachment path.

- Works for chats regardless of window state (closed, minimized, on another
  Space) — the window-visibility requirement disappears.
- Distinguishes self vs. incoming perfectly (a message you send posts no
  notification).
- Read-only and local: no server contact, no ban risk, no focus stealing, no
  login, no DB decryption.
- By default, retained notifications establish a no-flood startup baseline;
  `--replay-existing` explicitly processes messages still retained from watcher
  downtime, with `(room_id, msg_id)` deduplication.
- `--durable` is the long-running mode: it records each event in a private
  openkakao-owned SQLite inbox before delivery, atomically leases work across
  concurrent processes, automatically reconciles retained notifications after
  restart, and retries failed or rate-limited sinks with capped exponential
  backoff and at-least-once semantics. Eight failed attempts quarantine one
  event so it cannot block later work; terminal tombstones are pruned after a
  24-hour grace period once Notification Center no longer retains the source.
- A per-user `launchd` example (`examples/launchd/com.openkakao.notif-watch.plist`)
  supervises the durable receiver with `RunAtLoad`, `KeepAlive`, and restart
  throttling.

Structural limits (documented): muted / notifications-off chats and the chat
you're currently focused on post no notification, so they aren't seen; it's a
forward-only live stream, not history. `ax-watch` complements it for those
chats when the window is open.

## Safe outbound: proposal-first AX sending (`safe-send`)

Agent-originated replies use a durable `SafeSendOutbox` instead of invoking AX
directly. `propose` is local-only and idempotent when given a source-event key;
`approve` is interactive-only and claims the immutable target/message before
calling the AX adapter. The proposal code is followed by macOS device-owner
authentication, and direct real `local-send` uses the same OS boundary;
`local-send-photo` shares it.
Successful verification becomes `sent`. Adapter errors, process interruption,
or an indeterminate completion after Return may have been pressed become
`uncertain` and are never retried automatically. A failure proven to happen
before that commit point returns to `proposed` for another explicit review.
Native AX verification authenticates KakaoTalk's bundle/team code signature,
requires one unique exact-name row, refuses existing drafts, reads the selected
row and composer value back before commit, and requires both a cleared composer
and an additional exact-text outgoing message bubble afterward. A five-minute
sending lease distinguishes a live concurrent execution from a crashed process,
while conservative per-chat and global budgets limit damage even after explicit
approval.

The production send path calls macOS Accessibility/CoreGraphics directly. Orca
and agent `computer-use` skills may be used as read-only development diagnostics,
but are not runtime dependencies and are never part of approval or delivery.

This improves delivery safety but cannot create an authoritative target
identity: current KakaoTalk builds do not expose a usable chat id to the AX
sender, so exact display-name allowlisting and ambiguity refusal remain required.

## Sealed: local DB tailing (SQLCipher) — NO-GO on current builds

The originally-planned north star — tailing the local SQLCipher message DB by
`log_id` — is **NO-GO** on KakaoTalk macOS 26.5.0+ and is not being pursued.
Findings (see `docs/research/db-key-derivation.md`, issues #40 / #43): the DB
name and key are no longer a derivable PBKDF2 recipe but values stored
AES-encrypted via NTSetting; a dynamic hook is blocked because re-signing the
app to attach a debugger loses its Keychain access and it logs itself out, so it
never opens the message DB offline. Recovering the key would require either
disabling SIP to debug the untouched app, or reversing the NTSetting AES store —
neither pursued. If a future build reopens this, it returns as a fresh effort.

## If notification coverage isn't enough: message-bubble diffing (AX, Tier 2)

The correct AX-only way to also catch messages in a chat you keep open (where
the unread count stays 0) is to track the actual message **bubbles** in each
open window and emit on genuinely new bubbles — distinguishing your own sends
by bubble alignment/author. This is deliberately *not* the preview-text-change
heuristic (which fires on your own sends and any room activity). It only works
for open windows, so it complements list-level unread for closed chats. Build
this only when real usage hits the open-room gap — until then, YAGNI.
