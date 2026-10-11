# OpenKakao-rs Improvement Plan

Based on analysis of [KakaoTalk is making me LOCO](https://jusung.dev/posts/kakao-talk-is-making-me-local/) by Jusung,
a full codebase audit, and review of public LOCO protocol implementations.

---

## 1. Blog Post Key Takeaways

| Finding | Detail | Impact on openkakao-cli |
|---------|--------|----------------------|
| **RSA key rotated** | `0xF3188...` (node-kakao era) -> `0xA3B076...` (current) | Our key matches current (`A3B076...`) - OK |
| **Handshake type 12 -> 16** | `key_type` field in handshake packet | We use 16 - OK. KiwiTalk/loco-protocol-rs uses **15** - differs! |
| **ticket.lsl changed** | Was string, now `string[]` (array) | Our code handles array - OK |
| **Port field moved** | `ticket.lslp` -> `wifi.ports[0]` | Our code reads `wifi.ports` - OK |
| **Status field moved** | Was in packet header, now in BSON body | Our code checks both - OK |
| **Mac secondary device auth** | Login without logging out phone; uses `/mac/account/login.json` with X-VC | We have `generate_xvc()` and `login_with_xvc()` - implemented |
| **-999 "Upgrade required"** | Version string must match recent KakaoTalk | We use version from Cache.db user-agent - OK if KakaoTalk is updated |
| **Ban risk** | Matrix bridge showed ban warnings; unclear trigger | No mitigation currently |

### Critical difference: `key_type` 15 vs 16

- **KiwiTalk/loco-protocol-rs** (`storycraft`): Uses `key_type: 15` in handshake
- **Our implementation**: Uses `key_type: 16` (from Mach-O binary analysis)
- **Blog post**: Mentions handshake changed from 12 to 16
- **Hypothesis**: `key_type` may be server-version-dependent; both 15 and 16 might work depending on server

---

## 2. Why LOCO Login Fails (-950)

The `-950` error occurs at LOGINLIST, *after* successful BOOKING and CHECKIN.

### Root Cause (Confirmed via Live Testing, 2026-03-07)

**The token from Mac Cache.db is REST-only. LOCO requires a different `access_token` from `login.json`.**

| What we tested | Result |
|---------------|--------|
| AES-128-GCM handshake | **Working** (key_size=256, key_type=16, encrypt_type=3, total=268 bytes) |
| REST API with Cache.db token | **Working** (more_settings.json returns status=0) |
| LOCO LOGINLIST with same token | **-950** (rejected) |
| Changed `os` to `"android"` | Still -950 (not an os field issue) |
| Quit KakaoTalk (session conflict test) | Still -950 (not a session conflict) |
| Removed `pcst` field (per loco-wrapper) | Still -950 |
| login.json with cached password | status=12 (password expired/changed) |
| renew_token.json with refresh_token | status=-998 (refresh token expired) |

**Conclusion**: The 138-char token from Cache.db HTTP response headers is a REST bearer token. LOCO LOGINLIST requires an `access_token` that comes from the `login.json` response body — but that response is **encrypted by KakaoTalk** before being stored in Cache.db, and we cannot decrypt it. The cached `login.json` request body contains an expired password, so re-calling login.json also fails.

### Key Finding: No Working Implementation Uses `os: "mac"`

All known working LOCO implementations (loco-wrapper, node-kakao, KiwiTalk) use `os: "android"` or `os: "win32"` and obtain their token from `android/login.json` or equivalent — NOT from macOS Cache.db.

### AES-GCM Migration (DONE)

[NetRiceCake/loco-wrapper](https://github.com/NetRiceCake/loco-wrapper) (Java, last commit 2025-12-10, **confirmed working** with KakaoTalk 25.9.2):

| Field | Old (KiwiTalk/node-kakao) | New (loco-wrapper) | **Ours** |
|-------|--------------------------|---------------------|----------|
| `key_type` | 15 | **16** | 16 (OK) |
| `encrypt_type` | 2 (AES-128-CFB) | **3 (AES-128-GCM)** | ~~2~~ **3 (FIXED)** |
| AES mode | CFB-128, 16-byte IV | **GCM, 12-byte nonce** | **GCM (FIXED)** |
| Secure frame | `[size(4)][iv(16)][ciphertext]` | `[size(4)][nonce(12)+ciphertext+tag]` | **New format (FIXED)** |

### Remaining Options to Obtain LOCO Token

1. **mitmproxy**: Intercept KakaoTalk's live `login.json` response to extract the access_token. Requires: `brew install mitmproxy`, HTTPS cert trust, proxy config.
2. **Android emulator**: Run KakaoTalk Android, call `android/login.json` with email+password to get token directly (like loco-wrapper does).
3. **Frida/lldb**: Hook KakaoTalk process to capture decrypted login.json response. Blocked by code signing currently.
4. **Manual password update**: User changes KakaoTalk password, then login.json works with new password. Simplest but requires user action.

---

## 3. Reference Implementations

| Project | Language | Status | Key Techniques | Link |
|---------|----------|--------|---------------|------|
| **loco-wrapper** | Java (Netty) | **Active (Dec 2025)** | **Working!** `key_type=16`, `encrypt_type=3` (AES-GCM), new X-VC seeds | [github.com/NetRiceCake/loco-wrapper](https://github.com/NetRiceCake/loco-wrapper) |
| **KiwiTalk** | Rust+TS (Tauri) | Archived (2023) | Full LOCO client, `key_type=15`, `prtVer="1.0"`, `rp` field, `pcst` | [github.com/KiwiTalk/KiwiTalk](https://github.com/KiwiTalk/KiwiTalk) |
| **loco-protocol-rs** | Rust | Archived (2023) | IO-free secure layer, clean handshake impl | [github.com/storycraft/loco-protocol-rs](https://github.com/storycraft/loco-protocol-rs) |
| **node-kakao** | TypeScript | Unmaintained (4yr) | Original LOCO RE work, old RSA key | [github.com/storycraft/node-kakao](https://github.com/storycraft/node-kakao) |
| **kakaotalk_analysis** | Python (mitmproxy) | Active (2024) | MITM scripts, CFB analysis, secret chat | [github.com/stulle123/kakaotalk_analysis](https://github.com/stulle123/kakaotalk_analysis) |
| **matrix-appservice-kakaotalk** | Python+JS | Semi-maintained | Matrix bridge, ban warnings | [src.miscworks.net/.../matrix-appservice-kakaotalk](https://src.miscworks.net/fair/matrix-appservice-kakaotalk.git) |
| **pykakao** | Python | Unmaintained | Simple LOCO/HTTP wrapper | [github.com/hallazzang/pykakao](https://github.com/hallazzang/pykakao) |

### Specific field differences (loco-wrapper vs ours)

The current Android 26.7.1 app was also checked directly on 2026-09-02. See the
[signed APK static-analysis note](research/android-apk-26.7.1.md). That pass
confirmed `prtVer` and `rp`. A signed Mac 26.7.0 binary cross-check confirmed
`GETCONF.userId`; `LOGINLIST.isSw` was not observed in the Mac initializer and
remains absent from the current Mac profile.

```
loco-wrapper LOGINLIST:                 Ours (current):
  os: "android"                           os: "mac"           <-- DIFFERS (but not root cause)
  prtVer: "1"                             prtVer: "1"         OK
  dtype: (not sent)                       dtype: 2            Extra but harmless
  pcst: (not sent)                        pcst: (not sent)    OK (removed)
  rp: [6 bytes]                           rp: [6 bytes]       OK (added)
  lbk: 0                                  lbk: 0              OK
  isSw: false                             (not sent)          Not observed in Mac initializer
  token: from login.json response         token: from Cache.db <-- ROOT CAUSE
```

---

## 4. Proposed Hardening Features

### 4.1 `doctor` Command (THIS PR)

A diagnostic command that checks environment health without making any changes:

```
openkakao-cli doctor
```

Output:
- KakaoTalk.app installed version (from Info.plist)
- KakaoTalk process running status
- Cache.db existence and freshness
- Token validity (REST API check)
- LOCO booking connectivity (GETCONF)
- LOCO checkin connectivity (CHECKIN)
- Credential file status
- Protocol constants (RSA key fingerprint, handshake type, etc.)

### 4.2 Protocol Version Management (FUTURE)

Make LOGINLIST fields configurable/updatable without recompiling:
- `prtVer` ("1" vs "1.0")
- `pcst` field
- `rp` bytes
- App version override

### 4.3 Safer Auth: Mac Secondary Device Flow (FUTURE)

- Detect if user is logged in on phone before attempting LOCO
- Warn about single-device logout risk
- Implement proper token renewal chain

### 4.4 Rate Limiting and Safety Warnings (FUTURE)

- Add configurable rate limits to LOCO commands
- Display ban risk warning on first use
- Track request frequency per session
- Implement exponential backoff on errors

### 4.5 Improved Error Reporting (THIS PR)

- Structured error codes with explanations
- Actionable hints for common failures (-950, TLS EOF, timeout)
- `--verbose` flag for detailed protocol tracing

### 4.6 Full Chat History Access (THIS PR)

#### How message reading works

| Method | Command | Direction | Max Range | Limitation |
|--------|---------|-----------|-----------|------------|
| **REST** (`read`) | `GET /messaging/chats/{id}/messages` | Backward from cursor | Recent only | Pilsner proxy only caches chats recently opened in KakaoTalk Mac app. Most chats return empty. |
| **LOCO** (`loco-read`) | `SYNCMSG {chatId, cur, cnt, max}` | Forward: `cur` -> `max` | **Full history** | Requires working LOCO connection (currently blocked by -950/AES-GCM). |

#### SYNCMSG pagination details

SYNCMSG scans **forward** from `cur` (exclusive) to `max` (inclusive), returning up to `cnt` messages per batch.
- `cur=0, max=lastLogId` → gets oldest messages first, paginating forward
- `isOK=false` in response → more messages available, continue with `cur=max_log_in_batch`
- `isOK=true` → reached the end, no more messages

#### What blocks full history

1. **LOCO -950 (AES-GCM migration)**: Primary blocker. Fix `encrypt_type` to unblock.
2. **Ban risk**: Aggressive SYNCMSG requests may trigger Kakao's abuse detection. Mitigated by:
   - `--delay-ms 100` (default) between batches
   - 50 messages per batch (conservative)
3. **Server-side limits**: Unknown. KakaoTalk app itself fetches full history on device migration, so the server does support it.
4. **REST API limits**: Pilsner only serves recently cached chats. Not suitable for full history.

#### CLI options implemented (THIS PR)

```
# REST (read) - limited to pilsner cache
openkakao-cli read <chat_id> --all              # Fetch all cached messages
openkakao-cli read <chat_id> --cursor <logId>   # Resume from logId
openkakao-cli read <chat_id> --since 2025-01-01 # Filter by date
openkakao-cli read <chat_id> -n 50              # Last N messages

# LOCO (loco-read) - full history when LOCO works
openkakao-cli loco-read <chat_id> --all                     # Full history
openkakao-cli loco-read <chat_id> --all --cursor <logId>    # Resume from logId
openkakao-cli loco-read <chat_id> --all --since 2024-06-01  # Filter by date
openkakao-cli loco-read <chat_id> --all --delay-ms 200      # Slower rate limit
openkakao-cli loco-read <chat_id> -n 100                    # Last N messages
```

#### Resumable fetch strategy

On disconnect or error during `--all`, the CLI prints:
```
[loco-read] Connection lost: ...
[loco-read] Resume with: openkakao-cli loco-read <chat_id> --all --cursor <last_logId>
```
This allows resuming from the last successful batch without re-fetching.

#### Max-reach strategy (when full history is blocked)

If LOCO remains broken, the fallback hierarchy is:
1. **REST `read --all`**: Gets whatever pilsner has cached (usually recent few hundred messages)
2. **Export from KakaoTalk app**: KakaoTalk Mac has a "내보내기" (export) feature that dumps chat as txt
3. **SQLCipher DB**: `~/Library/Application Support/com.kakao.KakaoTalkMac/chat_data/*.db` — encrypted with SQLCipher, key derivation unknown
4. **MITM proxy**: Use mitmproxy to intercept LOCO traffic from the real KakaoTalk app (see kakaotalk_analysis repo)

---

## 5. Implementation Plan (This PR)

### Phase 1: `doctor` command
- [x] Add `Doctor` subcommand to CLI
- [x] Check KakaoTalk.app version from `/Applications/KakaoTalk.app/Contents/Info.plist`
- [x] Check KakaoTalk process status via `pgrep`
- [x] Check Cache.db existence and modification time
- [x] Check saved credentials file
- [x] Verify token via REST API
- [x] Test LOCO booking (GETCONF) connectivity
- [x] Display protocol constants for debugging

### Phase 2: Improved LOGINLIST fields
- [x] Add proper `rp` bytes (6-byte BSON binary)
- [x] Remove `pcst` (loco-wrapper doesn't send it)
- [x] Confirm `os: "mac"` is NOT the -950 cause (tested "android" — same result)
- [ ] Obtain LOCO-compatible token (see Section 2: Remaining Options)

### Phase 3: Error reporting improvements
- [x] Add actionable messages for -950 errors
- [x] Add hints for TLS handshake failures
- [x] Print protocol version info on failure

### Phase 4: Full history access
- [x] Add `--cursor` option to `read` and `loco-read`
- [x] Add `--since YYYY-MM-DD` date filter to both commands
- [x] Add `--delay-ms` rate limiting to `loco-read`
- [x] Add batch progress reporting during `--all`
- [x] Print resume cursor on disconnect for `loco-read --all`
- [x] Implement AES-128-GCM (handshake confirmed working; -950 persists due to token issue)

### Phase 5: Connection resilience
- [x] Add exponential backoff retry to `full_connect`
- [ ] Add token expiry detection + auto-refresh
- [ ] Add reconnect logic for long-running SYNCMSG operations

### Phase 7: v0.6.0 Quality, Resilience, Display
- [x] Extract crate as lib (`lib.rs`) for integration testing
- [x] Integration test suite: LOCO packet round-trip, crypto, CLI smoke, MessageDb
- [x] Watch reconnect resilience: `reconnect_delay()` helper with jitter
- [x] Watch `--resume` flag with `watch_state.json` persistence
- [x] Process SYNCMSG push events with `[sync]` prefix
- [x] Track `last_log_ids` per chat during watch
- [x] Rich message rendering: photo dimensions, video duration, file name+size
- [x] `Default` impls for `LocoEncryptor` and `PacketBuilder`
- [x] `MessageDb::open_at()` for testable DB paths
- [x] Version bump to 0.6.0
- [x] Codebase modularization: 12 command modules + `lib.rs`
- [x] CI pipeline: parallel test/lint/build-macos jobs with caching
- [x] Custom error types (`OpenKakaoError`) with retryable distinction
- [x] Analytics: stats, cache, cache-search, cache-stats commands
- [x] Homebrew formula for macOS distribution

### Phase 8: v0.7.0 Polish, Error Model, Output Consistency
- [x] Homebrew formula bump to v0.7.0
- [x] OpenKakaoError adoption: removed `#[allow(dead_code)]`, adopted in watch.rs, send.rs, read.rs, loco_helpers.rs
- [x] Removed unused error variants (AuthExhausted, RateLimited, Credential, OkResult, transient_network)
- [x] Profile module split: `profile.rs` (1784 lines) → `profile/` directory (mod.rs, hints.rs, graph.rs, probe.rs, app_state.rs)
- [x] `--json` output for send, send-file, watch (NDJSON)
- [x] `output_json()` helper in util.rs
- [x] Dead code cleanup: removed `creds()`/`set_creds()` from rest.rs, `format_booking_config()` from loco/client.rs
- [x] Zero `#[allow(dead_code)]` suppressions remaining
- [x] Zero clippy warnings
- [x] Version bump to 0.7.0

### Phase 6: Chat type safety (ban risk mitigation)
- [x] Add `is_open_chat()` / `extract_chat_type()` helpers
- [x] Block `send` to open chats unless `--force`
- [x] Block `loco-read --all` on open chats unless `--force`
- [x] Enforce minimum 500ms delay for open chat reads
- [x] Show chat type label (DM/Group/OpenDM/OpenGroup) in send confirmation

#### Chat type priority order
| Priority | Type | Risk | Notes |
|----------|------|------|-------|
| 1 (highest) | 1:1 DM (`DirectChat`) | Low | Primary testing target |
| 2 | Group (`MultiChat`) | Low-Medium | Test after DM stable |
| 3 (lowest) | Open Chat (`OpenDirectChat`, `OpenMultiChat`) | High | `--force` required for write ops, higher rate limits enforced |

- Open chats: `--force` required for `send` and `loco-read --all`
- Open chats: minimum 500ms between SYNCMSG batches (auto-raised)
- Future: consider separate rate-limit profiles per chat type

---

## References

- Blog: [KakaoTalk is making me LOCO](https://jusung.dev/posts/kakao-talk-is-making-me-local/)
- Security analysis: [stulle123 - Not so Secret](https://stulle123.github.io/posts/kakaotalk/secret-chat/)
- KiwiTalk login.rs: LOGINLIST field reference with `rp`, `pcst`, `prtVer`
- loco-protocol-rs secure/client.rs: Handshake with `key_type=15`
