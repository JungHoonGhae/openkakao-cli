<div align="center">
  <p><a href="README.md">한국어</a> · <b>English</b></p>
  <h1>OpenKakao</h1>
  <p><b>Hand your KakaoTalk to an AI agent. Every message it sends is verified.</b></p>
  <p>It drives the official KakaoTalk Mac app you are already signed in to, through macOS Accessibility.<br />
  No extra login, no protocol traffic, no database decryption, and your keyboard and mouse stay yours.</p>
  <p>
    <a href="https://github.com/JungHoonGhae/openkakao-cli/releases"><img src="https://img.shields.io/github/v/release/JungHoonGhae/openkakao-cli?label=release&color=2ea44f" alt="Release" /></a>
    <a href="#get-started"><img src="https://img.shields.io/badge/brew-openkakao--cli-FBB040?logo=homebrew&logoColor=white" alt="Homebrew" /></a>
    <img src="https://img.shields.io/badge/platform-macOS-000000?logo=apple&logoColor=white" alt="macOS" />
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.95-orange?logo=rust" alt="Rust 1.95" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT License" /></a>
    <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  </p>
  <p>
    <a href="#what-you-can-hand-off"><b>What it does</b></a> ·
    <a href="#why-it-is-safe-and-fast"><b>Safe and fast</b></a> ·
    <a href="#how-it-compares"><b>Compare</b></a> ·
    <a href="#approvals-and-safeguards"><b>Safeguards</b></a> ·
    <a href="#get-started"><b>Get started</b></a> ·
    <a href="https://openkakao.vercel.app/"><b>Docs</b></a>
  </p>
  <table>
    <tr>
      <td align="center"><b>0.47 s</b><br /><sub>send, verified by the new bubble</sub></td>
      <td align="center"><b>0.30 s</b><br /><sub>read the last 20 messages</sub></td>
      <td align="center"><b>No extra login</b><br /><sub>the official app you already run</sub></td>
      <td align="center"><b>Focus stays yours</b><br /><sub>keyboard and mouse untouched</sub></td>
    </tr>
  </table>
  <img src="assets/demo.gif" alt="Real screen: asked in plain words, Claude Code uses openkakao to send a to-do list and reply to it in KakaoTalk (6x speed; name and photo hidden)" width="820" />
</div>

## What you can hand off

### An AI assistant answers while you are away

When you are out, SSH from your phone into your Mac and hand it to your agent. It notices new messages through macOS notifications even when the chat windows are closed, sums up what you missed, and writes and sends the replies. Whoever is working at that Mac keeps their screen. Chats you registered get the reply right away; any other chat needs Touch ID on that Mac first.

### Years of ad-channel backlog, cleared without losing the notices that matter

It goes through your unread chats and empties the ones that hold only ads with KakaoTalk's own 읽음 처리 (mark as read), without opening them. Order, login and billing notices are picked out and reported to you first, and chats with people are left alone until you say so. Measured: 96 channels and 328 messages cleared in two runs.

### Back up and search a whole chat history

It uses KakaoTalk's own 텍스트 파일로 저장 (save as text file) to save a chat's entire history as CSV (date, sender, message) in a few seconds. Find "contract" across the whole history without scrolling up for minutes, or search a backup you saved earlier, instantly.

### Your own tone, person by person

You do not write to your manager the way you write to a friend. For each chat it asks once, then keeps your newest 200 messages from that chat on this Mac, in a file only you can read. Your agent writes in that chat's tone from them. Messages the tool sent are left out of what it learns.

### Replies, reactions and photos, all verified

Text, photos, files, replies, edits, deletes and reactions are all read back from the KakaoTalk window. A send counts only when the new bubble appears, the upload finishes and shows 공유 (share), the bubble shows 수정됨 (edited), or the reaction count goes up. If it cannot confirm, it says so and stops. It never sends the same message again.

## Why it is safe and fast

- **Only the official, already-signed-in app.** It first checks that the running KakaoTalk carries Kakao's official code signature, then reads and presses only what that app exposes through Accessibility.
- **No extra login session.** It never signs in as another device. The only session is the app you already have open.
- **No protocol traffic, no database decryption.** It does not talk to Kakao's servers directly and does not unlock the local chat database.
- **Never takes your keyboard or mouse.** The default paths never bring KakaoTalk to the front and never send keystrokes, so whoever is at that Mac keeps working.
- **Every send verified.** Nothing counts as done until the result is read back from the screen.
- **Fast.** It reads the app's UI structure directly instead of capturing the screen and matching images. The times above are measured on a Mac Studio with KakaoTalk 26.8 and re-measured every release ([`bench/results.csv`](bench/results.csv)).

## How it compares

People automate KakaoTalk in five broad ways. The table only carries what each project states in its own README and docs (checked 2026-11-07). A cell their docs do not settle is marked —.

