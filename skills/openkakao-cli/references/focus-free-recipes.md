# Focus-free recipes for KakaoTalk on macOS (field notes, 2026-11-06)

> Since `openkakao-cli` 1.10.0 these paths are built in: `local-send`, `local-send-file`, `local-reply`, `local-edit`, `--open-if-needed` and `init --all-spaces` implement them natively, and the CLI also knows facts measured later that day (file bubbles have no text area; a file still uploading shows only its name, `업로드 중` and `취소`; same-minute messages show the clock only on the last one; the message table ends with an empty spacer row; KakaoTalk's picker greys out `.bin`). Keep this file for older installs and for inspecting the AX tree when a command reports a UI mismatch.

Everything here was verified on KakaoTalk 26.9.0 / macOS 26.5, driven over SSH from another Mac, with the user's own app staying frontmost the whole time. Nothing below posts key or mouse events, raises KakaoTalk, or touches the clipboard: it only reads and sets Accessibility (AX) attributes and performs AX actions. That is what makes it safe to run while the user is typing somewhere else.

Why this exists: `openkakao-cli` 1.9.0 `local-send`, `ax-read` and `local-send-photo` raise KakaoTalk, then post Return / ⌘V to its pid. If the user is typing in another app, the raise is lost within the 1 s focus check and the command fails with `could not verify focus on exact target composer` (after ~44 s of chat-list scanning), and the user's app is left unfocused. Until the CLI ships this path natively, use the scripts below (JXA via `osascript -l JavaScript`). Each one needs Accessibility permission for the process that runs it (over SSH: `/usr/libexec/sshd-keygen-wrapper`).

Window titles are the chat display names; the main chat-list window is titled `카카오톡` on a Korean UI. The memo chat (나와의 채팅) window is titled with your own display name. All recipes assume the target chat window is already open; recipe 3 opens one.

## 1. Send text to an open chat window (~3 s)

Facts:

- The composer is an `AXTextArea` with description `메시지 입력` in the chat window.
- Setting the composer's `AXValue` puts text in but does NOT enable the `전송` button (KakaoTalk never sees a text-did-change). Insert through `AXSelectedText` instead; that goes through the text view's insert path and enables `전송`.
- `전송` is an `AXButton` with title `전송` that supports `AXPress`.
- Verify before pressing: composer `AXValue` equals the message and `전송` is enabled. Verify after: composer is empty. Newlines inside the message survive.

```javascript
// osascript -l JavaScript send-text.js "<window title>" "<message>"
function run(argv) {
  const CHAT = argv[0], MSG = argv[1];
  const se = Application('System Events');
  const p = se.processes.byName('KakaoTalk');
  let w; try { w = p.windows.byName(CHAT); w.name(); } catch (e) { return 'window not open: ' + CHAT; }
  let ta = null, btn = null;
  (function walk(el, d) {
    if (d > 10 || (ta && btn)) return;
    let r = ''; try { r = el.role(); } catch (e) { return; }
    if (r === 'AXTextArea') { try { if (el.description() === '메시지 입력') ta = el; } catch (e) {} }
    if (r === 'AXButton') { try { if (el.title() === '전송') btn = el; } catch (e) {} }
    if (r === 'AXTable' || r === 'AXRow') return;            // skip the message list
    let k = []; try { k = el.uiElements(); } catch (e) {}
    for (const x of k) walk(x, d + 1);
  })(w, 0);
  if (!ta || !btn) return 'missing composer or send button';
  if (ta.value()) return 'draft present, not sending: ' + JSON.stringify(ta.value());
  ta.attributes.byName('AXSelectedText').value = MSG; delay(0.4);
  if (ta.value() !== MSG || !btn.enabled()) { ta.value = ''; return 'insert failed, composer reset'; }
  btn.click(); delay(1.0);                                     // click() = AXPress
  return ta.value() === '' ? 'sent' : 'unclear: composer=' + JSON.stringify(ta.value());
}
```

Read the result back with `openkakao-cli ax-read "<window title>" -n 3 --json` (slow, ~25 s on 1.9.0) or by reading the last `AXRow` of the window's `AXTable`.

## 2. Send a file to an open chat window (~10 s)

Facts:

- The composer toolbar has an `AXButton` whose help text starts with `파일전송` (⌘O). It advertises no actions, but `AXPress` still works and opens the native file panel as an `AXSheet` attached to the chat window (description `열기`) without activating KakaoTalk.
- Inside the sheet: a sidebar `AXOutline` (description `사이드바`, rows such as 데스크탑 / 문서 / 다운로드 / iCloud Drive), the file list `AXOutline` (description `목록 보기`; each row's name is an `AXTextField`, size and date are `AXStaticText`), and buttons `취소`, `열기` (disabled until a row is selected), `뒤로`, `앞으로`, `검색`.
- Selecting a row is `row.selected = true` (AX `AXSelected`). Selecting a sidebar folder reloads the list in under a second.
- After `열기`, KakaoTalk shows its own preview sheet (text `파일 전송`) with `취소` and `N개 전송` (for one file: `1개 전송`). `AXPress` on it sends.
- Proof of delivery: a new last row whose texts include the file name (plus `유효기간`, `용량`); `ax-read` renders it as `[파일]`.
- Stage the file in a folder the sidebar reaches in one click (Desktop, Documents, Downloads). Any file type works; `.docx` verified.
- 1.9.0 `local-send-photo` fails on a Korean macOS because it looks for the English `Cancel` / `Open` / `Send 1 files` titles, and it leaves the panel open. Close it with `취소` via AX (never with Escape, see warnings).

```javascript
// osascript -l JavaScript send-file.js "<window title>" "<sidebar folder, e.g. 데스크탑>" "<file name>"
function run(argv) {
  const CHAT = argv[0], FOLDER = argv[1], FILE = argv[2];
  const se = Application('System Events');
  const p = se.processes.byName('KakaoTalk');
  let w; try { w = p.windows.byName(CHAT); w.name(); } catch (e) { return 'window not open: ' + CHAT; }
  const log = [];
  function findAll(root, pred, maxDepth, stop) {
    const hits = [];
    (function walk(el, d) {
      if (d > maxDepth) return;
      let r = ''; try { r = el.role(); } catch (e) { return; }
      if (pred(el, r)) hits.push(el);
      if (stop.indexOf(r) >= 0) return;
      let k = []; try { k = el.uiElements(); } catch (e) {}
      for (const x of k) walk(x, d + 1);
    })(root, 0);
    return hits;
  }
  const texts = (row) => findAll(row, (el, r) => r === 'AXStaticText' || r === 'AXTextField', 6, [])
    .map(x => { try { return String(x.value()); } catch (e) { return ''; } });
  const sheet = () => { try { const s = w.sheets(); return s.length ? s[0] : null; } catch (e) { return null; } };
  const button = (root, title) => findAll(root, (el, r) => { if (r !== 'AXButton') return false; try { return el.title() === title; } catch (e) { return false; } }, 9, ['AXOutline', 'AXTable'])[0];
  function cancelSheet() { const s = sheet(); if (!s) return; const c = button(s, '취소'); if (c) c.click(); }

  // 1) open the file panel
  const attach = findAll(w, (el, r) => { if (r !== 'AXButton') return false; try { return (el.help() || '').indexOf('파일전송') === 0; } catch (e) { return false; } }, 10, ['AXTable'])[0];
  if (!attach) return '파일전송 button not found';
  attach.actions.byName('AXPress').perform();
  let s = null; for (let i = 0; i < 20 && !s; i++) { delay(0.25); s = sheet(); }
  if (!s) return 'file panel did not open';

  // 2) folder, then file
  const outlines = findAll(s, (el, r) => r === 'AXOutline', 9, ['AXOutline', 'AXTable']);
  const sidebar = outlines.find(o => { try { return o.description() === '사이드바'; } catch (e) { return false; } });
  const list = outlines.find(o => { try { return o.description() !== '사이드바'; } catch (e) { return true; } });
  if (!sidebar || !list) { cancelSheet(); return 'panel outlines not found'; }
  const folderRow = sidebar.rows().find(r => texts(r).indexOf(FOLDER) >= 0);
  if (!folderRow) { cancelSheet(); return 'sidebar folder not found: ' + FOLDER; }
  folderRow.selected = true;
  let fileRow = null;
  for (let i = 0; i < 12 && !fileRow; i++) { delay(0.5); fileRow = list.rows().find(r => texts(r).indexOf(FILE) >= 0) || null; }
  if (!fileRow) { cancelSheet(); return 'file not found in ' + FOLDER + ': ' + FILE; }
  fileRow.selected = true; delay(0.5);
  const open = button(s, '열기');
  if (!open || !open.enabled()) { cancelSheet(); return '열기 not enabled'; }
  open.click();

  // 3) KakaoTalk preview sheet: "N개 전송"
  let send = null;
  for (let i = 0; i < 16 && !send; i++) {
    delay(0.5);
    const cur = sheet(); if (!cur) continue;
    send = findAll(cur, (el, r) => { if (r !== 'AXButton') return false; try { return /^\d+개 전송$/.test(el.title()) && el.enabled(); } catch (e) { return false; } }, 9, ['AXOutline', 'AXTable'])[0] || null;
  }
  if (!send) { cancelSheet(); return 'preview send button not found (panel cancelled)'; }
  send.click(); delay(2.0);

  // 4) proof: last rows mention the file name
  const table = findAll(w, (el, r) => r === 'AXTable', 10, ['AXTable'])[0];
  const rows = table.rows();
  const tail = rows.slice(Math.max(0, rows.length - 3)).map(r => texts(r).join(' | '));
  return (tail.some(t => t.indexOf(FILE) >= 0) ? 'sent: ' : 'unverified: ') + tail.join(' / ');
}
```

## 3. Open a closed chat window, show a hidden main window

Facts:

- Chat-list rows in the main window (`AXTable`) expose no open action, but each row's `AXCell` supports `AXShowMenu`. The menu it opens is discoverable as an `AXMenu` child of the application (JXA `process.menus()`), typically within 0.4 s. Items: `채팅방 열기 | 채팅방 이름 설정 | 채팅방 상단 고정 | 알림 | 읽음 처리 | 폴더에 추가 | 새폴더 만들기 | 숨김`. `AXPress` on `채팅방 열기` opens the chat window; the app stays in the background.
- Only loaded rows can be matched. If the chat is not in the loaded list, scroll the list or use search (see warnings about the search field).
- A hidden main window (after its close button) comes back with the app menu bar: `창` menu > `채팅` (`AXPress`). The `창` menu also lists every open chat window by title. Do not use `tell application "KakaoTalk" to reopen` (it activates the app and steals focus); `open -g -a KakaoTalk` does nothing for a hidden window but is fine for launching the app when it is not running.

```javascript
// osascript -l JavaScript open-chat.js "<chat display name>"
function run(argv) {
  const CHAT = argv[0];
  const se = Application('System Events');
  const p = se.processes.byName('KakaoTalk');
  try { p.windows.byName(CHAT).name(); return 'already open'; } catch (e) {}
  let main; try { main = p.windows.byName('카카오톡'); main.name(); } catch (e) {
    // hidden main window: 창 > 채팅
    const bar = p.menuBars()[0]; const idx = bar.menuBarItems.name().indexOf('창');
    const item = bar.menuBarItems[idx].menus()[0].menuItems().find(i => { try { return i.name() === '채팅'; } catch (e) { return false; } });
    if (!item) return 'main window hidden and 창>채팅 not found';
    item.click(); delay(1.2); main = p.windows.byName('카카오톡');
  }
  let table = null;
  (function walk(el, d) { if (d > 10 || table) return; let r = ''; try { r = el.role(); } catch (e) { return; } if (r === 'AXTable') { table = el; return; } let k = []; try { k = el.uiElements(); } catch (e) {} for (const x of k) walk(x, d + 1); })(main, 0);
  if (!table) return 'chat list not found';
  let cell = null;
  for (const row of table.rows()) {
    let name = ''; try { name = row.entireContents().filter(x => { try { return x.role() === 'AXStaticText'; } catch (e) { return false; } })[0].value(); } catch (e) {}
    if (name === CHAT) { try { cell = row.uiElements().find(c => c.role() === 'AXCell') || row; } catch (e) { cell = row; } break; }
  }
  if (!cell) return 'chat not in loaded list (scroll or search first)';
  cell.actions.byName('AXShowMenu').perform();
  let menu = null;
  for (let i = 0; i < 15 && !menu; i++) { delay(0.2); try { menu = p.menus().find(m => m.role() === 'AXMenu') || null; } catch (e) {} }
  if (!menu) return 'menu not found: do nothing else; ask the user to dismiss any visible menu by clicking elsewhere';
  const openItem = menu.menuItems().find(i => { try { return i.name() === '채팅방 열기'; } catch (e) { return false; } });
  if (!openItem) { menu.actions.byName('AXCancel').perform(); return '채팅방 열기 not in menu'; }
  openItem.actions.byName('AXPress').perform(); delay(1.2);
  try { p.windows.byName(CHAT).name(); return 'opened'; } catch (e) { return 'pressed, but window not visible yet'; }
}
```

## 4. Incoming messages: `notif-watch`

`openkakao-cli notif-watch --replay-existing --json` reads KakaoTalk's entries in the macOS Notification Center database and needed no extra permission in this test (84 retained messages replayed at once). Each event carries `chat_name`, `author_nickname`, `message`, `attachment` (local path for images), `chat_id`, `log_id`, `received_at`. It works with the window closed and never touches focus. Limits: a muted chat and the chat the user is currently looking at post no notifications; it is forward-only (plus whatever Notification Center still retains); KakaoTalk's notification preview must be on or `message` is empty. `--durable` keeps a local inbox with retries; see the repo's launchd example.

## Warnings (all measured)

- Never post the Escape key to KakaoTalk's pid: KakaoTalk closes the focused chat window on Escape, and the main window too. Close menus with `AXCancel` and sheets with their `취소` button.
- Key events posted to KakaoTalk's pid (`CGEventPostToPid`) do nothing while KakaoTalk is not frontmost: text set through AX stayed in the composer after a posted Return. Anything that needs Return, ⌘V, ⌘O or ⌘⇧G needs a raise, so prefer the AX-only recipes above.
- Context menus on message bubbles (`AXShowMenu` on the bubble's `AXTextArea` or `AXCell`) open visibly but are often not discoverable through AX (found once, then not found in 5 s polls three times in a row), so reply / edit / delete automation is unreliable in 1.9.0-era scripts. If you try: poll with a bound, type nothing when the menu is not found, and ask the user to dismiss any menu left on screen by clicking elsewhere. The chat-list row menu (recipe 3) was reliable.
- Mentions: inserting `@` plus part of a name into a group chat composer makes a second `AXTable` (candidate members) appear inside the chat window. Picking a candidate through AX and getting a real mention token is not verified; clear the composer afterwards.
- The main window's search field exists only after the `검색` button is pressed, and setting its `AXValue` does not re-filter the list; insert through `AXSelectedText` instead (not verified).
- `ax-read` on 1.9.0 spends ~20 s on a chat-list snapshot even when the chat window is open; reading the window's `AXTable` rows directly is near instant.

## Window visibility across Spaces

The Accessibility API does not list windows that sit on another Space. The Dock option "Assign To > All Desktops" fixes it, and it is just a preference:

```bash
defaults write com.apple.spaces app-bindings -dict-add com.kakao.kakaotalkmac AllSpaces
killall Dock   # the Dock restarts (about a second of blinking); ask the user first
```

Over SSH, grant Accessibility to `/usr/libexec/sshd-keygen-wrapper` (see SKILL.md). macOS has no `timeout` command.
