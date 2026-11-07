<div align="center">
  <p><a href="README.md">한국어</a> · <b>English</b></p>
  <h1>OpenKakao</h1>
  <p><b>KakaoTalk from your terminal and your AI agents. Sends land in under half a second, verified.</b></p>
  <p>It drives the official KakaoTalk app you are already signed in to, through macOS Accessibility.<br />
  No server login, no protocol traffic, no database decryption. The binary is <code>openkakao-cli</code>.</p>
  <p>
    <a href="https://github.com/JungHoonGhae/openkakao-cli/releases"><img src="https://img.shields.io/github/v/release/JungHoonGhae/openkakao-cli?label=release&color=2ea44f" alt="Release" /></a>
    <a href="#install"><img src="https://img.shields.io/badge/brew-openkakao--cli-FBB040?logo=homebrew&logoColor=white" alt="Homebrew" /></a>
    <img src="https://img.shields.io/badge/platform-macOS-000000?logo=apple&logoColor=white" alt="macOS" />
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.95-orange?logo=rust" alt="Rust 1.95" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT License" /></a>
    <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  </p>
  <p>
    <a href="#install"><b>Install</b></a> ·
    <a href="#performance"><b>Performance</b></a> ·
    <a href="#features"><b>Features</b></a> ·
    <a href="#agents"><b>Agents</b></a> ·
    <a href="#safety-model"><b>Safety model</b></a> ·
    <a href="https://openkakao.vercel.app/"><b>Docs</b></a>
  </p>
  <table>
    <tr>
      <td align="center"><b>0.30 s</b><br /><sub>read the last 20 messages</sub></td>
      <td align="center"><b>0.47 s</b><br /><sub>send, verified by the bubble</sub></td>
      <td align="center"><b>~90×</b><br /><sub>faster than v1.9</sub></td>
      <td align="center"><b>9</b><br /><sub>new commands in 1.14</sub></td>
    </tr>
  </table>
  <img src="assets/demo.gif" alt="openkakao-cli sending, replying, editing, sending a photo and searching in the memo chat, at 2x speed with measured timings" width="820" />
</div>

## What's new

1.14 alone added 9 new commands. The full record is in the [CHANGELOG](CHANGELOG.md).

- **1.14** chat backup and search (`ax-export`, `ax-search`), inbox cleanup (`ax-chats --unread`, `local-mark-read`), friend memos (`ax-friend`), reactions, chat renames, per-chat tone (`style`), folded messages in full (`--expand`)
- **1.13** a chat card before every send, long messages that KakaoTalk folds past 500 characters are verified too, the new signature line `⌞ₛₑₙₜ ᵥᵢₐ ₒₚₑₙₖₐₖₐₒ`
- **1.12** message delete (`local-delete`), photos and files are `sent` only once uploaded, the agent skill ships with each CLI release

## Install

```bash
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli   # Homebrew 7+ requires trusting third-party taps
brew install openkakao-cli
```

Then run `init` once. It checks Accessibility and sign-in, registers the chats you want to send to, and can install the agent skill. It is safe to re-run.

```bash
openkakao-cli init
openkakao-cli ax-read "Chat name" -n 20       # read recent messages
openkakao-cli local-send "Chat name" "Hi" -y   # send
```

KakaoTalk must be running and signed in, and your terminal needs Accessibility permission. Chats are found by their exact window title (the memo chat is titled with your own display name). To build from source, run `cargo install --path .` (`rust-toolchain.toml` picks Rust 1.95).

## Performance

Both columns come from the same Mac Studio with KakaoTalk 26.8 (v1.14.0 measured on 2026-11-07). Times are wall clock from start to exit, and write commands include their verification.