| Approach | Project | Own login | Reads local DB | Takes keyboard/mouse | Sends | Verifies send | Platform |
|---|---|---|---|---|---|---|---|
| Protocol (LOCO) client that signs in as another device | [node-kakao](https://github.com/storycraft/node-kakao) | Yes, registers a sub-device | No | No | Yes | — | Node.js library, marked not maintained |
| | [KiwiTalk](https://github.com/KiwiTalk/KiwiTalk) | Yes, its own login screen | No | No | — | — | Desktop app for OSes without KakaoTalk (Ubuntu, Arch…) |
| Reads the local encrypted DB | [kakaocli](https://github.com/silver-flight-group/kakaocli) | No, auto-signs in the official app | Yes, read-only (Full Disk Access) | — | Yes, via Accessibility | — | macOS 14+ |
| Mouse/keyboard macro on the screen | [kakaotalk_automation](https://github.com/rainmankim/kakaotalk_automation) (pyautogui) | No, the official app | No | Yes, clicks and types where screen images match | Yes | — | — |
| Android bot bridge | [remote-kakao](https://github.com/remote-kakao/core) | No, KakaoTalk on the phone | — | — | Yes | — | Android phone (KakaoTalk, MessengerBot) + Node.js |
| Accessibility on the official app | [kmsg](https://github.com/channprj/kmsg) | No | No | Regular commands may activate KakaoTalk (not `--background-safe` reads) | Yes, text and images | — | macOS 13+ |
| | [katok](https://github.com/sunghyun-k/katok-cli) | No | No | — | Yes, text | — | macOS 13+ |
| | **openkakao-cli** | No | No | No (default paths) | Yes: text, photos, files, replies, edits, deletes, reactions | Yes, reads the result back from the screen | macOS |

kakaocli is a hybrid: it reads through the database and sends through Accessibility. openkakao's Accessibility automation also started from kakaocli (see [Contributing](#contributing)).

## Approvals and safeguards

- **You choose where it may send.** Chats you register during setup get messages right away. Any other chat needs Touch ID (or the login password) on that Mac for every send.
- **It shows which chat before it sends.** Member count, recent senders and last activity, plus the three newest messages when the chat is not registered.
- **Your agent can be limited to proposals.** A proposed message goes out only after a person types its 12-character code in a terminal.
- **No guessing.** When several chats share a name it waits until you pick one, and a half-written draft in the composer is never cleared without your say.
- **Nothing unverified is sent again.** There are no automatic retries, so a message never goes out twice.
- **Tool-sent messages are marked.** A small signature line and an invisible watermark end each one, so they stay recognisable. You can send without the signature if you prefer.

The list of registered chats is a file your account can write, so shells and agents on this Mac can send to those chats without a prompt. Register only chats that are fine to send to.

## Get started

KakaoTalk for Mac must be running and signed in.

```bash
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli   # required since Homebrew 7
brew install openkakao-cli
openkakao-cli init
```

`init` checks Accessibility and sign-in, registers the chats you want to send to, and can install the agent skill. It is safe to re-run. To use it over SSH, grant Accessibility to `/usr/libexec/sshd-keygen-wrapper` on that Mac once.

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

Then just tell your agent what you want.

```text
Sum up the KakaoTalk messages since lunch. In the family chat, reply in my tone: "I'll call after the meeting."
```

Your agent reads the [skill](skills/openkakao-cli/SKILL.md) and picks the commands itself. To install only the skill, run `npx skills add JungHoonGhae/openkakao-cli`. The skill lives in this repository, so it updates with every CLI release. To use the commands yourself, see the [CLI reference](https://openkakao.vercel.app/docs/cli/overview).

## What's new

- **1.14** chat backup and search, unread-chat cleanup and mark-as-read, reactions, friend memos, chat renames, a tone per chat, folded long messages read in full
- **1.13** a chat card before every send, long messages that KakaoTalk folds past 500 characters are verified too, a new signature line
- **1.12** message delete, photos and files count as sent only once uploaded, the agent skill ships with each CLI release

The full record is in the [CHANGELOG](CHANGELOG.md).

## Docs

**[openkakao.vercel.app](https://openkakao.vercel.app/)** · [Quickstart](https://openkakao.vercel.app/docs/getting-started/quickstart) · [CLI reference](https://openkakao.vercel.app/docs/cli/overview) · [Agent workflows](https://openkakao.vercel.app/docs/automation/llm-agent-workflows) · [SKILL.md](skills/openkakao-cli/SKILL.md)

If something is stuck, start with `openkakao-cli doctor` (read-only). Error codes and their fixes are in [SKILL.md](skills/openkakao-cli/SKILL.md#reading-results). The old server-login and LOCO commands do not work on recent KakaoTalk builds and are deprecated; the research is kept in the [protocol docs](https://openkakao.vercel.app/docs/protocol/overview).

## Contributing

Bug reports and PRs are welcome; please read [CONTRIBUTING.md](CONTRIBUTING.md) first. Development builds use `cargo build --release`.

- Contributors: [@twoimo](https://github.com/twoimo) ([#36](https://github.com/JungHoonGhae/openkakao-cli/pull/36)), [@e-jung](https://github.com/e-jung) ([#51](https://github.com/JungHoonGhae/openkakao-cli/pull/51), [#57](https://github.com/JungHoonGhae/openkakao-cli/pull/57)), [@NetRiceCake](https://github.com/NetRiceCake) ([#60](https://github.com/JungHoonGhae/openkakao-cli/pull/60)), [@zinoopark](https://github.com/zinoopark) ([#62](https://github.com/JungHoonGhae/openkakao-cli/pull/62))
- `local-send`'s Accessibility automation was ported to Rust from [kakaocli](https://github.com/silver-flight-group/kakaocli) (MIT), and its `CGEventPostToPid` event delivery follows [Peekaboo](https://github.com/steipete/Peekaboo) (MIT).

If this project helps you, you can support it here.

<a href="https://www.buymeacoffee.com/lucas.ghae"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="40" /></a>

MIT License

---

<sub>Unofficial project; not affiliated with or endorsed by Kakao Corp.</sub>
