# KakaoTalk 26.7 receive-envelope static analysis

Analyzed on **2026-09-07**, using the retained Android 26.7.1 APK and the
installed Mac 26.7.0 app. This pass traces incoming message decoding beyond
the earlier [request-field analysis](android-apk-26.7.1.md).

## Provenance and method

The base APK SHA-256 was recomputed and matches the previously verified artifact:
`703e864c6da6fc3ba629d6197d327a18ed734865f0abe6b3d3f949246d0c88ec`.
The original signer/split verification is recorded in the earlier note; it was
not repeated here. The retained JADX reconstruction is cross-checked against
APKTool's smali for the specific field accesses and parser call sites below.

The installed universal Mac executable still has SHA-256
`f29adb10fb010fab3734df75e7624dbd6c99c4f032e4b1a9fcebec2d77431c09`.
`codesign --verify --deep --strict /Applications/KakaoTalk.app` succeeded;
`codesign -dv` reports bundle `com.kakao.KakaoTalkMac`, team `L75WVXX68A`.
`Info.plist` reports 26.7.0. `otool -arch arm64 -ov` was used to inspect the
Objective-C class, property and ivar metadata without loading the executable.

No Kakao server request, login, runtime hook, message send, or read receipt was
performed. Tests use invented BSON records, fake credentials, an unconnected
client, and temporary CLI-owned SQLite databases. Proprietary binaries,
decompiled source, and local disassembly remain outside version control.

## Evidence

The following Android filenames refer to the local JADX
`decompiled/base/sources/com/quram/mi/type/` directory. Kotlin source names
survive in metadata even though class names are obfuscated.

| Source | Observed behavior |
| --- | --- |
| `zf70.java`, `MsgJob.kt`, method `a` | Reads envelope `chatId` and `authorNickname`, calls `tg8.b(body, "chatLog")`, and rejects a missing chat log. |
| `tg8.java`, `ChatLog.kt` | Reads the nested log's `chatId`, `logId`, `prevId`, `type`, `authorId`, `message`, `sendAt`, and string `attachment`. Identity fields use 64-bit integers; `type` and `sendAt` use 32-bit integers. |
| `i9b1.java`, `SyncMsgJob.kt`, method `r` | Reads `isOK` and parses `chatLogs` as a collection, plus optional synchronization fields. |
| `nk30.java`, `LocoMethod.kt` | Marks `MSG` as push and `SYNCMSG` as a request/response method. A `SYNCMSG` batch is not evidence of a separate single-message server push. |

Smali cross-checks are in
`decoded/base/smali_classes9/com/quram/mi/ocr/`: `zf70.smali` contains
`chatLog` at line 452 and the parser invocation at line 456;
`tg8.smali` contains `message` at line 1284, `sendAt` at 1424, and
`attachment` at 1584; `i9b1.smali` contains `isOK` at 546 and `chatLogs`
at 667. These are locations in the retained APKTool reconstruction, not stable
offsets across builds or decompiler versions.

The signed Mac app independently exposes:

- `LocoMsgPushNotice`: `chatId`, `authorNickname`, and a `chatLog` property
  typed as `LocoChatLog`.
- `LocoChatLog`: message/attachment properties, 64-bit chat/log/author IDs,
  and a 32-bit `sentAt` ivar.
- `LocoSyncMsgResponse`: `isOK`, `chatLogs` typed as `LocoChatLogs`,
  `minLogId`, and `jsi`.

Mac property metadata corroborates the nested model; it does not by itself
prove every BSON key or every conditional decoding path. The Android decoder
provides the concrete wire-key evidence. Existing Mac-compatible flat decoding
is therefore retained rather than assuming a single universal format.
The arm64 `LocoMsgPushNotice` initializer at `0x1013359bc` also delegates to
the superclass `initWithJSONObject:` before applying its `pushAlert` default;
it does not expose an independent complete list of message decoding keys.

## Implemented improvements

Before this change, `watch` read `MSG.logId`, `MSG.type`, and `MSG.msg`
directly from the envelope. A valid nested `chatLog` consequently produced a
type-0 placeholder, log ID 0, and no cache entry. `SYNCMSG.chatLogs` similarly
became one placeholder instead of individual messages. The history reader
already recognized `message`; the watcher renderer only recognized `msg`.

`src/loco/message.rs` now decodes incoming envelopes before watch effects:

- `MSG.chatLog` supplies log identity, content, author, attachment, and time;
  the envelope can supply chat identity and author nickname.
- `SYNCMSG.chatLogs` yields one record per entry. Empty batches yield no
  events. Single nested and flat records remain accepted for compatibility;
  this does not assert that current servers emit all these forms.
- The renderer prefers `message`, falling back to legacy `msg`. An explicitly
  empty `message` stays empty. Unknown types can still display their text.
- Invalid nested values, missing/nonpositive IDs, out-of-range message types,
  conflicting chat identities, and error responses are rejected before events
  or cache/cursor changes. An explicit malformed `chatLog` never falls back to
  unrelated outer fields. A malformed batch rejects the whole batch before any
  entry is delivered; the watcher reports the issue to stderr and continues.
- Each accepted sync record uses its own chat filter, author, and attachment.
  Sync JSON now also includes `author_id` and `attachment`.
- MSG/SYNCMSG resume watermarks take the maximum accepted log ID for each chat,
  so out-of-order or older records cannot move the watermark backward. This is
  cursor protection, not event deduplication or gap recovery.

Normal MSG hooks and optional effects use the decoded record. SYNCMSG retains
its existing output/cache-only behavior. No authentication, request identity,
write permission, or approval mechanism changes are needed for this repair.

## Verification and limits

`tests/loco_message_test.rs` exercises synthetic wire-encoded BSON envelopes,
large IDs above JavaScript's exact integer range, both integer widths, legacy
flat input, batch and empty input, nested media, malformed records, and error
responses. Watcher tests exercise the actual MSG/SYNCMSG handlers against a
temporary cache, including filtering, cursor advance/non-regression, and
rejecting a malformed batch before partial writes. Renderer tests cover empty
and unknown-type text precedence.

Validation on the local Mac: `cargo test` passed 405 tests (13 newly added),
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passed,
and `cargo build --release` plus the release binary's `--version` smoke test
succeeded. The final legacy-text assertion was also rerun in its renderer test.

This establishes offline decoding and handler behavior. It does not establish
live LOCO authentication, server-side replay completeness, or that a particular
Mac account will receive every observed Android envelope. The existing
notification/AX receive paths and the local database research limits in
`ROADMAP.md` remain separate from this LOCO improvement.