| Task | Command | v1.9 | v1.14.0 |
|---|---|--:|--:|
| Read an open chat's last 20 messages | `ax-read` | 26.8 s | **0.30 s** (~90×) |
| Read folded long messages in full | `ax-read --expand` | — | **0.69 s** |
| Send, verified by the new bubble | `local-send` | 44 s | **0.47 s** (~93×) |
| Edit a sent message, verified by `수정됨` | `local-edit` | — | **2.0 s** |
| Delete for me | `local-delete --for-me` | — | **3.2 s** |
| Back up a whole chat as CSV | `ax-export` | — | **4.2 s** |
| List unread chats (about 670 chats) | `ax-chats --unread` | — | **12.4 s** |

— means the command did not exist in v1.9. A chat without an open window is found and opened through KakaoTalk's own search with `--open-if-needed` (v1.10 6.5 s → v1.11 3.2 s, worst case measured with 140 matching chats). [`bench/run.sh`](bench/run.sh) re-measures this table in the memo chat every release and appends to [`bench/results.csv`](bench/results.csv).

## Features

Every default path is focus-free. KakaoTalk is never brought to the front and no keyboard or mouse input is simulated, so whoever is at that Mac keeps working.

### Send and verify

```bash
openkakao-cli local-send "Chat name" $'line one\nline two' -y               # newlines stay in one bubble
openkakao-cli local-send-file "Chat name" ./report.pdf -y                   # docx, xlsx, pdf, zip…, up to 300 MB by default
openkakao-cli local-send-photo "Chat name" ./photo.jpg -y
openkakao-cli local-reply "Chat name" "Got it" --to-last -y
openkakao-cli local-edit "Chat name" --match "original text" "fixed text" -y
openkakao-cli local-delete "Chat name" --match "original text" --for-everyone -y   # or --for-me
openkakao-cli local-react "Chat name" --to-last --reaction 엄지척 -y
```

Text counts as sent when the new bubble appears, files and photos once the upload finishes (the `공유` button), and edits once the bubble shows `수정됨`. Add `--open-if-needed` for chats without an open window and `--dry-run` to preview. Reply, edit, delete and react are experimental, and `--match` takes the bubble's whole text exactly.

### Read and receive

```bash
openkakao-cli ax-read "Chat name" -n 20 --json   # sender, time, and whether you sent it
openkakao-cli ax-read "Chat name" --expand       # messages folded behind 전체보기, in full
openkakao-cli notif-watch --json                 # new messages, even with windows closed; your own sends excluded
openkakao-cli notif-watch --durable --json       # stores events in a local inbox first, survives restarts
```

`notif-watch` reads the macOS Notification Center and hands events to `--hook-cmd` or `--webhook-url`. To also catch muted chats, which raise no notification, run `ax-watch` alongside it; it watches the chat list.

### Inbox cleanup

```bash
openkakao-cli ax-chats --unread                          # unread chats: count, members, time, preview
openkakao-cli local-mark-read "Ad channel" "Shop channel"   # KakaoTalk's 읽음 처리 without opening the chat
```

Only exact names are marked, and a chat counts as done when its unread badge is gone. Measured: 96 channels and 328 messages cleared in two runs.

### Backup and search

```bash
openkakao-cli ax-export "Chat name" --output ~/backup/   # the whole history as CSV, via KakaoTalk's 텍스트 파일로 저장
openkakao-cli ax-search "Chat name" "contract"           # search the whole history; --from searches an earlier backup
```

The CSV has three columns: `Date, User, Message`. `ax-search` exports, searches and removes the export; `--keep` keeps it.

### Friends and chats

```bash
openkakao-cli ax-friends "Minsu"                          # friends-tab search: name and status message
openkakao-cli ax-friend "Kim Minsu"                       # the private memo and the name you gave them
openkakao-cli ax-friend "Kim Minsu" --memo "owns the November meeting" -y
openkakao-cli local-rename-chat "Chat name" "New name" -y   # a chat name only this account sees
```

Renames (`local-rename-chat`, `ax-friend --rename`) need Touch ID and never produce a name on the allowlist.

### Setup and recovery

