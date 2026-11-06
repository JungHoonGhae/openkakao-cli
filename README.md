<div align="center">
  <h1>OpenKakao</h1>
  <p>macOS용 카카오톡 데스크탑 앱을 위한 비공식 CLI입니다.</p>
  <p>터미널에서 직접 쓰기 좋고, JSON 출력, watch, hook, webhook 흐름으로 AI나 agent가 호출하기에도 적합합니다.</p>
  <p>실행 바이너리는 <code>openkakao-cli</code>입니다.</p>
</div>

<p align="center">
  <a href="#quick-start"><strong>Quick Start</strong></a> ·
  <a href="#핵심"><strong>핵심</strong></a> ·
  <a href="#문서"><strong>문서</strong></a> ·
  <a href="#claude-code-skill"><strong>Claude Code Skill</strong></a>
</p>

<p align="center">
  <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="MIT License" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.95-orange.svg" alt="Rust" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/status-active-brightgreen" alt="Status Active" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/docs-fumadocs-black" alt="Docs" /></a>
</p>

**한국어** | [English](README.en.md)

> [!TIP]
> **로그인 없이 바로 동작합니다.** `local-send`/`ax-read`는 macOS Accessibility API로 카카오톡 UI를 직접 읽고 조작해서, 서버 세션 없이도 실제 메시지 전송과 최근 대화 읽기를 지원합니다. KakaoTalk 앱이 실행 중이고 로그인만 되어 있으면 됩니다 — 아래 [Quick Start](#quick-start) 참고.

> [!NOTE]
> 서버 로그인(`login --save`/`login --manual`)은 최근 KakaoTalk macOS 빌드에서 대부분 동작하지 않습니다 ([#15](https://github.com/JungHoonGhae/openkakao-cli/issues/15), [#20](https://github.com/JungHoonGhae/openkakao-cli/issues/20), [#22](https://github.com/JungHoonGhae/openkakao-cli/issues/22)). **미등록 기기로 로그인을 반복 시도하지 마세요** — 카카오가 계정의 "서브 디바이스 로그인"을 차단하거나 계정을 제재할 수 있습니다(실제 피해 사례가 보고되었습니다). 로컬 SQLCipher DB(`local-chats`/`local-read`/`local-search`)도 최신 빌드에서 키 유도 공식이 어긋나 신뢰할 수 없습니다 — 읽기는 `ax-read`, 수신 감지는 `notif-watch`(알림 스트림, 창 무관·자기발신 배제) 또는 `ax-watch`를 쓰세요.

> [!WARNING]
> 이 프로젝트는 카카오(Kakao Corp.)와 무관한 비공식 CLI입니다. 연구, 자동화, 로컬 워크플로 용도로 만들었고, 카카오의 승인이나 보증을 받지 않았습니다.
> 사용 방식에 따라 카카오 이용약관 또는 운영정책 위반으로 해석될 수 있으며, 그 경우 사용자 계정이 정지되거나 영구 삭제될 수 있습니다.
> 사용 전에 관련 정책을 직접 확인하고, 모든 책임은 사용자 본인에게 있음을 전제로 신중히 사용하세요.

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

<p align="center">
  <img src="assets/thumbnail-ko.png" alt="openkakao" width="720" />
</p>

## Quick Start

### 로그인 없이 쓰기 (권장)

서버 로그인이 필요 없는 경로입니다. KakaoTalk 앱이 실행 중이고 로그인되어 있기만 하면 됩니다.

```bash
# Homebrew (Homebrew 7+는 서드파티 tap formula를 먼저 신뢰해야 설치·업그레이드됩니다)
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli
brew install openkakao-cli

# 1. 초기 설정: 손쉬운 사용 권한·로그인·Space 점검, 전송 화이트리스트, 에이전트 스킬 설치
openkakao-cli init                                       # 대화형
openkakao-cli init --yes --allow-chat "나와의 채팅에 표시되는 이름" --skill claude   # 스크립트용
#    화이트리스트는 직접 입력한 이름만 등록됩니다(~/.config/openkakao/config.toml의
#    [safety] allow_ax_send / allowed_send_chats). 다시 실행해도 안전합니다.

# 2. 메시지 보내기 — 서버 접촉 없음, 실제 카톡 UI를 직접 조작
#    열려 있는 채팅 창이면 카카오톡을 앞으로 띄우지 않고 보냅니다(포커스 유지)
openkakao-cli local-send "채팅방 표시 이름" "Hello from CLI!" --dry-run   # 미리보기
openkakao-cli local-send "채팅방 표시 이름" "Hello from CLI!" -y         # 실제 전송
openkakao-cli local-send-file "채팅방 표시 이름" ./보고서.docx --dry-run   # 파일(docx·pdf·zip 등) 미리보기
openkakao-cli local-send-file "채팅방 표시 이름" ./보고서.docx -y         # 공식 앱으로 파일 전송
openkakao-cli local-send-photo "채팅방 표시 이름" ./family.jpg -y        # 사진 전송

# 3. 최근 메시지 읽기 — 같은 방식(AX)으로 화면에 보이는 메시지를 스크랩
openkakao-cli ax-read "채팅방 표시 이름" -n 20

# 열린 채팅 창을 읽기 전용으로 점검 (입력창·전송 버튼·파일 버튼)
openkakao-cli doctor --chat "채팅방 표시 이름"

# (실험) 열린 채팅 창에서 답장·수정(포커스 유지). 대상 말풍선은 전체 텍스트가 정확히 같아야 함
openkakao-cli local-reply "채팅방 표시 이름" "답장 내용" --to-last --dry-run      # 대상 말풍선만 확인
openkakao-cli local-reply "채팅방 표시 이름" "답장 내용" --match "원문 전체" -y
openkakao-cli local-edit "채팅방 표시 이름" --match "보낸 원문 전체" "고친 내용" -y

# 4. 수신 감지 (AX) — 채팅 목록을 폴링해 안읽음이 늘면 hook/webhook 발화 (서버 접촉 없음)
openkakao-cli ax-watch --hook-cmd 'my-script.sh'

# 5. 수신 감지 (알림 스트림) — macOS 알림 센터 DB를 폴링해 새 메시지를 감지
#    창을 안 띄워도 동작하고, 자기가 보낸 메시지는 배제됩니다 (로그인·서버 접촉 없음)
openkakao-cli notif-watch --json
openkakao-cli notif-watch --hook-keyword '긴급' --hook-cmd 'my-script.sh'
# 감시가 꺼져 있던 동안 도착해 알림 센터에 남은 메시지도 시작 즉시 처리
openkakao-cli notif-watch --replay-existing --json
# 장기 실행 권장: 전달 전 로컬 인박스에 저장하고 재시작 후 미전달 이벤트 재시도
openkakao-cli notif-watch --durable --json
```

> [!IMPORTANT]
> **포커스를 뺏지 않는 경로가 기본입니다** (KakaoTalk 26.9.0 / macOS 26에서 확인). 카카오톡을 앞으로 띄우지 않고, 합성 키·마우스 입력도 쓰지 않습니다.
> - `ax-read`·`local-send*`는 제목이 정확히 일치하는 **이미 열린 채팅 창**을 바로 씁니다. 같은 제목 창이 둘 이상이면 거부합니다.
> - 텍스트: 입력창의 `AXSelectedText`로 넣고 `전송` 버튼을 `AXPress`로 누릅니다.
> - 파일·사진: `파일전송` 버튼 → 열기 패널의 사이드바 폴더 → 파일 행 → `열기` → 미리보기의 `1개 전송`을 모두 AX로 누릅니다. 바탕화면·문서·다운로드 최상위가 아닌 파일은 같은 이름으로 `~/Downloads`에 잠시 하드 링크(볼륨이 다르면 복사)해 보내고, 전송이 확인되면 지웁니다. 같은 이름의 다른 파일이 이미 있으면 `staging_conflict`로 멈춥니다.
> - 창이 닫힌 채팅은 기본적으로 `chat_window_not_open` 에러와 해결 방법을 돌려줍니다. `--open-if-needed`를 붙이면 채팅 목록 행의 컨텍스트 메뉴(`채팅방 열기`)로 엽니다. 이때도 앱을 앞으로 띄우지 않지만 채팅이 읽음 처리됩니다. 메인 창이 숨겨져 있기만 하면 메뉴 막대의 `창 > 채팅`으로 다시 보입니다.
>
> 전송 버튼이 없는 빌드에는 `--fallback-keys`가 있습니다. 텍스트는 Return, 파일은 클립보드와 ⌘V, 창 열기는 더블클릭으로 처리하는 예전 경로입니다. 이 경로만 카카오톡을 잠깐 앞으로 띄웁니다. 키보드·마우스가 `--wait-idle`초(기본 2초, `[ax] wait_idle_secs`) 동안 쉬어야 시작하고, 60초 안에 조건이 안 되면 `user_active`로 멈춥니다. 오류 경로를 포함해 끝나면 이전 앱으로 포커스를 돌려줍니다. 백그라운드 카카오톡은 pid로 보낸 키 이벤트를 받지 않으므로, 키를 쓰는 단계는 앞으로 띄워야만 동작합니다.

> [!TIP]
> **`ax-watch` vs `notif-watch`** — 둘 다 로그인 없이 수신을 감지합니다. `notif-watch`는 macOS 알림 센터 DB(평문 SQLite)를 읽으므로 **카카오톡 창이 닫혀/최소화돼 있거나 다른 Space에 있어도 동작**하고, 알림은 수신에만 뜨므로 **자기 발신을 자동으로 배제**합니다. 기본으로 시작 전 알림은 기준선으로만 삼으며, 감시 중단 사이의 알림까지 처리하려면 `--replay-existing`을 명시하세요. 장기 실행에는 `--durable`을 권장합니다. 이 모드는 이벤트를 `~/.config/openkakao/receive_inbox.db`에 먼저 저장하고 원자적 lease로 한 worker만 처리하며, 실패 시 지수 backoff로 재시도합니다. 8회 실패한 이벤트는 격리해 뒤 이벤트를 막지 않고, Notification Center에서 사라진 완료·격리 기록은 24시간 유예 후 정리합니다. 전달 의미론은 **at-least-once**입니다(외부 처리는 `chat_id`+`log_id`를 멱등 키로 쓰세요). 대신 **음소거·알림 끈 방**이나 **지금 포커스 중인 방**은 알림이 안 떠 감지하지 못하고, Notification Center에서 이미 삭제된 메시지는 재생할 수 없습니다. 창을 열어두고 그 방들까지 잡아야 하면 `ax-watch`를 병행하세요. 이벤트는 `event_type`, `chat_name`, `chat_id`(방ID), `log_id`(메시지ID), `message`, `attachment`, `received_at`를 담은 NDJSON입니다.

### 서버 로그인 기반 (현재 대부분 깨짐)

```bash
# 1. 인증 정보 저장 — 최신 빌드에서는 대부분 실패합니다 (#15, #20, #22)
openkakao-cli login --manual --save
#    (예전 빌드에서 캐시 추출이 되는 경우: openkakao-cli login --save)

# 2. 채팅방 목록
openkakao-cli chats

# 3. 메시지 읽기
openkakao-cli read <chat_id> -n 20

# 4. 메시지 보내기 (LOCO write — opt-in 필요: safety.allow_loco_write = true)
openkakao-cli send <chat_id> "Hello from CLI!"

# 로컬 DB 읽기 (현재 최신 빌드에서 키 유도 실패로 신뢰 불가 — ax-read 권장)
openkakao-cli local-chats
openkakao-cli local-read <chat_id>
```

필요할 때만 예전 cache-backed 경로를 강제합니다.

```bash
openkakao-cli chats --rest
openkakao-cli read <chat_id> --rest
openkakao-cli members <chat_id> --rest
```

### For Agent

```bash
# 로그인 없이 읽기 (서버 통신 없음, AX 기반)
openkakao-cli ax-read "채팅방 표시 이름" -n 20 --json

# 에이전트는 전송하지 않고 영속 제안만 생성
openkakao-cli --no-prefix safe-send propose "채팅방 표시 이름" "message" \
  --reply-chat-id 42 --reply-log-id 99 \
  --idempotency-key 'reply:42:99:policy-v1' --json
openkakao-cli safe-send list --json

# 실행 전 미리보기
openkakao-cli send <chat_id> "message" --dry-run --json

# 구조화된 출력
openkakao-cli --json chats
openkakao-cli --json read <chat_id> -n 20

# 실시간 이벤트 감시
openkakao-cli watch --json

# 로컬 hook 또는 webhook 흐름으로 연결
openkakao-cli --unattended --allow-watch-side-effects watch \
  --hook-cmd 'jq . > /tmp/openkakao-event.json'
```

Claude Code·Codex에서 바로 쓰려면:

```bash
openkakao-cli init --skill all   # 또는: npx skills add JungHoonGhae/skills@openkakao-cli
```

## 핵심

- `local-send`/`ax-read`로 **로그인 없이** 실제 메시지 전송·읽기 (macOS Accessibility API로 카톡 UI를 직접 조작, 서버 통신 없음)
- macOS 카카오톡 앱에서 인증 정보 추출
- 채팅, 메시지, 멤버, 친구, 프로필 조회
- LOCO 기반 메시지 전송, 실시간 watch, 미디어 처리
- `--json` 출력으로 `jq`, `cron`, SQLite, LLM 흐름과 연결 가능
- `watch`, `hook`, `webhook`로 로컬 자동화와 에이전트 워크플로에 연결 가능
- `friends --local`, `profile --local`, `profile --chat-id`로 일부 조회 복구 가능
- `local-chats`, `local-read`, `local-search`로 로컬 DB 읽기 시도 (최신 빌드에서는 키 유도 실패로 신뢰 불가 — `ax-read` 권장)
- `--dry-run`으로 실행 전 미리보기
- `send --me`로 나와의 채팅에 바로 전송 (테스트용)
- LOCO write 기본 비활성 — `safety.allow_loco_write = true`로 opt-in
- `local-send`/`local-send-photo`도 기본 비활성 — `safety.allow_ax_send = true` + `safety.allowed_send_chats` 화이트리스트로 opt-in
- 에이전트 전송에는 `safe-send` 권장 — 제안은 로컬 Outbox에만 저장되고 실제 승인은 사람의 대화형 터미널을 요구. 화이트리스트에 없는 방은 ⚠ 표시와 함께 macOS Touch ID/로그인 암호 인증도 요구

## 이런 경우에 잘 맞습니다

- 채팅 기록을 JSON으로 읽어서 다른 도구로 넘기고 싶을 때
- 카카오톡을 로컬 스크립트나 운영 도구의 입력 채널로 쓰고 싶을 때
- watch 이벤트를 hook이나 webhook으로 받아 후속 작업을 실행하고 싶을 때
- 사람이 직접 쓰는 CLI와 AI가 호출하는 로컬 인터페이스를 같이 두고 싶을 때

## 안전 모드

v1.1.0부터 LOCO write 작업(send, delete, edit, react)은 **기본 비활성**입니다.
계정 보호를 위해 서버에 쓰기 요청을 보내는 명령은 명시적 opt-in이 필요합니다.

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_loco_write = true
```

`local-send`, `local-send-file`, `local-send-photo`(AX 기반 실전송)도 기본 비활성입니다(`local-send`는 v1.4.0부터). 별도로 opt-in이 필요하고, **채팅방 화이트리스트**(`openkakao-cli init --allow-chat "<이름>"`)에 정확한 이름으로 등록한 방은 사람이 미리 승인한 것으로 보아 매번 인증하지 않습니다. 화이트리스트에 없는 방은 `⚠ NOT in [safety] allowed_send_chats`로 표시되고, 전송 직전 macOS 사용자 인증(Touch ID 또는 로그인 암호)을 통과해야 합니다. 로컬 DB의 chat-id로 대상을 다시 검증할 수 없으므로, 실전송은 이 밖에도 다음을 요구합니다: 제목이 정확히 일치하는 열린 채팅 창 하나(같은 제목 창이 둘 이상이면 거부. `--open-if-needed`로 새로 열 때는 채팅 목록에서 정확히 일치하는 행 하나), 카카오톡 코드서명:

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_ax_send = true
allowed_send_chats = ["나와의 채팅에 표시되는 이름", "다른 허용 채팅방 이름"]
```

에이전트나 자동화에는 직접 `local-send -y` 대신 `safe-send`를 권장합니다:

```bash
# 1. 제안만 저장 — 이 명령은 절대 메시지를 보내지 않음
openkakao-cli safe-send propose "채팅방 표시 이름" "검토할 메시지"

# 2. 활성 제안과 uncertain 상태 확인
openkakao-cli safe-send list

# 3. 사람이 실제 터미널에서 대상·내용을 검토하고 12자리 코드를 입력해 승인
#    (화이트리스트에 없는 방이면 macOS Touch ID 또는 로그인 암호도 요구)
openkakao-cli safe-send approve <intent_id>

# 폐기
openkakao-cli safe-send cancel <intent_id>
```

제안은 15분 후 만료되며 `~/.config/openkakao/safe_send_outbox.db`(권한 `0600`)에 저장됩니다. 승인에는 unattended 우회가 없고, 대화형 터미널의 12자리 코드를 통과해야 합니다. 화이트리스트에 없는 방은 직접 `local-send*`·`local-reply`·`local-edit`와 마찬가지로 macOS device-owner 인증도 필요합니다. 화이트리스트는 내 계정이 쓸 수 있는 설정 파일이므로, 등록한 방에는 이 Mac의 로컬 프로세스(셸·에이전트)가 인증 없이 보낼 수 있습니다. 정말 보내도 되는 방만 등록합니다. `[safety] device_owner_auth_grace_secs`(기본 0)를 켜면 한 번 인증한 뒤 그 초 동안은 다시 묻지 않습니다(sudo 방식, 최대 900초). 승인 시각은 내 계정이 쓸 수 있는 파일이라 그동안은 이 Mac의 어떤 로컬 프로세스(셸·에이전트)든 허용 목록 채팅에 보내거나 창을 연장할 수 있습니다. 모든 로컬 프로세스가 내 것인 Mac에서만 켭니다. 전송 claim 간 최소 10초, 방별 시간당 3건, 전체 시간당 10건·일일 20건을 넘길 수 없습니다. 최종 커밋(`전송` 버튼, `--fallback-keys`에서는 Return) 이전에 실패했음이 확실하면 `not_sent`로 분류해 제안을 다시 검토할 수 있고, 커밋 이후 결과가 애매하거나 실행이 중단되면 `uncertain`으로 격리되어 자동 재시도하지 않습니다. 실행 중인 앱은 공식 KakaoTalk 번들·팀 코드서명으로 확인하고, 같은 제목의 열린 창이나 같은 표시 이름의 채팅 목록 행이 둘 이상이면 거부합니다. 대상 창과 입력값을 커밋 직전에 다시 읽어 확인하며, 입력창이 비워지면서 동일 내용의 새 **발신** 메시지 버블이 증가한 경우만 전송 완료로 인정합니다.

이 경로는 `openkakao-cli`가 macOS Accessibility/CoreGraphics를 직접 호출하는 네이티브 구현입니다. Orca나 에이전트의 `$computer-use` 스킬은 설치·실행에 필요하지 않으며, 범용 UI 클릭/키 입력 대신 `safe-send propose`와 사람의 승인 interface를 사용합니다.

읽기 전용 작업은 항상 사용 가능합니다:

| 명령 | 설명 | 서버 통신 |
|------|------|-----------|
| `ax-read <chat_name>` | 화면에 열린 채팅의 최근 메시지 스크랩 (AX) | 없음 |
| `ax-watch` | 채팅 목록을 폴링해 안읽음 증가 시 hook/webhook 발화 (AX, 로그인 불필요) | 없음 |
| `notif-watch` | macOS 알림 센터 DB를 폴링해 새 수신 메시지 감지 — 창 무관·자기발신 배제 (로그인 불필요) | 없음 |
| `local-chats` | 로컬 DB 채팅 목록 (최신 빌드에서 신뢰 불가) | 없음 |
| `local-read <id>` | 로컬 DB 메시지 읽기 (최신 빌드에서 신뢰 불가) | 없음 |
| `local-search "keyword"` | 로컬 DB 검색 (최신 빌드에서 신뢰 불가) | 없음 |
| `chats --rest` | REST API 채팅 목록 | REST |
| `read <id> --rest` | REST API 메시지 읽기 | REST |
| `send ... --dry-run` | 전송 미리보기 | 없음 |
| `local-send ... --dry-run` | AX 텍스트 전송 미리보기 | 없음 |
| `local-send-file ... --dry-run` | AX 파일 전송 미리보기 (경로·크기 검증) | 없음 |
| `local-send-photo ... --dry-run` | AX 사진 전송 미리보기 | 없음 |
| `doctor --chat <chat_name>` | 열린 채팅 창 점검 (입력창·`전송` 버튼·파일 버튼), 아무것도 누르지 않음 | 없음 |
| `local-reply/local-edit ... --dry-run` | (실험) 답장·수정할 말풍선만 찾아 보여 줌 | 없음 |
| `safe-send propose/list` | 영속 전송 제안 생성·조회 (실제 전송 없음) | 없음 |

> [!NOTE]
> 이미 열린 채팅 창을 쓰는 `ax-read`/`local-send*`는 메인 창이 필요 없습니다. `ax-watch`와 `--open-if-needed`는 macOS Accessibility API로 카카오톡의 **메인 채팅 목록 창**을 찾아야 동작합니다. 이 창이 **최소화**돼 있거나 현재 보고 있는 것과 **다른 macOS Space(가상 데스크탑)**에 있으면 찾지 못합니다(숨겨져 있기만 하면 `창 > 채팅`으로 포커스 없이 다시 띄우고, 그 밖에는 명확한 에러와 해결 방법을 냅니다). 계속 겪는다면 `openkakao-cli init --all-spaces`로 한 번만 설정해두세요(Dock 아이콘 우클릭 → Options → Assign To → All Desktops와 같습니다).
>
> **`notif-watch`는 이 제약을 받지 않습니다** — AX 창이 아니라 macOS 알림 센터 DB를 읽으므로 창 상태와 무관하게 동작합니다(대신 음소거·포커스 중인 방은 알림이 안 떠 감지 못함).

## 문제 해결

`--json` 실패는 stdout에 `{"error": {"code", "message", "fix", "disposition"}}` 형태로 나옵니다. `disposition.retry`가 `safe`(`delivery: not_delivered`)일 때만 그대로 재시도해도 중복 전송이 없습니다. `uncertain`과 `unknown`은 카카오톡을 먼저 확인합니다. `doctor`(읽기 전용)로 같은 항목을 먼저 확인할 수 있습니다.

| code | 원인 | 해결 |
|------|------|------|
| `accessibility_not_granted` | 실행 중인 앱에 손쉬운 사용 권한이 없음 | 시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용에서 터미널(또는 Orca 등)을 켭니다. SSH에서는 `/usr/libexec/sshd-keygen-wrapper`를 추가합니다 |
| `kakaotalk_not_running` | 카카오톡이 실행 중이 아님 | `open -g -b com.kakao.KakaoTalkMac`(백그라운드 실행) 또는 `openkakao-cli init` |
| `kakaotalk_signed_out` | `로그인` 창만 있음 | 이 Mac에서 로그인 |
| `main_window_not_visible`, `kakaotalk_not_on_all_spaces` | 메인 창이 닫혔거나 다른 Space에 있음 | `openkakao-cli init --all-spaces`(Dock → 옵션 → 지정 → 모든 데스크탑과 같은 설정을 기록하고 Dock을 한 번 재시작) |
| `chat_window_not_open` | 같은 제목의 채팅 창이 열려 있지 않음 | 채팅 창을 한 번 열어 두거나 `--open-if-needed` |
| `chat_not_found`, `chat_list_filtered` | 채팅 목록에 로드되지 않았거나 검색어로 걸러짐 | 검색창을 비우고 목록을 스크롤하거나 탭을 바꿔 다시 그리게 합니다. 목록은 메인 창이 다시 그려지기 전까지 예전 상태일 수 있습니다 |
| `send_button_missing` | 이 빌드가 `전송` 버튼을 AX에 노출하지 않음 | `--fallback-keys` |
| `menu_not_found` | 컨텍스트 메뉴를 AX에서 찾지 못함(다른 동작은 하지 않음) | 다시 시도하거나 채팅 창을 직접 엽니다. 메뉴가 떠 있으면 다른 곳을 클릭해 닫습니다. **Esc는 누르지 마세요**(카카오톡이 창을 닫습니다) |
| `composer_busy` | 입력창에 초안이 있거나 답장·수정 중 | 카카오톡에서 보내거나 지우거나 배너의 X로 취소 |
| `message_not_found`, `message_ambiguous` | 대상 말풍선을 못 찾았거나 같은 텍스트가 둘 이상 | 최근 약 40행만 찾습니다. 화면에 보이게 스크롤하거나 `--to-last` |
| `action_unverified` | 답장·수정 상태가 확인되지 않아 더 진행하지 않음 | 채팅 창을 확인하고 남은 배너를 X로 닫습니다(Esc 금지) |
| `staging_conflict` | `~/Downloads`에 같은 이름의 다른 파일이 있음 | 파일을 다운로드·바탕화면·문서 최상위로 옮겨 보내거나 이름을 바꿉니다 |
| `user_active` | `--fallback-keys` 경로에서 계속 입력 중이었음 | 잠시 손을 뗀 뒤 다시 시도하거나 `--wait-idle 0` |
| `delivery_uncertain` | 전송했을 수도 있지만 확인하지 못함 | 카카오톡에서 직접 확인합니다. 그냥 재시도하지 마세요 |
| `file_type_refused` | 카카오톡 파일 선택창이 이 형식을 회색으로 막음(`.bin`에서 확인) | `.zip`으로 압축해 보내거나 문서·이미지 형식을 고릅니다 |
| Homebrew `untrusted tap` | Homebrew 7+ 신뢰 정책 | `brew trust --formula junghoonghae/openkakao/openkakao-cli` |

## 요구 사항

| Requirement | Notes |
|-------------|-------|
| macOS | 카카오톡 데스크탑 앱 설치 및 로그인 필요 |
| Rust 1.95.0 | 소스 빌드 시 (`rust-toolchain.toml`에서 자동 선택) |

## 설치

### Homebrew

```bash
brew tap JungHoonGhae/openkakao
brew trust --formula junghoonghae/openkakao/openkakao-cli   # Homebrew 7+
brew install openkakao-cli
openkakao-cli init
```

설치나 업그레이드가 신뢰되지 않은 tap이라며 거부되면 `brew trust` 줄을 실행하세요. `doctor`도 같은 해결 방법을 보여 줍니다.

### From source

```bash
git clone https://github.com/JungHoonGhae/openkakao-cli.git
cd openkakao-cli
cargo install --path .
```

## 문서

- 문서 사이트: https://openkakao.vercel.app/
- 빠른 시작: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI 레퍼런스: https://openkakao.vercel.app/docs/cli/overview/
- 자동화 개요: https://openkakao.vercel.app/docs/automation/overview/
- LLM / agent 워크플로: https://openkakao.vercel.app/docs/automation/llm-agent-workflows/
- watch 패턴: https://openkakao.vercel.app/docs/automation/watch-patterns/
- 프로토콜 문서: https://openkakao.vercel.app/docs/protocol/overview/
- Android 26.7.1 LOCO 정적 분석: [docs/research/android-apk-26.7.1.md](docs/research/android-apk-26.7.1.md)

Reverse engineering / local app-state diff:

```bash
openkakao-cli profile-hints --local-graph --json
openkakao-cli profile-hints --app-state --json > /tmp/profile-before.json
openkakao-cli profile-hints --app-state --app-state-diff /tmp/profile-before.json --json
```

## Claude Code Skill

```bash
openkakao-cli init --skill claude   # 또는 codex / all
# 직접 설치: npx skills add JungHoonGhae/skills@openkakao-cli
```

`init`은 `npx`가 있으면 `npx skills add`를 쓰고, 없으면 `SKILL.md`를 `~/.claude/skills/openkakao-cli/`(또는 `~/.codex/skills/openkakao-cli/`)에 직접 내려받습니다. 이미 설치돼 있으면 그대로 두고 설치 위치만 보여 줍니다.

## 개발

```bash
cd openkakao-cli
cargo build --release
```

자세한 사용법, 운영 메모, 프로토콜 설명은 문서 사이트에 정리되어 있습니다.

## Support

이 프로젝트가 도움이 되셨다면 응원해 주세요:

<a href="https://www.buymeacoffee.com/lucas.ghae">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="50">
</a>

## Contributing

버그 제보와 PR 환영합니다.

## Acknowledgments

- [kakaocli](https://github.com/silver-flight-group/kakaocli) (MIT) — `local-send`의 macOS Accessibility API 기반 카톡 UI 자동 조작(채팅방 행 선택, 입력창 탐색·전송) 로직을 Rust로 이식했습니다 (`src/ax_send.rs`).
- [Peekaboo](https://github.com/steipete/Peekaboo) (MIT) — `local-send`에서 `CGEventPostToPid`로 이벤트를 대상 프로세스에 직접 전달하는 방식을 참고해, kakaocli가 겪던 포그라운드 활성화 타이밍 레이스([silver-flight-group/kakaocli#9](https://github.com/silver-flight-group/kakaocli/issues/9))를 우회했습니다.

### Contributors

- [@twoimo](https://github.com/twoimo) — `local-send` fast path: 이미 열린 채팅창을 재사용해 전체 채팅목록 AX 스냅샷(수십 초)을 회피 ([#35](https://github.com/JungHoonGhae/openkakao-cli/pull/35) → [#36](https://github.com/JungHoonGhae/openkakao-cli/pull/36)).
- [@e-jung](https://github.com/e-jung) — KakaoTalk 26.7에서 저장된 사용자 ID를 활성 계정 마커로 검증해 복구하고 회귀 테스트 추가 ([#51](https://github.com/JungHoonGhae/openkakao-cli/pull/51)).

## License

MIT
