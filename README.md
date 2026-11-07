<div align="center">
  <p><b>한국어</b> · <a href="README.en.md">English</a></p>
  <h1>OpenKakao</h1>
  <p><b>카카오톡 일을 AI 에이전트에게 맡기세요. 보낸 메시지는 확인까지 합니다.</b></p>
  <p>이미 로그인된 공식 카카오톡 Mac 앱을 macOS 손쉬운 사용(Accessibility)으로 움직입니다.<br />
  추가 로그인도, 프로토콜 통신도, DB 복호화도 없고, 키보드와 마우스는 계속 내가 씁니다.</p>
  <p>
    <a href="https://github.com/JungHoonGhae/openkakao-cli/releases"><img src="https://img.shields.io/github/v/release/JungHoonGhae/openkakao-cli?label=release&color=2ea44f" alt="Release" /></a>
    <a href="#시작하기"><img src="https://img.shields.io/badge/brew-openkakao--cli-FBB040?logo=homebrew&logoColor=white" alt="Homebrew" /></a>
    <img src="https://img.shields.io/badge/platform-macOS-000000?logo=apple&logoColor=white" alt="macOS" />
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.95-orange?logo=rust" alt="Rust 1.95" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT License" /></a>
    <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  </p>
  <p>
    <a href="#이런-일을-맡길-수-있습니다"><b>할 수 있는 일</b></a> ·
    <a href="#왜-안전하고-빠른가"><b>안전과 속도</b></a> ·
    <a href="#다른-방법과-비교"><b>비교</b></a> ·
    <a href="#승인과-안전장치"><b>안전장치</b></a> ·
    <a href="#시작하기"><b>시작하기</b></a> ·
    <a href="https://openkakao.vercel.app/"><b>문서</b></a>
  </p>
  <table>
    <tr>
      <td align="center"><b>0.47초</b><br /><sub>보내고 새 말풍선까지 확인</sub></td>
      <td align="center"><b>0.30초</b><br /><sub>최근 메시지 20개 읽기</sub></td>
      <td align="center"><b>추가 로그인 없음</b><br /><sub>이미 켜 둔 공식 앱으로</sub></td>
      <td align="center"><b>포커스 그대로</b><br /><sub>키보드·마우스를 뺏지 않음</sub></td>
    </tr>
  </table>
  <img src="assets/demo.gif" alt="실제 화면: 고객이 카톡으로 견적을 물으면 Claude Code가 openkakao로 견적서 PDF와 일정표 이미지를 보내고, 요약 답장과 리액션까지 합니다(8배속, 이름은 가림)" width="820" />
</div>

## 이런 일을 맡길 수 있습니다

### 자리를 비운 사이, AI 비서가 대신 답합니다

밖에서는 휴대폰으로 Mac에 SSH로 접속해 에이전트에게 맡기면 됩니다. 채팅 창이 닫혀 있어도 새 메시지를 macOS 알림으로 알아채고, 놓친 대화를 요약하고, 답장을 써서 보냅니다. 그 Mac 앞에서 누가 일하고 있어도 화면을 가로채지 않습니다. 미리 등록한 방에는 바로 보내고, 그 밖의 방은 그 Mac에서 Touch ID를 거쳐야 나갑니다.

### 몇 년 쌓인 광고 채널, 중요한 알림은 챙기고 비웁니다

안 읽은 방을 훑어서 광고만 쌓인 채널은 방을 열지 않고 카카오톡의 '읽음 처리'로 비웁니다. 주문·로그인·청구 알림은 정리하기 전에 먼저 골라서 알려 주고, 사람과 나눈 대화방은 손대기 전에 물어봅니다. 실제로 채널 96개, 메시지 328개를 두 번 실행해 정리했습니다.

### 대화 기록을 통째로 백업하고 찾습니다

카카오톡의 '텍스트 파일로 저장'을 그대로 써서 한 방의 전체 기록을 몇 초 만에 CSV(날짜, 보낸 사람, 메시지)로 받습니다. 스크롤을 한참 올릴 필요 없이 전체 기록에서 "계약서"를 찾고, 예전에 받아 둔 백업에서도 바로 찾습니다.

### 사람마다 내 말투로 씁니다

