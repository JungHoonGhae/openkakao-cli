# KakaoTalk Android 26.7.1 LOCO static analysis

Status: research note. Analyzed on **2026-09-02**. This was a
static-only pass: no Kakao account authentication, server traffic, runtime
hooking, certificate pinning bypass, or message send was performed.

## Question

Which parts of the current Android KakaoTalk LOCO implementation can safely
validate or improve openkakao-cli's Mac client without substituting an Android
identity for the Mac profile?

## Artifact provenance

The official Play package is `com.kakao.talk`, published by Kakao Corp. At the
time of analysis, APKMirror listed 26.7.2 but did not offer the developer-
removed APK for download. The closest retrievable package was therefore 26.7.1
(`versionCode` 29260710), acquired as an XAPK through the open-source
[EFF apkeep](https://github.com/EFForg/apkeep) client.

| Artifact | SHA-256 |
|---|---|
| `com.kakao.talk@26.7.1.xapk` | `5892773b05caa3e666067a58372956b548e57ea06e5bde2102a0499241dba623` |
| Base APK | `703e864c6da6fc3ba629d6197d327a18ed734865f0abe6b3d3f949246d0c88ec` |
| arm64-v8a split | `2dffd54e486ec256dfa120b5d61bf4ac706f440a4009bffc95154704004de0ec` |

Google's `apksig` library verified the base APK and all 19 configuration splits
with an APK Signature Scheme v3 signer. The signer was consistent across the
set:

- certificate SHA-256: `2b06cc3d47782d7c497c07f17cb5f859cd6bbcb66829f3e67b96b7a44820d2ce`
- certificate SHA-1: `ecc45b902ac1e83c8be1758a257e67492de37456`
- subject: `OU=kakaoteam, O=kakao, C=ko`

The certificate fingerprint matches the one published for the official package
by APKMirror. Neither the XAPK nor decompiled proprietary source is committed to
this repository.

## Method

- `apksig` for signer and split-integrity verification
- `apktool` for Manifest and resource decoding
- `jadx` for DEX/Kotlin inspection and call-site tracing
- `strings` and binary metadata inspection for the arm64 native libraries
- field-by-field comparison with `src/loco/packet.rs` and
  `src/loco/client.rs`

JADX reported 1,335 failures among 49,639 classes. The LOCO classes cited by
this analysis decoded successfully; their findings were cross-checked against
Kotlin metadata, constructors, and the actual BSON serialization call sites.

## Findings

### Packet framing is unchanged

`LocoHeader.kt` serializes the same 22-byte header implemented by
`src/loco/packet.rs`: little-endian packet ID (4), status (2), an 11-byte
zero-padded UTF-8 method, body type (1), and little-endian body length (4).
No framing change is required.

### Current endpoint and transport split

- Production Booking uses `booking-loco.kakao.com:443` with TLS 1.3.
- Carriage/chat and Ticket endpoints use the TLS transport path.
- Trailer upload/download uses the `V2SL` record layer: AES-GCM-128 with a
  12-byte IV and 16-byte authentication tag, after an RSA-protected key setup.

This confirms the existing TLS Booking/chat direction. V2SL evidence is
specific to Trailer media transfer and is not a reason to replace chat TLS.
The Rust TLS configuration remains protocol-agile instead of forcing TLS 1.3,
because this Android observation does not establish the Mac server contract.

### Request schemas

| Method | Android 26.7.1 observation | openkakao-cli decision |
|---|---|---|
| `GETCONF` | `MCCMNC`, `os`, `userId` | Add the missing `userId`; retain Mac `model` pending Mac-specific evidence. |
| `CHECKIN` | `userId`, `os`, `ntype`, `appVer`, `lang`, optional `useSub` and nonblank `MCCMNC` | Existing core fields match. Retain Mac/sub-device `countryISO` and `useSub`. |
| `LOGINLIST` | `appVer`, `prtVer`, `os`, `lang`, `duuid`, `ntype`, `MCCMNC`, revision/sync fields, `rp`, `bg`, `oauthToken`, `isSw` | Do not add the Android-observed `isSw` without Mac serialization evidence. Retain Mac `dtype`; keep `prtVer: "1"` and the existing six-byte `rp`. |

The conservative rule is important: a field missing from an Android request
does not prove that the corresponding Mac sub-device field is obsolete.

### Signed Mac 26.7.0 cross-check

After the Android pass, the installed KakaoTalk Mac 26.7.0 build 1194 was
checked without launching it or contacting Kakao servers. macOS `codesign`
validated the app on disk under bundle ID `com.kakao.KakaoTalkMac` and team ID
`L75WVXX68A`. The main universal binary SHA-256 was
`f29adb10fb010fab3734df75e7624dbd6c99c4f032e4b1a9fcebec2d77431c09`.

Objective-C metadata in both binary architectures exposes a Booking request
initializer with `userId`, `MCCMNC`, and `os`. It also exposes the Mac
`LOGINLIST` initializer with `dtype`, `pcst`, `rp`, `bg`, sync IDs, and token
IDs, but without Android's `isSw` field. Initializer metadata shows request-
object inputs, not the final BSON key set: fields such as `model` or `pcst` may
be supplied or omitted on a separate or conditional serialization path. The
cross-check therefore supports adding `GETCONF.userId`, but it does not justify
removing existing compatibility fields or adding the Android-observed `isSw`.
The binary itself is not committed to this repository.

### Android account authentication is not portable to Mac

The APK's `android/account/` sub-device login path calculates `X-VC` from an
Android-specific seed and user agent. It differs from the Mac algorithm already
implemented in this project. Replacing the Mac formula, version, `os`, or user
agent with Android values would impersonate the wrong client profile and is not
supported by this evidence.

The Android flow can also trigger device/email verification and other account
state transitions. Implementing or probing it is a **NO-GO** based on static
analysis alone; it requires a separately authorized, human-observed validation
plan.

### Native libraries

The arm64 split contains 39 shared libraries, including SQLCipher and Kakao
media components. High-value LOCO request construction and transport selection
were present in DEX/Kotlin rather than hidden in the native libraries. No native
finding contradicted the DEX-derived framing or request schemas.

## Applied result

The production change is intentionally narrow:

1. Add the Android-observed and Mac-confirmed `userId` to `GETCONF`.
2. Keep `isSw` out of the Mac `LOGINLIST` request until Mac serialization
   evidence supports it.
3. Extract the three request bodies into pure builders and lock the selected
   fields, current compatibility policy, types, sync-pair ordering, and `rp`
   bytes with unit tests.

No Android identity constants, auth transformations, endpoint bypasses, or
write operations were added.

A [2026-09-07 receive-envelope follow-up](message-envelopes-26.7.md) traces
`MSG.chatLog`, the `message` field, and `SYNCMSG.chatLogs`, cross-checks the
signed Mac models, and fixes watcher decoding and resume cursor regression.