```bash
openkakao-cli init --allow-chat "Chat name"         # allowlist a chat (exact names only)
openkakao-cli doctor --chat "Chat name"             # inspect an open chat window, pressing nothing
openkakao-cli init --all-spaces                     # find KakaoTalk from any desktop (restarts the Dock once)
security add-generic-password -s openkakao.kakaotalk -a you@example.com -w   # save the password in the keychain, once
openkakao-cli ax-login --account you@example.com    # signed out by another device? sign back in remotely
```

## Agents

<div align="center">
<table>
  <tr>
    <td align="center"><strong>Works with</strong></td>
    <td align="center"><img src="docs/assets/logos/openclaw.svg" width="32" alt="OpenClaw" /><br /><sub>OpenClaw</sub></td>
    <td align="center"><img src="docs/assets/logos/claude.svg" width="32" alt="Claude Code" /><br /><sub>Claude Code</sub></td>
    <td align="center"><img src="docs/assets/logos/codex.svg" width="32" alt="Codex" /><br /><sub>Codex</sub></td>
    <td align="center"><img src="docs/assets/logos/cursor.svg" width="32" alt="Cursor" /><br /><sub>Cursor</sub></td>
    <td align="center"><img src="docs/assets/logos/bash.svg" width="32" alt="Bash" /><br /><sub>Bash</sub></td>
    <td align="center"><img src="docs/assets/logos/http.svg" width="32" alt="HTTP" /><br /><sub>HTTP</sub></td>
  </tr>
</table>
</div>

```bash
npx skills add JungHoonGhae/openkakao-cli   # or: openkakao-cli init --skill claude|codex|all
```

The skill lives in this repository at [`skills/openkakao-cli/`](skills/openkakao-cli/), so it updates with every CLI release.

- **Structured results.** With `--json`, failures come back as `{"error": {"code", "message", "fix", "disposition"}}`. Only `disposition.retry: safe` means a re-run cannot send twice.
- **Chat card.** Before every send, reply, edit and delete it shows the chat's member count, recent senders and last activity, plus the three newest messages when the chat is outside the allowlist.
- **A tone per chat.** `style "Chat name" --learn` keeps your newest 200 messages in that chat on this Mac only (mode 0600), so an agent can write the way you write there.
- **Human-approved sends.** `safe-send propose` stores a proposal and never sends. It goes out only after a person types its 12-character code in an interactive terminal (`safe-send approve`).
- **Tool-sent messages are marked.** A signature line and an invisible watermark end each message, and `ax-read --json` reports them as `via_openkakao`. `--no-prefix` sends the text exactly as given.
- **SSH.** Drive the Mac that runs KakaoTalk over SSH. Grant Accessibility to `/usr/libexec/sshd-keygen-wrapper` on that Mac once.

```bash
openkakao-cli style "Chat name" --json                             # learned / not_asked / declined
openkakao-cli safe-send propose "Chat name" "message to review" --json   # stores a proposal, sends nothing
openkakao-cli safe-send approve <intent_id>                        # a person approves in a terminal
```

## Safety model

OpenKakao drives only the official KakaoTalk app that is already signed in. It checks the running app's official code signature first, and it never logs in to a server, speaks the protocol, or decrypts the database.

- **Allowlisted chats send instantly, the rest ask for Touch ID.** A chat registered with `init --allow-chat` counts as approved in advance. Any other chat is flagged `⚠ NOT in [safety] allowed_send_chats` and needs Touch ID (or the login password) for every send.
- **No guessing.** Chats that share a name wait until you choose one with `--pick N`, which also asks for Touch ID. A draft in the composer is cleared only with `--discard-draft`, and it is printed first. Two bubbles with the same text are left alone.
- **Nothing unverified is sent again.** When the result cannot be confirmed, the command reports `delivery_uncertain` and stops. There are no automatic retries.

```toml
# ~/.config/openkakao/config.toml (written by init)
[safety]
allow_ax_send = true
allowed_send_chats = ["Your name", "Family"]
```