팀장님께 쓰는 말과 친구에게 쓰는 말은 다릅니다. 방마다 처음 한 번만 물어본 뒤, 그 방에서 내가 직접 쓴 최근 메시지 200개를 이 Mac에 나만 읽을 수 있는 파일로 저장합니다. 에이전트는 그걸 보고 그 방의 말투로 씁니다. 도구가 보낸 메시지는 학습에서 빠집니다.

### 답장, 리액션, 사진까지 보낸 건 전부 확인합니다

텍스트, 사진, 파일, 답장, 수정, 삭제, 리액션 모두 결과를 카카오톡 화면에서 다시 읽습니다. 새 말풍선이 생기고, 업로드가 끝나 '공유' 버튼이 뜨고, '수정됨'이 붙고, 리액션 수가 늘어야 성공입니다. 확인하지 못하면 그렇다고 알리고 멈출 뿐, 같은 메시지를 다시 보내지 않습니다.

## 왜 안전하고 빠른가

- **이미 로그인된 공식 앱만 씁니다.** 실행 중인 카카오톡이 카카오의 공식 코드 서명을 가졌는지 먼저 확인하고, 그 앱이 손쉬운 사용으로 보여 주는 화면만 읽고 누릅니다.
- **새로 로그인하는 세션이 없습니다.** 다른 기기로 로그인하지 않습니다. 쓰는 건 지금 켜 둔 그 앱 하나입니다.
- **프로토콜 통신도, DB 복호화도 없습니다.** 카카오 서버와 직접 주고받지 않고, 로컬 대화 DB를 풀어 읽지도 않습니다.
- **키보드와 마우스를 뺏지 않습니다.** 기본 경로는 카카오톡을 앞으로 띄우지 않고 키 입력도 보내지 않으니, 그 Mac에서 하던 일을 그대로 이어 갈 수 있습니다.
- **보낸 건 반드시 확인합니다.** 화면에서 결과를 읽어야 성공으로 칩니다.
- **빠릅니다.** 화면을 캡처해 이미지로 맞춰 보지 않고 앱의 UI 구조를 바로 읽기 때문입니다. 위의 시간은 Mac Studio와 카카오톡 26.8에서 릴리스마다 다시 재는 실측값입니다([`bench/results.csv`](bench/results.csv)).

## 다른 방법과 비교

카카오톡을 자동화하는 방법은 크게 다섯 갈래입니다. 아래 표에는 각 프로젝트가 README와 문서에 직접 적은 내용만 옮겼습니다(2026-11-07 확인). 문서에서 확인하지 못한 칸은 —로 두었습니다.

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

