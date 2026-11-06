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
calling the AX adapter. An exact `allowed_send_chats` entry is the human's standing
approval; any other target needs macOS device-owner authentication before
the commit, for `safe-send approve` and direct `local-send*` alike.
Successful verification becomes `sent`. Adapter errors, process interruption,
or an indeterminate completion after Return may have been pressed become
`uncertain` and are never retried automatically. A failure proven to happen
before that commit point returns to `proposed` for another explicit review.
Native AX verification authenticates KakaoTalk's bundle/team code signature,
requires exactly one open chat window titled exactly like the target (or one
unique exact-name chat-list row when `--open-if-needed` opens it), refuses
existing drafts, reads the target window and composer value back before commit,
and requires both a cleared composer and an additional exact-text outgoing
message bubble afterward. A five-minute
sending lease distinguishes a live concurrent execution from a crashed process,
while conservative per-chat and global budgets limit damage even after explicit
approval.

The production send path calls macOS Accessibility/CoreGraphics directly. Orca
and agent `computer-use` skills may be used as read-only development diagnostics,
but are not runtime dependencies and are never part of approval or delivery.

This improves delivery safety but cannot create an authoritative target
identity: current KakaoTalk builds do not expose a usable chat id to the AX
sender, so exact display-name allowlisting and ambiguity refusal remain required.

## Focus-free AX automation (landed) and what is left

Since the 2026-11 focus-free work, the default AX paths never raise KakaoTalk
or post input events. Text is sent with `AXSelectedText` + `AXPress 전송`, files
through the open panel by Accessibility alone, and closed chats are opened from
the chat-list context menu (`채팅방 열기`). All of this was measured on
KakaoTalk 26.9.0, macOS 26. Raise-and-key routes remain behind
`--fallback-keys`, gated by an idle wait and a frontmost-app restore. Open
follow-ups, all to be verified on a signed-in Mac before relying on them:

- **Staged-file deletion timing.** A file outside Desktop/Documents/Downloads
  is hard-linked (or copied) into `~/Downloads` and removed once the bubble is
  verified. If KakaoTalk re-reads a large file by path after the bubble
  appears, deleting early could break that upload. Measure a large upload
  first; if needed, delete only once the bubble shows its final size.
- **Open-panel view modes.** The file row is found in the list view (`목록
  보기`). Icon or column view yields no list outline and the send refuses;
  switching the view by AX, or using the `검색` field, is untested.