The allowlist is a file your account can write, so shells and agents on this Mac can send to the chats on it without a prompt. List only chats that are fine to send to.

## Troubleshooting

`--json` failures carry a `fix` next to the `code`. Start with `openkakao-cli doctor` (read-only).

| Code | When | Fix |
|---|---|---|
| `accessibility_not_granted` | the app running the command lacks Accessibility | System Settings → Privacy & Security → Accessibility, turn on your terminal (over SSH: `/usr/libexec/sshd-keygen-wrapper`) |
| `chat_window_not_open` | no open window has that exact title | add `--open-if-needed`, or open the chat once |
| `chat_not_found` | no chat has that name | names that differ only in spacing or case are listed with their last message; confirm, then re-run with that exact name |
| `chat_ambiguous` | several chats share the name | pick one of the numbered candidates with `--pick N` (Touch ID) |
| `composer_busy` | a draft or a reply/edit banner is in the composer | add `--discard-draft` (prints the draft first), or clear it in KakaoTalk |
| `delivery_uncertain` | it may have been sent, but could not be verified | check with `ax-read` first; a blind retry sends it twice |
| `delete_unavailable` | KakaoTalk does not offer that delete (memo chat, someone else's message, or older than 24 hours for everyone) | leave it, or use `--for-me` to remove it from this Mac |
| `user_active` | a KakaoTalk menu is already open, or the cursor is over it | click elsewhere to close it and re-run (never press Esc: KakaoTalk closes its windows) |

The remaining codes are listed in [`SKILL.md`](skills/openkakao-cli/SKILL.md#reading-results).

## Docs

**[openkakao.vercel.app](https://openkakao.vercel.app/)** · [Quickstart](https://openkakao.vercel.app/docs/getting-started/quickstart/) · [CLI reference](https://openkakao.vercel.app/docs/cli/overview/) · [local-send](https://openkakao.vercel.app/docs/cli/local-send/) · [Agent workflows](https://openkakao.vercel.app/docs/automation/llm-agent-workflows/)

<details>
<summary>Legacy server-login and LOCO commands (deprecated)</summary>

The server-login and LOCO commands such as `login`, `chats`, `read`, `send` and `watch`, and the local database commands (`local-chats`, `local-read`, `local-search`), do not work on recent KakaoTalk builds and are deprecated. Do not retry them in a loop; use the `ax-*` and `local-*` commands above. The research is kept in the [protocol docs](https://openkakao.vercel.app/docs/protocol/overview/) and the [Android 26.7.1 LOCO static analysis](docs/research/android-apk-26.7.1.md).

</details>

## Contributing

Bug reports and PRs are welcome; please read [CONTRIBUTING.md](CONTRIBUTING.md) first. Development builds use `cargo build --release`.

- Contributors: [@twoimo](https://github.com/twoimo) ([#36](https://github.com/JungHoonGhae/openkakao-cli/pull/36)), [@e-jung](https://github.com/e-jung) ([#51](https://github.com/JungHoonGhae/openkakao-cli/pull/51), [#57](https://github.com/JungHoonGhae/openkakao-cli/pull/57)), [@NetRiceCake](https://github.com/NetRiceCake) ([#60](https://github.com/JungHoonGhae/openkakao-cli/pull/60)), [@zinoopark](https://github.com/zinoopark) ([#62](https://github.com/JungHoonGhae/openkakao-cli/pull/62))
- `local-send`'s Accessibility automation was ported to Rust from [kakaocli](https://github.com/silver-flight-group/kakaocli) (MIT), and its `CGEventPostToPid` event delivery follows [Peekaboo](https://github.com/steipete/Peekaboo) (MIT).

If this project helps you, you can support it here.

<a href="https://www.buymeacoffee.com/lucas.ghae"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="40" /></a>

MIT License

---

<sub>Unofficial project; not affiliated with or endorsed by Kakao Corp.</sub>