kakaocli는 읽기는 DB로, 보내기는 손쉬운 사용으로 하는 혼합 방식입니다. openkakao의 손쉬운 사용 조작도 kakaocli에서 출발했습니다([기여](#기여) 참고).

## 승인과 안전장치

- **보낼 방은 내가 정합니다.** 처음 설정할 때 등록한 방에는 바로 보내고, 등록하지 않은 방은 보낼 때마다 그 Mac에서 Touch ID(또는 로그인 암호)를 거칩니다.
- **보내기 전에 어느 방인지 보여 줍니다.** 인원수, 최근에 말한 사람, 마지막 대화 시각을 띄우고, 등록하지 않은 방이면 최근 메시지 3개도 함께 보여 줍니다.
- **에이전트는 제안만 하게 할 수도 있습니다.** 제안된 메시지는 사람이 터미널에서 12자리 코드를 입력해 승인해야 나갑니다.
- **추측하지 않습니다.** 이름이 같은 방이 여럿이면 고를 때까지 멈추고, 입력창에 쓰다 만 글은 허락 없이 지우지 않습니다.
- **확인하지 못한 건 다시 보내지 않습니다.** 자동 재시도가 없어서 같은 메시지가 두 번 가지 않습니다.
- **도구가 보낸 메시지는 표시가 남습니다.** 메시지 끝의 작은 서명 줄과 보이지 않는 워터마크로 알아볼 수 있습니다. 원하면 서명 없이 보낼 수도 있습니다.

등록 목록은 내 계정이 쓸 수 있는 파일이라, 이 Mac의 셸이나 에이전트도 등록된 방에는 바로 보낼 수 있습니다. 정말 보내도 되는 방만 넣으세요.

## 시작하기

카카오톡 Mac 앱이 실행·로그인돼 있으면 됩니다.

```bash
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli   # Homebrew 7부터 필요
brew install openkakao-cli
openkakao-cli init
```

`init`은 손쉬운 사용 권한과 로그인 상태를 점검하고, 보낼 방을 등록하고, 원하면 에이전트 스킬까지 설치합니다. 여러 번 실행해도 안전합니다. SSH로 쓰려면 그 Mac에서 `/usr/libexec/sshd-keygen-wrapper`에 손쉬운 사용 권한을 한 번 주면 됩니다.

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

그다음은 에이전트에게 평소처럼 말하면 됩니다.

```text
점심 이후에 온 카톡 요약해 줘. 가족방에는 내 말투로 "회의 끝나고 전화할게"라고 답해 주고.
```

에이전트는 [스킬](skills/openkakao-cli/SKILL.md)을 읽고 필요한 명령을 알아서 고릅니다. 스킬만 따로 설치하려면 `npx skills add JungHoonGhae/openkakao-cli`를 실행하세요. 스킬은 이 저장소에 있어 CLI와 같은 릴리스로 갱신됩니다. 터미널에서 직접 쓰고 싶다면 [CLI 레퍼런스](https://openkakao.vercel.app/docs/cli/overview)를 보세요.

## 새 소식

- **1.14** 대화 백업과 검색, 안 읽은 방 정리와 읽음 처리, 리액션, 친구 메모, 방 이름 바꾸기, 방마다 다른 말투, 접힌 긴 메시지 읽기
- **1.13** 보내기 전 방 확인 카드, 500자가 넘어 접힌 메시지도 전송 확인, 새 서명 줄
- **1.12** 메시지 삭제, 사진·파일은 업로드가 끝나야 '보냄', 에이전트 스킬을 CLI와 같은 릴리스로 배포

전체 기록은 [CHANGELOG](CHANGELOG.md)에 있습니다.

## 문서

**[openkakao.vercel.app](https://openkakao.vercel.app/)** · [빠른 시작](https://openkakao.vercel.app/docs/getting-started/quickstart) · [CLI 레퍼런스](https://openkakao.vercel.app/docs/cli/overview) · [에이전트 워크플로](https://openkakao.vercel.app/docs/automation/llm-agent-workflows) · [SKILL.md](skills/openkakao-cli/SKILL.md)

막히면 `openkakao-cli doctor`(읽기 전용)부터 실행해 보세요. 오류 코드와 해결법은 [SKILL.md](skills/openkakao-cli/SKILL.md#reading-results)에 있습니다. 예전 서버 로그인·LOCO 명령은 최근 카카오톡에서 동작하지 않아 지원을 중단했고, 분석 기록은 [프로토콜 문서](https://openkakao.vercel.app/docs/protocol/overview)에 남아 있습니다.

## 기여

버그 제보와 PR 환영합니다. [CONTRIBUTING.md](CONTRIBUTING.md)를 먼저 읽어 주세요. 개발 빌드는 `cargo build --release`입니다.

- 기여해 주신 분들: [@twoimo](https://github.com/twoimo) ([#36](https://github.com/JungHoonGhae/openkakao-cli/pull/36)), [@e-jung](https://github.com/e-jung) ([#51](https://github.com/JungHoonGhae/openkakao-cli/pull/51), [#57](https://github.com/JungHoonGhae/openkakao-cli/pull/57)), [@NetRiceCake](https://github.com/NetRiceCake) ([#60](https://github.com/JungHoonGhae/openkakao-cli/pull/60)), [@zinoopark](https://github.com/zinoopark) ([#62](https://github.com/JungHoonGhae/openkakao-cli/pull/62))
- `local-send`의 손쉬운 사용 조작 로직은 [kakaocli](https://github.com/silver-flight-group/kakaocli)(MIT)에서 Rust로 옮겨 왔고, `CGEventPostToPid`로 이벤트를 직접 전달하는 방식은 [Peekaboo](https://github.com/steipete/Peekaboo)(MIT)를 참고했습니다.

도움이 되셨다면 응원해 주세요.

<a href="https://www.buymeacoffee.com/lucas.ghae"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="40" /></a>

MIT License

---

<sub>카카오(Kakao Corp.)와 제휴하거나 보증받지 않은 비공식 프로젝트입니다.</sub>