- **Chat-list search (landed for `--open-if-needed`).** 검색 shows the field,
  `AXSelectedText` filters (`AXValue` does not), and pressing 검색 again closes
  the search, clears its term and restores the list in about 0.3 s (measured
  on 26.8). Open: `ax-watch` and `doctor` still scan the full list, and a term
  matching many member names (the account's own name matched 140 rows) still
  costs about 10 ms per row.
- **Opening a 1:1 chat by URL.** `open -g "kakaotalk://friend?action=chat&user_id=<id>"`
  (string found in the 26.8 binary) might open a closed 1:1 chat without
  activation. `user_id` is not in the AX tree; it may come from `notif-watch`
  payloads. No group-chat equivalent was found. Optional.
- **Share extension** (`MacTalk Share.appex`, files ≤ 30) runs its own chat
  picker in a separate process that takes focus, so it is not focus-free. It
  is an alternative only if the open-panel route proves worse.
- `--fallback-keys` overwrites the clipboard. Restoring the previous contents
  needs every pasteboard type saved across processes; it is not done yet.
- Multi-line text: `AXSelectedText` is expected to keep newlines. A read-back
  mismatch fails before the commit and removes the inserted text.
- Typing `@` in a group chat opens a member-suggestion `AXTable` inside the
  chat window. Lookups take the message table before any insertion, but
  `ax-read` on a window with a pending `@` draft could pick the wrong table.

### Follow-ups from the 2026-11-06 device run

- Verify that a context menu opened with `AXShowMenu` belongs to the target
  element (its menu-layer window should touch the element's frame); today a
  pre-existing menu is refused and a cursor over the menu aborts the press.
- Text-send and reply verification now anchor on the baseline's newest row
  (same AX element, same content), as the attachment path does. Confirm on a
  device that a repeated short message pushed out of the 12-row tail verifies
  instead of reading as `delivery_uncertain`.
- A closed 1:1 chat with no row in the chat list (friends tab, or
  `kakaotalk://friend?action=chat`) and mentions remain unimplemented.
- A same-name file removed and recreated in ~/Downloads while KakaoTalk's picker
  still knew the old entry never selected again (seen once after a manual
  cleanup, not reproduced); the CLI's own staging never does this, it refuses
  with `staging_conflict` instead.
- KakaoTalk's picker greys out some file types (`.bin` measured, `.zip` of the
  same bytes selects); `file_type_refused` reports it only after the 30 s
  selection deadline because AX does not expose the row's enabled state.
- KakaoTalk 26.9 draws a file bubble as a share button, the file name,
  validity and size texts and open buttons (no text area or image); `ax-read`
  shows it as `[파일]` with the clock label as the time.

## Message actions on an open chat window (reply/edit experimental, delete designed)

The measured building blocks: each bubble row is `AXRow > AXCell (AXShowMenu) >
[AXStaticText time, AXImage, AXTextArea text (AXShowMenu)]`. `AXShowMenu` on
the bubble's `AXTextArea` opens KakaoTalk's context menu without activating
the app. Bubble menus are **not** among the application's AX children. They
are on screen as a KakaoTalk-owned window at `kCGWindowLayer` 101, so they are
found the way `show_context_menu` now does it: poll
`CGWindowListCopyWindowInfo(OnScreenOnly)` for a window owned by KakaoTalk's
pid (never its localized owner name) at layer ≥ 100, hit-test
`(midX, minY + 12)` system-wide, and climb `AXParent` to the `AXMenu`. Its
items (오려두기, 복사하기, 붙여넣기, 공유…, 대화 캡처, 나에게서만 삭제, 태그, 수정,
답장, 공유, ChatGPT, 공지; group chats add 모든 대화상대에게서 삭제 within the
deletion window) are pressed with `AXPress`, and `AXCancel` closes the menu (its
layer-101 window disappears). After every menu operation no menu-layer window
may remain; `doctor` warns when one is open.

Shipped as experimental (verified end to end on the memo chat, user's app
frontmost throughout), with the exact allowlist, `--dry-run` showing the
targeted bubble, whole-text matching that refuses duplicates, and verification
by re-reading rows:

- `local-reply <chat> <message> --to-last|--match "<text>"`: press 답장, then
  **confirm reply mode before inserting any text**. The composer description
  must read `답장 메시지 입력` and a `…에게 답장` banner must appear outside
  the message table. A run where the menu step silently failed once sent a
  plain message instead. Then the `AXSelectedText` + `전송` commit follows,
  verified by an additional exact-text outgoing bubble. The quote itself is
  not exposed in the AX row.
- `local-edit <chat> --match "<old>" "<new>"`: press 수정, then require the
  composer `수정 메시지 입력` preloaded with exactly `<old>` and a `메시지 수정`
  banner. Clear `AXValue` and insert with `AXSelectedText`: setting
  `AXSelectedTextRange` to the full range did not select all and appended
  instead. Press the `수정` button, and require a `수정됨` bubble reading
  `<new>` while one fewer reads `<old>`.
- Leaving a reply/edit mode after a pre-commit failure is not measured: the
  composer is cleared and a remaining banner is reported (`action_unverified`).
  Finding the banner's close control would allow a clean `--abort`.

Still designed only:

- `local-delete <chat> --match "<text>" [--for-everyone]`: press the matching
  삭제 item, confirm only a dialog whose buttons are recognized, and verify the
  bubble is gone or marked deleted. This is irreversible, so own outgoing
  bubbles only and device-owner authentication first. The confirmation dialog
  is not measured yet.
- Mentions cannot be completed focus-free. After `@Pe` is inserted, the
  suggestion `AXTable` lists candidates (`Pearl 진주`, `Pearl_진주`). But
  `AXSelected`, `AXPress` or `AXConfirm` on the row, cell or table, or typing
  the full name, do not turn the text into a mention token. A `--mention`
  option would therefore need the idle-gated raise + Return fallback; until
  then mentions stay manual.

## MCP server mode (designed, not implemented)

`openkakao-cli mcp` would serve the Model Context Protocol over stdio so agents
call typed tools instead of shelling out:

- Tools: `list_open_chats` (open chat-window titles), `read_chat` (`ax-read`,
  bounded `count`), `doctor` (the read-only AX checks), and `propose_send` /
  `propose_file` (the `safe-send` outbox; these never send).
- Sending stays human-gated. The server exposes no tool that commits a send by
  itself. A proposal returns its intent id and 12-character approval code, and
  a human approves in a terminal (`safe-send approve`) with device-owner
  authentication. An optional `confirm_send` tool could do the same through MCP
  elicitation, but only with the approval code typed by the human and the OS
  authentication prompt.
- The same config gates apply (`allow_ax_send`, exact `allowed_send_chats`,
  `[ax]` focus policy), and tool errors carry the CLI's stable `code` and `fix`.
- No new dependency is needed: JSON-RPC over stdio with `serde_json`, one
  request at a time, and the AX calls run on the main thread.

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
