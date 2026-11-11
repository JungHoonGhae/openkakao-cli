# OpenKakao

**카카오톡 일을 AI 에이전트에게 맡기는 macOS CLI. 보낸 메시지는 화면에서 다시 읽어 확인합니다.**

[![CI](https://img.shields.io/github/actions/workflow/status/JungHoonGhae/openkakao-cli/openkakao-cli-ci.yml?branch=main&label=ci)](https://github.com/JungHoonGhae/openkakao-cli/actions/workflows/openkakao-cli-ci.yml)
[![Release](https://img.shields.io/github/v/release/JungHoonGhae/openkakao-cli?label=release)](https://github.com/JungHoonGhae/openkakao-cli/releases/latest)
[![macOS](https://img.shields.io/badge/platform-macOS-lightgrey?logo=apple)](#설치)
[![MIT License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

**한국어** · [English](README.en.md)

`openkakao-cli`는 이미 로그인된 공식 카카오톡 Mac 앱을 macOS 손쉬운 사용(Accessibility)으로 움직여서 읽고, 보내고, 정리합니다. 추가 로그인, 프로토콜 통신, DB 복호화가 없습니다.

![실제 화면: 고객이 카톡으로 견적을 물으면 Claude Code가 openkakao로 견적서 PDF와 일정표 이미지를 보내고, 요약 답장과 리액션까지 합니다(8배속, 이름은 가림)](assets/demo.gif)

```bash
openkakao-cli ax-read "가족" -n 20
openkakao-cli local-send "가족" "회의 끝나고 전화할게"
openkakao-cli local-send-file "김대리" ~/Downloads/견적서.pdf
openkakao-cli ax-search "김대리" "계약서"
```

최근 메시지 읽기, 보내기, 파일 보내기, 전체 기록 검색입니다.

## 특징

- **이미 로그인된 공식 앱만 씁니다.** 실행 중인 카카오톡이 카카오의 공식 코드 서명을 가졌는지 먼저 확인하고, 그 앱이 손쉬운 사용으로 보여 주는 화면만 읽고 누릅니다. 다른 기기로 로그인하지 않고, 카카오 서버와 직접 통신하지 않고, 로컬 대화 DB를 풀어 읽지 않습니다.
- **키보드와 마우스를 뺏지 않습니다.** 기본 경로는 카카오톡을 앞으로 띄우지 않고 키 입력도 보내지 않으니, 그 Mac에서 하던 일을 그대로 이어 갈 수 있습니다. 카카오톡 창이 다른 데스크톱(Space)에 있어도 그 자리에서 읽고 텍스트를 보냅니다.
- **보낸 건 화면에서 확인합니다.** 새 말풍선이 생기고, 업로드가 끝나 '공유' 버튼이 뜨고, '수정됨'이 붙고, 리액션 수가 늘어야 성공입니다. 확인하지 못하면 그렇다고 알리고 멈춥니다.
- **빠릅니다.** 화면을 캡처해 이미지로 맞춰 보지 않고 앱의 UI 구조를 바로 읽습니다. 텍스트를 보내고 확인하는 데 0.5\~0.7초, 최근 메시지 20개를 읽는 데 0.3\~0.5초가 걸렸습니다(Mac Studio, 카카오톡 26.8, 1.14\~1.18 릴리스의 [측정 기록](bench/results.csv)).

## 설치

macOS에서 카카오톡 Mac 앱이 실행·로그인돼 있으면 됩니다.

```bash
curl -fsSL https://raw.githubusercontent.com/JungHoonGhae/openkakao-cli/main/install.sh | sh
openkakao-cli init
```

[설치 스크립트](install.sh)는 최신 릴리스 바이너리를 받아 체크섬을 확인하고 `~/.local/bin`에 넣습니다. 이 폴더가 PATH에 없으면 추가할 줄을 알려 줍니다. 업데이트할 때도 같은 줄을 다시 실행하면 됩니다.

`init`은 손쉬운 사용 권한과 로그인 상태를 점검하고, 보낼 방을 등록하고, 원하면 에이전트 스킬까지 설치합니다. 여러 번 실행해도 안전합니다. SSH로 쓰려면 그 Mac에서 `/usr/libexec/sshd-keygen-wrapper`에 손쉬운 사용 권한을 한 번 주면 됩니다.

Homebrew로 설치할 수도 있습니다. `brew trust`는 Homebrew 7부터 필요합니다.

```bash
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli
brew install openkakao-cli
openkakao-cli init
```

## 빠른 시작

```bash
# 읽기 전용 점검
openkakao-cli doctor

# 안 읽은 방 목록 (방이 수백 개면 20초쯤 걸립니다)
openkakao-cli ax-chats --unread

# 최근 메시지 읽기. 창이 닫혀 있으면 열어서 읽습니다
openkakao-cli ax-read "가족" -n 20 --open-if-needed

# 무엇이 나갈지 미리 본 다음, 어느 방인지 확인받고 보내기
openkakao-cli local-send "가족" "회의 끝나고 전화할게" --dry-run
openkakao-cli local-send "가족" "회의 끝나고 전화할게"
```

- 방 이름은 카카오톡에 보이는 그대로 적습니다. 이름이 같은 방이 여럿이면 고를 때까지 멈춥니다.
- `--open-if-needed`로 연 방은 읽음 처리됩니다.

에이전트를 쓴다면 [스킬](#ai-에이전트에서-쓰기)을 설치한 뒤 평소처럼 말하면 됩니다.

> 점심 이후에 온 카톡 요약해 줘. 가족방에는 내 말투로 "회의 끝나고 전화할게"라고 답해 줘.

> 안 읽은 방 중에 광고만 쌓인 채널은 읽음 처리해 줘. 주문·결제 알림은 먼저 알려 주고.

> 김대리 방 전체 기록에서 "계약서"를 찾아 줘.

> 오늘 생일인 친구를 알려 주고, 보낼 인사말 초안을 써 줘.

## 할 수 있는 일

| 하고 싶은 일 | 명령 |
|---|---|
| 대화 읽기, 안 읽은 방 훑어보기 | `ax-read`, `ax-chats --unread` |
| 텍스트·사진·파일 보내기 | `local-send`, `local-send-photo`, `local-send-file` |
| 답장·수정·삭제·리액션 (실험 기능) | `local-reply`, `local-edit`, `local-delete`, `local-react` |
| 새 메시지 알아채기 | `notif-watch` |
| 대화 기록 백업과 검색 | `ax-export`, `ax-search` |
| 안 읽은 방 정리: 읽음 처리, 광고 채널 차단, 방 나가기 | `local-mark-read`, `ax-friend --block`, `local-leave-chat` |
| 친구 찾기, 생일, 메모, 1:1 채팅 열기 | `ax-friends`, `ax-friends --birthdays`, `ax-friend` |
| 방마다 내 말투로 쓰기 | `style` |
| 에이전트는 제안만, 승인은 사람이 | `safe-send` |
| 점검, 설정, 로그아웃된 카카오톡 다시 로그인 | `doctor`, `init`, `ax-login` |

모든 명령에 `--json`과 `--help`가 있습니다. 명령별 옵션과 오류 코드는 [SKILL.md](skills/openkakao-cli/SKILL.md)에 정리돼 있습니다.

## AI 에이전트에서 쓰기

```bash
npx skills add JungHoonGhae/openkakao-cli
```

- **스킬이 CLI와 함께 갱신됩니다.** [스킬](skills/openkakao-cli/SKILL.md)은 이 저장소에 있어 CLI와 같은 릴리스로 나갑니다. `openkakao-cli init`에서도 설치할 수 있습니다. Claude Code, Codex, OpenClaw, Cursor처럼 셸 명령을 실행하는 에이전트면 됩니다.
- **실패도 JSON으로 알립니다.** 실패하면 아래 같은 JSON 객체 하나를 출력하고 종료 코드 1로 끝납니다. 다시 시도해도 되는지는 `disposition`으로 판단합니다.
- **새 메시지를 기다릴 수 있습니다.** `openkakao-cli --json notif-watch`는 받은 메시지를 한 줄에 하나씩 내보냅니다. macOS 알림을 읽기 때문에 채팅 창이 닫혀 있어도 됩니다.
- **자리에 없어도 됩니다.** 밖에서 휴대폰으로 Mac에 SSH로 접속해 에이전트에게 맡길 수 있습니다. 그 Mac 앞에서 누가 일하고 있어도 화면을 가로채지 않습니다.

```json
{
  "error": {
    "code": "chat_window_not_open",
    "message": "…",
    "fix": "…",
    "disposition": { "delivery": "not_delivered", "retry": "safe" }
  }
}
```

## 안전장치

- **보낼 방은 내가 정합니다.** 처음 설정할 때 등록한 방에는 바로 보내고, 등록하지 않은 방은 보낼 때마다 그 Mac에서 Touch ID(또는 로그인 암호)를 거칩니다.
- **보내기 전에 어느 방인지 보여 줍니다.** 인원수, 최근에 말한 사람, 마지막 대화 시각을 띄우고, 등록하지 않은 방이면 최근 메시지 3개도 함께 보여 줍니다.
- **에이전트는 제안만 하게 할 수도 있습니다.** `safe-send`로 제안된 메시지는 사람이 터미널에서 12자리 코드를 입력해 승인해야 나갑니다.
- **추측하지 않습니다.** 이름이 같은 방이 여럿이면 고를 때까지 멈추고, 입력창에 쓰다 만 글은 허락 없이 지우지 않습니다.
- **확인하지 못한 건 다시 보내지 않습니다.** 자동 재시도가 없어서 같은 메시지가 두 번 가지 않습니다.
- **되돌릴 수 없는 일은 더 까다롭습니다.** 방 나가기는 그 Mac에서 Touch ID를 거치고, 차단은 채널만 됩니다. 사람은 차단하지 않습니다.
- **도구가 보낸 메시지는 표시가 남습니다.** 메시지 끝의 작은 서명 줄과 보이지 않는 워터마크로 알아볼 수 있습니다. 원하면 서명 없이 보낼 수도 있습니다.

등록 목록은 내 계정이 쓸 수 있는 설정 파일이라, 이 Mac의 셸이나 에이전트도 등록된 방에는 바로 보낼 수 있습니다. 정말 보내도 되는 방만 넣으세요. Touch ID 확인을 건너뛰는 옵션은 사람이 설정 파일에서 직접 허용했을 때만 동작합니다.

## 알아 둘 점

- **macOS 전용입니다.** 카카오톡 Mac 앱이 실행 중이고 로그인돼 있어야 합니다. 로그아웃됐다면 `ax-login`이 키체인에 저장해 둔 암호로 다시 로그인합니다.
- **`ax-read`는 채팅 창에 그려진 최근 메시지만 읽습니다.** 전체 기록이 필요하면 `ax-export`나 `ax-search`를 쓰세요.
- **답장·수정·삭제·리액션은 실험 기능입니다.** 개발하고 측정한 버전은 카카오톡 26.8과 26.9입니다. 업데이트로 화면 구성이 달라지면 추측하지 않고 오류 코드와 함께 멈춥니다.
- **다른 데스크톱에 있는 채팅 창으로는 파일·사진 전송과 내보내기를 할 수 없습니다.** 텍스트, 답장, 수정, 삭제는 됩니다.
- **예전 서버 로그인·LOCO 명령은 지원을 중단했습니다.** `login`, `send`, `read`, `watch` 같은 명령은 최근 카카오톡에서 동작하지 않습니다. 분석 기록은 [프로토콜 문서](https://openkakao.vercel.app/docs/protocol/overview)에 남아 있습니다.

## 다른 방법과 비교

카카오톡을 자동화하는 방법은 크게 다섯 갈래이고, openkakao는 공식 앱을 손쉬운 사용으로 움직이는 쪽입니다. 아래 표에는 각 프로젝트가 README와 문서에 직접 적은 내용만 옮겼습니다(2026-11-07 확인). 문서에서 확인하지 못한 칸은 —로 두었습니다.

<details>
<summary>비교표 펼치기</summary>

| 방식 | 프로젝트 | 별도 로그인 | 로컬 DB 읽기 | 키보드·마우스 점유 | 보내기 | 전송 확인 | 플랫폼 |
|---|---|---|---|---|---|---|---|
| 다른 기기로 로그인하는 프로토콜(LOCO) 클라이언트 | [node-kakao](https://github.com/storycraft/node-kakao) | 예, 서브 기기로 등록 | 아니요 | 아니요 | 예 | — | Node.js 라이브러리, 유지보수 중단 표기 |
| | [KiwiTalk](https://github.com/KiwiTalk/KiwiTalk) | 예, 자체 로그인 화면 | 아니요 | 아니요 | — | — | 카카오톡이 없는 OS(Ubuntu, Arch 등)를 위한 데스크톱 앱 |
| 로컬 암호화 DB 읽기 | [kakaocli](https://github.com/silver-flight-group/kakaocli) | 아니요, 공식 앱에 자동 로그인 | 예, 읽기 전용 (전체 디스크 접근 권한) | — | 예, 손쉬운 사용으로 | — | macOS 14+ |
| 화면을 차지하는 마우스·키보드 매크로 | [kakaotalk_automation](https://github.com/rainmankim/kakaotalk_automation) (pyautogui) | 아니요, 공식 앱 | 아니요 | 예, 화면 이미지로 위치를 찾아 클릭·입력 | 예 | — | — |
| 안드로이드 봇 앱 연결 | [remote-kakao](https://github.com/remote-kakao/core) | 아니요, 폰의 카카오톡 | — | — | 예 | — | 안드로이드 폰(카카오톡, MessengerBot) + Node.js |
| 공식 앱 손쉬운 사용 자동화 | [kmsg](https://github.com/channprj/kmsg) | 아니요 | 아니요 | 일반 명령은 카카오톡을 활성화할 수 있음 (`--background-safe` 읽기는 제외) | 예, 텍스트·이미지 | — | macOS 13+ |
| | [katok](https://github.com/sunghyun-k/katok-cli) | 아니요 | 아니요 | — | 예, 텍스트 | — | macOS 13+ |
| | **openkakao-cli** | 아니요 | 아니요 | 아니요 (기본 경로) | 예, 텍스트·사진·파일·답장·수정·삭제·리액션 | 예, 화면에서 결과를 다시 읽음 | macOS |

</details>

kakaocli는 읽기는 DB로, 보내기는 손쉬운 사용으로 하는 혼합 방식입니다. openkakao의 손쉬운 사용 조작도 kakaocli에서 출발했습니다([기여](#기여) 참고).

## 문서

- [SKILL.md](skills/openkakao-cli/SKILL.md): 전체 명령과 옵션, 오류 코드와 해결법
- [보내기·읽기 레퍼런스](https://openkakao.vercel.app/docs/cli/local-send): `local-send`, `local-send-photo`, `ax-read`
- [CHANGELOG](CHANGELOG.md): 버전별 변경 내역

막히면 `openkakao-cli doctor`(읽기 전용)부터 실행해 보세요.

## 기여

버그 제보와 PR을 환영합니다. [CONTRIBUTING.md](CONTRIBUTING.md)를 먼저 읽어 주세요. 개발 빌드는 `cargo build --release`입니다.

- 기여해 주신 분들: [@twoimo](https://github.com/twoimo) ([#36](https://github.com/JungHoonGhae/openkakao-cli/pull/36)), [@e-jung](https://github.com/e-jung) ([#51](https://github.com/JungHoonGhae/openkakao-cli/pull/51), [#57](https://github.com/JungHoonGhae/openkakao-cli/pull/57)), [@NetRiceCake](https://github.com/NetRiceCake) ([#60](https://github.com/JungHoonGhae/openkakao-cli/pull/60)), [@zinoopark](https://github.com/zinoopark) ([#62](https://github.com/JungHoonGhae/openkakao-cli/pull/62))
- `local-send`의 손쉬운 사용 조작 로직은 [kakaocli](https://github.com/silver-flight-group/kakaocli)(MIT)에서 Rust로 옮겨 왔습니다. `CGEventPostToPid`로 이벤트를 직접 전달하는 방식은 [Peekaboo](https://github.com/openclaw/Peekaboo)(MIT)를, 다른 데스크톱의 창을 찾는 방식은 [AltTab](https://github.com/lwouis/alt-tab-macos)을 참고했습니다.

도움이 되셨다면 [Buy Me a Coffee](https://www.buymeacoffee.com/lucas.ghae)로 응원해 주세요.

## 라이선스

[MIT](LICENSE). 카카오(Kakao Corp.)와 제휴하거나 보증받지 않은 비공식 프로젝트입니다.
