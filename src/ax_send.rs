//! AX-automation-based message and photo sending.
//!
//! Drives the real KakaoTalk macOS UI via the Accessibility API instead of
//! the LOCO protocol, so it works even though server login (`-100`) and
//! LOCO auth are broken (see README deprecation notice). No network or
//! KakaoTalk-server contact happens anywhere in this module.
//!
//! Ported from the sibling Swift project kakaocli
//! (https://github.com/silver-flight-group/kakaocli, MIT), with one
//! reliability fix borrowed from steipete's Peekaboo
//! (https://github.com/openclaw/Peekaboo): key/click events are posted
//! directly to KakaoTalk's pid via `CGEventPostToPid` instead of first
//! activating the app to the foreground, which avoids the focus-timing
//! race that causes kakaocli's `send` to hang
//! (https://github.com/silver-flight-group/kakaocli/issues/9).
//!
//! The real implementation only compiles on macOS — `accessibility`/
//! `core-graphics` link Apple-only frameworks, which fails to even build on
//! other platforms (see `Cargo.toml`'s macOS-only target dependencies). A
//! stub with the same public API stands in on other platforms so the crate
//! still builds and lints in cross-platform CI.

// Only `imp::open_chat` (macOS-only) actually calls these outside of
// tests, so on other platforms — where `mod imp` doesn't compile and
// `mod stub` never needs to match a chat row at all — they're otherwise
// flagged as dead code by the real (non-test) build.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ChatMatch {
    Found(usize),
    NotFound,
    Ambiguous(usize),
}

// The non-macOS transport always returns an error before it can construct an
// outcome, but callers still match this shared API so cross-platform builds
// need the variants available.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AxDeliveryOutcome {
    Verified,
    Uncertain { reason: String },
}

/// One message bubble scraped from a chat window's AX message list.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AxMessage {
    /// The time label's date (`AXHelp`, e.g. "2026. 6. 17.") and clock
    /// ("오전 8:49"), whichever the row shows.
    pub time: Option<String>,
    /// Who sent an incoming message, as KakaoTalk names them in this chat.
    pub sender: Option<String>,
    pub text: String,
    pub outgoing: bool,
}

/// What `ax-read` sees of a chat: its member count (a group chat's
/// `대화상대 N명` button; a 1:1 chat has none) and its newest messages.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Default)]
pub struct ChatView {
    pub members: Option<u32>,
    /// KakaoTalk's chat id as retained notifications suggest it (see
    /// `notif_watch::resolve_chat_id`): for display only, never for approval.
    pub chat_id: Option<i64>,
    pub messages: Vec<AxMessage>,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn member_count(button_title: &str) -> Option<u32> {
    button_title
        .trim()
        .strip_prefix("대화상대")?
        .trim()
        .strip_suffix('명')?
        .trim()
        .parse()
        .ok()
}

/// KakaoTalk names the sender only on the first bubble of a run, so the
/// rest of the run inherits it. An incoming run that started above the
/// observed rows stays unnamed.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn carry_senders(messages: &mut [AxMessage]) {
    let mut current: Option<String> = None;
    for message in messages {
        if message.outgoing {
            current = None;
        } else if message.sender.is_some() {
            current.clone_from(&message.sender);
        } else {
            message.sender.clone_from(&current);
        }
    }
}

/// Chat names that differ from `target` only in spacing or letter case: a
/// `chat_not_found` hint, never a match.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn same_but_spacing(name: &str, target: &str) -> bool {
    let fold = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    name != target && fold(name) == fold(target)
}

/// The terminal card printed before KakaoTalk is changed, so the person can
/// tell which chat this is: members, recent senders, when it was last
/// active, and, with `lines`, its newest messages.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn chat_card(name: &str, view: &ChatView, lines: usize) -> String {
    use crate::util::{escape_terminal_text as esc, truncate};
    let mut facts = vec![match view.chat_id {
        Some(id) => format!(
            "chat '{}' (id {id} per notifications, unverified)",
            esc(name)
        ),
        None => format!("chat '{}'", esc(name)),
    }];
    if let Some(members) = view.members {
        facts.push(format!("{members} members"));
    }
    if let Some(time) = view.messages.iter().rev().find_map(|m| m.time.as_deref()) {
        facts.push(format!("last message {}", esc(time)));
    }
    let mut senders: Vec<&str> = Vec::new();
    for sender in view
        .messages
        .iter()
        .rev()
        .filter_map(|m| m.sender.as_deref())
    {
        if !senders.contains(&sender) && senders.len() < 5 {
            senders.push(sender);
        }
    }
    if !senders.is_empty() {
        facts.push(format!("recent senders: {}", esc(&senders.join(", "))));
    }
    let mut card = facts.join(" · ");
    let start = view.messages.len().saturating_sub(lines);
    for message in &view.messages[start..] {
        let who = if message.outgoing {
            "me"
        } else {
            message.sender.as_deref().unwrap_or("?")
        };
        let time = message
            .time
            .as_deref()
            .map(|t| format!("[{}] ", esc(t)))
            .unwrap_or_default();
        let text = message
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        card.push_str(&format!(
            "\n  {time}{}: {}",
            esc(who),
            esc(&truncate(&text, 60))
        ));
    }
    card
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryObservation {
    pub text: String,
    pub outgoing: bool,
}

// The macOS driver's timing and bubble vocabulary. They are declared here —
// outside the macOS-only `mod imp` — for the same reason as the types above:
// the bounds and exact-match text they encode are safety contracts that tests
// must keep asserting on every platform, not only where KakaoTalk AX exists.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PHOTO_BUBBLE_PLACEHOLDER: &str = "[사진]";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const OPEN_CHAT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
// Large camera originals can leave file-picker stages — listing the
// folder, enabling NSOpenPanel's Open button, decoding KakaoTalk's own
// preview — slow while Quick Look/metadata inspection finishes. Keep this
// wait bounded, but do not reuse the much shorter chat-navigation timeout.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const FILE_PICKER_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn match_chat_row(row_names: &[Option<String>], target: &str) -> ChatMatch {
    let mut matches = row_names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_deref() == Some(target));

    match (matches.next(), matches.next()) {
        (None, _) => ChatMatch::NotFound,
        (Some((idx, _)), None) => ChatMatch::Found(idx),
        (Some(_), Some(_)) => {
            let count = row_names
                .iter()
                .filter(|name| name.as_deref() == Some(target))
                .count();
            ChatMatch::Ambiguous(count)
        }
    }
}

/// The row a human chose with `--pick N` (1-based, top to bottom) among the
/// rows named exactly `target`; `None` when nothing (valid) was picked.
// ponytail: the pick is list order at run time, and a new message can reorder
// unpinned rows between the listing and the pick; bind it to the row's
// (time, preview) fingerprint if that ever bites.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn picked_row(
    row_names: &[Option<String>],
    target: &str,
    pick: Option<usize>,
) -> Option<usize> {
    let mut matches = row_names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_deref() == Some(target))
        .map(|(index, _)| index);
    matches.nth(pick?.checked_sub(1)?)
}

/// The same message text, whether or not it ends with openkakao's invisible
/// watermark (`--match` is written without it).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn same_text(bubble: &str, text: &str) -> bool {
    let bare = |t: &str| {
        t.strip_suffix(crate::util::WATERMARK)
            .unwrap_or(t)
            .to_string()
    };
    bare(bubble) == bare(text)
}

/// KakaoTalk folds a long message behind 전체보기: the bubble's AX text is
/// the first 500 UTF-16 units and `…` (26.8, measured 2026-11-07). The floor
/// keeps a short bubble that happens to end in `…` from matching.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const FOLDED_BUBBLE_MIN_UTF16: usize = 400;

/// Whether a bubble shows `sent`: the whole text, or KakaoTalk's folded
/// preview of it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn bubble_shows(bubble: &str, sent: &str) -> bool {
    bubble == sent
        || folded_prefix(bubble)
            .is_some_and(|prefix| sent.len() > prefix.len() && sent.starts_with(prefix))
}

/// The visible part of a bubble KakaoTalk folded behind 전체보기.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn folded_prefix(bubble: &str) -> Option<&str> {
    bubble
        .strip_suffix('…')
        .filter(|prefix| prefix.encode_utf16().count() >= FOLDED_BUBBLE_MIN_UTF16)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn has_new_exact_outgoing_message(
    before: &[DeliveryObservation],
    after: &[DeliveryObservation],
    target: &str,
) -> bool {
    let matches_target = |observation: &&DeliveryObservation| {
        observation.outgoing && bubble_shows(&observation.text, target)
    };
    let before_count = before.iter().filter(matches_target).count();
    let after_count = after.iter().filter(matches_target).count();
    after_count > before_count
}

/// A cleared composer plus a new outgoing exact-text bubble. With the
/// baseline's newest row found again (`appended_start`, see
/// [`appended_rows_start`]), one such bubble among the rows appended after it
/// is enough, so an older identical bubble that scrolls out of the observed
/// tail cannot turn a real send into `delivery_uncertain`. The counting rule
/// applies without the anchor, and also when nothing follows it: KakaoTalk
/// keeps a failed upload's bubble pinned at the bottom and inserts new
/// messages above it (measured 2026-11-07).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn is_verified_delivery(
    before: &[DeliveryObservation],
    after: &[DeliveryObservation],
    appended_start: Option<usize>,
    target: &str,
    composer_value: Option<&str>,
) -> bool {
    composer_value == Some("")
        && match appended_start {
            Some(start) => {
                after
                    .get(start..)
                    .unwrap_or_default()
                    .iter()
                    .any(|row| row.outgoing && bubble_shows(&row.text, target))
                    || has_new_exact_outgoing_message(before, after, target)
            }
            None => has_new_exact_outgoing_message(before, after, target),
        }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const FILE_BUBBLE_PLACEHOLDER: &str = "[파일]";

/// What `local-send-file` / `local-send-photo` deliver. Photos keep their
/// image-content checks, JPEG/PNG-only clipboard staging for `--fallback-keys`,
/// and accept only `[사진]` (or the file name) as a delivery placeholder.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentKind {
    Photo,
    File,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl AttachmentKind {
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::File => "file",
        }
    }

    /// Bubble placeholders (as `ax-read` renders them) that prove this send.
    /// KakaoTalk can draw a document bubble with a file-type icon, which
    /// `ax-read` classifies as `[사진]`, so files accept both.
    fn accepted_placeholders(self) -> &'static [&'static str] {
        match self {
            Self::Photo => &[PHOTO_BUBBLE_PLACEHOLDER],
            Self::File => &[FILE_BUBBLE_PLACEHOLDER, PHOTO_BUBBLE_PLACEHOLDER],
        }
    }

    /// Whether the file-URL clipboard paste may carry this file. Photos keep
    /// the JPEG/PNG-only staging they shipped with.
    pub(crate) fn clipboard_allows(self, path: &std::path::Path) -> bool {
        match self {
            Self::File => true,
            Self::Photo => path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "jpg" | "jpeg" | "png"
                    )
                }),
        }
    }
}

/// One message-table row as seen by attachment delivery verification.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentObservation {
    pub placeholder: Option<String>,
    pub outgoing: bool,
    pub mentions_filename: bool,
}

/// Evidence that the commit produced one more outgoing attachment bubble than
/// the pre-commit baseline: `"file_name"` when a new outgoing row shows the
/// exact file name, else the kind's placeholder whose count grew. Counting
/// (instead of searching) keeps an older identical bubble from verifying a new
/// send, and a baseline row that scrolls out of the observed tail can only
/// make the result `None` (uncertain), never a false success.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn new_attachment_evidence(
    before: &[AttachmentObservation],
    after: &[AttachmentObservation],
    kind: AttachmentKind,
) -> Option<&'static str> {
    let grew = |matches: &dyn Fn(&AttachmentObservation) -> bool| {
        let count = |rows: &[AttachmentObservation]| {
            rows.iter()
                .filter(|row| row.outgoing && matches(row))
                .count()
        };
        count(after) > count(before)
    };
    if grew(&|row| row.mentions_filename) {
        return Some("file_name");
    }
    kind.accepted_placeholders()
        .iter()
        .copied()
        .find(|placeholder| grew(&|row| row.placeholder.as_deref() == Some(*placeholder)))
}

/// Where the rows appended after the commit begin: just past the baseline's
/// newest row, found again by identity. `None` when that row is no longer in
/// the observed tail.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn appended_rows_start<T>(
    baseline_last: Option<&T>,
    after: &[T],
    same: impl Fn(&T, &T) -> bool,
) -> Option<usize> {
    match baseline_last {
        None => Some(0),
        Some(last) => after
            .iter()
            .rposition(|row| same(last, row))
            .map(|index| index + 1),
    }
}

/// Delivery evidence for an attachment. With the baseline's newest row found
/// again (`appended_start`), only rows appended after it count, so an older
/// row that is re-classified later (a late image or share button) cannot
/// verify a send. Without that anchor, or when nothing follows it (a failed
/// upload's bubble stays pinned below new rows), the counting rule applies
/// only if the table actually gained rows.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn attachment_delivery_evidence(
    before: (&[AttachmentObservation], usize),
    after: (&[AttachmentObservation], usize),
    appended_start: Option<usize>,
    kind: AttachmentKind,
) -> Option<&'static str> {
    let ((before_rows, before_total), (after_rows, after_total)) = (before, after);
    match appended_start {
        Some(start) => {
            let appended = after_rows.get(start..).unwrap_or_default();
            let outgoing = || appended.iter().filter(|row| row.outgoing);
            if outgoing().any(|row| row.mentions_filename) {
                return Some("file_name");
            }
            kind.accepted_placeholders()
                .iter()
                .copied()
                .find(|placeholder| {
                    outgoing().any(|row| row.placeholder.as_deref() == Some(*placeholder))
                })
                .or_else(|| {
                    (after_total > before_total)
                        .then(|| new_attachment_evidence(before_rows, after_rows, kind))
                        .flatten()
                })
        }
        None if after_total > before_total => {
            new_attachment_evidence(before_rows, after_rows, kind)
        }
        None => None,
    }
}
/// What a verified attachment send reports to the caller.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentReceipt {
    pub verified_by: &'static str,
    pub preview_shows_filename: bool,
    /// The file was temporarily staged in ~/Downloads (and removed again).
    pub staged: bool,
}

/// How AX commands may use the foreground, set once by `main` from flags and
/// `[ax]` config.
// ponytail: process-wide instead of threaded through every AX caller (the
// commands, safe-send approve); main sets it once before dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AxPolicy {
    /// Required keyboard/mouse idle time before KakaoTalk may be raised.
    pub wait_idle: std::time::Duration,
    /// Open a chat that has no open window from the chat list (raises KakaoTalk).
    pub open_if_needed: bool,
    /// Commit text with the raise + Return path instead of the `전송` button.
    pub fallback_keys: bool,
    /// `--pick N`: which exact-name chat-list row to open when several match.
    pub pick: Option<usize>,
    /// `--discard-draft`: clear (and print) a composer draft before a text send.
    pub discard_draft: bool,
    /// `--dangerously-skip-allowlist`: a chat outside the allowlist needs no
    /// device-owner authentication (`--pick` still does).
    pub skip_allowlist: bool,
}

pub(crate) const DEFAULT_WAIT_IDLE: std::time::Duration = std::time::Duration::from_secs(2);

impl Default for AxPolicy {
    fn default() -> Self {
        Self {
            wait_idle: DEFAULT_WAIT_IDLE,
            open_if_needed: false,
            fallback_keys: false,
            pick: None,
            discard_draft: false,
            skip_allowlist: false,
        }
    }
}

/// `[safety] allowed_send_chats`, set once by `main`. Listing a chat there is
/// the human's standing approval, so AX writes to it skip the per-send
/// device-owner authentication; any other exact name still needs it. Unset
/// (no config loaded) means every target needs it.
static ALLOWED_SEND_CHATS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

pub(crate) fn set_allowed_send_chats(chats: Vec<String>) {
    let _ = ALLOWED_SEND_CHATS.set(chats);
}

/// Exact display-name match only: `허용된` never stands for `허용된 방`.
pub(crate) fn is_allowlisted(allowed: &[String], chat: &str) -> bool {
    allowed.iter().any(|name| name == chat)
}

/// Why an AX write needs device-owner authentication, or `None` when the
/// allowlist already stands for the human's approval. A `--pick` among
/// same-name chats always needs it: the name-based allowlist covers every
/// chat with that name, not the one picked.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn auth_reason(allowlisted: bool, picked_row: Option<&str>) -> Option<String> {
    match (picked_row, allowlisted) {
        (Some(row), _) => Some(format!("picked among same-name chats: {row}")),
        (None, false) => Some("which is NOT in allowed_send_chats".to_string()),
        (None, true) => None,
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn needs_device_owner_auth(chat: &str) -> bool {
    !ALLOWED_SEND_CHATS
        .get()
        .is_some_and(|allowed| is_allowlisted(allowed, chat))
}

static AX_POLICY: std::sync::OnceLock<AxPolicy> = std::sync::OnceLock::new();

pub(crate) fn set_policy(policy: AxPolicy) {
    let _ = AX_POLICY.set(policy);
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn policy() -> AxPolicy {
    AX_POLICY.get().copied().unwrap_or_default()
}

pub(crate) fn skips_allowlist() -> bool {
    policy().skip_allowlist
}

/// Upper bound on waiting for the user to go idle before raising KakaoTalk.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MAX_IDLE_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum IdleDecision {
    Proceed,
    Wait,
    TimedOut,
}

/// Whether KakaoTalk may be raised now. An unreadable idle time proceeds: the
/// wait only keeps the user's keystrokes out of KakaoTalk, while the exact
/// window/composer read-backs remain the safety gate.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn idle_decision(
    idle: Option<std::time::Duration>,
    threshold: std::time::Duration,
    waited: std::time::Duration,
    max_wait: std::time::Duration,
) -> IdleDecision {
    match idle {
        _ if threshold.is_zero() => IdleDecision::Proceed,
        None => IdleDecision::Proceed,
        Some(idle) if idle >= threshold => IdleDecision::Proceed,
        _ if waited >= max_wait => IdleDecision::TimedOut,
        _ => IdleDecision::Wait,
    }
}

/// `HIDIdleTime` (nanoseconds) from `ioreg -c IOHIDSystem` output.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_hid_idle(ioreg: &str) -> Option<std::time::Duration> {
    ioreg
        .lines()
        .find_map(|line| line.split_once("\"HIDIdleTime\" = "))
        .and_then(|(_, value)| value.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_nanos)
}

/// Give the foreground back only when this run left KakaoTalk in front of an
/// app the user was using. If the user already moved elsewhere, leave them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn should_restore_focus(previous: Option<i32>, current: Option<i32>, kakao_pid: i32) -> bool {
    matches!((previous, current), (Some(previous), Some(current)) if previous != kakao_pid && current == kakao_pid)
}

/// Whether an `AXStaticText` value is KakaoTalk's upload-progress label. A file
/// still uploading shows its name, this label and a 취소 button; the share
/// button, size and validity appear only once the upload is done.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_uploading_label(value: &str) -> bool {
    let value = value.trim().trim_end_matches(['…', '.']).trim();
    matches!(value, "업로드 중" | "Uploading" | "uploading")
}

/// Whether an `AXStaticText` value is a message's clock label ("오후 5:18",
/// "14:32", "2:05 PM"), as opposed to a file name, size or other text.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_clock_label(value: &str) -> bool {
    clock_of(value).is_some()
}

/// The clock of a time label. In a group chat it can carry how many members
/// have not read the message yet on the line above: "2\n오후 4:07" (26.8,
/// measured 2026-11-07).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn clock_of(value: &str) -> Option<&str> {
    let value = value.trim();
    let (unread, clock) = value.rsplit_once('\n').unwrap_or(("", value));
    let unread = unread.trim();
    (unread.bytes().all(|byte| byte.is_ascii_digit()) && is_bare_clock(clock)).then(|| clock.trim())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_bare_clock(value: &str) -> bool {
    let mut rest = value.trim();
    for marker in ["오전", "오후", "AM", "PM", "am", "pm"] {
        rest = rest
            .strip_prefix(marker)
            .or_else(|| rest.strip_suffix(marker))
            .map(str::trim)
            .unwrap_or(rest);
    }
    let Some((hours, minutes)) = rest.split_once(':') else {
        return false;
    };
    let digits = |text: &str, max: usize| {
        !text.is_empty() && text.len() <= max && text.bytes().all(|byte| byte.is_ascii_digit())
    };
    digits(hours, 2) && minutes.len() == 2 && digits(minutes, 2)
}

/// The single window a signed-out KakaoTalk shows (it reuses the
/// `Main Window` identifier). Only `로그인` is verified on 26.9.
/// What the 로그인 window shows, from its texts (KakaoTalk 26.8, Korean UI).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoginScreen {
    /// The account/password form.
    Form,
    /// The QR code page, live or expired.
    Qr,
    /// "카카오계정 또는 비밀번호를 다시 확인해 주세요."
    Rejected,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn login_screen(texts: &[String]) -> LoginScreen {
    let shows = |needle: &str| texts.iter().any(|text| text.contains(needle));
    if shows("다시 확인해 주세요") {
        LoginScreen::Rejected
    } else if shows("QR코드로 로그인할 수 있습니다") || shows("인증 시간이 만료")
    {
        LoginScreen::Qr
    } else {
        LoginScreen::Form
    }
}

/// Sheets on the 로그인 window that `ax-login` may confirm: the logout notice
/// ("…로그아웃 되었습니다.") and leaving the QR page ("인증 진행을 중단하시겠습니까?").
/// Any other sheet is left to a human.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn is_dismissable_login_sheet(texts: &[String]) -> bool {
    texts
        .iter()
        .any(|text| text.contains("로그아웃 되었습니다") || text.contains("인증 진행을 중단"))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_login_window_title(title: &str) -> bool {
    matches!(title.trim(), "로그인" | "Login" | "Log In")
}

/// Center of an AX frame in global display coordinates. Displays above or
/// left of the main one have negative origins, so only the size is checked.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn frame_center(x: f64, y: f64, width: f64, height: f64) -> Option<(f64, f64)> {
    let finite = [x, y, width, height].iter().all(|value| value.is_finite());
    (finite && width >= 1.0 && height >= 1.0).then(|| (x + width / 2.0, y + height / 2.0))
}

/// Localized (ko/en) titles of the native open panel's and KakaoTalk's
/// preview's buttons. A Korean macOS shows `취소` / `열기`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const CANCEL_TITLES: &[&str] = &["취소", "Cancel"];
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const OPEN_TITLES: &[&str] = &["열기", "Open"];

/// One button of a sheet, as seen when choosing the preview's send button.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SheetButton {
    pub title: Option<String>,
    pub enabled: bool,
    /// The sheet's `AXDefaultButton`.
    pub is_default: bool,
    /// The sheet's `AXCancelButton`, or a localized Cancel title.
    pub is_cancel: bool,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_cancel_title(title: Option<&str>) -> bool {
    title.is_some_and(|title| CANCEL_TITLES.contains(&title.trim()))
}

/// Whether a sheet over a chat window only acknowledges something: exactly
/// one enabled button titled 확인/OK and nothing to choose, like KakaoTalk
/// 26.8's "예상하지 못한 오류가 발생했습니다." or "파일 전송 · 전송 불가" alerts
/// left behind by a failed upload (measured 2026-11-07). Pressing it decides
/// nothing, so a send may dismiss it; any other sheet is a human's question.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn is_acknowledgement_only(buttons: &[SheetButton]) -> bool {
    matches!(buttons, [only] if only.enabled
        && matches!(only.title.as_deref().map(str::trim), Some("확인" | "OK")))
}

/// The attachment count in a preview send title: `1개 전송` (Korean
/// KakaoTalk 26.9) or `Send 1 files` (English, #57).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn preview_send_count(title: &str) -> Option<u32> {
    let title = title.trim();
    if let Some(count) = title.strip_suffix("개 전송") {
        return count.trim().parse().ok();
    }
    let rest = title.strip_prefix("Send ")?;
    let count = rest
        .strip_suffix(" files")
        .or_else(|| rest.strip_suffix(" file"))?;
    count.trim().parse().ok()
}

/// Choose KakaoTalk's attachment-preview send button. Only a cancellable
/// sheet that is not the open panel itself (no 열기/Open button) qualifies,
/// so an error alert's lone OK or the panel's Open are never chosen. Then a
/// unique enabled button titled `N개 전송` / `Send N files` with N = 1 (any
/// other N refuses: more than this file is staged), else a send-like title
/// (`…전송…`, `…보내…`, `Send…`) that is the sheet's default or unique. An
/// alert's default `확인` is never chosen.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pick_preview_send_button(buttons: &[SheetButton]) -> Option<usize> {
    let title = |button: &SheetButton| button.title.as_deref().unwrap_or("").trim().to_string();
    let is_open_panel = buttons
        .iter()
        .any(|button| OPEN_TITLES.contains(&title(button).as_str()));
    if is_open_panel
        || !buttons
            .iter()
            .any(|button| button.enabled && button.is_cancel)
    {
        return None;
    }
    let candidates: Vec<usize> = buttons
        .iter()
        .enumerate()
        .filter(|(_, button)| button.enabled && !button.is_cancel)
        .map(|(index, _)| index)
        .collect();
    let unique = |matches: &dyn Fn(&SheetButton) -> bool| {
        let mut found = candidates
            .iter()
            .copied()
            .filter(|&index| matches(&buttons[index]));
        match (found.next(), found.next()) {
            (Some(index), None) => Some(index),
            _ => None,
        }
    };
    let counted: Vec<(usize, u32)> = candidates
        .iter()
        .filter_map(|&index| {
            preview_send_count(&title(&buttons[index])).map(|count| (index, count))
        })
        .collect();
    match counted.as_slice() {
        [(index, 1)] => return Some(*index),
        [] => {}
        _ => return None,
    }
    let send_like = |button: &SheetButton| {
        let title = title(button);
        title.starts_with("Send") || title.contains("전송") || title.contains("보내")
    };
    unique(&|button| button.is_default && send_like(button)).or_else(|| unique(&send_like))
}

/// A standard folder the open panel's sidebar lists, which the AX-only file
/// route can select with one click.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerFolder {
    Desktop,
    Documents,
    Downloads,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl PickerFolder {
    /// Sidebar row labels (ko/en).
    fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Desktop => &["데스크탑", "데스크톱", "Desktop"],
            Self::Documents => &["문서", "Documents"],
            Self::Downloads => &["다운로드", "Downloads"],
        }
    }
}

/// The sidebar folder whose top level holds the file, if any. Files anywhere
/// else are staged into Downloads first.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn picker_folder(
    parent: &std::path::Path,
    desktop: &std::path::Path,
    documents: &std::path::Path,
    downloads: &std::path::Path,
) -> Option<PickerFolder> {
    [
        (downloads, PickerFolder::Downloads),
        (desktop, PickerFolder::Desktop),
        (documents, PickerFolder::Documents),
    ]
    .into_iter()
    .find(|(folder, _)| *folder == parent)
    .map(|(_, folder)| folder)
}

/// Which open-panel list row is the file. Names must be NFC-normalized.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum ListPick {
    /// The row shows the full file name.
    Exact(usize),
    /// The row shows only the stem (Finder hides the extension); the preview
    /// must then show the exact file name before anything is sent.
    Stem(usize),
    Ambiguous(usize),
    /// Not (yet) listed, or a row's name is unreadable so a stem match could
    /// be another file.
    NotFound,
}

/// Prefer full-name matches. Finder hides many extensions by default, so a
/// bare stem may match, but only when every row's name is readable (an
/// unreadable row could be this very file, which would make a stem match the
/// wrong one) and only uniquely.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pick_list_row(names: &[Option<String>], filename: &str) -> ListPick {
    let matching = |wanted: &str| -> Vec<usize> {
        names
            .iter()
            .enumerate()
            .filter(|(_, name)| name.as_deref().map(str::trim) == Some(wanted))
            .map(|(index, _)| index)
            .collect()
    };
    match matching(filename).as_slice() {
        [index] => return ListPick::Exact(*index),
        [] => {}
        many => return ListPick::Ambiguous(many.len()),
    }
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(filename);
    if stem == filename || names.iter().any(Option::is_none) {
        return ListPick::NotFound;
    }
    match matching(stem).as_slice() {
        [index] => ListPick::Stem(*index),
        [] => ListPick::NotFound,
        many => ListPick::Ambiguous(many.len()),
    }
}

/// KakaoTalk's key in `com.apple.spaces` `app-bindings` (stored lower-case).
const SPACES_BINDING_ID: &str = "com.kakao.kakaotalkmac";

/// Whether Mission Control binds KakaoTalk to every Space (Dock → Options →
/// Assign To → All Desktops), read from `com.apple.spaces` `app-bindings`.
fn kakaotalk_on_all_spaces(spaces: &plist::Value) -> bool {
    spaces
        .as_dictionary()
        .and_then(|root| root.get("app-bindings"))
        .and_then(plist::Value::as_dictionary)
        .and_then(|bindings| {
            bindings
                .iter()
                .find(|(app, _)| app.eq_ignore_ascii_case(SPACES_BINDING_ID))
        })
        .and_then(|(_, binding)| binding.as_string())
        == Some("AllSpaces")
}

/// `Some(assigned)` from ~/Library/Preferences/com.apple.spaces.plist, `None`
/// when it cannot be read.
pub(crate) fn kakaotalk_spaces_assignment() -> Option<bool> {
    let path = dirs::home_dir()?.join("Library/Preferences/com.apple.spaces.plist");
    plist::Value::from_file(path)
        .ok()
        .map(|spaces| kakaotalk_on_all_spaces(&spaces))
}

/// Which message bubble `local-reply` / `local-edit` act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BubbleTarget {
    /// The newest text bubble.
    Last,
    /// The one text bubble whose whole text is exactly this.
    Text(String),
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum BubblePick {
    Found(usize),
    NotFound,
    Ambiguous(usize),
}

/// Pick the target among text bubbles `(text, outgoing)` in chronological
/// order. `own_only` limits it to outgoing bubbles (only those can be
/// edited). Exact whole-text matches only; two matches refuse.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pick_bubble(bubbles: &[(String, bool)], target: &BubbleTarget, own_only: bool) -> BubblePick {
    let eligible = |index: &usize| !own_only || bubbles[*index].1;
    let indices: Vec<usize> = (0..bubbles.len()).filter(eligible).collect();
    match target {
        BubbleTarget::Last => indices
            .last()
            .map_or(BubblePick::NotFound, |&index| BubblePick::Found(index)),
        BubbleTarget::Text(text) => {
            let matching: Vec<usize> = indices
                .into_iter()
                .filter(|&index| same_text(&bubbles[index].0, text))
                .collect();
            match matching.as_slice() {
                [index] => BubblePick::Found(*index),
                [] => BubblePick::NotFound,
                many => BubblePick::Ambiguous(many.len()),
            }
        }
    }
}

/// What the composer is doing, from its `AXDescription` (KakaoTalk 26.9:
/// `메시지 입력`, `답장 메시지 입력`, `수정 메시지 입력`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposerMode {
    Normal,
    Reply,
    Edit,
    Unknown,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn composer_mode(description: Option<&str>) -> ComposerMode {
    match description.map(str::trim) {
        Some(description) if description.ends_with("답장 메시지 입력") => {
            ComposerMode::Reply
        }
        Some(description) if description.ends_with("수정 메시지 입력") => ComposerMode::Edit,
        Some("메시지 입력") => ComposerMode::Normal,
        _ => ComposerMode::Unknown,
    }
}

/// The banner KakaoTalk shows above the composer in reply (`나에게 답장`,
/// `<이름>에게 답장`) or edit (`메시지 수정`) mode.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mode_banner(text: &str) -> ComposerMode {
    let text = text.trim();
    if text.ends_with("에게 답장") {
        ComposerMode::Reply
    } else if text == "메시지 수정" {
        ComposerMode::Edit
    } else {
        ComposerMode::Unknown
    }
}

/// An edit is proven when an outgoing bubble now reads `new` and is marked
/// `수정됨`, and one fewer outgoing bubble reads `old` than before.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn edit_verified(
    before: &[(String, bool, bool)],
    after: &[(String, bool, bool)],
    old: &str,
    new: &str,
) -> bool {
    let outgoing_with = |rows: &[(String, bool, bool)], text: &str| {
        rows.iter()
            .filter(|(body, outgoing, _)| *outgoing && same_text(body, text))
            .count()
    };
    let edited_now = after
        .iter()
        .any(|(body, outgoing, edited)| *outgoing && *edited && bubble_shows(body, new));
    edited_now && outgoing_with(after, old) + 1 == outgoing_with(before, old)
}

/// What `local-delete` removes, named as KakaoTalk 26.8 names the bubble-menu
/// item. KakaoTalk itself decides which of the two it offers: 모든 대화
/// 상대에게서 삭제 only for your own messages within 24 hours of sending
/// (KakaoTalk 25.7+, 5 minutes before) and never in the memo chat, so the menu,
/// not a copy of that policy, is what decides (measured 2026-11-07).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeleteScope {
    Everyone,
    Me,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl DeleteScope {
    pub(crate) fn menu_item(self) -> &'static str {
        match self {
            Self::Everyone => "모든 대화 상대에게서 삭제",
            Self::Me => "나에게서만 삭제",
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::Me => "me",
        }
    }
}

/// The 삭제 button of KakaoTalk's delete confirmation ("나에게서만 삭제 ·
/// 현재 사용중인 기기에서만 삭제됩니다." 취소/삭제), only when the sheet is
/// headed by exactly the requested scope and has one enabled 삭제.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pick_delete_confirm(
    texts: &[String],
    buttons: &[SheetButton],
    scope: DeleteScope,
) -> Option<usize> {
    if !texts.iter().any(|text| text.trim() == scope.menu_item()) {
        return None;
    }
    let deletes: Vec<usize> = buttons
        .iter()
        .enumerate()
        .filter(|(_, button)| {
            button.enabled && button.title.as_deref().map(str::trim) == Some("삭제")
        })
        .map(|(index, _)| index)
        .collect();
    match deletes.as_slice() {
        [index] => Some(*index),
        _ => None,
    }
}

/// A delete took effect when the newest rows hold exactly one fewer bubble
/// with the target's text and direction: removed (나에게서만) or replaced by
/// KakaoTalk's deletion notice (모든 대화 상대에게서). If an older identical
/// bubble enters the observed tail the counts tie, which reads as uncertain,
/// never as a false success.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn delete_verified(
    before: &[(String, bool)],
    after: &[(String, bool)],
    text: &str,
    outgoing: bool,
) -> bool {
    let count = |rows: &[(String, bool)]| {
        rows.iter()
            .filter(|(body, from_me)| *from_me == outgoing && same_text(body, text))
            .count()
    };
    count(after) + 1 == count(before)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxCheckStatus {
    Ok,
    Warn,
    Fail,
}

/// One read-only AX readiness check for `doctor` and `init`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone)]
pub(crate) struct AxCheck {
    pub name: &'static str,
    pub status: AxCheckStatus,
    pub detail: String,
    pub code: Option<crate::error::ErrorCode>,
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn idle_wait_proceeds_waits_and_times_out() {
        let two = Duration::from_secs(2);
        let max = Duration::from_secs(60);
        let zero = Duration::ZERO;
        assert_eq!(
            idle_decision(Some(Duration::from_secs(3)), two, zero, max),
            IdleDecision::Proceed
        );
        assert_eq!(
            idle_decision(Some(two), two, zero, max),
            IdleDecision::Proceed
        );
        assert_eq!(
            idle_decision(
                Some(Duration::from_millis(400)),
                two,
                Duration::from_secs(5),
                max
            ),
            IdleDecision::Wait
        );
        assert_eq!(
            idle_decision(Some(Duration::from_millis(400)), two, max, max),
            IdleDecision::TimedOut
        );
        // --wait-idle 0 disables the wait; an unreadable idle time fails open.
        assert_eq!(
            idle_decision(Some(zero), zero, zero, max),
            IdleDecision::Proceed
        );
        assert_eq!(idle_decision(None, two, zero, max), IdleDecision::Proceed);
    }

    #[test]
    fn parses_ioreg_hid_idle_time() {
        let output = "+-o IOHIDSystem  <class IOHIDSystem>\n    {\n      \"HIDIdleTime\" = 2500000000\n    }\n";
        assert_eq!(parse_hid_idle(output), Some(Duration::from_millis(2500)));
        assert_eq!(parse_hid_idle("no idle here"), None);
        assert_eq!(parse_hid_idle("\"HIDIdleTime\" = not-a-number"), None);
    }

    #[test]
    fn focus_is_restored_only_when_kakaotalk_took_it_from_another_app() {
        let kakao = 100;
        assert!(should_restore_focus(Some(7), Some(kakao), kakao));
        // The user moved on by themselves: leave them alone.
        assert!(!should_restore_focus(Some(7), Some(8), kakao));
        assert!(!should_restore_focus(Some(7), Some(7), kakao));
        // KakaoTalk was already in front, or the frontmost app is unknown.
        assert!(!should_restore_focus(Some(kakao), Some(kakao), kakao));
        assert!(!should_restore_focus(None, Some(kakao), kakao));
        assert!(!should_restore_focus(Some(7), None, kakao));
    }

    #[test]
    fn login_window_titles_are_recognized_exactly() {
        let texts = |all: &[&str]| all.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        assert_eq!(
            login_screen(&texts(&[
                "카카오계정 또는 비밀번호를 다시 확인해 주세요.",
                "또는"
            ])),
            LoginScreen::Rejected
        );
        assert_eq!(
            login_screen(&texts(&["인증 시간이 만료되었습니다.", "00:00"])),
            LoginScreen::Qr
        );
        assert_eq!(login_screen(&texts(&["또는"])), LoginScreen::Form);
        assert!(is_dismissable_login_sheet(&texts(&[
            "로그아웃 되었습니다.",
            "다른 기기에서 로그인해 로그아웃 되었습니다."
        ])));
        assert!(is_dismissable_login_sheet(&texts(&[
            "인증 진행을 중단하시겠습니까?"
        ])));
        assert!(!is_dismissable_login_sheet(&texts(&[
            "계정을 삭제하시겠습니까?"
        ])));
        assert!(is_login_window_title("로그인"));
        assert!(is_login_window_title(" 로그인 "));
        assert!(!is_login_window_title("카카오톡"));
        assert!(!is_login_window_title("로그인 도움말"));
    }

    #[test]
    fn frame_center_handles_displays_above_the_main_one() {
        // Observed on a real two-display setup: a chat window at (1499, -1410).
        assert_eq!(
            frame_center(1499.0, -1410.0, 380.0, 1410.0),
            Some((1689.0, -705.0))
        );
        assert_eq!(frame_center(-1920.0, 0.0, 10.0, 10.0), Some((-1915.0, 5.0)));
        assert_eq!(frame_center(0.0, 0.0, 0.0, 10.0), None);
        assert_eq!(frame_center(f64::NAN, 0.0, 10.0, 10.0), None);
    }

    #[test]
    fn spaces_binding_is_read_case_insensitively() {
        let mut bindings = plist::Dictionary::new();
        bindings.insert("com.kakao.kakaotalkmac".into(), "AllSpaces".into());
        let mut root = plist::Dictionary::new();
        root.insert(
            "app-bindings".into(),
            plist::Value::Dictionary(bindings.clone()),
        );
        assert!(kakaotalk_on_all_spaces(&plist::Value::Dictionary(
            root.clone()
        )));

        let mut other = plist::Dictionary::new();
        other.insert("com.kakao.KakaoTalkMac".into(), "AllSpaces".into());
        root.insert("app-bindings".into(), plist::Value::Dictionary(other));
        assert!(kakaotalk_on_all_spaces(&plist::Value::Dictionary(
            root.clone()
        )));

        // Bound to one specific desktop, or not bound at all.
        bindings.insert("com.kakao.kakaotalkmac".into(), "some-space-uuid".into());
        root.insert("app-bindings".into(), plist::Value::Dictionary(bindings));
        assert!(!kakaotalk_on_all_spaces(&plist::Value::Dictionary(root)));
        assert!(!kakaotalk_on_all_spaces(&plist::Value::Dictionary(
            plist::Dictionary::new()
        )));
    }

    fn bubbles(list: &[(&str, bool)]) -> Vec<(String, bool)> {
        list.iter()
            .map(|(text, out)| (text.to_string(), *out))
            .collect()
    }

    #[test]
    fn bubble_targets_are_exact_and_refuse_ambiguity() {
        let chat = bubbles(&[
            ("hi", false),
            ("같은 말", true),
            ("ok", true),
            ("같은 말", false),
        ]);
        assert_eq!(
            pick_bubble(&chat, &BubbleTarget::Last, false),
            BubblePick::Found(3)
        );
        assert_eq!(
            pick_bubble(&chat, &BubbleTarget::Last, true),
            BubblePick::Found(2)
        );
        assert_eq!(
            pick_bubble(&chat, &BubbleTarget::Text("ok".into()), true),
            BubblePick::Found(2)
        );
        assert_eq!(
            pick_bubble(&chat, &BubbleTarget::Text("같은 말".into()), false),
            BubblePick::Ambiguous(2)
        );
        // Only own bubbles can be edited: the incoming duplicate is ignored.
        assert_eq!(
            pick_bubble(&chat, &BubbleTarget::Text("같은 말".into()), true),
            BubblePick::Found(1)
        );
        assert_eq!(
            pick_bubble(&chat, &BubbleTarget::Text("o".into()), false),
            BubblePick::NotFound
        );
        assert_eq!(
            pick_bubble(&[], &BubbleTarget::Last, false),
            BubblePick::NotFound
        );
    }

    #[test]
    fn composer_modes_and_banners_follow_the_measured_labels() {
        assert_eq!(composer_mode(Some("메시지 입력")), ComposerMode::Normal);
        assert_eq!(composer_mode(Some("답장 메시지 입력")), ComposerMode::Reply);
        assert_eq!(composer_mode(Some("수정 메시지 입력")), ComposerMode::Edit);
        assert_eq!(composer_mode(None), ComposerMode::Unknown);
        assert_eq!(composer_mode(Some("Message")), ComposerMode::Unknown);
        assert_eq!(mode_banner("나에게 답장"), ComposerMode::Reply);
        assert_eq!(mode_banner("Pearl 진주에게 답장"), ComposerMode::Reply);
        assert_eq!(mode_banner("메시지 수정"), ComposerMode::Edit);
        assert_eq!(mode_banner("오후 3:14"), ComposerMode::Unknown);
    }

    #[test]
    fn delete_confirmation_must_name_the_requested_scope() {
        let texts = |heading: &str| {
            vec![
                heading.to_string(),
                "현재 사용중인 기기에서만 삭제됩니다.".to_string(),
            ]
        };
        let buttons = [
            button("취소", true, false, true),
            button("삭제", true, true, false),
        ];
        assert_eq!(
            pick_delete_confirm(&texts("나에게서만 삭제"), &buttons, DeleteScope::Me),
            Some(1)
        );
        // A "for me" sheet never confirms a "for everyone" request, and back.
        assert_eq!(
            pick_delete_confirm(&texts("나에게서만 삭제"), &buttons, DeleteScope::Everyone),
            None
        );
        assert_eq!(
            pick_delete_confirm(
                &texts("모든 대화 상대에게서 삭제"),
                &buttons,
                DeleteScope::Me
            ),
            None
        );
        assert_eq!(
            pick_delete_confirm(
                &texts("모든 대화 상대에게서 삭제"),
                &buttons,
                DeleteScope::Everyone
            ),
            Some(1)
        );
        // No enabled 삭제, or two of them: refuse.
        let disabled = [
            button("취소", true, false, true),
            button("삭제", false, true, false),
        ];
        assert_eq!(
            pick_delete_confirm(&texts("나에게서만 삭제"), &disabled, DeleteScope::Me),
            None
        );
        let two = [
            button("삭제", true, false, false),
            button("삭제", true, true, false),
        ];
        assert_eq!(
            pick_delete_confirm(&texts("나에게서만 삭제"), &two, DeleteScope::Me),
            None
        );
    }

    #[test]
    fn a_delete_needs_exactly_one_fewer_matching_bubble() {
        let row = |text: &str, outgoing: bool| (text.to_string(), outgoing);
        let before = vec![
            row("지울 말", true),
            row("지울 말", false),
            row("다른 말", true),
        ];
        let removed = vec![row("지울 말", false), row("다른 말", true)];
        assert!(delete_verified(&before, &removed, "지울 말", true));
        let notice = vec![
            row("메시지가 삭제되었습니다.", true),
            row("지울 말", false),
            row("다른 말", true),
        ];
        assert!(delete_verified(&before, &notice, "지울 말", true));
        // Nothing changed, or only the incoming copy went.
        assert!(!delete_verified(&before, &before, "지울 말", true));
        let wrong = vec![row("지울 말", true), row("다른 말", true)];
        assert!(!delete_verified(&before, &wrong, "지울 말", true));
    }

    #[test]
    fn an_edit_needs_the_new_text_marked_edited_and_the_old_text_gone() {
        let row = |text: &str, outgoing: bool, edited: bool| (text.to_string(), outgoing, edited);
        let before = vec![row("오타 있음", true, false), row("다른 말", false, false)];
        let edited = vec![row("오타 고침", true, true), row("다른 말", false, false)];
        assert!(edit_verified(&before, &edited, "오타 있음", "오타 고침"));
        // Not marked 수정됨: a new message, not an edit.
        let sent = vec![row("오타 있음", true, false), row("오타 고침", true, false)];
        assert!(!edit_verified(&before, &sent, "오타 있음", "오타 고침"));
        // The old bubble is still there.
        let both = vec![row("오타 있음", true, false), row("오타 고침", true, true)];
        assert!(!edit_verified(&before, &both, "오타 있음", "오타 고침"));
    }

    #[test]
    fn default_policy_is_focus_free() {
        let policy = AxPolicy::default();
        assert!(!policy.open_if_needed);
        assert!(!policy.fallback_keys);
        assert_eq!(policy.wait_idle, DEFAULT_WAIT_IDLE);
        assert!(MAX_IDLE_WAIT > DEFAULT_WAIT_IDLE);
    }

    fn row(placeholder: Option<&str>, outgoing: bool, mentions: bool) -> AttachmentObservation {
        AttachmentObservation {
            placeholder: placeholder.map(str::to_string),
            outgoing,
            mentions_filename: mentions,
        }
    }

    #[test]
    fn upload_progress_labels_are_recognized() {
        for label in ["업로드 중", "업로드 중...", "업로드 중…", "Uploading…"] {
            assert!(is_uploading_label(label), "{label}");
        }
        for text in ["업로드", "openkakao-큰파일-30MB.zip", "용량: 30.0MB", ""] {
            assert!(!is_uploading_label(text), "{text}");
        }
    }

    #[test]
    fn clock_labels_are_recognized() {
        for label in [
            "오후 5:18",
            "오전 11:02",
            "14:32",
            "2:05 PM",
            "AM 9:00",
            " 9:41 ",
            "2\n오후 4:07",
        ] {
            assert!(is_clock_label(label), "{label}");
        }
        assert_eq!(clock_of("2\n오후 4:07"), Some("오후 4:07"));
        for text in [
            "openkakao-시험.txt",
            "용량: 42.0bytes",
            "유효기간: ~2026. 11. 20.",
            "2026. 11. 6.",
            "1:2",
            "123:45",
            "",
            "친구\n오후 4:07",
        ] {
            assert!(!is_clock_label(text), "{text}");
        }
    }

    #[test]
    fn file_delivery_needs_a_new_outgoing_attachment_bubble() {
        let before = vec![row(None, false, false), row(Some("[파일]"), true, true)];
        let mut appended = before.clone();
        appended.push(row(Some("[파일]"), true, false));
        assert_eq!(
            new_attachment_evidence(&before, &appended, AttachmentKind::File),
            Some("[파일]")
        );

        let mut named = before.clone();
        named.push(row(Some("[사진]"), true, true));
        assert_eq!(
            new_attachment_evidence(&before, &named, AttachmentKind::File),
            Some("file_name")
        );

        // An unchanged list, an incoming file, or a new text bubble prove nothing.
        assert_eq!(
            new_attachment_evidence(&before, &before, AttachmentKind::File),
            None
        );
        let mut incoming = before.clone();
        incoming.push(row(Some("[파일]"), false, true));
        assert_eq!(
            new_attachment_evidence(&before, &incoming, AttachmentKind::File),
            None
        );
        let mut text = before.clone();
        text.push(row(None, true, false));
        assert_eq!(
            new_attachment_evidence(&before, &text, AttachmentKind::File),
            None
        );
    }

    #[test]
    fn a_baseline_row_leaving_the_observed_tail_never_fakes_success() {
        // Observing only the last rows: the new bubble pushes an identical old
        // one out of view, so the count stays equal -> uncertain, not verified.
        let before = vec![row(Some("[파일]"), true, false), row(None, false, false)];
        let after = vec![row(None, false, false), row(Some("[파일]"), true, false)];
        assert_eq!(
            new_attachment_evidence(&before, &after, AttachmentKind::File),
            None
        );
    }

    #[test]
    fn photo_delivery_keeps_the_strict_photo_bubble_contract() {
        let before = vec![row(None, false, false)];
        let mut file = before.clone();
        file.push(row(Some("[파일]"), true, false));
        assert_eq!(
            new_attachment_evidence(&before, &file, AttachmentKind::Photo),
            None
        );
        // A picked image that KakaoTalk shows as a named file bubble.
        let mut named = before.clone();
        named.push(row(Some("[파일]"), true, true));
        assert_eq!(
            new_attachment_evidence(&before, &named, AttachmentKind::Photo),
            Some("file_name")
        );
        let mut photo = before.clone();
        photo.push(row(Some("[사진]"), true, false));
        assert_eq!(
            new_attachment_evidence(&before, &photo, AttachmentKind::Photo),
            Some("[사진]")
        );
    }

    fn button(title: &str, enabled: bool, is_default: bool, is_cancel: bool) -> SheetButton {
        SheetButton {
            title: Some(title.to_string()),
            enabled,
            is_default,
            is_cancel,
        }
    }

    #[test]
    fn cancel_and_open_titles_are_localized() {
        assert!(is_cancel_title(Some("취소")));
        assert!(is_cancel_title(Some("Cancel")));
        assert!(!is_cancel_title(Some("열기")));
        assert!(!is_cancel_title(None));
        assert!(OPEN_TITLES.contains(&"열기") && OPEN_TITLES.contains(&"Open"));
    }

    #[test]
    fn preview_send_titles_carry_the_attachment_count() {
        assert_eq!(preview_send_count("1개 전송"), Some(1));
        assert_eq!(preview_send_count(" 2개 전송 "), Some(2));
        assert_eq!(preview_send_count("Send 1 files"), Some(1));
        assert_eq!(preview_send_count("Send 1 file"), Some(1));
        assert_eq!(preview_send_count("전송"), None);
        assert_eq!(preview_send_count("Send"), None);
    }

    #[test]
    fn preview_send_button_is_found_without_a_hard_coded_localized_title() {
        // Korean KakaoTalk 26.9 (measured) and English (#57).
        let korean = [
            button("취소", true, false, true),
            button("1개 전송", true, false, false),
        ];
        assert_eq!(pick_preview_send_button(&korean), Some(1));
        let english = [
            button("Cancel", true, false, true),
            button("Send 1 files", true, false, false),
        ];
        assert_eq!(pick_preview_send_button(&english), Some(1));
        // Unknown title, but the sheet marks it as its default button.
        let default = [
            button("취소", true, false, true),
            button("원본 보기", true, false, false),
            button("파일 1개 보내기", true, true, false),
        ];
        assert_eq!(pick_preview_send_button(&default), Some(2));
        // No count and no default: a unique send-like title decides.
        let titled = [
            button("취소", true, false, true),
            button("편집", true, false, false),
            button("전송", true, false, false),
        ];
        assert_eq!(pick_preview_send_button(&titled), Some(2));
    }

    #[test]
    fn only_a_lone_ok_alert_counts_as_acknowledgement() {
        assert!(is_acknowledgement_only(&[button(
            "확인", true, true, false
        )]));
        assert!(is_acknowledgement_only(&[button("OK", true, false, false)]));
        // "전송을 취소하시겠습니까?" asks a question: never answer it.
        assert!(!is_acknowledgement_only(&[
            button("취소", true, false, true),
            button("확인", true, true, false),
        ]));
        assert!(!is_acknowledgement_only(&[button(
            "확인", false, true, false
        )]));
        assert!(!is_acknowledgement_only(&[button(
            "1개 전송",
            true,
            true,
            false
        )]));
        assert!(!is_acknowledgement_only(&[]));
    }

    #[test]
    fn preview_send_button_refuses_alerts_panels_counts_and_ambiguity() {
        // An error alert has no Cancel: never press its OK.
        assert_eq!(
            pick_preview_send_button(&[button("확인", true, true, false)]),
            None
        );
        // The open panel itself (its 열기 is the default button) is not a preview.
        let panel = [
            button("취소", true, false, true),
            button("열기", true, true, false),
            button("뒤로", true, false, false),
        ];
        assert_eq!(pick_preview_send_button(&panel), None);
        // More than this one file is staged.
        let two = [
            button("취소", true, false, true),
            button("2개 전송", true, true, false),
        ];
        assert_eq!(pick_preview_send_button(&two), None);
        // Two send-like buttons and no default: refuse to guess.
        let ambiguous = [
            button("취소", true, false, true),
            button("전송", true, false, false),
            button("원본으로 전송", true, false, false),
        ];
        assert_eq!(pick_preview_send_button(&ambiguous), None);
        // An alert with 취소 and a default 확인 is not a preview.
        let alert = [
            button("취소", true, false, true),
            button("확인", true, true, false),
        ];
        assert_eq!(pick_preview_send_button(&alert), None);
        // A lone unlabeled extra button is not enough.
        let lone = [
            button("취소", true, false, true),
            button("확인", true, false, false),
        ];
        assert_eq!(pick_preview_send_button(&lone), None);
        // The send button is still disabled.
        let disabled = [
            button("취소", true, false, true),
            button("1개 전송", false, true, false),
        ];
        assert_eq!(pick_preview_send_button(&disabled), None);
    }

    #[test]
    fn only_top_level_standard_folders_use_the_sidebar() {
        use std::path::Path;
        let (desktop, documents, downloads) = (
            Path::new("/Users/me/Desktop"),
            Path::new("/Users/me/Documents"),
            Path::new("/Users/me/Downloads"),
        );
        let folder = |parent: &str| picker_folder(Path::new(parent), desktop, documents, downloads);
        assert_eq!(folder("/Users/me/Downloads"), Some(PickerFolder::Downloads));
        assert_eq!(folder("/Users/me/Desktop"), Some(PickerFolder::Desktop));
        assert_eq!(folder("/Users/me/Documents"), Some(PickerFolder::Documents));
        // A subfolder is not one sidebar click away: it is staged instead.
        assert_eq!(folder("/Users/me/Downloads/reports"), None);
        assert_eq!(folder("/tmp"), None);
        assert!(PickerFolder::Desktop.labels().contains(&"데스크탑"));
        assert!(PickerFolder::Downloads.labels().contains(&"다운로드"));
    }

    #[test]
    fn list_rows_prefer_full_names_and_refuse_unsafe_stem_matches() {
        let names = |list: &[Option<&str>]| -> Vec<Option<String>> {
            list.iter().map(|name| name.map(str::to_string)).collect()
        };
        let file = "보고서 v2.docx";
        assert_eq!(
            pick_list_row(&names(&[Some("a.txt"), Some("보고서 v2.docx")]), file),
            ListPick::Exact(1)
        );
        // Extensions hidden: the stem matches, but only when unique...
        assert_eq!(
            pick_list_row(&names(&[Some("a"), Some("보고서 v2")]), file),
            ListPick::Stem(1)
        );
        assert_eq!(
            pick_list_row(&names(&[Some("보고서 v2"), Some("보고서 v2")]), file),
            ListPick::Ambiguous(2)
        );
        // ...and never while a row is unreadable: it could be this file and
        // the stem row another one (e.g. 보고서 v2.pdf).
        assert_eq!(
            pick_list_row(&names(&[None, Some("보고서 v2")]), file),
            ListPick::NotFound
        );
        // A full-name match is still fine next to unreadable rows.
        assert_eq!(
            pick_list_row(&names(&[None, Some("보고서 v2.docx")]), file),
            ListPick::Exact(1)
        );
        assert_eq!(
            pick_list_row(&names(&[Some("보고서"), Some("v2.docx")]), file),
            ListPick::NotFound
        );
        assert_eq!(
            pick_list_row(&names(&[Some("README")]), "README"),
            ListPick::Exact(0)
        );
    }

    #[test]
    fn pick_chooses_among_exact_duplicates_only() {
        let names = [
            Some("귀몬❤️".to_string()),
            Some("귀몬".to_string()),
            None,
            Some("귀몬❤️".to_string()),
        ];
        assert_eq!(picked_row(&names, "귀몬❤️", Some(1)), Some(0));
        assert_eq!(picked_row(&names, "귀몬❤️", Some(2)), Some(3));
        assert_eq!(picked_row(&names, "귀몬❤️", Some(3)), None);
        assert_eq!(picked_row(&names, "귀몬❤️", Some(0)), None);
        assert_eq!(picked_row(&names, "귀몬❤️", None), None);
    }

    #[test]
    fn only_an_exact_allowlisted_name_skips_authentication() {
        let allowed = vec!["허용된 방".to_string()];
        assert!(is_allowlisted(&allowed, "허용된 방"));
        assert!(!is_allowlisted(&allowed, "허용된"));
        assert!(!is_allowlisted(&allowed, "허용된 방 "));
        assert!(!is_allowlisted(&[], "허용된 방"));
        // No allowlist loaded: every target authenticates.
        assert!(needs_device_owner_auth("허용된 방"));
    }

    #[test]
    fn a_pick_among_duplicates_always_authenticates() {
        assert_eq!(auth_reason(true, None), None);
        assert!(auth_reason(false, None).unwrap().contains("NOT in"));
        // Allowlisted by name, yet the name covers several chats.
        assert!(auth_reason(true, Some("오후 5:30 · 📌 pinned"))
            .unwrap()
            .contains("📌 pinned"));
    }

    #[test]
    fn appended_rows_are_found_by_identity() {
        let same = |a: &u32, b: &u32| a == b;
        assert_eq!(
            appended_rows_start(Some(&7), &[5, 6, 7, 8, 9], same),
            Some(3)
        );
        assert_eq!(appended_rows_start(Some(&7), &[7], same), Some(1));
        assert_eq!(appended_rows_start(None, &[1, 2], same), Some(0));
        // The anchor scrolled out of the observed tail.
        assert_eq!(appended_rows_start(Some(&7), &[8, 9], same), None);
    }

    #[test]
    fn only_appended_rows_prove_an_attachment_send() {
        let kind = AttachmentKind::File;
        let old_file = row(Some("[파일]"), true, false);
        let before = vec![row(None, false, false), old_file.clone()];
        // An older row re-classified later (a late share button) sits before
        // the anchor: it does not verify anything.
        let reclassified = vec![row(Some("[파일]"), true, false), old_file.clone()];
        assert_eq!(
            attachment_delivery_evidence((&before, 20), (&reclassified, 20), Some(2), kind),
            None
        );
        // A new outgoing file bubble after the anchor does.
        let mut sent = before.clone();
        sent.push(row(Some("[파일]"), true, false));
        assert_eq!(
            attachment_delivery_evidence((&before, 20), (&sent, 21), Some(2), kind),
            Some("[파일]")
        );
        let mut named = before.clone();
        named.push(row(None, true, true));
        assert_eq!(
            attachment_delivery_evidence((&before, 20), (&named, 21), Some(2), kind),
            Some("file_name")
        );
        // An incoming attachment after the anchor proves nothing.
        let mut incoming = before.clone();
        incoming.push(row(Some("[파일]"), false, true));
        assert_eq!(
            attachment_delivery_evidence((&before, 20), (&incoming, 21), Some(2), kind),
            None
        );
        // Without the anchor, counting applies only if the table gained rows.
        assert_eq!(
            attachment_delivery_evidence((&before, 20), (&reclassified, 20), None, kind),
            None
        );
        assert_eq!(
            attachment_delivery_evidence((&before, 20), (&sent, 21), None, kind),
            Some("[파일]")
        );
    }

    #[test]
    fn clipboard_staging_keeps_photos_to_jpeg_and_png_but_allows_any_file() {
        use std::path::Path;
        assert!(AttachmentKind::Photo.clipboard_allows(Path::new("/tmp/a.JPG")));
        assert!(AttachmentKind::Photo.clipboard_allows(Path::new("/tmp/a.png")));
        assert!(!AttachmentKind::Photo.clipboard_allows(Path::new("/tmp/a.heic")));
        assert!(!AttachmentKind::Photo.clipboard_allows(Path::new("/tmp/noext")));
        assert!(AttachmentKind::File.clipboard_allows(Path::new("/tmp/report.docx")));
        assert!(AttachmentKind::File.clipboard_allows(Path::new("/tmp/noext")));
    }
}

#[cfg(test)]
mod match_tests {
    use super::*;

    #[test]
    fn empty_list_is_not_found() {
        assert_eq!(match_chat_row(&[], "Alice"), ChatMatch::NotFound);
    }

    #[test]
    fn single_exact_match_is_found() {
        let names = [Some("Alice".to_string())];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Found(0));
    }

    #[test]
    fn substring_does_not_match() {
        // "Alice" must not match a group chat named "Alice & Bob".
        let names = [Some("Alice & Bob".to_string())];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::NotFound);
    }

    #[test]
    fn exact_match_among_non_matching_rows() {
        let names = [
            Some("Alice & Bob".to_string()),
            Some("Alice".to_string()),
            Some("Carol".to_string()),
        ];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Found(1));
    }

    #[test]
    fn duplicate_names_are_ambiguous() {
        let names = [Some("Alice".to_string()), Some("Alice".to_string())];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Ambiguous(2));
    }

    #[test]
    fn unreadable_rows_are_ignored_not_matched() {
        // A row whose name AX couldn't read (None) must never match, and
        // must not affect matching of the other rows.
        let names = [None, Some("Alice".to_string()), None];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Found(1));
    }

    #[test]
    fn delivery_verification_requires_a_new_exact_message() {
        let before = vec![
            DeliveryObservation {
                text: "same message".to_string(),
                outgoing: true,
            },
            DeliveryObservation {
                text: "older".to_string(),
                outgoing: false,
            },
        ];
        let unchanged = before.clone();
        let mut appended = before.clone();
        appended.push(DeliveryObservation {
            text: "same message".to_string(),
            outgoing: true,
        });

        assert!(!has_new_exact_outgoing_message(
            &before,
            &unchanged,
            "same message"
        ));
        assert!(has_new_exact_outgoing_message(
            &before,
            &appended,
            "same message"
        ));

        let mut incoming = before.clone();
        incoming.push(DeliveryObservation {
            text: "same message".to_string(),
            outgoing: false,
        });
        assert!(!has_new_exact_outgoing_message(
            &before,
            &incoming,
            "same message"
        ));
    }

    #[test]
    fn chat_card_names_members_senders_and_last_activity() {
        assert_eq!(member_count("대화상대 7명"), Some(7));
        assert_eq!(member_count("프로필"), None);
        assert!(same_but_spacing("우리친구", "우리 친구"));
        assert!(same_but_spacing("Team A", "team a"));
        assert!(!same_but_spacing("우리친구", "우리친구"));
        assert!(!same_but_spacing("우리친구들", "우리 친구"));

        let msg = |sender: Option<&str>, outgoing, time: Option<&str>, text: &str| AxMessage {
            time: time.map(str::to_string),
            sender: sender.map(str::to_string),
            text: text.to_string(),
            outgoing,
        };
        let mut messages = vec![
            msg(None, false, None, "윗 줄에서 이어짐"),
            msg(Some("구자원"), false, None, "안녕"),
            msg(None, false, Some("2026. 11. 7. 오전 8:35"), "생일 축하해"),
            msg(None, true, None, "고마워"),
            msg(
                Some("변민지"),
                false,
                Some("2026. 11. 7. 오전 8:49"),
                "🎉\n🥳",
            ),
        ];
        carry_senders(&mut messages);
        assert_eq!(messages[0].sender, None);
        assert_eq!(messages[2].sender.as_deref(), Some("구자원"));
        assert_eq!(messages[3].sender, None);

        let view = ChatView {
            members: Some(7),
            chat_id: None,
            messages,
        };
        assert_eq!(
            chat_card("우리친구", &view, 0),
            "chat '우리친구' · 7 members · last message 2026. 11. 7. 오전 8:49 · recent senders: 변민지, 구자원"
        );
        let card = chat_card("우리친구", &view, 2);
        assert!(card.ends_with("\n  me: 고마워\n  [2026. 11. 7. 오전 8:49] 변민지: 🎉 🥳"));
    }

    #[test]
    fn a_folded_long_bubble_shows_its_message() {
        // KakaoTalk 26.8 shows the first 500 UTF-16 units and `…`.
        let sent = format!("{}💡 코스: 회룡사 → 저녁", "가".repeat(498));
        let folded = format!("{}💡…", "가".repeat(498));
        assert!(bubble_shows(&folded, &sent));
        assert!(bubble_shows(&sent, &sent));
        // A different ending, a short `…` bubble, or the folded text itself.
        assert!(!bubble_shows(
            &folded,
            &format!("{}🎵 다른 글", "가".repeat(498))
        ));
        assert!(!bubble_shows("음…", "음…ㅋㅋ"));
        assert!(!bubble_shows(
            &folded,
            &folded[..folded.len() - '…'.len_utf8()]
        ));

        let before = vec![];
        let after = vec![DeliveryObservation {
            text: folded,
            outgoing: true,
        }];
        assert!(is_verified_delivery(&before, &after, None, &sent, Some("")));
    }

    #[test]
    fn delivery_verification_also_requires_the_composer_to_clear() {
        let before = vec![DeliveryObservation {
            text: "older".to_string(),
            outgoing: false,
        }];
        let after = vec![
            before[0].clone(),
            DeliveryObservation {
                text: "new message".to_string(),
                outgoing: true,
            },
        ];

        assert!(!is_verified_delivery(
            &before,
            &after,
            None,
            "new message",
            Some("new message")
        ));
        assert!(is_verified_delivery(
            &before,
            &after,
            None,
            "new message",
            Some("")
        ));
        assert!(!is_verified_delivery(
            &before,
            &after,
            None,
            "new message",
            None
        ));
    }

    #[test]
    fn anchored_delivery_survives_a_repeat_scrolling_out_of_the_tail() {
        let ok = |outgoing| DeliveryObservation {
            text: "ok".to_string(),
            outgoing,
        };
        // A full tail of earlier "ok"s; the send pushes the oldest one out,
        // so the count of outgoing "ok" rows stays the same.
        let before = vec![ok(true), ok(true), ok(true)];
        let after = vec![ok(true), ok(true), ok(true)];
        assert!(!is_verified_delivery(&before, &after, None, "ok", Some("")));
        // The baseline's newest row was found again at index 1: one row appended.
        assert!(is_verified_delivery(
            &before,
            &after,
            Some(2),
            "ok",
            Some("")
        ));
        // Nothing appended, or only an incoming copy, proves nothing.
        assert!(!is_verified_delivery(
            &before,
            &after,
            Some(3),
            "ok",
            Some("")
        ));
        let incoming = vec![ok(true), ok(true), ok(false)];
        assert!(!is_verified_delivery(
            &before,
            &incoming,
            Some(2),
            "ok",
            Some("")
        ));
        // The composer must still be cleared.
        assert!(!is_verified_delivery(
            &before,
            &after,
            Some(2),
            "ok",
            Some("ok")
        ));
    }

    #[test]
    fn a_failed_bubble_pinned_below_new_rows_does_not_hide_a_send() {
        let row = |text: &str| DeliveryObservation {
            text: text.to_string(),
            outgoing: true,
        };
        // The baseline's newest row is a failed photo; the new text bubble is
        // inserted above it, so nothing follows the anchor.
        let before = vec![row("older"), row("[사진]")];
        let after = vec![row("older"), row("hi"), row("[사진]")];
        assert!(is_verified_delivery(
            &before,
            &after,
            Some(3),
            "hi",
            Some("")
        ));
        assert!(!is_verified_delivery(
            &before,
            &before,
            Some(2),
            "hi",
            Some("")
        ));

        let photo = |outgoing| AttachmentObservation {
            placeholder: Some("[사진]".to_string()),
            outgoing,
            mentions_filename: false,
        };
        let before = vec![photo(true)];
        let after = vec![photo(true), photo(true)];
        assert_eq!(
            attachment_delivery_evidence((&before, 1), (&after, 2), Some(2), AttachmentKind::Photo),
            Some("[사진]")
        );
        // The table did not grow: a re-read of the same rows proves nothing.
        assert_eq!(
            attachment_delivery_evidence((&before, 2), (&after, 2), Some(2), AttachmentKind::Photo),
            None
        );
    }

    #[test]
    fn photo_bubble_placeholder_verifies_only_a_new_outgoing_photo_bubble() {
        // The macOS photo-commit loop verifies with exactly this placeholder
        // (KakaoTalk renders sent images as "[사진]"), so a verified photo
        // send means one NEW outgoing placeholder bubble appeared.
        assert_eq!(PHOTO_BUBBLE_PLACEHOLDER, "[사진]");
        let before = vec![DeliveryObservation {
            text: "earlier text".to_string(),
            outgoing: false,
        }];
        let mut sent_photo = before.clone();
        sent_photo.push(DeliveryObservation {
            text: PHOTO_BUBBLE_PLACEHOLDER.to_string(),
            outgoing: true,
        });
        assert!(has_new_exact_outgoing_message(
            &before,
            &sent_photo,
            PHOTO_BUBBLE_PLACEHOLDER,
        ));

        // An incoming photo, or an outgoing text bubble, is not a photo send.
        let mut incoming_photo = before.clone();
        incoming_photo.push(DeliveryObservation {
            text: PHOTO_BUBBLE_PLACEHOLDER.to_string(),
            outgoing: false,
        });
        assert!(!has_new_exact_outgoing_message(
            &before,
            &incoming_photo,
            PHOTO_BUBBLE_PLACEHOLDER,
        ));
        let mut outgoing_text = before.clone();
        outgoing_text.push(DeliveryObservation {
            text: "not a photo bubble".to_string(),
            outgoing: true,
        });
        assert!(!has_new_exact_outgoing_message(
            &before,
            &outgoing_text,
            PHOTO_BUBBLE_PLACEHOLDER,
        ));

        // A placeholder bubble that already existed must not re-verify.
        assert!(!has_new_exact_outgoing_message(
            &sent_photo,
            &sent_photo,
            PHOTO_BUBBLE_PLACEHOLDER,
        ));
    }

    #[test]
    fn uncertain_outcome_is_a_hard_error_so_senders_never_retry() {
        assert!(outcome_to_result(AxDeliveryOutcome::Verified).is_ok());

        let reason = "photo may have been sent but no bubble appeared";
        let error = outcome_to_result(AxDeliveryOutcome::Uncertain {
            reason: reason.to_string(),
        })
        .unwrap_err();
        assert_eq!(error.to_string(), reason);
    }

    #[test]
    fn file_picker_wait_outlasts_chat_navigation_but_stays_bounded() {
        // Large originals need longer picker waits than tight UI navigation,
        // but the wait must remain bounded so a stalled picker fails closed
        // instead of hanging forever.
        assert!(FILE_PICKER_READY_TIMEOUT > OPEN_CHAT_TIMEOUT);
        assert!(FILE_PICKER_READY_TIMEOUT <= std::time::Duration::from_secs(60));
    }
}

#[cfg(target_os = "macos")]
mod imp {

    use std::collections::HashSet;
    use std::io::Read;
    use std::process::{Child, Command, Stdio};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes};
    use accessibility_sys::kAXPressAction;
    use accessibility_sys::{
        kAXValueTypeCGPoint, kAXValueTypeCGSize, AXIsProcessTrusted,
        AXUIElementCopyElementAtPosition, AXUIElementCopyMultipleAttributeValues,
        AXUIElementCreateSystemWide, AXUIElementGetPid, AXUIElementRef, AXValueGetTypeID,
        AXValueGetValue, AXValueRef,
    };
    use anyhow::{anyhow, Context, Result};
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFEqual, CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    use core_graphics::display::CGDisplay;
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, KeyCode,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::{CGPoint, CGSize};

    // Declared on the cross-platform module (see there) so their bounds and
    // exact-match text stay testable wherever the crate's CI runs.
    use super::{AxMessage, ChatView};
    use super::{FILE_PICKER_READY_TIMEOUT, OPEN_CHAT_TIMEOUT, PHOTO_BUBBLE_PLACEHOLDER};
    use crate::error::{coded, error_code, ErrorCode};

    const KAKAOTALK_BUNDLE_ID: &str = "com.kakao.KakaoTalkMac";
    const KAKAOTALK_TEAM_ID: &str = "L75WVXX68A";
    const RETURN_KEYCODE: u16 = 36;
    // Scoped delivery verification: a single already-open window's bubbles show
    // the sent text near-instantly, so this can be short (unlike the old
    // app-wide scan). Normally succeeds on the first poll.
    const VERIFY_TIMEOUT: Duration = Duration::from_secs(3);
    // An attachment bubble can lag the commit while KakaoTalk starts the
    // upload; waiting longer only turns would-be `uncertain` results into
    // verified ones, it never verifies without a new bubble.
    const ATTACHMENT_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
    const VERIFY_POLL_INTERVAL: Duration = Duration::from_millis(200);
    const COMPOSER_VERIFY_TIMEOUT: Duration = Duration::from_secs(1);
    const COMPOSER_VERIFY_POLL_INTERVAL: Duration = Duration::from_millis(50);
    /// How long an unchanged open-panel list is given to reload after a
    /// sidebar click when no path popup can confirm the folder.
    const LIST_RELOAD_SETTLE: Duration = Duration::from_millis(1500);
    /// How long a verified send waits for KakaoTalk to finish uploading before
    /// a staged copy in ~/Downloads is removed (it is kept on timeout).
    const UPLOAD_FINISH_TIMEOUT: Duration = Duration::from_secs(300);
    const SNAPSHOT_MAX_DEPTH: usize = 128;
    const SNAPSHOT_MAX_NODES: usize = 20_000;
    // A large chat list can exceed five seconds on newer KakaoTalk/macOS builds.
    // Keep a firm deadline while allowing the normal AX walk to finish.
    const SNAPSHOT_MAX_DURATION: Duration = Duration::from_secs(30);
    const AX_MESSAGE_TIMEOUT_SECS: f32 = 1.0;

    struct DisplayWakeGuard {
        child: Child,
    }

    impl Drop for DisplayWakeGuard {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn hold_display_awake() -> Result<DisplayWakeGuard> {
        let process_id = std::process::id().to_string();
        let child = Command::new("/usr/bin/caffeinate")
            .args(["-dimsu", "-w", &process_id])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("could not hold a display wake assertion for KakaoTalk automation")?;
        // Give powerd a bounded moment to publish KakaoTalk's AX windows when
        // the display was asleep but the user session remained unlocked. An
        // awake display needs no wait (it cost every file send 300 ms).
        let asleep =
            core_graphics::display::CGDisplay::active_displays().map_or(true, |displays| {
                displays.is_empty()
                    || displays
                        .iter()
                        .any(|id| core_graphics::display::CGDisplay::new(*id).is_asleep())
            });
        if asleep {
            sleep(Duration::from_millis(300));
        }
        Ok(DisplayWakeGuard { child })
    }

    /// Find the running, Kakao-signed KakaoTalk process.
    ///
    /// We shell out rather than link `NSRunningApplication`/AppKit bindings
    /// because this is the only place we need a pid lookup and it keeps the
    /// dependency surface small (matches `local_db.rs`'s existing convention of
    /// shelling out to `ioreg` for platform info).
    pub fn find_kakaotalk_pid() -> Result<i32> {
        let output = Command::new("pgrep")
            .args(["-x", "KakaoTalk"])
            .output()
            .context("failed to run pgrep")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let candidates: Vec<i32> = stdout
            .lines()
            .filter_map(|line| line.trim().parse::<i32>().ok())
            .collect();
        if candidates.is_empty() {
            return Err(coded(
                ErrorCode::KakaotalkNotRunning,
                format!("KakaoTalk is not running (`{KAKAOTALK_BUNDLE_ID}` not found)"),
            ));
        }

        let authentic: Vec<i32> = candidates
            .into_iter()
            .filter(|pid| process_has_expected_kakao_signature(*pid))
            .collect();
        match authentic.as_slice() {
            [pid] => Ok(*pid),
            [] => anyhow::bail!(
                "a process named KakaoTalk is running, but its bundle signing identity is not {KAKAOTALK_BUNDLE_ID}/{KAKAOTALK_TEAM_ID}; refusing AX access"
            ),
            _ => anyhow::bail!(
                "multiple authentic KakaoTalk processes are running; refusing to choose an AX target"
            ),
        }
    }

    fn process_has_expected_kakao_signature(pid: i32) -> bool {
        openkakao_cli::human_auth::validate_kakaotalk_process(pid).is_ok()
    }

    /// Check the calling process has been granted Accessibility permission
    /// before touching the AX tree at all. Without this, every AXUIElement
    /// call below just silently fails or returns empty results, which
    /// previously surfaced as a confusing "chat not found" error with no
    /// hint that the real cause was a missing permission grant.
    fn ensure_ax_permission() -> Result<()> {
        if unsafe { AXIsProcessTrusted() } {
            Ok(())
        } else {
            Err(coded(
                ErrorCode::AccessibilityNotGranted,
                permission_message(),
            ))
        }
    }

    fn permission_message() -> String {
        let over_ssh =
            std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
        if over_ssh {
            "Accessibility permission is not granted to this SSH session (its responsible \
             process is /usr/libexec/sshd-keygen-wrapper)"
                .to_string()
        } else {
            "Accessibility permission is not granted to the app running openkakao-cli".to_string()
        }
    }

    fn trace(started: Instant, step: &str) {
        if crate::util::debug_enabled() {
            eprintln!(
                "[ax] {:>8.1} ms  {step}",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    fn bounded_application(pid: i32) -> Result<AXUIElement> {
        let app = AXUIElement::application(pid);
        app.set_messaging_timeout(AX_MESSAGE_TIMEOUT_SECS)
            .map_err(|error| {
                anyhow!("could not set a bounded KakaoTalk AX messaging timeout: {error:?}")
            })?;
        Ok(app)
    }

    fn role(el: &AXUIElement) -> String {
        el.role().map(|s| s.to_string()).unwrap_or_default()
    }

    /// Read a string attribute by raw name (works for attributes with no typed
    /// accessor in the `accessibility` crate, e.g. `AXIdentifier`).
    fn attr_as_string(el: &AXUIElement, name: &str) -> Option<String> {
        let attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new(name));
        el.attribute(&attr)
            .ok()
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
    }

    fn attr_as_bool(el: &AXUIElement, name: &str) -> Option<bool> {
        let attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new(name));
        el.attribute(&attr)
            .ok()
            .and_then(|value| value.downcast::<CFBoolean>())
            .map(bool::from)
    }

    fn attr_as_i64(el: &AXUIElement, name: &str) -> Option<i64> {
        let attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new(name));
        el.attribute(&attr)
            .ok()
            .and_then(|value| value.downcast::<core_foundation::number::CFNumber>())
            .and_then(|number| number.to_i64())
    }

    fn attr_as_element(el: &AXUIElement, name: &str) -> Option<AXUIElement> {
        let attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new(name));
        el.attribute(&attr)
            .ok()
            .and_then(|value| value.downcast::<AXUIElement>())
    }

    fn focus_and_verify(element: &AXUIElement, label: &str) -> Result<()> {
        let focused_attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXFocused"));
        if !element
            .is_settable(&focused_attr)
            .map_err(|error| anyhow!("could not inspect {label} focus support: {error:?}"))?
        {
            anyhow::bail!("{label} is not AX-focusable; refusing synthetic key delivery");
        }
        element
            .set_attribute(&focused_attr, CFBoolean::true_value().as_CFType())
            .map_err(|error| anyhow!("could not focus {label}: {error:?}"))?;

        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        loop {
            if attr_as_bool(element, "AXFocused") == Some(true) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                anyhow::bail!("could not verify focus on {label}; refusing synthetic key delivery");
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }
    }

    fn focus_window_and_verify(
        app: &AXUIElement,
        window: &AXUIElement,
        focus: &mut FocusGuard,
    ) -> Result<()> {
        focus.before_raise()?;
        let frontmost_attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXFrontmost"));
        app.set_attribute(&frontmost_attr, CFBoolean::true_value().as_CFType())
            .map_err(|error| anyhow!("could not make KakaoTalk frontmost: {error:?}"))?;
        window
            .perform_action(&CFString::new("AXRaise"))
            .map_err(|error| anyhow!("could not raise exact target window: {error:?}"))?;

        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        loop {
            let focused_matches = attr_as_element(app, "AXFocusedWindow").is_some_and(|focused| {
                let is_window =
                    unsafe { CFEqual(focused.as_CFTypeRef(), window.as_CFTypeRef()) != 0 };
                let mut current = focused.clone();
                let mut descends_from_window = false;
                for _ in 0..6 {
                    if unsafe { CFEqual(current.as_CFTypeRef(), window.as_CFTypeRef()) != 0 } {
                        descends_from_window = true;
                        break;
                    }
                    let Some(parent) = attr_as_element(&current, "AXParent") else {
                        break;
                    };
                    current = parent;
                }
                let is_attached_sheet = role(&focused) == "AXSheet" && descends_from_window;
                is_window || is_attached_sheet
            });
            if attr_as_bool(app, "AXFrontmost") == Some(true) && focused_matches {
                return Ok(());
            }
            if Instant::now() >= deadline {
                anyhow::bail!("could not verify exact KakaoTalk target window focus");
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }
    }

    /// Find KakaoTalk's main chat-list window, as opposed to any individual
    /// open-chat windows (which are separate `AXWindow`s titled with the
    /// other party's — or your own, for the self chat — display name).
    ///
    /// The Accessibility API only sees windows on the active Space, and
    /// restoring a minimized or off-Space window automatically risks stealing
    /// the user's foreground, so both cases are reported (with a fix) instead.
    /// A signed-out KakaoTalk shows one `로그인` window that reuses the main
    /// window's identifier.
    fn find_main_window(app: &AXUIElement) -> Result<AXUIElement> {
        let windows = app
            .windows()
            .map_err(|e| anyhow!("AXWindows read failed: {e:?}"))?;
        let title_of = |window: &AXUIElement| window.title().map(|t| t.to_string()).ok();
        let signed_out = || {
            coded(
                ErrorCode::KakaotalkSignedOut,
                "KakaoTalk is signed out (its only window is the 로그인 sign-in window)",
            )
        };
        let Some(window) = windows
            .iter()
            .find(|w| attr_as_string(w, "AXIdentifier").as_deref() == Some("Main Window"))
            .map(|w| w.clone())
        else {
            if windows
                .iter()
                .any(|w| title_of(&w).is_some_and(|t| super::is_login_window_title(&t)))
            {
                return Err(signed_out());
            }
            return Err(coded(
                ErrorCode::MainWindowNotVisible,
                "could not find KakaoTalk's main chat-list window (closed, or on another Space)",
            ));
        };
        if title_of(&window).is_some_and(|t| super::is_login_window_title(&t)) {
            return Err(signed_out());
        }

        // A minimized window still shows up here (unlike one on another
        // Space), but restoring it via AXMinimized=false was observed to bring
        // KakaoTalk to the foreground, so it is deliberately not restored.
        if attr_as_bool(&window, "AXMinimized") == Some(true) {
            return Err(coded(
                ErrorCode::MainWindowMinimized,
                "KakaoTalk's main chat-list window is minimized",
            ));
        }

        Ok(window)
    }

    /// A single recursive snapshot of an AX subtree, capturing each node's
    /// role, text attributes and frame once so later lookups (`find_first`,
    /// `find_all`) run entirely in memory instead of re-walking the tree via
    /// AX's cross-process IPC on every call. Building this snapshot costs
    /// roughly the same as one `find_descendants_by_role` call; the win is
    /// not calling `find_descendants_by_role` dozens of times against
    /// overlapping subtrees, which is what made opening a chat row take ~9s
    /// against an 84-row chat list before this change.
    struct AxNode {
        element: AXUIElement,
        role: String,
        value: Option<String>,
        help: Option<String>,
        description: Option<String>,
        title: Option<String>,
        position_x: Option<f64>,
        position_y: Option<f64>,
        width: Option<f64>,
        height: Option<f64>,
        children: Vec<AxNode>,
    }

    struct SnapshotBudget {
        started_at: Instant,
        nodes: usize,
        visited: HashSet<usize>,
        /// Every admitted element, kept alive for the walk. `visited` compares
        /// object addresses, and a fresh `AXChildren` copy reuses the address
        /// of one that was already dropped: without this, every open-panel
        /// row after the first read as "already visited" and lost its label.
        retained: Vec<AXUIElement>,
        exhausted: bool,
    }

    impl SnapshotBudget {
        fn new() -> Self {
            Self {
                started_at: Instant::now(),
                nodes: 0,
                visited: HashSet::new(),
                retained: Vec::new(),
                exhausted: false,
            }
        }

        /// Count `element` against the budget; `false` means skip it (already
        /// visited, or the budget ran out, which also marks it exhausted).
        fn admit(&mut self, element: &AXUIElement, depth: usize) -> bool {
            if !self.visited.insert(element.as_concrete_TypeRef() as usize) {
                return false;
            }
            self.retained.push(element.clone());
            if depth > SNAPSHOT_MAX_DEPTH
                || self.nodes >= SNAPSHOT_MAX_NODES
                || self.started_at.elapsed() >= SNAPSHOT_MAX_DURATION
            {
                self.exhausted = true;
                return false;
            }
            self.nodes += 1;
            true
        }

        fn check(&self) -> Result<()> {
            if self.exhausted {
                anyhow::bail!(
                    "KakaoTalk AX snapshot exceeded its safety budget ({} nodes in {:.1}s; limits: depth {}, nodes {}, time {}s); refusing incomplete UI data. Close extra KakaoTalk windows or reduce the visible chat list and retry",
                    self.nodes,
                    self.started_at.elapsed().as_secs_f64(),
                    SNAPSHOT_MAX_DEPTH,
                    SNAPSHOT_MAX_NODES,
                    SNAPSHOT_MAX_DURATION.as_secs()
                );
            }
            Ok(())
        }
    }

    /// One `AXUIElementCopyMultipleAttributeValues` round-trip. Missing
    /// attributes come back as error placeholders in the same call
    /// (`options = 0`), so they cost nothing extra and fail their downcast.
    fn copy_attributes(element: &AXUIElement, names: &[&str]) -> Option<CFArray<CFType>> {
        let names = CFArray::from_CFTypes(
            &names
                .iter()
                .map(|name| CFString::new(name).as_CFType())
                .collect::<Vec<_>>(),
        );
        let mut values_ref: CFArrayRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyMultipleAttributeValues(
                element.as_concrete_TypeRef(),
                names.as_concrete_TypeRef(),
                0,
                &mut values_ref,
            )
        };
        (err == 0 && !values_ref.is_null())
            .then(|| unsafe { CFArray::<CFType>::wrap_under_create_rule(values_ref) })
    }

    fn string_in(values: &CFArray<CFType>, index: isize) -> Option<String> {
        values
            .get(index)
            .and_then(|value| value.downcast::<CFString>())
            .map(|value| value.to_string())
    }

    /// Wrap an `AXChildren` value under the get rule (retain), the same +1
    /// semantics the typed `.children()` accessor gives. `ConcreteCFType` is
    /// only implemented for the untyped `CFArray<*const c_void>`.
    fn children_in(values: &CFArray<CFType>, index: isize) -> Vec<AXUIElement> {
        values
            .get(index)
            .and_then(|value| value.downcast::<CFArray<*const std::ffi::c_void>>())
            .map(|children| {
                children
                    .iter()
                    .map(|child| unsafe {
                        AXUIElement::wrap_under_get_rule(*child as AXUIElementRef)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A single recursive snapshot of an AX subtree, fetching each node's
    /// attributes in one batched IPC call so later lookups (`find_first`,
    /// `find_all`) run in memory instead of re-walking KakaoTalk over IPC.
    fn snapshot(root: &AXUIElement) -> Result<AxNode> {
        snapshot_with(root, false)
    }

    /// Like [`snapshot`] but without walking into `AXTable`s or `AXOutline`s:
    /// each list node (role, frame, description) is kept, its rows are not.
    /// The chat list, a chat's message history and the open panel's file list
    /// hold nearly all of their window's nodes, so window-level lookups
    /// (composer, buttons, sheets, search field) stay cheap; rows are read
    /// separately and only as far as needed.
    fn snapshot_shallow(root: &AXUIElement) -> Result<AxNode> {
        let started = Instant::now();
        let node = snapshot_with(root, true)?;
        if crate::util::debug_enabled() {
            eprintln!(
                "[ax] shallow snapshot of {}: {} nodes in {:.1} ms",
                node.role,
                node.count(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        Ok(node)
    }

    fn snapshot_with(root: &AXUIElement, prune_tables: bool) -> Result<AxNode> {
        let mut budget = SnapshotBudget::new();
        let node = snapshot_bounded(root, 0, &mut budget, prune_tables);
        budget.check()?;
        Ok(node)
    }

    fn snapshot_bounded(
        root: &AXUIElement,
        depth: usize,
        budget: &mut SnapshotBudget,
        prune_tables: bool,
    ) -> AxNode {
        if !budget.admit(root, depth) {
            return AxNode::leaf(root, String::new());
        }
        // Order matters: these indices are read back positionally below.
        let Some(values) = copy_attributes(
            root,
            &[
                "AXRole",
                "AXChildren",
                "AXValue",
                "AXHelp",
                "AXDescription",
                "AXPosition",
                "AXSize",
                "AXTitle",
            ],
        ) else {
            // Rare: the batch call failed for this element. Keep a leaf with
            // just the role so one failed call doesn't drop the whole subtree.
            return AxNode::leaf(root, role(root));
        };
        if budget.started_at.elapsed() >= SNAPSHOT_MAX_DURATION {
            budget.exhausted = true;
        }

        let value_of = |index: isize, kind, out: *mut std::ffi::c_void| -> bool {
            values.get(index).is_some_and(|value| {
                value.type_of() == unsafe { AXValueGetTypeID() }
                    && unsafe { AXValueGetValue(value.as_CFTypeRef() as AXValueRef, kind, out) }
            })
        };
        let mut point = CGPoint::new(0.0, 0.0);
        let has_point = value_of(5, kAXValueTypeCGPoint, (&mut point as *mut CGPoint).cast());
        let mut size = CGSize::new(0.0, 0.0);
        let has_size = value_of(6, kAXValueTypeCGSize, (&mut size as *mut CGSize).cast());

        let node_role = string_in(&values, 0).unwrap_or_default();
        let mut children = Vec::new();
        if !(prune_tables && matches!(node_role.as_str(), "AXTable" | "AXOutline")) {
            for child in children_in(&values, 1) {
                if budget.exhausted {
                    break;
                }
                children.push(snapshot_bounded(&child, depth + 1, budget, prune_tables));
            }
        }

        AxNode {
            element: root.clone(),
            role: node_role,
            value: string_in(&values, 2),
            help: string_in(&values, 3),
            description: string_in(&values, 4),
            title: string_in(&values, 7),
            position_x: has_point.then_some(point.x),
            position_y: has_point.then_some(point.y),
            width: has_size.then_some(size.width),
            height: has_size.then_some(size.height),
            children,
        }
    }

    impl AxNode {
        fn leaf(element: &AXUIElement, role: String) -> Self {
            AxNode {
                element: element.clone(),
                role,
                value: None,
                help: None,
                description: None,
                title: None,
                position_x: None,
                position_y: None,
                width: None,
                height: None,
                children: Vec::new(),
            }
        }

        /// First descendant (pre-order, self included) with the given role —
        /// same traversal order `find_descendants_by_role(...).first()` used,
        /// just resolved from the in-memory tree instead of a fresh AX walk.
        fn find_first(&self, target_role: &str) -> Option<&AxNode> {
            if self.role == target_role {
                return Some(self);
            }
            for child in &self.children {
                if let Some(found) = child.find_first(target_role) {
                    return Some(found);
                }
            }
            None
        }

        /// All descendants (pre-order, self included) with the given role.
        fn find_all<'a>(&'a self, target_role: &str, out: &mut Vec<&'a AxNode>) {
            if self.role == target_role {
                out.push(self);
            }
            for child in &self.children {
                child.find_all(target_role, out);
            }
        }

        /// Move every `AXRow` in this subtree (self included) into `out`, in
        /// pre-order.
        fn into_rows(self, out: &mut Vec<AxNode>) {
            if self.role == "AXRow" {
                out.push(self);
                return;
            }
            for child in self.children {
                child.into_rows(out);
            }
        }

        fn any(&self, matches: &dyn Fn(&AxNode) -> bool) -> bool {
            matches(self) || self.children.iter().any(|child| child.any(matches))
        }

        fn count(&self) -> usize {
            1 + self.children.iter().map(AxNode::count).sum::<usize>()
        }

        /// Horizontal center, which is all bubble-direction checks need.
        fn center_x(&self) -> Option<f64> {
            Some(self.position_x? + self.width? / 2.0)
        }
    }

    /// The chat list's rows and their names, read with one `AXRole`,
    /// `AXChildren`, `AXValue` round-trip per node and stopping inside each
    /// row at its first `AXStaticText` (the chat name, the same pre-order
    /// match `find_first` makes). Avatars, previews, timestamps and badges
    /// after the name are never fetched.
    fn chat_list_rows(table: &AXUIElement) -> Result<Vec<(AXUIElement, Option<String>)>> {
        rows_with_label(table, &["AXStaticText"], "chat list names")
    }

    /// Every `AXRow` under `container` with the value of its first descendant
    /// whose role is in `label_roles` (pre-order), fetching nothing after it.
    fn rows_with_label(
        container: &AXUIElement,
        label_roles: &[&str],
        what: &str,
    ) -> Result<Vec<(AXUIElement, Option<String>)>> {
        let started = Instant::now();
        let mut budget = SnapshotBudget::new();
        let mut rows = Vec::new();
        collect_rows(container, 0, &mut budget, label_roles, &mut rows);
        budget.check()?;
        if crate::util::debug_enabled() {
            eprintln!(
                "[ax] {what}: {} rows, {} nodes in {:.1} ms",
                rows.len(),
                budget.nodes,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        Ok(rows)
    }

    fn collect_rows(
        element: &AXUIElement,
        depth: usize,
        budget: &mut SnapshotBudget,
        label_roles: &[&str],
        rows: &mut Vec<(AXUIElement, Option<String>)>,
    ) {
        if !budget.admit(element, depth) {
            return;
        }
        let Some(values) = copy_attributes(element, &["AXRole", "AXChildren"]) else {
            return;
        };
        let children = children_in(&values, 1);
        if string_in(&values, 0).as_deref() == Some("AXRow") {
            let label = children
                .iter()
                .find_map(|child| first_label(child, depth + 1, budget, label_roles))
                .flatten();
            rows.push((element.clone(), label));
            return;
        }
        for child in children {
            if budget.exhausted {
                return;
            }
            collect_rows(&child, depth + 1, budget, label_roles, rows);
        }
    }

    /// `Some(value)` of the first node (pre-order) whose role is in `roles`,
    /// `None` if the subtree has none.
    fn first_label(
        element: &AXUIElement,
        depth: usize,
        budget: &mut SnapshotBudget,
        roles: &[&str],
    ) -> Option<Option<String>> {
        if !budget.admit(element, depth) {
            return None;
        }
        let values = copy_attributes(element, &["AXRole", "AXChildren", "AXValue"])?;
        if string_in(&values, 0).is_some_and(|role| roles.contains(&role.as_str())) {
            return Some(string_in(&values, 2));
        }
        children_in(&values, 1)
            .iter()
            .find_map(|child| first_label(child, depth + 1, budget, roles))
    }

    /// Unicode NFC. Names on disk may be decomposed (NFD) while AX shows
    /// composed Hangul, so file names are compared normalized.
    fn nfc(text: &str) -> String {
        use core_foundation::string::CFStringRef;
        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFStringCreateMutableCopy(
                allocator: *const std::ffi::c_void,
                max_length: isize,
                string: CFStringRef,
            ) -> *mut std::ffi::c_void;
            fn CFStringNormalize(string: *mut std::ffi::c_void, form: isize);
        }
        const NORMALIZATION_FORM_C: isize = 2;
        let source = CFString::new(text);
        unsafe {
            let copy = CFStringCreateMutableCopy(std::ptr::null(), 0, source.as_concrete_TypeRef());
            if copy.is_null() {
                return text.to_string();
            }
            CFStringNormalize(copy, NORMALIZATION_FORM_C);
            CFString::wrap_under_create_rule(copy as CFStringRef).to_string()
        }
    }

    /// Post a CGEvent to KakaoTalk's pid directly (no `activate()` foreground
    /// switch — this is the Peekaboo-style fix for the focus race that hangs
    /// kakaocli's send path).
    fn post_key_to_pid(pid: i32, keycode: u16, key_down: bool) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("failed to create CGEventSource"))?;
        let event = CGEvent::new_keyboard_event(source, keycode, key_down)
            .map_err(|_| anyhow!("failed to create keyboard CGEvent"))?;
        event.post_to_pid(pid);
        Ok(())
    }

    fn attr_as_frame(el: &AXUIElement) -> Option<(CGPoint, CGSize)> {
        let value_of = |name: &str, kind, out: *mut std::ffi::c_void| -> bool {
            let attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new(name));
            let Ok(value) = el.attribute(&attr) else {
                return false;
            };
            value.type_of() == unsafe { AXValueGetTypeID() }
                && unsafe { AXValueGetValue(value.as_CFTypeRef() as AXValueRef, kind, out) }
        };
        let mut point = CGPoint::new(0.0, 0.0);
        let mut size = CGSize::new(0.0, 0.0);
        if value_of(
            "AXPosition",
            kAXValueTypeCGPoint,
            (&mut point as *mut CGPoint).cast(),
        ) && value_of(
            "AXSize",
            kAXValueTypeCGSize,
            (&mut size as *mut CGSize).cast(),
        ) {
            Some((point, size))
        } else {
            None
        }
    }

    /// True if the topmost on-screen AX element at `point` is `target` or one
    /// of its descendants. Uses the system-wide element so another app's window
    /// covering the row fails the check.
    fn hit_test_is_within(point: CGPoint, target: &AXUIElement) -> bool {
        let system = unsafe { AXUIElement::wrap_under_create_rule(AXUIElementCreateSystemWide()) };
        let mut hit_ref: AXUIElementRef = std::ptr::null_mut();
        let err = unsafe {
            AXUIElementCopyElementAtPosition(
                system.as_concrete_TypeRef(),
                point.x as f32,
                point.y as f32,
                &mut hit_ref,
            )
        };
        if err != 0 || hit_ref.is_null() {
            return false;
        }
        let mut current = unsafe { AXUIElement::wrap_under_create_rule(hit_ref) };
        for _ in 0..8 {
            if unsafe { CFEqual(current.as_CFTypeRef(), target.as_CFTypeRef()) != 0 } {
                return true;
            }
            match current.parent() {
                Ok(parent) => current = parent,
                Err(_) => return false,
            }
        }
        false
    }

    fn post_mouse_to_hid(kind: CGEventType, point: CGPoint, clicks: i64) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| anyhow!("failed to create CGEventSource"))?;
        let event = CGEvent::new_mouse_event(source, kind, point, CGMouseButton::Left)
            .map_err(|_| anyhow!("failed to create mouse CGEvent"))?;
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, clicks);
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn cursor_location() -> Option<CGPoint> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
        CGEvent::new(source).ok().map(|event| event.location())
    }

    /// Keeps KakaoTalk automation from fighting the user for the foreground.
    ///
    /// Every action that can bring KakaoTalk (or one of its panels) to the
    /// front calls [`FocusGuard::before_raise`] first: it waits until the
    /// user's keyboard and mouse have been idle for `--wait-idle` seconds (so
    /// their typing cannot land in KakaoTalk) and remembers the frontmost app.
    /// [`FocusGuard::restore`], also run on drop and therefore on every error
    /// path, hands the foreground back if KakaoTalk still holds it.
    struct FocusGuard {
        kakao_pid: i32,
        previous: Option<i32>,
        armed: bool,
    }

    impl FocusGuard {
        fn new(kakao_pid: i32) -> Self {
            Self {
                kakao_pid,
                previous: None,
                armed: false,
            }
        }

        fn before_raise(&mut self) -> Result<()> {
            // Every raise waits, even when an earlier step already raised:
            // the user may have resumed typing since (e.g. after Touch ID).
            wait_for_user_idle()?;
            if self.armed {
                return Ok(());
            }
            self.previous = frontmost_pid();
            self.armed = true;
            if crate::util::debug_enabled() {
                eprintln!(
                    "[ax] focus: KakaoTalk may be raised (frontmost pid {:?})",
                    self.previous
                );
            }
            Ok(())
        }

        /// Remember the frontmost app without the idle wait, for steps that
        /// are not expected to raise KakaoTalk: if one does anyway, `restore`
        /// still hands the foreground back.
        fn watch(&mut self) {
            if !self.armed {
                self.previous = frontmost_pid();
                self.armed = true;
            }
        }

        /// Hand the foreground back if KakaoTalk holds it, and keep watching:
        /// the device-owner authentication dialog can activate KakaoTalk when
        /// it closes (seen 2026-11-06 after its open-panel sheet and bubble
        /// menus), so the restore after the commit must still be armed.
        fn restore(&mut self) {
            if !self.armed {
                return;
            }
            let current = frontmost_pid();
            let Some(previous) = self
                .previous
                .filter(|_| super::should_restore_focus(self.previous, current, self.kakao_pid))
            else {
                return;
            };
            let frontmost: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXFrontmost"));
            let result = AXUIElement::application(previous)
                .set_attribute(&frontmost, CFBoolean::true_value().as_CFType());
            if crate::util::debug_enabled() {
                eprintln!("[ax] focus: handed the foreground back to pid {previous} ({result:?})");
            }
        }
    }

    impl Drop for FocusGuard {
        fn drop(&mut self) {
            self.restore();
        }
    }

    fn frontmost_pid() -> Option<i32> {
        let focused = attr_as_element(&AXUIElement::system_wide(), "AXFocusedApplication")?;
        let mut pid = 0;
        (unsafe { AXUIElementGetPid(focused.as_concrete_TypeRef(), &mut pid) } == 0).then_some(pid)
    }

    /// Keyboard/mouse idle time from the IOHIDSystem registry entry. Unlike
    /// CGEventSource it also works from SSH sessions.
    fn hid_idle_time() -> Option<Duration> {
        let output = Command::new("/usr/sbin/ioreg")
            .args(["-c", "IOHIDSystem", "-r", "-d", "1", "-k", "HIDIdleTime"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        super::parse_hid_idle(&String::from_utf8_lossy(&output.stdout))
    }

    fn wait_for_user_idle() -> Result<()> {
        let threshold = super::policy().wait_idle;
        let started = Instant::now();
        loop {
            match super::idle_decision(
                hid_idle_time(),
                threshold,
                started.elapsed(),
                super::MAX_IDLE_WAIT,
            ) {
                super::IdleDecision::Proceed => {
                    trace(started, "user idle; raising is allowed");
                    return Ok(());
                }
                super::IdleDecision::TimedOut => {
                    return Err(coded(
                        ErrorCode::UserActive,
                        format!(
                            "the keyboard or mouse stayed in use for {}s, so KakaoTalk was not raised (needs {}s idle)",
                            super::MAX_IDLE_WAIT.as_secs(),
                            threshold.as_secs_f64()
                        ),
                    ));
                }
                super::IdleDecision::Wait => sleep(Duration::from_millis(250)),
            }
        }
    }

    /// Open a chat row that exposes no AX open action by double-clicking its
    /// center. KakaoTalk ignores mouse events posted to its pid, so the click
    /// goes through the HID tap — which is why the app is raised first and a
    /// system-wide hit test must prove the exact row is the topmost element at
    /// that point immediately before clicking. The cursor is restored after.
    fn double_click_verified_row(
        app: &AXUIElement,
        row: &AXUIElement,
        focus: &mut FocusGuard,
    ) -> Result<()> {
        focus.before_raise()?;
        let frontmost: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXFrontmost"));
        app.set_attribute(&frontmost, CFBoolean::true_value().as_CFType())
            .map_err(|error| anyhow!("could not bring KakaoTalk to the front: {error:?}"))?;
        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        while attr_as_bool(app, "AXFrontmost") != Some(true) {
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk did not become frontmost");
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }

        let (origin, size) =
            attr_as_frame(row).ok_or_else(|| anyhow!("chat row has no readable frame"))?;
        // Global coordinates: rows on a display above or left of the main one
        // have negative origins.
        let (x, y) = super::frame_center(origin.x, origin.y, size.width, size.height)
            .ok_or_else(|| anyhow!("chat row has an empty frame"))?;
        let center = CGPoint::new(x, y);
        if !hit_test_is_within(center, row) {
            anyhow::bail!(
                "chat row is off-screen or covered at its center; scroll it into view and retry"
            );
        }

        let restore = cursor_location();
        let clicked = (1..=2).try_for_each(|clicks| {
            post_mouse_to_hid(CGEventType::LeftMouseDown, center, clicks)?;
            post_mouse_to_hid(CGEventType::LeftMouseUp, center, clicks)?;
            sleep(Duration::from_millis(30));
            Ok::<_, anyhow::Error>(())
        });
        if let Some(point) = restore {
            let _ = CGDisplay::warp_mouse_cursor_position(point);
        }
        clicked
    }

    fn press_return(pid: i32) -> Result<()> {
        post_key_to_pid(pid, RETURN_KEYCODE, true)?;
        post_key_to_pid(pid, RETURN_KEYCODE, false)?;
        Ok(())
    }

    /// Run a short helper process with a hard deadline and return its stdout.
    fn run_bounded(command: &mut Command, timeout: Duration, what: &str) -> Result<String> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start {what}"))?;
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let (mut stdout, mut stderr) = (String::new(), String::new());
                    if let Some(mut pipe) = child.stdout.take() {
                        let _ = pipe.read_to_string(&mut stdout);
                    }
                    if let Some(mut pipe) = child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    if !status.success() {
                        anyhow::bail!("{what} failed ({status}): {}", stderr.trim());
                    }
                    return Ok(stdout);
                }
                Err(error) => return Err(error).with_context(|| format!("{what} failed")),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("{what} timed out");
                }
                Ok(None) => sleep(Duration::from_millis(50)),
            }
        }
    }

    /// ⌘V to the frontmost app. Pid-posted ⌘V was tried during #57 and did
    /// not reach KakaoTalk, so this stays a System Events keystroke; callers
    /// must have raised and focus-verified the exact composer first.
    fn press_command_v() -> Result<()> {
        let script = r#"tell application "System Events" to keystroke "v" using command down"#;
        run_bounded(
            Command::new("osascript").args(["-e", script]),
            OPEN_CHAT_TIMEOUT,
            "the clipboard paste into KakaoTalk",
        )
        .map(drop)
    }

    /// Put exactly one file URL on the general pasteboard (KakaoTalk turns a
    /// pasted file URL into its own single-file preview) and return the
    /// pasteboard's `changeCount` right after the write.
    fn stage_file_url(path: &std::path::Path) -> Result<i64> {
        let script = r#"
ObjC.import("AppKit");
function run(argv) {
    const pasteboard = $.NSPasteboard.generalPasteboard;
    pasteboard.clearContents;
    const fileURL = $.NSURL.fileURLWithPath($(argv[0]));
    const objects = $.NSArray.arrayWithObject(fileURL);
    if (!pasteboard.writeObjects(objects)) {
        throw new Error("failed to write file URL to pasteboard");
    }
    return pasteboard.changeCount;
}
"#;
        let output = run_bounded(
            Command::new("osascript")
                .args(["-l", "JavaScript", "-e", script, "--"])
                .arg(path),
            OPEN_CHAT_TIMEOUT,
            "staging the verified file on the macOS clipboard",
        )?;
        output
            .trim()
            .parse()
            .with_context(|| format!("unexpected pasteboard change count {output:?}"))
    }

    fn pasteboard_change_count() -> Result<i64> {
        let output = run_bounded(
            Command::new("osascript").args([
                "-l",
                "JavaScript",
                "-e",
                r#"ObjC.import("AppKit"); $.NSPasteboard.generalPasteboard.changeCount"#,
            ]),
            OPEN_CHAT_TIMEOUT,
            "reading the macOS pasteboard change count",
        )?;
        output
            .trim()
            .parse()
            .with_context(|| format!("unexpected pasteboard change count {output:?}"))
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn press_command_shift_g() -> Result<()> {
        let flags = CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift;
        for key_down in [true, false] {
            let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                .map_err(|_| anyhow!("failed to create CGEventSource"))?;
            let event = CGEvent::new_keyboard_event(source, KeyCode::ANSI_G, key_down)
                .map_err(|_| anyhow!("failed to create keyboard CGEvent"))?;
            event.set_flags(flags);
            // This shortcut belongs to the frontmost native open panel, not
            // merely to the KakaoTalk process. KakaoTalk can have a main and
            // chat window on different Spaces, and CGEventPostToPid may route
            // the shortcut to the wrong one even after AXRaise. Posting at the
            // HID event tap lets AppKit deliver it to the verified, raised
            // picker-bearing chat window.
            event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn single_click(point: CGPoint) -> Result<()> {
        for event_type in [CGEventType::LeftMouseDown, CGEventType::LeftMouseUp] {
            let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                .map_err(|_| anyhow!("failed to create CGEventSource"))?;
            let event = CGEvent::new_mouse_event(source, event_type, point, CGMouseButton::Left)
                .map_err(|_| anyhow!("failed to create mouse CGEvent"))?;
            event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 1);
            event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    /// Switch the main window to the chat-list ("chatrooms") tab if it isn't
    /// already there — the chat-list `AXTable` only exists while that tab is
    /// active; the Friends tab renders an `AXOutline` instead. Pressing the
    /// tab is an element-scoped AX action and does not raise KakaoTalk.
    ///
    /// Returns the snapshot to use afterward (re-taken only if the tab was
    /// pressed). `shallow` skips the table's rows, which is all `open_chat`
    /// needs; `ax-watch` asks for the full tree.
    fn ensure_chatrooms_tab(main_window: &AXUIElement, shallow: bool) -> Result<AxNode> {
        let take = |window: &AXUIElement| {
            if shallow {
                snapshot_shallow(window)
            } else {
                snapshot(window)
            }
        };
        let snap = take(main_window)?;
        if snap.find_first("AXTable").is_some() {
            return Ok(snap);
        }
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        if let Some(tab) = buttons
            .iter()
            .find(|b| attr_as_string(&b.element, "AXIdentifier").as_deref() == Some("chatrooms"))
        {
            let _ = tab.element.perform_action(&CFString::new(kAXPressAction));
            sleep(Duration::from_millis(400));
            return take(main_window);
        }
        Ok(snap)
    }

    /// KakaoTalk's chat-list search while openkakao uses it. Dropping it
    /// presses 검색 again, which closes the search, clears its term and
    /// restores the full list (measured on 26.8: 288 ms back to 670 rows).
    struct ChatSearch {
        window: AXUIElement,
    }

    impl Drop for ChatSearch {
        fn drop(&mut self) {
            if let Some(button) = search_button(&self.window) {
                let _ = button.perform_action(&CFString::new(kAXPressAction));
            }
        }
    }

    fn search_button(window: &AXUIElement) -> Option<AXUIElement> {
        let snap = snapshot_shallow(window).ok()?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        buttons
            .into_iter()
            .find(|button| button.description.as_deref() == Some("검색"))
            .map(|button| button.element.clone())
    }

    /// Chat-list rows with the name each one shows.
    type ChatRows = Vec<(AXUIElement, Option<String>)>;

    fn table_row_count(table: &AXUIElement) -> Option<isize> {
        let rows: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXRows"));
        let value = table.attribute(&rows).ok()?;
        Some(value.downcast::<CFArray>()?.len())
    }

    /// The chat-list rows KakaoTalk's search leaves for `name`, read one by one
    /// (a handful instead of every loaded row: ~10 ms instead of ~5.6 s for 670
    /// rows on 26.8). 검색 is pressed, the name is inserted with
    /// `AXSelectedText` (setting `AXValue` does not filter), and the rows are
    /// read once their count changed and settled. The search also matches
    /// member names, so callers still match exact chat names. `None` (and the
    /// search closed again) whenever any step does not behave as measured, or
    /// when the search is already open (the user's own), so the caller can
    /// scan the full list instead.
    fn filter_chat_list(
        window: &AXUIElement,
        snap: &AxNode,
        name: &str,
    ) -> Option<(ChatSearch, ChatRows)> {
        filter_list(window, snap, name, "AXTable")
    }

    /// [`filter_chat_list`] for the list with `list_role`: the chat list's
    /// `AXTable` or the friends tab's `AXOutline` (same 검색 button).
    fn filter_list(
        window: &AXUIElement,
        snap: &AxNode,
        name: &str,
        list_role: &str,
    ) -> Option<(ChatSearch, ChatRows)> {
        if snap.find_first("AXTextField").is_some() {
            return None;
        }
        let table = |snap: &AxNode| snap.find_first(list_role).map(|t| t.element.clone());
        let before = table_row_count(&table(snap)?)?;
        search_button(window)?
            .perform_action(&CFString::new(kAXPressAction))
            .ok()?;
        let search = ChatSearch {
            window: window.clone(),
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        let field = loop {
            if let Some(field) = snapshot_shallow(window)
                .ok()
                .and_then(|snap| snap.find_first("AXTextField").map(|f| f.element.clone()))
            {
                break field;
            }
            if Instant::now() >= deadline {
                return None;
            }
            sleep(Duration::from_millis(50));
        };
        let focused: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXFocused"));
        let selected_text: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXSelectedText"));
        field
            .set_attribute(&focused, CFBoolean::true_value().as_CFType())
            .ok()?;
        field
            .set_attribute(&selected_text, CFString::new(name).as_CFType())
            .ok()?;
        let filtered = snapshot_shallow(window)
            .ok()
            .and_then(|snap| table(&snap))?;
        // Settled: the count left the full list's and read the same twice.
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut last = None;
        loop {
            sleep(Duration::from_millis(100));
            let count = table_row_count(&filtered)?;
            if count != before && last == Some(count) {
                break;
            }
            if Instant::now() >= deadline {
                return None;
            }
            last = Some(count);
        }
        let rows = chat_list_rows(&filtered).ok()?;
        Some((search, rows))
    }

    /// The text currently typed into the main window's chat-list search field,
    /// if any. While it is set KakaoTalk only lists matching rows.
    fn search_field_term(main_snapshot: &AxNode) -> Option<String> {
        let mut fields = Vec::new();
        main_snapshot.find_all("AXTextField", &mut fields);
        fields
            .into_iter()
            .filter_map(|field| field.value.clone())
            .find(|value| !value.trim().is_empty())
    }

    /// `chat_not_found`'s hint: chats whose names differ only in spacing or
    /// case, with their last message, so the person can tell which one they
    /// meant. KakaoTalk's search keeps spaces, so a name with spaces is also
    /// searched without them. Nothing is opened: the name must still match.
    fn near_name_hint(
        main_window: &AXUIElement,
        snap: &AxNode,
        name: &str,
        rows: ChatRows,
        search: Option<ChatSearch>,
    ) -> String {
        let near = |rows: &ChatRows| -> Vec<String> {
            rows.iter()
                .filter_map(|(row, row_name)| {
                    let row_name = row_name.as_deref()?;
                    super::same_but_spacing(row_name, name).then(|| {
                        format!(
                            "\n  '{}' ({})",
                            crate::util::escape_terminal_text(row_name),
                            row_summary(row)
                        )
                    })
                })
                .collect()
        };
        let mut found = near(&rows);
        let compact: String = name.split_whitespace().collect();
        if found.is_empty() && compact != name {
            drop(search);
            // Searching again before the full list is back reads its rows
            // as the result (measured: 31 stale rows instead of 1).
            let restored = restored_chat_list(main_window);
            let base = restored.as_ref().unwrap_or(snap);
            if let Some((_search, rows)) = filter_chat_list(main_window, base, &compact) {
                found = near(&rows);
            }
        }
        if found.is_empty() {
            String::new()
        } else {
            format!(
                "\nsimilar chat names (not opened; re-run with the exact name):{}",
                found.concat()
            )
        }
    }

    /// The main window once a closed search has restored the full chat list:
    /// no search field and the same row count twice in a row.
    fn restored_chat_list(main_window: &AXUIElement) -> Option<AxNode> {
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut last = None;
        while Instant::now() < deadline {
            let snap = snapshot_shallow(main_window).ok()?;
            if snap.find_first("AXTextField").is_none() {
                let count = snap
                    .find_first("AXTable")
                    .and_then(|table| table_row_count(&table.element));
                if count.is_some() && last == count {
                    return Some(snap);
                }
                last = count;
            }
            sleep(Duration::from_millis(100));
        }
        None
    }

    struct OpenChat {
        window: AXUIElement,
        /// The window was already open: nothing was opened or raised.
        was_open: bool,
    }

    /// Return the one open chat window titled exactly `chat_display_name`.
    ///
    /// An already-open exact-title window is used directly: no chat-list walk
    /// and no foreground change (two open windows with that exact title still
    /// refuse). Opening a chat that has no window requires `--open-if-needed`;
    /// that path needs one unique exact-name chat-list row and may raise
    /// KakaoTalk behind the idle wait. Callers decide when to restore focus.
    fn open_chat(
        app: &AXUIElement,
        chat_display_name: &str,
        focus: &mut FocusGuard,
    ) -> Result<OpenChat> {
        let started = Instant::now();
        if let Some(window) = find_chat_window(app, chat_display_name)? {
            trace(started, "exact chat window already open");
            return Ok(OpenChat {
                window,
                was_open: true,
            });
        }
        if !super::policy().open_if_needed {
            return Err(coded(
                ErrorCode::ChatWindowNotOpen,
                format!("no open KakaoTalk chat window is titled exactly '{chat_display_name}'"),
            ));
        }

        // Remember the user's app before anything touches KakaoTalk's menus or
        // windows (`창 > 채팅` included), so it can always be handed back.
        focus.watch();
        let main_window = find_or_show_main_window(app)?;
        let snap = ensure_chatrooms_tab(&main_window, true)?;
        let table = snap.find_first("AXTable").ok_or_else(|| {
            coded(
                ErrorCode::ChatListUnavailable,
                "could not find the chat list in KakaoTalk's main window",
            )
        })?;
        // KakaoTalk's own search narrows the list to a handful of rows (and
        // reaches chats that are not loaded); without it every loaded row is
        // named one AX call at a time. `search` closes it when this returns,
        // after the row was opened (closing re-creates the rows).
        let (rows, search) = match filter_chat_list(&main_window, &snap, chat_display_name) {
            Some((search, rows)) => {
                trace(started, "chat list filtered by KakaoTalk search");
                (rows, Some(search))
            }
            None => (chat_list_rows(&table.element)?, None),
        };

        // Match the row's first AXStaticText (the chat name) exactly, never as
        // a substring, and refuse duplicates: there is no chat id to
        // disambiguate with (see SafetyConfig::allowed_send_chats).
        let row_names: Vec<Option<String>> = rows.iter().map(|(_, name)| name.clone()).collect();
        let row = match super::match_chat_row(&row_names, chat_display_name) {
            super::ChatMatch::Found(index) => rows[index].0.clone(),
            super::ChatMatch::NotFound => {
                return Err(match search_field_term(&snap) {
                    Some(term) => coded(
                        ErrorCode::ChatListFiltered,
                        format!(
                            "chat '{chat_display_name}' is not in KakaoTalk's chat list, which is filtered by the search term '{term}'"
                        ),
                    ),
                    None if search.is_some() => {
                        let matched = rows.len();
                        let hint =
                            near_name_hint(&main_window, &snap, chat_display_name, rows, search);
                        coded(
                            ErrorCode::ChatNotFound,
                            format!(
                                "KakaoTalk's chat search lists no chat named exactly '{chat_display_name}' ({matched} rows matched the term){hint}"
                            ),
                        )
                    }
                    None => coded(
                        ErrorCode::ChatNotFound,
                        format!(
                            "chat '{chat_display_name}' not found among the {} loaded chat-list rows",
                            rows.len()
                        ),
                    ),
                });
            }
            super::ChatMatch::Ambiguous(count) => {
                let same_name =
                    |n: usize| super::picked_row(&row_names, chat_display_name, Some(n));
                if let Some(index) = super::policy().pick.and_then(same_name) {
                    let summary = row_summary(&rows[index].0);
                    // Safety-relevant, so always shown, not only under debug.
                    eprintln!(
                        "--pick {}: opening '{}' ({summary}); the allowlist names every chat \
                         called this, so a send needs device-owner authentication",
                        super::policy().pick.unwrap_or_default(),
                        crate::util::escape_terminal_text(chat_display_name),
                    );
                    let _ = PICKED_DUPLICATE.set(summary);
                    rows[index].0.clone()
                } else {
                    let candidates: Vec<String> = (1..=count)
                        .filter_map(|n| same_name(n).map(|index| (n, index)))
                        .map(|(n, index)| format!("  {n}) {}", row_summary(&rows[index].0)))
                        .collect();
                    return Err(coded(
                        ErrorCode::ChatAmbiguous,
                        format!(
                            "chat name '{chat_display_name}' matches {count} chats in the list (top to bottom):\n{}\nre-run with --pick <N> to open one; openkakao-cli never guesses",
                            candidates.join("\n")
                        ),
                    ));
                }
            }
        };
        trace(started, "chat-list row chosen");

        if super::policy().fallback_keys {
            // --fallback-keys: the pre-26.9 route, which can raise KakaoTalk,
            // so it runs only after all tree reads, behind the idle wait.
            focus.before_raise()?;
            if !open_via_window_menu_probe(chat_display_name, started) {
                open_list_row(app, &main_window, &table.element, &row, focus)?;
            }
        } else {
            // The row's context menu offers 채팅방 열기, which KakaoTalk 26.9
            // performs without activating (measured). Nothing is raised; if
            // KakaoTalk comes forward anyway, the guard hands focus back.
            focus.watch();
            open_row_via_context_menu(app, &row)?;
        }

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            if let Some(window) = find_chat_window(app, chat_display_name)? {
                trace(started, "chat window opened from the list");
                return Ok(OpenChat {
                    window,
                    was_open: false,
                });
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "exact chat window '{chat_display_name}' did not open in time (it may have opened on another Space)"
                );
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// `--fallback-keys` only: KakaoTalk may restore an already-open chat on
    /// another macOS Space, where AXWindows cannot see it. After the chat list
    /// established that the name is unique, click exactly one Window-menu item
    /// with that title through System Events. The title is passed as argv so
    /// chat text is never interpolated into AppleScript.
    fn open_via_window_menu_probe(chat_display_name: &str, started: Instant) -> bool {
        let script = r#"
on run argv
    set targetTitle to item 1 of argv
    tell application "System Events"
        tell process "KakaoTalk"
            set matches to every menu item of menu "Window" of menu bar 1 whose name is targetTitle
            if (count of matches) is not 1 then return "not-found"
            click item 1 of matches
            return "clicked"
        end tell
    end tell
end run
"#;
        match run_bounded(
            Command::new("osascript").args(["-e", script, chat_display_name]),
            OPEN_CHAT_TIMEOUT,
            "KakaoTalk's Window-menu probe",
        ) {
            Ok(output) => output.trim() == "clicked",
            Err(error) => {
                trace(
                    started,
                    &format!("{error:#}; falling back to chat-row selection"),
                );
                false
            }
        }
    }

    const OPEN_CHAT_MENU_TITLES: &[&str] = &["채팅방 열기", "Open chat", "Open Chat"];
    const MENU_WAIT: Duration = Duration::from_secs(2);

    fn menu_items(menu: &AXUIElement) -> Vec<(AXUIElement, Option<String>)> {
        menu.children()
            .map(|items| {
                items
                    .iter()
                    .filter(|item| role(item) == "AXMenuItem")
                    .map(|item| {
                        let title = attr_as_string(&item, "AXTitle");
                        ((*item).clone(), title)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// On-screen KakaoTalk windows at a menu layer (kCGWindowLayer >= 100), as
    /// global (x, y, width, height). Open context menus live there even when
    /// they are absent from the AX tree. Matched by pid: the owner name is
    /// localized. Bounds need no Screen Recording permission.
    fn menu_windows(pid: i32) -> Vec<(f64, f64, f64, f64)> {
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::CFNumber;
        use core_graphics::window::{
            copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
            kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
        };
        let Some(windows) = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
        else {
            return Vec::new();
        };
        let number_in = |info: &CFDictionary<CFString, CFType>, key: &CFString| {
            info.find(key)
                .and_then(|value| value.downcast::<CFNumber>())
                .and_then(|number| number.to_f64())
        };
        let (owner_key, layer_key, bounds_key) = unsafe {
            (
                CFString::wrap_under_get_rule(kCGWindowOwnerPID),
                CFString::wrap_under_get_rule(kCGWindowLayer),
                CFString::wrap_under_get_rule(kCGWindowBounds),
            )
        };
        windows
            .iter()
            .filter_map(|window| {
                let info: CFDictionary<CFString, CFType> =
                    unsafe { CFDictionary::wrap_under_get_rule(*window as CFDictionaryRef) };
                let owner = number_in(&info, &owner_key)?;
                let layer = number_in(&info, &layer_key)?;
                if owner as i64 != i64::from(pid) || layer < 100.0 {
                    return None;
                }
                let bounds = info.find(&bounds_key)?.downcast::<CFDictionary>()?;
                let bounds: CFDictionary<CFString, CFType> =
                    unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
                let field = |name: &str| number_in(&bounds, &CFString::new(name));
                Some((field("X")?, field("Y")?, field("Width")?, field("Height")?))
            })
            .collect()
    }

    /// The `AXMenu` drawn in a menu-layer window: hit-test just below its top
    /// edge (the first item) and climb `AXParent` to the menu.
    /// The KakaoTalk menu in a layer-101 menu window, hit-tested within
    /// KakaoTalk only: a system-wide hit test finds whatever window is on
    /// top there, such as another app's security dialog, and misses the
    /// menu under it (measured 2026-11-07).
    fn menu_in_window(
        pid: i32,
        (x, y, width, _height): (f64, f64, f64, f64),
    ) -> Option<AXUIElement> {
        let system = AXUIElement::application(pid);
        let mut hit: AXUIElementRef = std::ptr::null_mut();
        let err = unsafe {
            AXUIElementCopyElementAtPosition(
                system.as_concrete_TypeRef(),
                (x + width / 2.0) as f32,
                (y + 12.0) as f32,
                &mut hit,
            )
        };
        if err != 0 || hit.is_null() {
            return None;
        }
        let mut current = unsafe { AXUIElement::wrap_under_create_rule(hit) };
        for _ in 0..8 {
            if role(&current) == "AXMenu" {
                return Some(current);
            }
            current = attr_as_element(&current, "AXParent")?;
        }
        None
    }

    fn app_pid(app: &AXUIElement) -> Option<i32> {
        let mut pid = 0;
        (unsafe { AXUIElementGetPid(app.as_concrete_TypeRef(), &mut pid) } == 0).then_some(pid)
    }

    /// `AXShowMenu` on `element` and the context menu it opened. Discovery is
    /// flaky, so three places are polled for a bounded time: the app's and the
    /// element's `AXMenu` children, and KakaoTalk's menu-layer windows (bubble
    /// menus only appear there). `None` means no menu was found and nothing
    /// else may be done.
    fn show_context_menu(app: &AXUIElement, element: &AXUIElement) -> Result<Option<AXUIElement>> {
        let pid = app_pid(app);
        // A menu already showing is the user's (or a leftover): acting on it
        // could press 답장 or 채팅방 열기 for the wrong target, so refuse.
        if pid.is_some_and(|pid| !menu_windows(pid).is_empty()) {
            return Err(coded(
                ErrorCode::UserActive,
                "a KakaoTalk menu is already open; close it by clicking elsewhere (never Esc) and retry",
            ));
        }
        // AXShowMenu can report an error (e.g. a timeout) after opening the
        // menu anyway, so look for it regardless; the caller then closes it.
        let shown = Instant::now();
        let result = element.perform_action(&CFString::new("AXShowMenu"));
        if crate::util::debug_enabled() {
            eprintln!(
                "[ax] AXShowMenu returned in {:.1} ms",
                shown.elapsed().as_secs_f64() * 1000.0
            );
        }
        if let Err(error) = result {
            if crate::util::debug_enabled() {
                eprintln!("[ax] AXShowMenu reported {error:?}; still looking for the menu");
            }
        }
        let deadline = Instant::now() + MENU_WAIT;
        loop {
            for parent in [app, element] {
                if let Ok(children) = parent.children() {
                    if let Some(menu) = children.iter().find(|child| role(child) == "AXMenu") {
                        return Ok(Some((*menu).clone()));
                    }
                }
            }
            if let Some(menu) = pid.and_then(|pid| {
                menu_windows(pid)
                    .into_iter()
                    .find_map(|bounds| menu_in_window(pid, bounds))
            }) {
                return Ok(Some(menu));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            sleep(Duration::from_millis(30));
        }
    }

    /// The mouse cursor in global display coordinates, if readable.
    fn mouse_location() -> Option<(f64, f64)> {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
        let point = CGEvent::new(source).ok()?.location();
        Some((point.x, point.y))
    }

    /// Whether the cursor is over one of KakaoTalk's menu-layer windows, where
    /// a click of the user's could land on the item about to be pressed or a
    /// destructive neighbour (삭제, 채팅방 나가기).
    fn cursor_over_menu(app: &AXUIElement) -> bool {
        let Some(pid) = app_pid(app) else {
            return false;
        };
        let Some((x, y)) = mouse_location() else {
            return false;
        };
        menu_windows(pid).iter().any(|(left, top, width, height)| {
            x >= *left && x <= left + width && y >= *top && y <= top + height
        })
    }

    /// Wait (bounded) until no KakaoTalk menu-layer window is on screen, so a
    /// menu operation never leaves a menu behind. Returns whether one remains.
    fn menu_still_open(app: &AXUIElement) -> bool {
        let Some(pid) = app_pid(app) else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if menu_windows(pid).is_empty() {
                return false;
            }
            if Instant::now() >= deadline {
                return true;
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// Open a chat-list row from its context menu (`채팅방 열기`). Never falls
    /// through to keys or clicks: a missing menu or item is reported, and a
    /// found menu is always closed with `AXCancel` unless its item was pressed.
    fn open_row_via_context_menu(app: &AXUIElement, row: &AXUIElement) -> Result<()> {
        press_row_menu_item(app, row, OPEN_CHAT_MENU_TITLES, "채팅방 열기")
    }

    /// Press one item of a chat-list row's context menu (`채팅방 열기`,
    /// `읽음 처리`), never keys or clicks; a found menu is always closed.
    fn press_row_menu_item(
        app: &AXUIElement,
        row: &AXUIElement,
        titles: &[&str],
        label: &str,
    ) -> Result<()> {
        // KakaoTalk 26.9 exposes AXShowMenu on the row's cell.
        let target = row
            .children()
            .ok()
            .and_then(|children| {
                children
                    .iter()
                    .find(|child| role(child) == "AXCell")
                    .map(|cell| (*cell).clone())
            })
            .unwrap_or_else(|| row.clone());
        let Some(menu) = show_context_menu(app, &target)? else {
            let stray = if menu_still_open(app) {
                "; a KakaoTalk menu is showing but could not be read, click elsewhere to close it (never Esc)"
            } else {
                ""
            };
            return Err(coded(
                ErrorCode::MenuNotFound,
                format!("the chat-list row's context menu did not appear to Accessibility in time{stray}"),
            ));
        };
        let items = menu_items(&menu);
        let open: Vec<_> = items
            .iter()
            .filter(|(_, title)| {
                title
                    .as_deref()
                    .is_some_and(|title| titles.contains(&title.trim()))
            })
            .collect();
        let [(item, _)] = open.as_slice() else {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
            let stray = if menu_still_open(app) {
                "; a menu is still showing, click elsewhere to close it (never Esc)"
            } else {
                ""
            };
            return Err(coded(
                ErrorCode::MenuNotFound,
                format!(
                    "the chat-list row's context menu has no unique {label} item (items: {:?}){stray}",
                    items
                        .iter()
                        .map(|(_, title)| title.clone())
                        .collect::<Vec<_>>()
                ),
            ));
        };
        if cursor_over_menu(app) {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
            return Err(coded(
                ErrorCode::UserActive,
                format!("the mouse cursor is over the chat-list row's context menu; {label} was not pressed"),
            ));
        }
        let pressed = item
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not press {label}: {error:?}"));
        if menu_still_open(app) {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
        }
        pressed
    }

    /// The main window, showing it first through the app's own menu bar
    /// (`창 > 채팅`, performed by AX without activating KakaoTalk) when it is
    /// merely hidden. A window on another Space stays a reported error.
    fn find_or_show_main_window(app: &AXUIElement) -> Result<AXUIElement> {
        let error = match find_main_window(app) {
            Ok(window) => return Ok(window),
            Err(error) => error,
        };
        if error_code(&error) != Some(ErrorCode::MainWindowNotVisible)
            || !show_main_window_via_menu_bar(app)
        {
            return Err(error);
        }
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            if let Ok(window) = find_main_window(app) {
                return Ok(window);
            }
            if Instant::now() >= deadline {
                return Err(error);
            }
            sleep(Duration::from_millis(100));
        }
    }

    fn show_main_window_via_menu_bar(app: &AXUIElement) -> bool {
        let Some(menu_bar) = attr_as_element(app, "AXMenuBar") else {
            return false;
        };
        let titled = |items: Vec<(AXUIElement, Option<String>)>, wanted: &[&str]| {
            let found: Vec<AXUIElement> = items
                .into_iter()
                .filter(|(_, title)| {
                    title
                        .as_deref()
                        .is_some_and(|title| wanted.contains(&title.trim()))
                })
                .map(|(item, _)| item)
                .collect();
            match found.as_slice() {
                [item] => Some(item.clone()),
                _ => None,
            }
        };
        let bar_items = menu_bar
            .children()
            .map(|items| {
                items
                    .iter()
                    .map(|item| ((*item).clone(), attr_as_string(&item, "AXTitle")))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let Some(window_menu) = titled(bar_items, &["창", "Window"])
            .and_then(|item| item.children().ok())
            .and_then(|children| {
                children
                    .iter()
                    .find(|child| role(child) == "AXMenu")
                    .map(|menu| (*menu).clone())
            })
        else {
            return false;
        };
        titled(menu_items(&window_menu), &["채팅", "Chats"])
            .is_some_and(|item| item.perform_action(&CFString::new(kAXPressAction)).is_ok())
    }

    /// Select the exact row and open it with an element-scoped action, or, on
    /// builds whose rows expose none, a hit-tested double-click.
    fn open_list_row(
        app: &AXUIElement,
        main_window: &AXUIElement,
        table: &AXUIElement,
        row: &AXUIElement,
        focus: &mut FocusGuard,
    ) -> Result<()> {
        // An already-open chat window keeps key focus, so the chat-list table
        // can't take AX focus below until the main window is raised.
        let _ = main_window.perform_action(&CFString::new("AXRaise"));

        // Select via AX attribute (works even for off-screen rows — this is the
        // fix kakaocli landed for its off-screen-row regression) rather than a
        // coordinate-based double click. `AXSelectedRows` is a settable
        // attribute of the *table*, not the row (has no typed accessor in the
        // `accessibility` crate either way, so it's addressed by raw name).
        let selected_rows_attr: AXAttribute<CFType> =
            AXAttribute::new(&CFString::new("AXSelectedRows"));
        let one_row = CFArray::from_CFTypes(std::slice::from_ref(row));
        table
            .set_attribute(&selected_rows_attr, one_row.as_CFType())
            .map_err(|e| anyhow!("failed to select chat row: {e:?}"))?;
        let selected_rows = table
            .attribute(&selected_rows_attr)
            .map_err(|error| anyhow!("failed to verify selected chat row: {error:?}"))?
            .downcast::<CFArray<*const std::ffi::c_void>>()
            .ok_or_else(|| anyhow!("selected chat row could not be read back"))?;
        let selected_exactly_once = selected_rows.len() == 1
            && selected_rows
                .get(0)
                .is_some_and(|selected| unsafe { CFEqual(*selected, row.as_CFTypeRef()) != 0 });
        if !selected_exactly_once {
            anyhow::bail!(
                "selected chat row did not match the exact requested row; refusing Return"
            );
        }

        // Open the exact selected row with an element-scoped AX action. A
        // process-wide synthetic Return here could land in a composer if focus
        // changed, so keyboard events are never used to open a row.
        let has_action = |element: &AXUIElement, wanted: &str| {
            element
                .action_names()
                .map(|actions| actions.iter().any(|action| action.to_string() == wanted))
                .unwrap_or(false)
        };
        if has_action(row, "AXPress") {
            row.perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("failed to open exact chat row with AXPress: {error:?}"))
        } else if has_action(row, "AXConfirm") {
            row.perform_action(&CFString::new("AXConfirm"))
                .map_err(|error| anyhow!("failed to open exact chat row with AXConfirm: {error:?}"))
        } else if has_action(table, "AXConfirm") {
            table
                .perform_action(&CFString::new("AXConfirm"))
                .map_err(|error| {
                    anyhow!("failed to confirm the exact selected chat row: {error:?}")
                })
        } else {
            // KakaoTalk 25.9+ exposes no AXPress/AXConfirm on chat rows or the
            // table (cells only offer AXShowMenu). Fall back to a double click
            // on the row, but only after a system-wide hit test proves the
            // exact row is what's actually on screen at that point — still no
            // process-wide Return.
            double_click_verified_row(app, row, focus).map_err(|error| {
                anyhow!(
                    "the exact selected KakaoTalk chat row exposes no element-scoped open action, and the hit-tested double-click fallback failed: {error}"
                )
            })
        }
    }

    /// Search a single root (a window, or the whole app as a fallback) for the
    /// message composer: an `AXScrollArea` that wraps an `AXTextArea` but no
    /// `AXTable` (which would make it the message list instead).
    /// The composer: the editable text area outside the message list. A
    /// read-only one never counts: KakaoTalk 26.8 shows the newest incoming
    /// message ("이모티콘을 보냈습니다.") in a 16 px text area above the
    /// composer while the history is scrolled up, which read as a draft.
    fn find_input_field(snap: &AxNode) -> Option<AXUIElement> {
        let mut scroll_areas = Vec::new();
        snap.find_all("AXScrollArea", &mut scroll_areas);
        let value: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXValue"));
        scroll_areas
            .into_iter()
            .filter(|area| area.find_first("AXTable").is_none())
            .filter_map(|area| area.find_first("AXTextArea"))
            .find(|field| field.element.is_settable(&value).unwrap_or(false))
            .map(|field| field.element.clone())
    }

    fn find_file_picker_sheet(window: &AXUIElement) -> Result<Option<AXUIElement>> {
        let snap = snapshot_with(window, true)?;
        Ok(snap
            .find_first("AXSheet")
            .map(|sheet| sheet.element.clone()))
    }

    fn button_with_shortcut(root: &AXUIElement, shortcut: &str) -> Result<Option<AXUIElement>> {
        let snap = snapshot_shallow(root)?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let matches: Vec<_> = buttons
            .into_iter()
            .filter(|button| {
                [&button.help, &button.description]
                    .into_iter()
                    .flatten()
                    .any(|label| label.ends_with(shortcut))
            })
            .collect();
        match matches.as_slice() {
            [] => Ok(None),
            [button] => Ok(Some(button.element.clone())),
            _ => anyhow::bail!("multiple KakaoTalk buttons advertise shortcut {shortcut}"),
        }
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn find_go_to_path_field(sheet: &AXUIElement) -> Result<Option<AXUIElement>> {
        let snap = snapshot(sheet)?;
        let mut text_fields = Vec::new();
        snap.find_all("AXTextField", &mut text_fields);
        let candidates: Vec<_> = text_fields
            .into_iter()
            .filter(|field| {
                attr_as_string(&field.element, "AXIdentifier").as_deref() == Some("PathTextField")
            })
            .collect();
        match candidates.as_slice() {
            [] => Ok(None),
            [field] => Ok(Some(field.element.clone())),
            _ => anyhow::bail!("file picker exposes multiple Go to Folder path fields"),
        }
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn sheet_has_selected_filename(sheet: &AXUIElement, filename: &str) -> Result<bool> {
        let snap = snapshot(sheet)?;
        let mut labels = Vec::new();
        snap.find_all("AXStaticText", &mut labels);
        snap.find_all("AXTextField", &mut labels);
        Ok(labels.into_iter().any(|label| {
            if label.value.as_deref() != Some(filename) {
                return false;
            }
            let mut current = label.element.clone();
            for _ in 0..8 {
                if attr_as_bool(&current, "AXSelected") == Some(true) {
                    return true;
                }
                let Some(parent) = attr_as_element(&current, "AXParent") else {
                    break;
                };
                current = parent;
            }
            false
        }))
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn selected_filename_row_center(
        sheet: &AXUIElement,
        filename: &str,
    ) -> Result<Option<CGPoint>> {
        let snap = snapshot(sheet)?;
        let mut rows = Vec::new();
        snap.find_all("AXRow", &mut rows);
        for row in rows {
            if attr_as_bool(&row.element, "AXSelected") != Some(true) {
                continue;
            }
            let mut labels = Vec::new();
            row.find_all("AXStaticText", &mut labels);
            row.find_all("AXTextField", &mut labels);
            if !labels
                .iter()
                .any(|label| label.value.as_deref() == Some(filename))
            {
                continue;
            }
            return Ok(row
                .position_x
                .zip(row.position_y)
                .zip(row.width.zip(row.height))
                .map(|((x, y), (width, height))| CGPoint::new(x + width / 2.0, y + height / 2.0)));
        }
        Ok(None)
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn unique_filename_element_center(
        sheet: &AXUIElement,
        filename: &str,
    ) -> Result<Option<(AXUIElement, CGPoint)>> {
        let snap = snapshot(sheet)?;
        let mut labels = Vec::new();
        snap.find_all("AXStaticText", &mut labels);
        snap.find_all("AXTextField", &mut labels);
        let has_text_field = labels
            .iter()
            .any(|label| label.role == "AXTextField" && label.value.as_deref() == Some(filename));
        let raw_targets: Vec<_> = labels
            .into_iter()
            .filter_map(|label| {
                if label.value.as_deref() != Some(filename)
                    || (has_text_field && label.role != "AXTextField")
                {
                    return None;
                }
                let mut target = label.element.clone();
                for _ in 0..8 {
                    if role(&target) == "AXRow" {
                        break;
                    }
                    let Some(parent) = attr_as_element(&target, "AXParent") else {
                        break;
                    };
                    target = parent;
                }
                let target = snapshot(&target).ok()?;
                let center = target
                    .position_x
                    .zip(target.position_y)
                    .zip(target.width.zip(target.height))
                    .map(|((x, y), (width, height))| {
                        CGPoint::new(x + width / 2.0, y + height / 2.0)
                    })?;
                Some((target.element, center))
            })
            .collect();
        let mut targets: Vec<(AXUIElement, CGPoint)> = Vec::new();
        for (element, center) in raw_targets {
            if !targets.iter().any(|(_, existing)| {
                (existing.x - center.x).abs() <= 2.0 && (existing.y - center.y).abs() <= 2.0
            }) {
                targets.push((element, center));
            }
        }
        match targets.as_slice() {
            [] => Ok(None),
            [(element, center)] => Ok(Some((element.clone(), *center))),
            _ => anyhow::bail!("file picker exposes multiple exact filename elements"),
        }
    }

    fn find_enabled_button_with_title(
        sheet: &AXUIElement,
        expected_title: &str,
    ) -> Result<Option<AXUIElement>> {
        let snap = snapshot(sheet)?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let candidates: Vec<_> = buttons
            .into_iter()
            .filter(|button| {
                attr_as_string(&button.element, "AXTitle").as_deref() == Some(expected_title)
                    && attr_as_bool(&button.element, "AXEnabled") == Some(true)
            })
            .collect();
        match candidates.as_slice() {
            [] => Ok(None),
            [button] => Ok(Some(button.element.clone())),
            _ => anyhow::bail!("file flow exposes multiple enabled '{expected_title}' buttons"),
        }
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn find_enabled_open_button(sheet: &AXUIElement) -> Result<Option<AXUIElement>> {
        if let Some(button) = attr_as_element(sheet, "AXDefaultButton") {
            if attr_as_bool(&button, "AXEnabled") == Some(true) {
                return Ok(Some(button));
            }
        }

        let snap = snapshot(sheet)?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let candidates: Vec<_> = buttons
            .into_iter()
            .filter(|button| {
                attr_as_bool(&button.element, "AXEnabled") == Some(true)
                    && (attr_as_string(&button.element, "AXIdentifier").as_deref()
                        == Some("OKButton")
                        || attr_as_string(&button.element, "AXTitle")
                            .is_some_and(|title| super::OPEN_TITLES.contains(&title.as_str())))
            })
            .collect();
        match candidates.as_slice() {
            [] => Ok(None),
            [button] => Ok(Some(button.element.clone())),
            _ => anyhow::bail!("file picker exposes multiple enabled Open controls"),
        }
    }

    fn sheet_has_exact_filename(sheet: &AXUIElement, filename: &str) -> Result<bool> {
        let snap = snapshot(sheet)?;
        let mut labels = Vec::new();
        snap.find_all("AXStaticText", &mut labels);
        snap.find_all("AXTextField", &mut labels);
        let target = nfc(filename);
        let matches = labels
            .into_iter()
            .filter(|label| {
                label
                    .value
                    .as_deref()
                    .is_some_and(|value| nfc(value) == target)
            })
            .count();
        if matches > 1 {
            anyhow::bail!("file flow exposes the exact filename more than once");
        }
        Ok(matches == 1)
    }

    #[allow(dead_code)] // Retained for the legacy native-picker path.
    fn wait_for_single_file_preview(
        window: &AXUIElement,
        filename: &str,
    ) -> Result<(AXUIElement, AXUIElement)> {
        let deadline = Instant::now() + FILE_PICKER_READY_TIMEOUT;
        loop {
            if let Some(preview) = find_file_picker_sheet(window)? {
                if sheet_has_exact_filename(&preview, filename)? {
                    if let Some(button) = find_enabled_button_with_title(&preview, "Send 1 files")?
                    {
                        return Ok((preview, button));
                    }
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "KakaoTalk did not expose the exact single-file attachment preview in time"
                );
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// Title of the chat window's send button (KakaoTalk 26.9.0, Korean UI).
    const SEND_BUTTON_TITLE: &str = "전송";
    /// Rows from the end of the message table that delivery verification
    /// compares. The committed bubble is the newest row; a larger window only
    /// makes every verification poll slower.
    const DELIVERY_TAIL_ROWS: usize = 12;

    /// The human-presence boundary every AX write passes right before it
    /// changes KakaoTalk. An allowlisted chat is already approved; any other
    /// target needs macOS device-owner authentication, and a declined prompt
    /// is `chat_not_allowed` (nothing was sent).
    /// `Ok(true)` when the macOS prompt ran (time passed: re-read any
    /// baseline), `Ok(false)` when the allowlist was the approval.
    /// Every action first prints which chat this is (members, recent senders,
    /// last activity); a chat that needs the prompt also shows its newest
    /// messages, so the person approving recognises it.
    fn require_send_approval(
        window: &AXUIElement,
        chat_display_name: &str,
        action: &str,
    ) -> Result<bool> {
        let picked = PICKED_DUPLICATE.get().map(String::as_str);
        // The card's chat id is shown, never trusted: it is matched from
        // message text, which anyone in a chat can copy.
        let view = chat_view(window, 8).ok();
        let allowlisted = !super::needs_device_owner_auth(chat_display_name);
        let reason = super::auth_reason(allowlisted || super::skips_allowlist(), picked);
        let lines = if allowlisted && picked.is_none() {
            0
        } else {
            3
        };
        if let Some(view) = &view {
            eprintln!("{}", super::chat_card(chat_display_name, view, lines));
        }
        let Some(reason) = reason else {
            return Ok(false);
        };
        let target =
            crate::util::escape_terminal_text(&crate::util::truncate(chat_display_name, 80));
        openkakao_cli::human_auth::require_device_owner_auth(&format!(
            "Approve one KakaoTalk {action} ‘{target}’ ({reason}) after reviewing the terminal preview"
        ))
        .map_err(|error| {
            let code = if picked.is_some() {
                ErrorCode::ChatAmbiguous
            } else {
                ErrorCode::ChatNotAllowed
            };
            coded(code, format!("'{chat_display_name}' ({reason}): {error:#}"))
        })?;
        Ok(true)
    }

    /// Set by `open_chat` when `--pick` chose among same-name chats (one
    /// command per process), so the send asks for device-owner authentication.
    static PICKED_DUPLICATE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    struct ChatWindowElements {
        input_field: AXUIElement,
        message_table: AXUIElement,
        /// The unique `전송` button, if this build exposes one.
        send_button: Option<AXUIElement>,
    }

    /// Resolve the composer, message table and send button from one shallow
    /// window snapshot (the message history itself is not walked).
    fn inspect_chat_window(root: &AXUIElement) -> Result<Option<ChatWindowElements>> {
        let snap = snapshot_shallow(root)?;
        let Some(input_field) = find_input_field(&snap) else {
            return Ok(None);
        };
        let Some(message_table) = snap.find_first("AXTable") else {
            return Ok(None);
        };
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let send_buttons: Vec<_> = buttons
            .into_iter()
            .filter(|button| button.title.as_deref() == Some(SEND_BUTTON_TITLE))
            .collect();
        let send_button = match send_buttons.as_slice() {
            [] => None,
            [button] => Some(button.element.clone()),
            _ => anyhow::bail!(
                "the chat window exposes {} '{SEND_BUTTON_TITLE}' buttons; refusing to guess",
                send_buttons.len()
            ),
        };
        Ok(Some(ChatWindowElements {
            input_field,
            message_table: message_table.element.clone(),
            send_button,
        }))
    }

    /// Wait (bounded) until the exact chat window exposes a readable composer
    /// and message table, as a just-opened window can take a moment.
    fn wait_for_chat_elements(
        app: &AXUIElement,
        chat_display_name: &str,
    ) -> Result<(AXUIElement, ChatWindowElements)> {
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            if let Some(window) = find_chat_window(app, chat_display_name)? {
                if let Some(elements) = inspect_chat_window(&window)? {
                    return Ok((window, elements));
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "exact chat window '{chat_display_name}' did not open with a readable composer in time"
                );
            }
            sleep(Duration::from_millis(150));
        }
    }

    fn table_center_x(table: &AXUIElement) -> Option<f64> {
        attr_as_frame(table).map(|(origin, size)| origin.x + size.width / 2.0)
    }

    /// Snapshot message-table rows from the newest backwards, one row subtree
    /// at a time, until `enough` (called with each newest-first row) says
    /// stop. Returns the rows in on-screen (chronological) order. A long chat
    /// history therefore costs only the rows actually needed instead of a
    /// full-table snapshot per read.
    fn tail_rows(
        table: &AXUIElement,
        mut enough: impl FnMut(&AxNode) -> bool,
    ) -> Result<(Vec<AxNode>, usize)> {
        let started = Instant::now();
        let children = table
            .children()
            .map_err(|error| anyhow!("could not read the message list's rows: {error:?}"))?;
        let mut newest_first = Vec::new();
        'children: for index in (0..children.len()).rev() {
            let Some(child) = children.get(index).map(|child| (*child).clone()) else {
                continue;
            };
            let mut rows = Vec::new();
            snapshot(&child)?.into_rows(&mut rows);
            for row in rows.into_iter().rev() {
                let stop = enough(&row);
                newest_first.push(row);
                if stop {
                    break 'children;
                }
            }
        }
        if crate::util::debug_enabled() {
            eprintln!(
                "[ax] message tail: {} of {} rows, {} nodes in {:.1} ms",
                newest_first.len(),
                children.len(),
                newest_first.iter().map(AxNode::count).sum::<usize>(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        newest_first.reverse();
        Ok((newest_first, children.len() as usize))
    }

    /// The newest `count` message bubbles of a chat window, in chronological
    /// order. A row with an `AXTextArea` is a text message; a row with no
    /// `AXTextArea` but an `AXImage` descendant becomes the placeholder
    /// "[사진]"; a row with a share-labeled `AXButton` ("공유") becomes
    /// "[파일]". Rows matching none of these (date separators, system notices)
    /// are skipped.
    fn read_recent_messages(window: &AXUIElement, count: usize) -> Result<Vec<AxMessage>> {
        let snap = snapshot_shallow(window)?;
        let Some(table) = snap.find_first("AXTable") else {
            return Ok(Vec::new());
        };
        let center = table.center_x();
        let mut found = 0;
        let (rows, _) = tail_rows(&table.element, |row| {
            found += usize::from(message_from_row(row, center).is_some());
            found >= count
        })?;
        let mut messages: Vec<AxMessage> = rows
            .iter()
            .filter_map(|row| message_from_row(row, center))
            .collect();
        super::carry_senders(&mut messages);
        Ok(messages)
    }

    /// The row's upload-progress label (an outgoing file still uploading).
    fn uploading_label(row: &AxNode) -> Option<&AxNode> {
        let mut texts = Vec::new();
        row.find_all("AXStaticText", &mut texts);
        texts
            .into_iter()
            .find(|node| node.value.as_deref().is_some_and(super::is_uploading_label))
    }

    /// The row's clock label, if one of its static texts reads like a time.
    fn time_label(row: &AxNode) -> Option<&AxNode> {
        let mut texts = Vec::new();
        row.find_all("AXStaticText", &mut texts);
        texts
            .into_iter()
            .find(|node| node.value.as_deref().is_some_and(super::is_clock_label))
    }

    /// A text bubble is outgoing when it sits right of the table's center. A
    /// placeholder row (file or photo) has no bubble to measure: KakaoTalk 26.9
    /// draws a file as a share button, name, validity and size texts and open
    /// buttons that straddle the center, so its clock label decides, drawn
    /// left of outgoing bubbles and right of incoming ones (measured
    /// 2026-11-06).
    fn row_is_outgoing(row: &AxNode, bubble: &AxNode, table_center: Option<f64>) -> bool {
        // Only our own files show an upload in progress.
        if uploading_label(row).is_some() {
            return true;
        }
        let Some(table_center) = table_center else {
            return false;
        };
        let bubble_center = bubble.center_x();
        // The clock sits on a card's outer side: left of our files and
        // photos, right of incoming ones. The card is everything else in the
        // row (a photo, or a file's name, size and 열기 buttons), without the
        // clock, 공유 and 프로필. The table's center is no reference, since a
        // wide window moves a right-aligned card's clock past it (26.8,
        // measured 2026-11-07).
        let card_center = {
            let clock = time_label(row).map(|t| t.element.clone());
            let mut nodes = Vec::new();
            for role in ["AXStaticText", "AXImage", "AXButton"] {
                row.find_all(role, &mut nodes);
            }
            let spans: Vec<(f64, f64)> = nodes
                .into_iter()
                .filter(|n| clock.as_ref().is_none_or(|c| !same_element(c, &n.element)))
                .filter(|n| !matches!(n.description.as_deref(), Some("공유" | "프로필")))
                .filter_map(|n| Some((n.position_x?, n.position_x? + n.width?)))
                .collect();
            let left = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
            let right = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
            (left.is_finite() && right.is_finite()).then(|| (left + right) / 2.0)
        };
        match (
            bubble.role.as_str(),
            time_label(row).and_then(AxNode::center_x),
        ) {
            ("AXTextArea", _) => bubble_center.is_some_and(|x| x > table_center),
            (_, Some(time_center)) => match card_center {
                Some(card) => time_center < card,
                None => time_center < table_center,
            },
            // A file row's share button sits on the outer side: left of an
            // outgoing bubble (measured), hence right of an incoming one.
            ("AXButton", None) => bubble_center.is_some_and(|x| x < table_center),
            (_, None) => bubble_center.is_some_and(|x| x > table_center),
        }
    }

    fn message_from_row(row: &AxNode, table_center_x: Option<f64>) -> Option<AxMessage> {
        let (text, bubble) = message_row_text(row)?;
        let outgoing = row_is_outgoing(row, bubble, table_center_x);
        // Same-minute messages show the clock only on the last one; a file
        // row's other texts (name, validity, size) are never a time.
        // The clock label's AXHelp is its date (26.8: "2026. 11. 7.").
        let stamp = |t: &AxNode| match (t.help.as_deref(), t.value.as_deref()) {
            (Some(date), Some(label)) if super::is_clock_label(label) => {
                super::clock_of(label).map(|clock| format!("{date} {clock}"))
            }
            (None, Some(label)) if super::is_clock_label(label) => {
                super::clock_of(label).map(str::to_string)
            }
            _ => t.help.clone().or_else(|| t.value.clone()),
        };
        // A same-minute row has no clock; its first static text then carries
        // the date in AXHelp, unless it is the sender's name (no AXHelp).
        let time = time_label(row)
            .or_else(|| {
                (bubble.role == "AXTextArea")
                    .then(|| row.find_first("AXStaticText"))
                    .flatten()
                    .filter(|t| t.help.is_some())
            })
            .and_then(stamp);
        Some(AxMessage {
            time,
            sender: (!outgoing).then(|| sender_label(row)).flatten(),
            text,
            outgoing,
        })
    }

    /// The sender's name on the first bubble of an incoming run: the row
    /// shows a 프로필 button and the name as its first non-clock text
    /// (26.8, measured 2026-11-07).
    fn sender_label(row: &AxNode) -> Option<String> {
        let mut buttons = Vec::new();
        row.find_all("AXButton", &mut buttons);
        if !buttons
            .iter()
            .any(|button| button.description.as_deref() == Some("프로필"))
        {
            return None;
        }
        let mut texts = Vec::new();
        row.find_all("AXStaticText", &mut texts);
        texts
            .into_iter()
            .filter_map(|node| node.value.as_deref().map(str::trim))
            // Not the clock, 수정됨, or the unread count ("1") KakaoTalk shows
            // beside a message someone has not read yet.
            .find(|value| {
                !value.is_empty()
                    && !super::is_clock_label(value)
                    && *value != "수정됨"
                    && !value.bytes().all(|b| b.is_ascii_digit())
            })
            .map(str::to_string)
    }

    /// A group chat's member count from its `대화상대 N명` button.
    fn chat_members(window: &AXUIElement) -> Option<u32> {
        let snap = snapshot_shallow(window).ok()?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        buttons
            .iter()
            .find_map(|button| button.title.as_deref().and_then(super::member_count))
    }

    /// `ax-read --expand`: replace each message KakaoTalk folded behind
    /// 전체보기 with its whole text. The bubble's 전체보기 button (unnamed,
    /// as wide as the bubble, and pressable although it lists no actions)
    /// opens a dialog with the whole message; it is read and closed with its
    /// 닫기 버튼, without raising KakaoTalk (26.8, measured 2026-11-07). A
    /// preview shown by two bubbles stays folded.
    fn expand_folded(
        app: &AXUIElement,
        window: &AXUIElement,
        messages: &mut [AxMessage],
    ) -> Result<()> {
        if !messages
            .iter()
            .any(|m| super::folded_prefix(&m.text).is_some())
        {
            return Ok(());
        }
        let snap = snapshot_shallow(window)?;
        let Some(table) = snap.find_first("AXTable") else {
            return Ok(());
        };
        let mut found = 0;
        let (rows, _) = tail_rows(&table.element, |row| {
            found += usize::from(message_row_text(row).is_some());
            found >= messages.len()
        })?;
        for message in messages.iter_mut() {
            if super::folded_prefix(&message.text).is_none() {
                continue;
            }
            let same: Vec<&AxNode> = rows
                .iter()
                .filter(|row| message_row_text(row).is_some_and(|(text, _)| text == message.text))
                .collect();
            let [row] = same.as_slice() else {
                eprintln!("note: two bubbles show the same folded preview; kept it folded");
                continue;
            };
            let full = read_see_all(app, row)?;
            if super::bubble_shows(&message.text, &full) {
                message.text = full;
            } else {
                eprintln!("note: KakaoTalk's 전체보기 text does not start with the preview; kept it folded");
            }
        }
        Ok(())
    }

    /// Open a folded bubble's 전체보기 dialog, read its text and close it.
    fn read_see_all(app: &AXUIElement, row: &AxNode) -> Result<String> {
        let mut buttons = Vec::new();
        row.find_all("AXButton", &mut buttons);
        let unnamed = |text: &Option<String>| text.as_deref().is_none_or(|t| t.trim().is_empty());
        let wide: Vec<&&AxNode> = buttons
            .iter()
            .filter(|b| unnamed(&b.description) && unnamed(&b.title))
            .filter(|b| b.width.is_some_and(|w| w >= 150.0))
            .collect();
        let [see_all] = wide.as_slice() else {
            anyhow::bail!("the folded bubble shows no single 전체보기 button");
        };
        let before = app_windows(app);
        see_all
            .element
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not press 전체보기: {error:?}"))?;
        let dialog = new_dialog(app, &before, OPEN_CHAT_TIMEOUT, |_| true)
            .ok_or_else(|| anyhow!("KakaoTalk's 전체보기 dialog did not appear"))?;
        let text = snapshot(&dialog)?
            .find_first("AXTextArea")
            .and_then(|t| t.value.clone());
        close_dialog(app, &dialog, "전체보기")?;
        text.ok_or_else(|| anyhow!("the 전체보기 dialog shows no text"))
    }

    fn app_windows(app: &AXUIElement) -> Vec<AXUIElement> {
        app.windows()
            .map(|list| list.iter().map(|w| (*w).clone()).collect())
            .unwrap_or_default()
    }

    fn same_element(left: &AXUIElement, right: &AXUIElement) -> bool {
        unsafe { CFEqual(left.as_CFTypeRef(), right.as_CFTypeRef()) != 0 }
    }

    /// A KakaoTalk dialog window that was not among `before` and passes `accept`.
    fn new_dialog(
        app: &AXUIElement,
        before: &[AXUIElement],
        timeout: Duration,
        accept: impl Fn(&AXUIElement) -> bool,
    ) -> Option<AXUIElement> {
        let deadline = Instant::now() + timeout;
        loop {
            let found = app_windows(app).into_iter().find(|w| {
                !before.iter().any(|old| same_element(old, w))
                    && attr_as_string(w, "AXSubrole").as_deref() == Some("AXDialog")
                    && accept(w)
            });
            if found.is_some() || Instant::now() >= deadline {
                return found;
            }
            sleep(Duration::from_millis(50));
        }
    }

    /// Close a dialog with its window's close button and wait until it is gone.
    fn close_dialog(app: &AXUIElement, dialog: &AXUIElement, what: &str) -> Result<()> {
        let close = attr_as_element(dialog, "AXCloseButton").ok_or_else(|| {
            anyhow!("the {what} dialog has no close button; close it in KakaoTalk")
        })?;
        let _ = close.perform_action(&CFString::new(kAXPressAction));
        let deadline = Instant::now() + Duration::from_secs(2);
        while app_windows(app).iter().any(|w| same_element(w, dialog)) {
            if Instant::now() >= deadline {
                anyhow::bail!("the {what} dialog did not close; close it in KakaoTalk");
            }
            sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    fn chat_view(window: &AXUIElement, count: usize) -> Result<ChatView> {
        let messages = read_recent_messages(window, count)?;
        // Notification Center may be unreadable (no Full Disk Access); the
        // id then stays unknown.
        let chat_id = window
            .title()
            .ok()
            .zip(crate::commands::notif_watch::retained_messages().ok())
            .and_then(|(title, retained)| {
                match crate::commands::notif_watch::resolve_chat_id(
                    &title.to_string(),
                    &messages,
                    &retained,
                ) {
                    crate::commands::notif_watch::ChatIdMatch::Found { chat_id, .. } => {
                        Some(chat_id)
                    }
                    _ => None,
                }
            });
        Ok(ChatView {
            members: chat_members(window),
            chat_id,
            messages,
        })
    }

    /// The newest message rows with their AX identity, for anchoring (spacer
    /// rows and date separators are skipped, as for attachments).
    fn delivery_observations(
        table: &AXUIElement,
    ) -> Result<Vec<(AXUIElement, super::DeliveryObservation)>> {
        let center = table_center_x(table);
        let mut seen = 0;
        let (rows, _) = tail_rows(table, |_| {
            seen += 1;
            seen >= DELIVERY_TAIL_ROWS
        })?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                let message = message_from_row(row, center)?;
                let observation = super::DeliveryObservation {
                    text: message.text,
                    outgoing: message.outgoing,
                };
                Some((row.element.clone(), observation))
            })
            .collect())
    }

    /// [`super::is_verified_delivery`] anchored on the baseline's newest row.
    /// The anchor must be the same AX element still showing the same content,
    /// so a recycled row view can never move it.
    fn text_delivery_verified(
        baseline: &[(AXUIElement, super::DeliveryObservation)],
        current: &[(AXUIElement, super::DeliveryObservation)],
        message: &str,
        composer_value: Option<&str>,
    ) -> bool {
        let start = super::appended_rows_start(baseline.last(), current, |left, right| {
            left.1 == right.1
                && unsafe { CFEqual(left.0.as_CFTypeRef(), right.0.as_CFTypeRef()) != 0 }
        });
        let observations = |rows: &[(AXUIElement, super::DeliveryObservation)]| -> Vec<_> {
            rows.iter()
                .map(|(_, observation)| observation.clone())
                .collect()
        };
        super::is_verified_delivery(
            &observations(baseline),
            &observations(current),
            start,
            message,
            composer_value,
        )
    }

    /// The newest message-table rows (with their AX identity, for anchoring)
    /// and the table's total row count. A row mentions the file only when a
    /// non-text node shows exactly its (NFC) name, so `a.pdf` never matches
    /// `data.pdf` and a typed message never counts.
    fn attachment_observations(
        table: &AXUIElement,
        filename: &str,
    ) -> Result<(usize, Vec<(AXUIElement, super::AttachmentObservation)>)> {
        let center = table_center_x(table);
        let mut seen = 0;
        let (rows, total) = tail_rows(table, |_| {
            seen += 1;
            seen >= DELIVERY_TAIL_ROWS
        })?;
        let target = nfc(filename);
        let shows_filename = |node: &AxNode| {
            node.role != "AXTextArea"
                && [&node.value, &node.title, &node.description]
                    .into_iter()
                    .flatten()
                    .any(|label| nfc(label.trim()) == target)
        };
        Ok((
            total,
            rows.iter()
                // Message rows only: the table ends with an empty spacer row
                // (and holds date separators), which would otherwise be the
                // baseline's "newest row" with nothing ever appended after it.
                .filter_map(|row| {
                    let (text, bubble) = message_row_text(row)?;
                    let observation = super::AttachmentObservation {
                        // Typed text that happens to read "[파일]" is not a bubble.
                        placeholder: (bubble.role != "AXTextArea").then_some(text),
                        outgoing: row_is_outgoing(row, bubble, center),
                        mentions_filename: row.any(&shows_filename),
                    };
                    Some((row.element.clone(), observation))
                })
                .collect(),
        ))
    }

    /// Classify one message row into displayable text: the row's own
    /// `AXTextArea` value if present, else a placeholder if the row looks
    /// like an image or file share, else `None` (not a real message row).
    fn message_row_text(row: &AxNode) -> Option<(String, &AxNode)> {
        if let Some(text_area) = row.find_first("AXTextArea") {
            if let Some(text) = &text_area.value {
                return Some((text.clone(), text_area));
            }
        }

        if let Some(image) = row.find_first("AXImage") {
            return Some((PHOTO_BUBBLE_PLACEHOLDER.to_string(), image));
        }

        let mut buttons = Vec::new();
        row.find_all("AXButton", &mut buttons);
        if let Some(button) = buttons
            .into_iter()
            .find(|button| button.description.as_deref() == Some("공유"))
        {
            return Some((super::FILE_BUBBLE_PLACEHOLDER.to_string(), button));
        }
        // A file still uploading has no share button yet (measured 2026-11-06
        // with a 30 MB file: icon, name, "업로드 중", 취소).
        if let Some(progress) = uploading_label(row) {
            return Some((super::FILE_BUBBLE_PLACEHOLDER.to_string(), progress));
        }

        None
    }

    /// Find exactly one already-open chat window by its full title. A substring
    /// match is unsafe here: an allowed target named `Alice` must never reuse an
    /// unrelated `Alice & Bob` window, and duplicate exact titles are ambiguous.
    fn find_chat_window(app: &AXUIElement, chat_display_name: &str) -> Result<Option<AXUIElement>> {
        let windows = app
            .windows()
            .map_err(|error| anyhow!("AXWindows read failed: {error:?}"))?;
        let titles: Vec<Option<String>> = windows
            .iter()
            .map(|window| window.title().map(|title| title.to_string()).ok())
            .collect();
        match super::match_chat_row(&titles, chat_display_name) {
            super::ChatMatch::NotFound => Ok(None),
            super::ChatMatch::Found(index) => windows
                .get(index as isize)
                .map(|window| (*window).clone())
                .map(Some)
                .ok_or_else(|| anyhow!("matched chat window disappeared during AX lookup")),
            super::ChatMatch::Ambiguous(count) => Err(coded(
                ErrorCode::ChatAmbiguous,
                format!(
                    "chat name '{chat_display_name}' matches {count} open windows exactly — ambiguous, refusing to guess"
                ),
            )),
        }
    }

    fn wait_for_composer_value(field: &AXUIElement, expected: &str) -> bool {
        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        loop {
            if attr_as_string(field, "AXValue").as_deref() == Some(expected) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }
    }

    /// Every button of a sheet with the facts `pick_preview_send_button`
    /// needs. The sheet's `AXDefaultButton`/`AXCancelButton` attributes are
    /// used where AppKit provides them; localized titles cover the rest.
    fn sheet_buttons(sheet: &AXUIElement) -> Result<Vec<(AXUIElement, super::SheetButton)>> {
        // Shallow: the open panel's file list can hold thousands of rows.
        let snap = snapshot_with(sheet, true)?;
        let default = attr_as_element(sheet, "AXDefaultButton");
        let cancel = attr_as_element(sheet, "AXCancelButton");
        let same = |left: &Option<AXUIElement>, right: &AXUIElement| {
            left.as_ref().is_some_and(|left| unsafe {
                CFEqual(left.as_CFTypeRef(), right.as_CFTypeRef()) != 0
            })
        };
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        Ok(buttons
            .into_iter()
            .map(|button| {
                let element = button.element.clone();
                let facts = super::SheetButton {
                    title: button.title.clone(),
                    enabled: attr_as_bool(&element, "AXEnabled") == Some(true),
                    is_default: same(&default, &element),
                    is_cancel: same(&cancel, &element)
                        || super::is_cancel_title(button.title.as_deref()),
                };
                (element, facts)
            })
            .collect())
    }

    /// The sheet's unique enabled Cancel (`AXCancelButton`, else `취소` or
    /// `Cancel`).
    fn find_cancel_button(sheet: &AXUIElement) -> Result<Option<AXUIElement>> {
        let cancels: Vec<_> = sheet_buttons(sheet)?
            .into_iter()
            .filter(|(_, facts)| facts.enabled && facts.is_cancel)
            .collect();
        match cancels.as_slice() {
            [] => Ok(None),
            [(button, _)] => Ok(Some(button.clone())),
            _ => anyhow::bail!("the sheet exposes several enabled Cancel buttons"),
        }
    }

    /// Press Cancel on whatever sheet the chat window shows. Used on every
    /// failure after KakaoTalk's file picker or preview opened, so nothing is
    /// left on the user's screen. Cancel can never send.
    /// The sheet attached to the chat window, if any. Reads only the window's
    /// direct children, so a poll costs a couple of AX calls.
    fn window_sheet(window: &AXUIElement) -> Option<AXUIElement> {
        let children = window.children().ok()?;
        (0..children.len())
            .filter_map(|index| children.get(index).map(|child| (*child).clone()))
            .find(|child| role(child) == "AXSheet")
    }

    /// A sheet's message, its static texts joined, for errors and stderr.
    fn sheet_text(sheet: &AXUIElement) -> String {
        let Ok(snap) = snapshot_with(sheet, true) else {
            return String::new();
        };
        let mut texts = Vec::new();
        snap.find_all("AXStaticText", &mut texts);
        texts
            .iter()
            .filter_map(|text| text.value.as_deref().map(str::trim))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Before a send or edit: dismiss alerts KakaoTalk left over the chat
    /// window that only acknowledge something (a failed upload leaves one or
    /// two, and they block the next file picker), printing each to stderr.
    /// Any other sheet asks a human and refuses with `chat_dialog_open`.
    fn clear_leftover_alerts(window: &AXUIElement) -> Result<()> {
        for _ in 0..3 {
            let Some(sheet) = window_sheet(window) else {
                return Ok(());
            };
            let text = sheet_text(&sheet);
            let buttons = sheet_buttons(&sheet)?;
            let facts: Vec<super::SheetButton> =
                buttons.iter().map(|(_, facts)| facts.clone()).collect();
            if !super::is_acknowledgement_only(&facts) {
                return Err(coded(
                    ErrorCode::ChatDialogOpen,
                    format!(
                        "KakaoTalk shows a dialog in this chat window ({text:?}); nothing was sent"
                    ),
                ));
            }
            eprintln!("Dismissing a KakaoTalk alert left in the chat window: {text:?}");
            buttons[0]
                .0
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("could not press the alert's 확인/OK: {error:?}"))?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while window_sheet(window).is_some_and(|now| unsafe {
                CFEqual(now.as_CFTypeRef(), sheet.as_CFTypeRef()) != 0
            }) {
                if Instant::now() >= deadline {
                    return Err(coded(
                        ErrorCode::ChatDialogOpen,
                        format!("KakaoTalk's alert ({text:?}) did not close after 확인/OK; nothing was sent"),
                    ));
                }
                sleep(Duration::from_millis(50));
            }
        }
        Err(coded(
            ErrorCode::ChatDialogOpen,
            "KakaoTalk kept showing alerts in this chat window; nothing was sent",
        ))
    }

    fn cancel_sheet_best_effort(window: &AXUIElement) {
        let Ok(Some(sheet)) = find_file_picker_sheet(window) else {
            return;
        };
        if let Ok(Some(cancel)) = find_cancel_button(&sheet) {
            let _ = cancel.perform_action(&CFString::new(kAXPressAction));
        }
    }

    /// Open KakaoTalk's own file picker (proving this chat accepts files) and
    /// cancel it again. KakaoTalk 26.7's native open panel does not reliably
    /// expose its Go-to-Folder field or enable 열기/Open (#57), so the file is
    /// pasted instead; on any failure the picker is dismissed before
    /// returning.
    fn open_and_dismiss_file_picker(window: &AXUIElement, send_files: &AXUIElement) -> Result<()> {
        send_files
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not open KakaoTalk's file picker: {error:?}"))?;
        let result = (|| {
            let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
            let sheet = loop {
                if let Some(sheet) = find_file_picker_sheet(window)? {
                    break sheet;
                }
                if Instant::now() >= deadline {
                    anyhow::bail!("KakaoTalk's file picker did not appear in time");
                }
                sleep(Duration::from_millis(100));
            };
            let cancel = find_cancel_button(&sheet)?.ok_or_else(|| {
                anyhow!(
                    "file picker did not expose a unique enabled 취소/Cancel button (buttons: {:?})",
                    sheet_buttons(&sheet)
                        .map(|buttons| buttons.into_iter().map(|(_, facts)| facts.title).collect::<Vec<_>>())
                        .unwrap_or_default()
                )
            })?;
            cancel
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("could not cancel KakaoTalk's file picker: {error:?}"))?;
            let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
            while find_file_picker_sheet(window)?.is_some() {
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "file picker did not close; the clipboard paste was not attempted"
                    );
                }
                sleep(Duration::from_millis(100));
            }
            Ok(())
        })();
        if result.is_err() {
            cancel_sheet_best_effort(window);
        }
        result
    }

    /// A same-name hard link (or, across volumes, a copy) of the file at the
    /// top level of ~/Downloads, so the open panel reaches it with one sidebar
    /// click and the recipient still sees the original name. It is created
    /// without ever overwriting anything and removed on drop unless kept
    /// (an uncertain send keeps it for KakaoTalk's own retry).
    struct StagedFile {
        path: std::path::PathBuf,
        /// (device, inode) of the entry this call created.
        identity: (u64, u64),
        keep: bool,
    }

    fn file_identity(path: &std::path::Path) -> Option<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;
        path.symlink_metadata()
            .ok()
            .map(|metadata| (metadata.dev(), metadata.ino()))
    }

    impl StagedFile {
        fn create(
            source: &std::path::Path,
            downloads: &std::path::Path,
            filename: &str,
        ) -> Result<Self> {
            let path = downloads.join(filename);
            let conflict = || {
                coded(
                    ErrorCode::StagingConflict,
                    format!("cannot stage '{filename}': ~/Downloads already has an entry with that name"),
                )
            };
            if path.symlink_metadata().is_ok() {
                return Err(conflict());
            }
            if let Err(link_error) = std::fs::hard_link(source, &path) {
                if link_error.kind() == std::io::ErrorKind::AlreadyExists {
                    return Err(conflict());
                }
                // Across volumes: copy into a file this call creates itself.
                // Only that file is ever removed again.
                let mut target = match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(target) => target,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        return Err(conflict())
                    }
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("could not stage '{filename}' in ~/Downloads (hard link failed: {link_error})")
                        })
                    }
                };
                let copied = std::fs::File::open(source)
                    .and_then(|mut source| std::io::copy(&mut source, &mut target).map(drop));
                if let Err(error) = copied {
                    drop(target);
                    let _ = std::fs::remove_file(&path);
                    return Err(error)
                        .with_context(|| format!("could not copy '{filename}' into ~/Downloads"));
                }
            }
            let identity = file_identity(&path)
                .with_context(|| format!("could not inspect the staged '{filename}'"))?;
            Ok(Self {
                path,
                identity,
                keep: false,
            })
        }
    }

    impl Drop for StagedFile {
        fn drop(&mut self) {
            // Remove only the entry created above: if something else took the
            // name meanwhile, leave it alone.
            if !self.keep && file_identity(&self.path) == Some(self.identity) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    const SIDEBAR_DESCRIPTIONS: &[&str] = &["사이드바", "Sidebar"];

    /// The open panel's sidebar and file-list outlines (the list is absent
    /// unless the panel is in list view).
    fn open_panel_outlines(panel: &AXUIElement) -> Result<(AXUIElement, Option<AXUIElement>)> {
        let snap = snapshot_shallow(panel)?;
        let mut outlines = Vec::new();
        snap.find_all("AXOutline", &mut outlines);
        let is_sidebar = |outline: &AxNode| {
            outline
                .description
                .as_deref()
                .is_some_and(|description| SIDEBAR_DESCRIPTIONS.contains(&description))
        };
        let sidebar = outlines
            .iter()
            .find(|outline| is_sidebar(outline))
            .map(|outline| outline.element.clone())
            .ok_or_else(|| anyhow!("the open panel shows no 사이드바/Sidebar"))?;
        let lists: Vec<_> = outlines
            .iter()
            .filter(|outline| !is_sidebar(outline))
            .collect();
        Ok((
            sidebar,
            match lists.as_slice() {
                [list] => Some(list.element.clone()),
                _ => None,
            },
        ))
    }

    fn select_row_and_verify(row: &AXUIElement, what: &str) -> Result<()> {
        let selected: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXSelected"));
        row.set_attribute(&selected, CFBoolean::true_value().as_CFType())
            .map_err(|error| anyhow!("could not select {what}: {error:?}"))?;
        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        while attr_as_bool(row, "AXSelected") != Some(true) {
            if Instant::now() >= deadline {
                anyhow::bail!("{what} did not read back as selected");
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }
        Ok(())
    }

    /// The panel's unique enabled 열기/Open (by title, else `AXDefaultButton`).
    fn wait_for_open_button(panel: &AXUIElement) -> Result<AXUIElement> {
        let deadline = Instant::now() + FILE_PICKER_READY_TIMEOUT;
        loop {
            let buttons = sheet_buttons(panel)?;
            let titled: Vec<_> = buttons
                .iter()
                .filter(|(_, facts)| {
                    facts.enabled
                        && facts
                            .title
                            .as_deref()
                            .is_some_and(|title| super::OPEN_TITLES.contains(&title.trim()))
                })
                .collect();
            let chosen = match titled.as_slice() {
                [(button, _)] => Some(button.clone()),
                [] => buttons
                    .iter()
                    .find(|(_, facts)| facts.enabled && facts.is_default && !facts.is_cancel)
                    .map(|(button, _)| button.clone()),
                _ => anyhow::bail!("the open panel exposes several enabled 열기/Open buttons"),
            };
            if let Some(button) = chosen {
                return Ok(button);
            }
            if Instant::now() >= deadline {
                anyhow::bail!("the open panel's 열기/Open button did not become enabled");
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// `Some(true)` when the open panel's path popup names `folder`,
    /// `Some(false)` when it names another folder, `None` without a popup.
    fn panel_shows_folder(
        panel: &AXUIElement,
        folder: super::PickerFolder,
    ) -> Result<Option<bool>> {
        let snap = snapshot_shallow(panel)?;
        let mut popups = Vec::new();
        snap.find_all("AXPopUpButton", &mut popups);
        if popups.is_empty() {
            return Ok(None);
        }
        Ok(Some(popups.iter().any(|popup| {
            [&popup.value, &popup.title, &popup.description]
                .into_iter()
                .flatten()
                .any(|label| folder.labels().contains(&label.trim()))
        })))
    }

    /// Choose one file with Accessibility only (measured on KakaoTalk 26.9,
    /// Korean macOS, the user's app frontmost throughout): AXPress the chat's
    /// 파일전송 ⌘O button, select the sidebar folder row, select the exact file
    /// row, press 열기, and return KakaoTalk's preview sheet with its
    /// `1개 전송` button. No raise, no input events, no clipboard. On any
    /// failure the sheet left showing is cancelled.
    fn pick_file_in_open_panel(
        window: &AXUIElement,
        send_files: &AXUIElement,
        folder: super::PickerFolder,
        filename: &str,
    ) -> Result<(AXUIElement, AXUIElement, bool)> {
        send_files
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not open KakaoTalk's file picker: {error:?}"))?;
        let result = (|| {
            let deadline = Instant::now() + FILE_PICKER_READY_TIMEOUT;
            let panel = loop {
                if let Some(sheet) = find_file_picker_sheet(window)? {
                    break sheet;
                }
                if Instant::now() >= deadline {
                    anyhow::bail!("KakaoTalk's file picker did not appear in time");
                }
                sleep(Duration::from_millis(100));
            };

            let (sidebar, initial_list) = open_panel_outlines(&panel)?;
            // Names shown before the sidebar click: without a path popup, an
            // unchanged list may still be the previous folder's (see below).
            // With a popup the folder is confirmed by name, so the list is not
            // read twice (a 420-file Downloads cost 830 ms here).
            let has_popup = panel_shows_folder(&panel, folder)?.is_some();
            let before_names: Option<Vec<Option<String>>> = match initial_list {
                Some(list) if !has_popup => Some(
                    rows_with_label(&list, &["AXTextField"], "open panel file list (initial)")?
                        .into_iter()
                        .map(|(_, name)| name.as_deref().map(nfc))
                        .collect(),
                ),
                _ => None,
            };
            let places = rows_with_label(
                &sidebar,
                &["AXStaticText", "AXTextField"],
                "open panel sidebar",
            )?;
            let matching: Vec<_> = places
                .iter()
                .filter(|(_, label)| {
                    label
                        .as_deref()
                        .is_some_and(|label| folder.labels().contains(&label.trim()))
                })
                .collect();
            let [(place, _)] = matching.as_slice() else {
                anyhow::bail!(
                    "the open panel sidebar has {} rows for {folder:?} (labels seen: {:?})",
                    matching.len(),
                    places
                        .iter()
                        .map(|(_, label)| label.clone())
                        .collect::<Vec<_>>()
                );
            };
            select_row_and_verify(place, &format!("the {folder:?} sidebar row"))?;
            let selected_at = Instant::now();

            // The file list reloads for the selected folder. Read it only once
            // the panel's path popup names that folder, so a row of the
            // previously shown folder is never picked. Without a popup, the
            // list must differ from the one shown before the click, or settle.
            let target = nfc(filename);
            let deadline = Instant::now() + FILE_PICKER_READY_TIMEOUT;
            let mut last_select_error = None;
            let (list, file_row, strict) = loop {
                let confirmed = panel_shows_folder(&panel, folder)?;
                if confirmed != Some(false) {
                    if let (_, Some(list)) = open_panel_outlines(&panel)? {
                        let rows =
                            rows_with_label(&list, &["AXTextField"], "open panel file list")?;
                        let names: Vec<Option<String>> = rows
                            .iter()
                            .map(|(_, name)| name.as_deref().map(nfc))
                            .collect();
                        let settled = confirmed.is_some()
                            || before_names.as_ref() != Some(&names)
                            || selected_at.elapsed() >= LIST_RELOAD_SETTLE;
                        let picked = match super::pick_list_row(&names, &target) {
                            _ if !settled => None,
                            // Without a confirmed folder, the preview must also
                            // show the exact file name before anything is sent.
                            super::ListPick::Exact(index) => Some((index, confirmed.is_none())),
                            super::ListPick::Stem(index) => Some((index, true)),
                            super::ListPick::Ambiguous(count) => anyhow::bail!(
                                "{count} open-panel rows match '{filename}' (hidden extensions?); refusing to guess"
                            ),
                            super::ListPick::NotFound => None,
                        };
                        if let Some((index, strict)) = picked {
                            let row = rows[index].0.clone();
                            // A just-added file makes the panel reload its list
                            // (size, icon), which leaves this row element stale:
                            // re-read the rows and try again until the deadline.
                            match select_row_and_verify(
                                &row,
                                &format!("'{filename}' in the open panel"),
                            ) {
                                Ok(()) => break (list, row, strict),
                                Err(error) => last_select_error = Some(error),
                            }
                        }
                    }
                }
                if Instant::now() >= deadline {
                    return Err(match last_select_error {
                        // Listed but never selectable: KakaoTalk greys out file
                        // types it does not accept (measured with .bin, while
                        // the same bytes as .zip, and .txt/.docx, select).
                        Some(error) => coded(
                            ErrorCode::FileTypeRefused,
                            format!(
                                "the open panel lists '{filename}' but never selects it ({error}); KakaoTalk's picker greys out file types it does not accept"
                            ),
                        ),
                        None => anyhow!(
                            "the open panel did not list '{filename}' in {folder:?} (it must be in list view, with every name readable)"
                        ),
                    });
                }
                sleep(Duration::from_millis(150));
            };
            let selected_rows: AXAttribute<CFType> =
                AXAttribute::new(&CFString::new("AXSelectedRows"));
            if let Some(selected) = list
                .attribute(&selected_rows)
                .ok()
                .and_then(|value| value.downcast::<CFArray<*const std::ffi::c_void>>())
            {
                let only_target = selected.len() == 1
                    && selected
                        .get(0)
                        .is_some_and(|row| unsafe { CFEqual(*row, file_row.as_CFTypeRef()) != 0 });
                if !only_target {
                    anyhow::bail!("the open panel selected more than the requested file");
                }
            }
            wait_for_open_button(&panel)?
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("could not press 열기/Open: {error:?}"))?;
            let (preview, send_button) = wait_for_attachment_preview(window)?;
            Ok((preview, send_button, strict))
        })();
        if result.is_err() {
            cancel_sheet_best_effort(window);
        }
        result
    }

    /// `--fallback-keys`: the #57 route. Open and cancel the picker, stage a
    /// file URL on the clipboard, raise the exact window behind the idle wait
    /// and paste with ⌘V. The paste is verified by the pasteboard's
    /// `changeCount`, so KakaoTalk read exactly this file.
    fn paste_file_with_keys(
        app: &AXUIElement,
        window: &AXUIElement,
        composer: &AXUIElement,
        send_files: &AXUIElement,
        path: &std::path::Path,
        noun: &str,
        focus: &mut FocusGuard,
    ) -> Result<(AXUIElement, AXUIElement)> {
        let existing_draft = attr_as_string(composer, "AXValue").ok_or_else(|| {
            anyhow!("could not read the exact target's composer before the {noun} paste")
        })?;
        if !existing_draft.is_empty() {
            return Err(coded(
                ErrorCode::ComposerBusy,
                "the exact target's composer already contains a draft; refusing to disturb it",
            ));
        }
        focus.before_raise()?;
        open_and_dismiss_file_picker(window, send_files)?;
        let staged = stage_file_url(path)?;
        focus_window_and_verify(app, window, focus)
            .with_context(|| format!("could not focus exact target for the {noun} paste"))?;
        focus_and_verify(
            composer,
            &format!("exact target composer for the {noun} paste"),
        )?;
        // From the paste on, KakaoTalk may show a preview: every failure
        // cancels it before returning.
        let pasted = (|| {
            press_command_v()?;
            let preview = wait_for_attachment_preview(window)?;
            if pasteboard_change_count()? != staged {
                anyhow::bail!(
                    "the clipboard changed between staging the file and KakaoTalk's paste; refusing to send an attachment that may not be this file"
                );
            }
            Ok(preview)
        })();
        pasted.map_err(|error| {
            cancel_sheet_best_effort(window);
            coded(ErrorCode::AttachmentUnverified, format!("{error:#}"))
        })
    }

    /// Send one file through the signed KakaoTalk app, focus-free by default:
    /// the native open panel is driven by Accessibility alone and KakaoTalk's
    /// `1개 전송` preview button is pressed after macOS device-owner
    /// authentication (verified on KakaoTalk 26.9.0, macOS 26). A file outside
    /// the top level of Desktop/Documents/Downloads is staged into Downloads
    /// under its own name and removed again after a verified send.
    /// `--fallback-keys` uses the older clipboard + ⌘V route instead.
    ///
    /// Errors are pre-commit; a `delivery_uncertain` error means the send
    /// button was pressed but no new outgoing attachment bubble could be
    /// proven.
    pub fn send_attachment_via_ax(
        chat_display_name: &str,
        path: &std::path::Path,
        kind: super::AttachmentKind,
    ) -> Result<super::AttachmentReceipt> {
        let started = Instant::now();
        let noun = kind.noun();
        let fallback_keys = super::policy().fallback_keys;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                coded(
                    ErrorCode::InvalidFile,
                    format!("{noun} filename is not valid UTF-8"),
                )
            })?;
        if fallback_keys && !kind.clipboard_allows(path) {
            return Err(coded(
                ErrorCode::InvalidFile,
                "--fallback-keys stages only JPEG or PNG photos; send other images without it or with local-send-file",
            ));
        }
        let home = dirs::home_dir().context("could not resolve the home directory")?;
        let standard = |name: &str| {
            let folder = home.join(name);
            folder.canonicalize().unwrap_or(folder)
        };
        let downloads = standard("Downloads");
        let folder = path.parent().and_then(|parent| {
            super::picker_folder(
                parent,
                &standard("Desktop"),
                &standard("Documents"),
                &downloads,
            )
        });

        let _display_wake = hold_display_awake()?;
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let mut focus = FocusGuard::new(pid);
        open_chat(&app, chat_display_name, &mut focus)?;
        let (window, elements) = wait_for_chat_elements(&app, chat_display_name)?;
        clear_leftover_alerts(&window)?;
        trace(started, "chat window inspected");
        attachment_observations(&elements.message_table, filename)
            .context("could not read the exact target's messages before opening the file picker")?;
        let send_files = button_with_shortcut(&window, "⌘O")?
            .ok_or_else(|| anyhow!("could not find KakaoTalk's unique 파일전송 (⌘O) button"))?;

        let mut staged = None;
        let (preview_sheet, send_button, strict) = if fallback_keys {
            let (preview, button) = paste_file_with_keys(
                &app,
                &window,
                &elements.input_field,
                &send_files,
                path,
                noun,
                &mut focus,
            )?;
            // The pasteboard change count already proved which file it is.
            (preview, button, false)
        } else {
            let folder = match folder {
                Some(folder) => folder,
                None => {
                    staged = Some(StagedFile::create(path, &downloads, filename)?);
                    super::PickerFolder::Downloads
                }
            };
            // Not expected to raise anything; if KakaoTalk comes forward
            // anyway, the guard hands the foreground back.
            focus.watch();
            pick_file_in_open_panel(&window, &send_files, folder, filename)?
        };

        // Everything up to the press is pre-commit: any failure cancels the
        // preview so no staged attachment is left on screen.
        let prepared = (|| {
            let preview_shows_filename = sheet_has_exact_filename(&preview_sheet, filename)?;
            if strict && !preview_shows_filename {
                return Err(coded(
                    ErrorCode::AttachmentUnverified,
                    format!(
                        "the open panel matched '{filename}' without its extension (or in an unconfirmed folder) and KakaoTalk's preview does not show the name; turn on Finder › Settings › Advanced › Show all filename extensions and retry"
                    ),
                ));
            }
            trace(started, "single-file preview ready");
            // Authentication may take a while and needs nothing from
            // KakaoTalk; give the user's app back first.
            focus.restore();
            require_send_approval(&window, chat_display_name, &format!("{noun} to"))?;
            // Revalidate the exact recipient, the previewed name and the very
            // same one-file send button immediately before the commit.
            if find_chat_window(&app, chat_display_name)?.is_none() {
                anyhow::bail!("exact target window disappeared before the {noun} commit");
            }
            if preview_shows_filename && !sheet_has_exact_filename(&preview_sheet, filename)? {
                anyhow::bail!(
                    "previewed {noun} changed before commit; the send button was not pressed"
                );
            }
            let buttons = sheet_buttons(&preview_sheet)?;
            let facts: Vec<super::SheetButton> =
                buttons.iter().map(|(_, facts)| facts.clone()).collect();
            let still_send = super::pick_preview_send_button(&facts).is_some_and(|index| unsafe {
                CFEqual(buttons[index].0.as_CFTypeRef(), send_button.as_CFTypeRef()) != 0
            });
            if !still_send {
                anyhow::bail!(
                    "KakaoTalk's preview changed before commit; the send button was not pressed"
                );
            }
            attachment_observations(&elements.message_table, filename)
                .with_context(|| {
                    format!(
                        "could not re-read the exact target's messages before the {noun} commit"
                    )
                })
                .map(|baseline| (baseline, preview_shows_filename))
        })();
        let ((baseline_total, baseline), preview_shows_filename) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                cancel_sheet_best_effort(&window);
                return Err(error);
            }
        };
        // KakaoTalk 26.7 can return AX error -25200 even after accepting the
        // send action. Always verify the bubble before classifying the result.
        let commit_error = send_button
            .perform_action(&CFString::new(kAXPressAction))
            .err();
        focus.restore();
        trace(started, "attachment committed");

        let observations = |rows: &[(AXUIElement, super::AttachmentObservation)]| -> Vec<super::AttachmentObservation> {
            rows.iter().map(|(_, observation)| observation.clone()).collect()
        };
        let before = observations(&baseline);
        let deadline = Instant::now() + ATTACHMENT_VERIFY_TIMEOUT;
        let failure = loop {
            match find_chat_window(&app, chat_display_name) {
                Ok(Some(_)) => {
                    match attachment_observations(&elements.message_table, filename) {
                        Ok((total, current)) => {
                            let start = super::appended_rows_start(
                                baseline.last().map(|(row, _)| row),
                                &current
                                    .iter()
                                    .map(|(row, _)| row.clone())
                                    .collect::<Vec<_>>(),
                                |left, right| unsafe {
                                    CFEqual(left.as_CFTypeRef(), right.as_CFTypeRef()) != 0
                                },
                            );
                            if let Some(verified_by) = super::attachment_delivery_evidence(
                                (&before, baseline_total),
                                (&observations(&current), total),
                                start,
                                kind,
                            ) {
                                trace(started, &format!("bubble appeared ({verified_by})"));
                                // The bubble only proves the upload started. KakaoTalk
                                // reads the file (or its staged copy) until it ends,
                                // so the send counts, and the copy goes, only then.
                                let baseline_rows: Vec<AXUIElement> =
                                    baseline.iter().map(|(row, _)| row.clone()).collect();
                                match wait_for_upload_to_finish(
                                    &window,
                                    &elements.message_table,
                                    &baseline_rows,
                                ) {
                                    UploadEnd::Finished => {
                                        trace(started, "upload finished");
                                        return Ok(super::AttachmentReceipt {
                                            verified_by,
                                            preview_shows_filename,
                                            staged: staged.is_some(),
                                        });
                                    }
                                    UploadEnd::Failed(alert) => break format!(
                                        "the {noun} bubble appeared, but KakaoTalk reported the upload failed ({alert:?}); its bubble shows a red ✕, which resends or cancels it in KakaoTalk"
                                    ),
                                    UploadEnd::Unfinished => break format!(
                                        "the {noun} bubble appeared, but it was still uploading after {}s",
                                        UPLOAD_FINISH_TIMEOUT.as_secs()
                                    ),
                                }
                            }
                        }
                        Err(error) => {
                            break format!("the {noun} may have been sent but AX verification failed: {error:#}");
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    break format!(
                        "the {noun} may have been sent but target-window verification became ambiguous: {error:#}"
                    );
                }
            }
            if Instant::now() >= deadline {
                break format!(
                    "the {noun} may have been sent but no new outgoing attachment bubble appeared in '{chat_display_name}' within {}s; inspect KakaoTalk before retrying",
                    ATTACHMENT_VERIFY_TIMEOUT.as_secs(),
                );
            }
            sleep(VERIFY_POLL_INTERVAL);
        };
        let kept = staged.as_mut().map(|staged| {
            staged.keep = true;
            format!("; the staged copy was kept at {}", staged.path.display())
        });
        Err(coded(
            ErrorCode::DeliveryUncertain,
            format!(
                "{failure}; commit action result: {commit_error:?}{}",
                kept.unwrap_or_default()
            ),
        ))
    }

    /// How the newest outgoing attachment's upload ended.
    enum UploadEnd {
        /// The row shows its 공유 button: KakaoTalk has the whole file.
        Finished,
        /// KakaoTalk put an alert over the chat window, with this text.
        Failed(String),
        /// Still no 공유 button after [`UPLOAD_FINISH_TIMEOUT`], or the tail
        /// could not be read.
        Unfinished,
    }

    /// Wait until the newest outgoing attachment row finishes uploading. The
    /// bubble appears as soon as the upload starts, but only a finished photo
    /// or file row shows its 공유 button (and clock); a photo in flight or
    /// failed shows neither, and no "업로드 중" label either (measured
    /// 2026-11-07, KakaoTalk 26.8). A failed upload also raises an alert,
    /// which is dismissed when it only acknowledges, so the window stays usable.
    fn wait_for_upload_to_finish(
        window: &AXUIElement,
        table: &AXUIElement,
        baseline: &[AXUIElement],
    ) -> UploadEnd {
        let deadline = Instant::now() + UPLOAD_FINISH_TIMEOUT;
        let center = table_center_x(table);
        // A failed upload's bubble from before this send stays pinned last.
        let new_row = |row: &AxNode| {
            !baseline
                .iter()
                .any(|old| unsafe { CFEqual(old.as_CFTypeRef(), row.element.as_CFTypeRef()) != 0 })
        };
        let outgoing_attachment = |row: &AxNode| {
            new_row(row)
                && message_row_text(row).is_some_and(|(text, bubble)| {
                    (text == PHOTO_BUBBLE_PLACEHOLDER || text == super::FILE_BUBBLE_PLACEHOLDER)
                        && row_is_outgoing(row, bubble, center)
                })
        };
        loop {
            if let Some(sheet) = window_sheet(window) {
                let text = sheet_text(&sheet);
                if let Err(error) = clear_leftover_alerts(window) {
                    eprintln!("{error:#}");
                }
                return UploadEnd::Failed(text);
            }
            let mut seen = 0;
            let finished = tail_rows(table, |row| {
                seen += 1;
                outgoing_attachment(row) || seen >= DELIVERY_TAIL_ROWS
            })
            .map(|(rows, _)| {
                rows.iter()
                    .rev()
                    .find(|row| outgoing_attachment(row))
                    .is_some_and(|row| has_share_button(row) && uploading_label(row).is_none())
            });
            match finished {
                Ok(true) => return UploadEnd::Finished,
                Ok(false) if Instant::now() < deadline => sleep(Duration::from_millis(250)),
                _ => return UploadEnd::Unfinished,
            }
        }
    }

    fn has_share_button(row: &AxNode) -> bool {
        let mut buttons = Vec::new();
        row.find_all("AXButton", &mut buttons);
        buttons
            .iter()
            .any(|button| button.description.as_deref() == Some("공유"))
    }

    /// KakaoTalk's attachment preview for the pasted file and its send
    /// button, chosen at runtime by `pick_preview_send_button` (the Korean
    /// title is not hard-coded). On timeout the sheet's buttons are reported
    /// so a new localization can be added.
    fn wait_for_attachment_preview(window: &AXUIElement) -> Result<(AXUIElement, AXUIElement)> {
        let deadline = Instant::now() + FILE_PICKER_READY_TIMEOUT;
        loop {
            let sheet = find_file_picker_sheet(window)?;
            if let Some(preview) = &sheet {
                let buttons = sheet_buttons(preview)?;
                let facts: Vec<super::SheetButton> =
                    buttons.iter().map(|(_, facts)| facts.clone()).collect();
                if let Some(index) = super::pick_preview_send_button(&facts) {
                    if crate::util::debug_enabled() {
                        eprintln!(
                            "[ax] attachment preview buttons {facts:?}; sending with #{index}"
                        );
                    }
                    return Ok((preview.clone(), buttons[index].0.clone()));
                }
            }
            if Instant::now() >= deadline {
                let seen = sheet
                    .and_then(|sheet| sheet_buttons(&sheet).ok())
                    .map(|buttons| {
                        buttons
                            .into_iter()
                            .map(|(_, facts)| facts)
                            .collect::<Vec<_>>()
                    });
                anyhow::bail!(
                    "KakaoTalk did not show a single-file preview with a recognizable send button in time (sheet buttons: {seen:?}); check the composer for a leftover pasted path"
                );
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// Read the most recent `count` messages visible in a chat's AX message
    /// list. Uses an already-open exact-title window directly (no chat-list
    /// walk, no foreground change); with `--open-if-needed` a closed chat is
    /// opened from the list first and the foreground handed back right after.
    /// No local SQLCipher DB access, so this works even when `local_db.rs`'s
    /// key derivation is stale for the installed KakaoTalk build. Only
    /// messages already rendered on screen are returned — older history
    /// requires scrolling up in KakaoTalk first.
    pub fn read_via_ax(chat_display_name: &str, count: usize, expand: bool) -> Result<ChatView> {
        let started = Instant::now();
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        trace(started, "KakaoTalk process verified");
        let mut focus = FocusGuard::new(pid);
        let chat = open_chat(&app, chat_display_name, &mut focus)?;
        focus.restore();

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            let mut view = chat_view(&chat.window, count)?;
            // A window that was already open is read once; a just-opened one
            // may need a moment to render its rows.
            if chat.was_open || !view.messages.is_empty() {
                trace(started, "messages read");
                if expand {
                    expand_folded(&app, &chat.window, &mut view.messages)?;
                    trace(started, "folded messages expanded");
                }
                return Ok(view);
            }
            if Instant::now() >= deadline {
                anyhow::bail!("chat window did not open (or has no visible messages) in time");
            }
            sleep(Duration::from_millis(150));
        }
    }

    /// Send to exactly one chat and classify the result by commit point.
    /// Errors returned from this function happened before the commit (the
    /// `전송` press, or Return with `--fallback-keys`), while `Uncertain`
    /// means the commit may have happened but a new message bubble could not
    /// be proven.
    ///
    /// The default commit is focus-free (verified on KakaoTalk 26.9.0, macOS
    /// 26): the message is inserted through the composer's `AXSelectedText`,
    /// which runs KakaoTalk's text-did-change path and enables `전송` (setting
    /// `AXValue` does not), and `전송` is pressed with `AXPress`. KakaoTalk is
    /// never raised and no synthetic input is posted.
    pub fn send_via_ax_classified(
        chat_display_name: &str,
        message: &str,
    ) -> Result<super::AxDeliveryOutcome> {
        let started = Instant::now();
        let fallback_keys = super::policy().fallback_keys;
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        trace(started, "KakaoTalk process verified");
        let mut focus = FocusGuard::new(pid);
        open_chat(&app, chat_display_name, &mut focus)?;
        let (window, elements) = wait_for_chat_elements(&app, chat_display_name)?;
        clear_leftover_alerts(&window)?;
        trace(started, "chat window inspected");
        let send_button = match (&elements.send_button, fallback_keys) {
            (Some(button), false) => Some(button.clone()),
            (None, false) => {
                return Err(coded(
                    ErrorCode::SendButtonMissing,
                    format!(
                        "the '{chat_display_name}' chat window exposes no '{SEND_BUTTON_TITLE}' button"
                    ),
                ));
            }
            (_, true) => None,
        };
        if send_button.is_some() {
            // The button commit never needs the foreground, so a window just
            // opened from the list gives the user's app back immediately.
            focus.restore();
        }

        let field = elements.input_field;
        let message_table = elements.message_table;
        let existing_draft = attr_as_string(&field, "AXValue").ok_or_else(|| {
            anyhow!(
                "could not read the exact target's composer before commit; refusing to type blindly"
            )
        })?;
        // A draft is cleared only with --discard-draft, and only after the
        // approval below, so a declined prompt leaves it untouched.
        let discard = match (existing_draft.is_empty(), super::policy().discard_draft) {
            (true, _) => None,
            (false, true) => Some(existing_draft),
            (false, false) => {
                return Err(coded(
                    ErrorCode::ComposerBusy,
                    format!(
                        "the exact target's composer already holds a {}-character draft; refusing to overwrite or append to it (--discard-draft clears it first and prints it)",
                        existing_draft.chars().count()
                    ),
                ));
            }
        };

        let baseline = delivery_observations(&message_table)?;
        trace(started, "delivery baseline read");

        // Keep the OS-mediated human-presence boundary inside the one native
        // transport implementation. This makes it impossible for a future
        // command caller to reach a real AX send merely by forgetting its own
        // prompt. At this point the signed process, exact window, empty
        // composer, and pre-send bubble baseline are all known; authentication
        // happens immediately before the composer is changed.
        focus.watch();
        require_send_approval(&window, chat_display_name, "message to")?;
        if let Some(draft) = discard {
            // Printed before it is cleared, so the text is never lost.
            eprintln!(
                "--discard-draft: clearing this draft from '{}': {}",
                crate::util::escape_terminal_text(chat_display_name),
                serde_json::to_string(&crate::util::escape_terminal_text(&draft))
                    .unwrap_or_default()
            );
            field
                .set_value(CFString::new("").as_CFType())
                .map_err(|error| anyhow!("could not clear the draft: {error:?}"))?;
            if !wait_for_composer_value(&field, "") {
                return Err(coded(
                    ErrorCode::ComposerBusy,
                    "the draft did not clear, so nothing was inserted or sent",
                ));
            }
        }

        let commit_note = match &send_button {
            Some(button) => {
                commit_with_send_button(&app, chat_display_name, &field, button, message)?
            }
            None => {
                commit_with_return_key(&app, pid, chat_display_name, &field, message, &mut focus)?
            }
        };
        // Verification below is read-only.
        focus.restore();
        trace(started, "message committed");
        if let Some(reason) = commit_note.uncertain {
            return Ok(super::AxDeliveryOutcome::Uncertain { reason });
        }

        // Scoped verify: require an additional exact-text bubble compared with
        // the pre-commit snapshot. Merely finding old identical text is not
        // proof that this send produced a new message.
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            match find_chat_window(&app, chat_display_name) {
                Ok(Some(_current_window)) => {
                    let current = match delivery_observations(&message_table) {
                        Ok(observations) => observations,
                        Err(error) => {
                            return Ok(super::AxDeliveryOutcome::Uncertain {
                                reason: format!(
                                    "sent the message but bounded AX verification failed: {error:#}"
                                ),
                            });
                        }
                    };
                    if text_delivery_verified(
                        &baseline,
                        &current,
                        message,
                        attr_as_string(&field, "AXValue").as_deref(),
                    ) {
                        trace(started, "delivery verified");
                        return Ok(super::AxDeliveryOutcome::Verified);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    return Ok(super::AxDeliveryOutcome::Uncertain {
                        reason: format!(
                            "sent the message but target-window verification became ambiguous: {error:#}"
                        ),
                    });
                }
            }
            if Instant::now() >= deadline {
                return Ok(super::AxDeliveryOutcome::Uncertain {
                    reason: format!(
                        "sent the message but no additional exact-text bubble appeared in the '{chat_display_name}' chat window within {}s — check KakaoTalk before retrying to avoid duplicates{}",
                        VERIFY_TIMEOUT.as_secs(),
                        commit_note.press_error.map(|error| format!("; 전송 press result: {error}")).unwrap_or_default()
                    ),
                });
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    struct CommitNote {
        /// Set when the commit may have happened but its delivery failed.
        uncertain: Option<String>,
        /// An AX error from pressing `전송` (the press can still have worked).
        press_error: Option<String>,
    }

    /// Insert `message` through `AXSelectedText`, wait for the exact read-back
    /// and an enabled `전송`, revalidate, then press it. Before the press,
    /// any failure removes only our own insertion and returns an error.
    fn commit_with_send_button(
        app: &AXUIElement,
        chat_display_name: &str,
        field: &AXUIElement,
        button: &AXUIElement,
        message: &str,
    ) -> Result<CommitNote> {
        let selected_text: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXSelectedText"));
        // Re-check right before inserting: a draft typed during Touch ID must
        // never be appended to or overwritten.
        if attr_as_string(field, "AXValue").as_deref() != Some("") {
            anyhow::bail!("the exact target's composer is no longer empty; nothing was inserted");
        }
        // The composer was empty a moment ago. Text equal to the message
        // (newlines normalized) is ours; while KakaoTalk is in the background it
        // receives no keystrokes, so anything in the composer is ours too.
        let clear_own_insertion = || {
            let normalize = |text: &str| text.replace("\r\n", "\n").replace('\r', "\n");
            let ours = attr_as_string(field, "AXValue").is_some_and(|value| {
                normalize(&value) == normalize(message)
                    || (!value.is_empty() && attr_as_bool(app, "AXFrontmost") != Some(true))
            });
            if ours {
                let _ = field.set_value(CFString::new("").as_CFType());
            }
        };
        if let Err(error) = field.set_attribute(&selected_text, CFString::new(message).as_CFType())
        {
            clear_own_insertion();
            anyhow::bail!("could not insert into the exact target composer: {error:?}");
        }
        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        loop {
            let inserted = attr_as_string(field, "AXValue").as_deref() == Some(message);
            if inserted && attr_as_bool(button, "AXEnabled") == Some(true) {
                break;
            }
            if Instant::now() >= deadline {
                clear_own_insertion();
                anyhow::bail!(
                    "{}; nothing was sent",
                    if inserted {
                        "KakaoTalk did not enable its 전송 button for the inserted message"
                    } else {
                        "could not verify the exact message in the target composer"
                    }
                );
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }
        let still_target = matches!(find_chat_window(app, chat_display_name), Ok(Some(_)));
        if !still_target || attr_as_string(field, "AXValue").as_deref() != Some(message) {
            clear_own_insertion();
            anyhow::bail!("the exact target changed before commit; 전송 was not pressed");
        }
        let press_error = button
            .perform_action(&CFString::new(kAXPressAction))
            .err()
            .map(|error| format!("{error:?}"));
        Ok(CommitNote {
            uncertain: None,
            press_error,
        })
    }

    /// The pre-26.9 commit (`--fallback-keys`): raise KakaoTalk and the exact
    /// window behind the idle wait, focus the composer, set it, and post
    /// Return to KakaoTalk's pid.
    fn commit_with_return_key(
        app: &AXUIElement,
        pid: i32,
        chat_display_name: &str,
        field: &AXUIElement,
        message: &str,
        focus: &mut FocusGuard,
    ) -> Result<CommitNote> {
        let window = find_chat_window(app, chat_display_name)?
            .ok_or_else(|| anyhow!("exact target window disappeared before commit"))?;
        // A background KakaoTalk does not keep composer focus while the user
        // types elsewhere, so this path raises the exact window first.
        focus_window_and_verify(app, &window, focus)
            .context("could not raise the exact target window for the Return-key commit")?;
        focus_and_verify(field, "exact target composer")?;
        if attr_as_string(field, "AXValue").as_deref() != Some("") {
            anyhow::bail!("the exact target's composer is no longer empty; nothing was inserted");
        }
        // KakaoTalk is frontmost here, so only an exact copy of the message is
        // known to be ours and removed on a pre-commit failure.
        let clear_own_text = || {
            if attr_as_string(field, "AXValue").as_deref() == Some(message) {
                let _ = field.set_value(CFString::new("").as_CFType());
            }
        };
        field
            .set_value(CFString::new(message).as_CFType())
            .map_err(|error| anyhow!("could not set the exact target composer: {error:?}"))?;
        if !wait_for_composer_value(field, message) {
            clear_own_text();
            anyhow::bail!(
                "could not verify the exact message in the target composer; Return was not pressed"
            );
        }

        // Re-establish and verify focus immediately before the irreversible
        // PID-scoped Return event. The composer read-back loop above may wait
        // for up to a second, during which another KakaoTalk control could
        // otherwise gain focus.
        if let Err(error) =
            focus_and_verify(field, "exact target composer immediately before commit")
        {
            clear_own_text();
            return Err(error);
        }
        if attr_as_string(field, "AXValue").as_deref() != Some(message) {
            clear_own_text();
            anyhow::bail!(
                "the exact target composer changed before commit; Return was not pressed"
            );
        }

        let uncertain = press_return(pid).err().map(|error| {
            format!(
                "Return delivery failed after commit began: {error:#}; inspect KakaoTalk before retrying"
            )
        });
        Ok(CommitNote {
            uncertain,
            press_error: None,
        })
    }

    /// Rows searched (newest first) for a reply/edit target.
    const BUBBLE_SEARCH_ROWS: usize = 40;
    const MODE_WAIT: Duration = Duration::from_secs(2);

    struct Bubble {
        text: String,
        outgoing: bool,
        /// The row shows KakaoTalk's `수정됨` (edited) mark.
        edited: bool,
        text_area: AXUIElement,
    }

    /// (text, outgoing, edited) of each bubble, chronological.
    type BubbleRows = Vec<(String, bool, bool)>;

    /// The newest text bubbles of a chat, chronological.
    fn text_bubbles(table: &AXUIElement) -> Result<Vec<Bubble>> {
        let center = table_center_x(table);
        let mut seen = 0;
        let (rows, _) = tail_rows(table, |_| {
            seen += 1;
            seen >= BUBBLE_SEARCH_ROWS
        })?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                let text_area = row.find_first("AXTextArea")?;
                let text = text_area.value.clone()?;
                let outgoing = center
                    .zip(text_area.center_x())
                    .is_some_and(|(table_center, bubble_center)| bubble_center > table_center);
                let mut labels = Vec::new();
                row.find_all("AXStaticText", &mut labels);
                let edited = labels
                    .iter()
                    .any(|label| label.value.as_deref().map(str::trim) == Some("수정됨"));
                Some(Bubble {
                    text,
                    outgoing,
                    edited,
                    text_area: text_area.element.clone(),
                })
            })
            .collect())
    }

    struct ComposerState {
        composer: AXUIElement,
        mode: super::ComposerMode,
        banner: super::ComposerMode,
        /// (button, title, enabled), outside the message table.
        buttons: Vec<(AXUIElement, String, bool)>,
    }

    fn composer_state(window: &AXUIElement) -> Result<ComposerState> {
        let snap = snapshot_shallow(window)?;
        let composer = find_input_field(&snap)
            .ok_or_else(|| anyhow!("the chat window has no readable composer"))?;
        let mode = super::composer_mode(attr_as_string(&composer, "AXDescription").as_deref());
        let mut labels = Vec::new();
        snap.find_all("AXStaticText", &mut labels);
        let banner = labels
            .iter()
            .filter_map(|label| label.value.as_deref())
            .map(super::mode_banner)
            .find(|mode| *mode != super::ComposerMode::Unknown)
            .unwrap_or(super::ComposerMode::Unknown);
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let buttons = buttons
            .into_iter()
            .filter_map(|button| {
                let title = button.title.clone()?;
                let enabled = attr_as_bool(&button.element, "AXEnabled") == Some(true);
                Some((button.element.clone(), title, enabled))
            })
            .collect();
        Ok(ComposerState {
            composer,
            mode,
            banner,
            buttons,
        })
    }

    /// Wait until both the composer's description and the banner say `mode`.
    fn wait_for_mode(window: &AXUIElement, mode: super::ComposerMode) -> Result<ComposerState> {
        let deadline = Instant::now() + MODE_WAIT;
        loop {
            let state = composer_state(window)?;
            if state.mode == mode && state.banner == mode {
                return Ok(state);
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::ActionUnverified,
                    format!(
                        "KakaoTalk did not enter {mode:?} mode (composer: {:?}, banner: {:?}); nothing was inserted",
                        state.mode, state.banner
                    ),
                ));
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// Best effort after a failure inside reply/edit mode: clear the composer
    /// and report whether the mode's banner is still showing.
    fn leave_mode_note(window: &AXUIElement) -> String {
        let Ok(state) = composer_state(window) else {
            return String::new();
        };
        let _ = state.composer.set_value(CFString::new("").as_CFType());
        if state.banner == super::ComposerMode::Unknown {
            return String::new();
        }
        // The reply/edit banner's own 닫기 (not the window's 닫기 버튼)
        // leaves the mode (26.8, measured 2026-11-07).
        if let Ok(snap) = snapshot_shallow(window) {
            let mut buttons = Vec::new();
            snap.find_all("AXButton", &mut buttons);
            if let Some(close) = buttons
                .iter()
                .find(|b| b.description.as_deref() == Some("닫기"))
            {
                let _ = close.element.perform_action(&CFString::new(kAXPressAction));
            }
        }
        let still = composer_state(window).map(|state| state.banner);
        if matches!(still, Ok(super::ComposerMode::Unknown)) {
            "; edit/reply mode was closed".to_string()
        } else {
            format!(
                "; the {:?} banner is still showing, close it with its X in KakaoTalk (never Esc)",
                state.banner
            )
        }
    }

    /// `AXShowMenu` on a bubble and `AXPress` its exact `item`; the menu is
    /// cancelled whenever it is found and the item is not pressed, and a menu
    /// left showing is cancelled again.
    fn press_bubble_menu_item(app: &AXUIElement, bubble: &AXUIElement, item: &str) -> Result<()> {
        press_menu_item_or(app, bubble, item, ErrorCode::MenuNotFound)
    }

    /// [`press_bubble_menu_item`], reporting a menu without `item` as
    /// `missing` (a delete scope KakaoTalk does not offer is not a lookup bug).
    fn press_menu_item_or(
        app: &AXUIElement,
        bubble: &AXUIElement,
        item: &str,
        missing: ErrorCode,
    ) -> Result<()> {
        let Some(menu) = show_context_menu(app, bubble)? else {
            let stray = if menu_still_open(app) {
                "; a KakaoTalk menu is showing but could not be read, click elsewhere to close it (never Esc)"
            } else {
                ""
            };
            return Err(coded(
                ErrorCode::MenuNotFound,
                format!(
                    "the message's context menu did not appear to Accessibility in time{stray}"
                ),
            ));
        };
        let items = menu_items(&menu);
        let found: Vec<_> = items
            .iter()
            .filter(|(_, title)| title.as_deref().map(str::trim) == Some(item))
            .collect();
        let [(entry, _)] = found.as_slice() else {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
            return Err(coded(
                missing,
                format!(
                    "the message's context menu has no unique '{item}' item (items: {:?})",
                    items
                        .iter()
                        .filter_map(|(_, title)| title.as_deref().map(str::trim))
                        .filter(|title| !title.is_empty())
                        .collect::<Vec<_>>()
                ),
            ));
        };
        if cursor_over_menu(app) {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
            return Err(coded(
                ErrorCode::UserActive,
                format!(
                    "the mouse cursor is over the message's context menu; '{item}' was not pressed"
                ),
            ));
        }
        let pressed = entry
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not press '{item}': {error:?}"));
        if menu_still_open(app) {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
        }
        pressed
    }

    /// What a reply/edit acted on (or would act on, for `--dry-run`).
    #[derive(Debug, Clone)]
    pub struct BubbleAction {
        pub target_text: String,
        pub target_outgoing: bool,
    }

    /// Shared start of `local-reply`/`local-edit`: the exact open window, an
    /// empty composer in its normal mode, and the one target bubble.
    fn bubble_target(
        chat_display_name: &str,
        target: &super::BubbleTarget,
        own_only: bool,
    ) -> Result<(
        AXUIElement,
        i32,
        FocusGuard,
        AXUIElement,
        ChatWindowElements,
        Bubble,
        BubbleRows,
    )> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let mut focus = FocusGuard::new(pid);
        open_chat(&app, chat_display_name, &mut focus)?;
        focus.restore();
        let (window, elements) = wait_for_chat_elements(&app, chat_display_name)?;
        let state = composer_state(&window)?;
        let empty = attr_as_string(&elements.input_field, "AXValue").as_deref() == Some("");
        if state.mode != super::ComposerMode::Normal || !empty {
            return Err(coded(
                ErrorCode::ComposerBusy,
                format!(
                    "the '{chat_display_name}' composer is not empty in its normal mode (mode: {:?})",
                    state.mode
                ),
            ));
        }
        let mut bubbles = text_bubbles(&elements.message_table)?;
        let pairs: Vec<(String, bool)> = bubbles
            .iter()
            .map(|bubble| (bubble.text.clone(), bubble.outgoing))
            .collect();
        let index = match super::pick_bubble(&pairs, target, own_only) {
            super::BubblePick::Found(index) => index,
            super::BubblePick::NotFound => {
                return Err(coded(
                    ErrorCode::MessageNotFound,
                    format!(
                        "no matching text bubble among the newest rows of '{chat_display_name}'"
                    ),
                ))
            }
            super::BubblePick::Ambiguous(count) => {
                return Err(coded(
                    ErrorCode::MessageAmbiguous,
                    format!("{count} bubbles in '{chat_display_name}' have exactly this text"),
                ))
            }
        };
        let baseline: BubbleRows = bubbles
            .iter()
            .map(|bubble| (bubble.text.clone(), bubble.outgoing, bubble.edited))
            .collect();
        let bubble = bubbles.swap_remove(index);
        Ok((app, pid, focus, window, elements, bubble, baseline))
    }

    /// Reply to one bubble without raising KakaoTalk (verified on 26.9.0):
    /// `AXShowMenu` on it, `AXPress` `답장`, require reply mode (composer
    /// `답장 메시지 입력` and a `…에게 답장` banner) **before inserting any
    /// text**, then the `AXSelectedText` + `전송` commit and the usual
    /// exact-text verification. Experimental.
    /// `local-react`: one reaction on one bubble through its menu's 리액션,
    /// focus-free. KakaoTalk opens its picker as a popover on the bubble's
    /// row; each reaction is a named button (사랑, 엄지척, 체크, 웃음, 축하해,
    /// …) and the first one of that name in 기본 is pressed (26.8, measured
    /// 2026-11-07). The memo chat has no 리액션.
    pub fn react_via_ax(
        chat_display_name: &str,
        target: &super::BubbleTarget,
        reaction: &str,
        dry_run: bool,
    ) -> Result<BubbleAction> {
        let (app, _pid, mut focus, window, _elements, bubble, _) =
            bubble_target(chat_display_name, target, false)?;
        let action = BubbleAction {
            target_text: bubble.text.clone(),
            target_outgoing: bubble.outgoing,
        };
        if dry_run {
            return Ok(action);
        }
        clear_leftover_alerts(&window)?;
        focus.watch();
        require_send_approval(&window, chat_display_name, "reaction in")?;
        let row = ancestor_row(&bubble.text_area)
            .ok_or_else(|| anyhow!("could not find the bubble's row"))?;
        let before = row_reactions(&row);
        press_menu_item_or(&app, &bubble.text_area, "리액션", ErrorCode::MenuNotFound)?;
        let deadline = Instant::now() + MENU_WAIT;
        let picker = loop {
            if let Some(picker) = find_popover(&row) {
                break picker;
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::MenuNotFound,
                    "KakaoTalk's reaction picker did not open",
                ));
            }
            sleep(Duration::from_millis(50));
        };
        let node = snapshot(&picker)?;
        let mut sections = Vec::new();
        node.find_all("AXList", &mut sections);
        // 기본 first: 최근 사용 repeats some of it.
        sections.sort_by_key(|section| section.description.as_deref() != Some("기본"));
        let mut names = Vec::new();
        let mut chosen = None;
        for section in &sections {
            let mut buttons = Vec::new();
            section.find_all("AXButton", &mut buttons);
            for button in buttons {
                let Some(name) = button.description.as_deref() else {
                    continue;
                };
                if chosen.is_none() && name == reaction {
                    chosen = Some(button.element.clone());
                }
                if !names.contains(&name.to_string()) {
                    names.push(name.to_string());
                }
            }
        }
        let Some(chosen) = chosen else {
            let _ = picker.perform_action(&CFString::new("AXCancel"));
            return Err(coded(
                ErrorCode::MenuNotFound,
                format!(
                    "KakaoTalk has no '{reaction}' reaction (reactions: {})",
                    names.join(", ")
                ),
            ));
        };
        if let Err(error) = chosen.perform_action(&CFString::new(kAXPressAction)) {
            let _ = picker.perform_action(&CFString::new("AXCancel"));
            return Err(anyhow!(
                "could not press the '{reaction}' reaction: {error:?}"
            ));
        }
        let close_picker = || {
            if let Some(picker) = find_popover(&row) {
                let _ = picker.perform_action(&CFString::new("AXCancel"));
            }
        };
        // A bubble without reactions gains badges; one that had some shows
        // no readable difference, and a press can then have taken back a
        // reaction you gave before (KakaoTalk toggles).
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            let after = row_reactions(&row);
            if after > before {
                close_picker();
                return Ok(action);
            }
            if before > 0 || Instant::now() >= deadline {
                close_picker();
                let why = if before > 0 {
                    "the bubble already had reactions, and KakaoTalk does not show whose they are, so this press may have added yours or taken back one you gave"
                } else {
                    "the bubble shows no reaction badge"
                };
                return Err(coded(
                    ErrorCode::DeliveryUncertain,
                    format!(
                        "pressed '{reaction}', but {why}; look at the bubble before pressing again"
                    ),
                ));
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    fn ancestor_row(element: &AXUIElement) -> Option<AXUIElement> {
        let mut current = attr_as_element(element, "AXParent");
        for _ in 0..6 {
            let node = current?;
            if role(&node) == "AXRow" {
                return Some(node);
            }
            current = attr_as_element(&node, "AXParent");
        }
        None
    }

    /// The reaction badges under a bubble: its row's unnamed pressable
    /// buttons (a badge such as ❤️ 1, and KakaoTalk's add-reaction button
    /// once there is one). KakaoTalk names neither the reactions nor who
    /// gave them (26.8, measured 2026-11-07).
    fn row_reactions(row: &AXUIElement) -> usize {
        let Ok(node) = snapshot(row) else {
            return 0;
        };
        let mut buttons = Vec::new();
        node.find_all("AXButton", &mut buttons);
        let blank = |t: &Option<String>| t.as_deref().is_none_or(|t| t.trim().is_empty());
        buttons
            .iter()
            .filter(|b| blank(&b.title) && blank(&b.description))
            .filter(|b| {
                b.element
                    .action_names()
                    .map(|actions| actions.iter().any(|a| a.to_string() == "AXPress"))
                    .unwrap_or(false)
            })
            .count()
    }

    pub fn reply_via_ax(
        chat_display_name: &str,
        target: &super::BubbleTarget,
        message: &str,
        dry_run: bool,
    ) -> Result<BubbleAction> {
        let (app, _pid, mut focus, window, elements, bubble, _) =
            bubble_target(chat_display_name, target, false)?;
        let action = BubbleAction {
            target_text: bubble.text.clone(),
            target_outgoing: bubble.outgoing,
        };
        if dry_run {
            return Ok(action);
        }
        // Only a real action may dismiss a leftover alert.
        clear_leftover_alerts(&window)?;
        focus.watch();
        require_send_approval(&window, chat_display_name, "reply in")?;
        let baseline = delivery_observations(&elements.message_table)?;
        focus.watch();
        press_bubble_menu_item(&app, &bubble.text_area, "답장")?;
        let state = match wait_for_mode(&window, super::ComposerMode::Reply) {
            Ok(state) => state,
            Err(error) => return Err(error.context(leave_mode_note(&window))),
        };
        let send = state
            .buttons
            .iter()
            .find(|(_, title, _)| title == SEND_BUTTON_TITLE)
            .map(|(button, _, _)| button.clone());
        let Some(send) = send else {
            let note = leave_mode_note(&window);
            return Err(coded(
                ErrorCode::SendButtonMissing,
                format!("reply mode shows no '{SEND_BUTTON_TITLE}' button{note}"),
            ));
        };
        let commit =
            commit_with_send_button(&app, chat_display_name, &state.composer, &send, message);
        focus.restore();
        let note = match commit {
            Ok(note) => note,
            Err(error) => return Err(error.context(leave_mode_note(&window))),
        };
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            let current = delivery_observations(&elements.message_table);
            if let Ok(current) = &current {
                if text_delivery_verified(
                    &baseline,
                    current,
                    message,
                    attr_as_string(&state.composer, "AXValue").as_deref(),
                ) {
                    return Ok(action);
                }
            }
            if current.is_err() || Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::DeliveryUncertain,
                    format!(
                        "the reply may have been sent but no additional exact-text bubble appeared in '{chat_display_name}' within {}s{}",
                        VERIFY_TIMEOUT.as_secs(),
                        note.press_error
                            .map(|error| format!("; 전송 press result: {error}"))
                            .unwrap_or_default()
                    ),
                ));
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    /// Edit one of your own bubbles without raising KakaoTalk (verified on
    /// 26.9.0): `AXPress` its menu's `수정`, require edit mode (composer
    /// `수정 메시지 입력` preloaded with the old text, a `메시지 수정` banner),
    /// replace the text (clear `AXValue`, then insert with `AXSelectedText`;
    /// selecting all via `AXSelectedTextRange` was measured to append), press
    /// the `수정` button, and require the bubble to read the new text with a
    /// `수정됨` mark while the old text is gone. Experimental.
    pub fn edit_via_ax(
        chat_display_name: &str,
        old: &str,
        new: &str,
        dry_run: bool,
    ) -> Result<BubbleAction> {
        let started = Instant::now();
        let target = super::BubbleTarget::Text(old.to_string());
        let (app, _pid, mut focus, window, elements, bubble, found_in) =
            bubble_target(chat_display_name, &target, true)?;
        trace(started, "target bubble found");
        let action = BubbleAction {
            target_text: bubble.text.clone(),
            target_outgoing: bubble.outgoing,
        };
        if dry_run {
            return Ok(action);
        }
        // Only a real action may dismiss a leftover alert.
        clear_leftover_alerts(&window)?;
        focus.watch();
        let prompted = require_send_approval(&window, chat_display_name, "message edit in")?;
        let rows = |bubbles: &[Bubble]| -> BubbleRows {
            bubbles
                .iter()
                .map(|bubble| (bubble.text.clone(), bubble.outgoing, bubble.edited))
                .collect()
        };
        // The rows the target was found in are the baseline, unless a macOS
        // prompt let time pass (saves a 40-row read, ~0.3 s).
        let before = if prompted {
            rows(&text_bubbles(&elements.message_table)?)
        } else {
            found_in
        };
        trace(started, "baseline ready");
        focus.watch();
        press_bubble_menu_item(&app, &bubble.text_area, "수정")?;
        trace(started, "수정 pressed");
        let prepared = (|| {
            let state = wait_for_mode(&window, super::ComposerMode::Edit)?;
            trace(started, "edit mode loaded");
            // KakaoTalk preloads the bubble's own text, watermark included.
            if !wait_for_composer_value(&state.composer, &bubble.text) {
                return Err(coded(
                    ErrorCode::ActionUnverified,
                    "edit mode did not load exactly the original text; nothing was changed",
                ));
            }
            let commit = state
                .buttons
                .iter()
                .filter(|(_, title, _)| title == "수정")
                .map(|(button, _, _)| button.clone())
                .collect::<Vec<_>>();
            let [commit] = commit.as_slice() else {
                return Err(coded(
                    ErrorCode::ActionUnverified,
                    "edit mode shows no unique '수정' button",
                ));
            };
            state
                .composer
                .set_value(CFString::new("").as_CFType())
                .map_err(|error| anyhow!("could not clear the edit composer: {error:?}"))?;
            if !wait_for_composer_value(&state.composer, "") {
                anyhow::bail!("the edit composer did not clear");
            }
            let selected_text: AXAttribute<CFType> =
                AXAttribute::new(&CFString::new("AXSelectedText"));
            state
                .composer
                .set_attribute(&selected_text, CFString::new(new).as_CFType())
                .map_err(|error| anyhow!("could not insert the edited text: {error:?}"))?;
            let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
            loop {
                let ready = attr_as_string(&state.composer, "AXValue").as_deref() == Some(new)
                    && attr_as_bool(commit, "AXEnabled") == Some(true);
                if ready {
                    break;
                }
                if Instant::now() >= deadline {
                    anyhow::bail!("the edit composer did not read back exactly the new text");
                }
                sleep(COMPOSER_VERIFY_POLL_INTERVAL);
            }
            if !matches!(find_chat_window(&app, chat_display_name), Ok(Some(_))) {
                anyhow::bail!("the exact target window changed before the edit commit");
            }
            Ok((state, commit.clone()))
        })();
        let (state, commit) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                focus.restore();
                return Err(error.context(leave_mode_note(&window)));
            }
        };
        trace(started, "edited text ready");
        let press_error = commit.perform_action(&CFString::new(kAXPressAction)).err();
        focus.restore();
        trace(started, "edit committed");
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            let cleared = attr_as_string(&state.composer, "AXValue").as_deref() == Some("");
            if let Ok(after) = text_bubbles(&elements.message_table) {
                if cleared && super::edit_verified(&before, &rows(&after), old, new) {
                    trace(started, "edit verified");
                    return Ok(action);
                }
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::DeliveryUncertain,
                    format!(
                        "the edit may have been applied but '{chat_display_name}' shows no 수정됨 bubble with the new text within {}s; check KakaoTalk; press result: {press_error:?}",
                        VERIFY_TIMEOUT.as_secs()
                    ),
                ));
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    /// KakaoTalk 26.8's message-selection mode, entered from a delete item:
    /// every row gets an `AXCheckBox` and the composer gives way to a bar,
    /// "삭제할 메시지를 선택하세요." with 취소 and 확인 (measured 2026-11-07).
    /// Returns (확인, 취소) while that bar shows.
    fn selection_bar(window: &AXUIElement) -> Result<Option<(AXUIElement, AXUIElement)>> {
        let snap = snapshot_shallow(window)?;
        let mut texts = Vec::new();
        snap.find_all("AXStaticText", &mut texts);
        if !texts.iter().any(|text| {
            text.value
                .as_deref()
                .is_some_and(|value| value.trim().ends_with("선택하세요."))
        }) {
            return Ok(None);
        }
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let titled = |title: &str| -> Vec<AXUIElement> {
            buttons
                .iter()
                .filter(|button| button.title.as_deref() == Some(title))
                .map(|button| button.element.clone())
                .collect()
        };
        match (titled("확인").as_slice(), titled("취소").as_slice()) {
            ([confirm], [cancel]) => Ok(Some((confirm.clone(), cancel.clone()))),
            _ => Ok(None),
        }
    }

    /// Back to the normal chat: the confirmation's 취소, then the bar's 취소.
    fn leave_selection_mode(window: &AXUIElement) {
        if let Some(sheet) = window_sheet(window) {
            if let Ok(Some(cancel)) = find_cancel_button(&sheet) {
                let _ = cancel.perform_action(&CFString::new(kAXPressAction));
                sleep(Duration::from_millis(200));
            }
        }
        if let Ok(Some((_, cancel))) = selection_bar(window) {
            let _ = cancel.perform_action(&CFString::new(kAXPressAction));
        }
    }

    /// Delete one bubble the way KakaoTalk's own menu does, without raising
    /// it: `AXShowMenu` on the bubble, press the scope's item (absent means
    /// KakaoTalk does not offer it: `delete_unavailable`), tick only the
    /// target's checkbox in the selection mode, 확인, then 삭제 on a
    /// confirmation headed by that scope. Every pre-commit failure leaves the
    /// selection mode, so nothing is deleted; after 삭제 the bubble must be
    /// gone (or replaced by KakaoTalk's notice) or it is `delivery_uncertain`.
    /// One friends-tab row: the name as this account shows it and the
    /// friend's status message.
    #[derive(Debug, Clone)]
    pub struct FriendRow {
        pub name: String,
        pub status: Option<String>,
    }

    /// The friend's 정보 변경 popover: the name this account set (empty:
    /// none), the friend's own name, and the private memo.
    #[derive(Debug, Clone)]
    pub struct FriendInfo {
        pub name: String,
        pub nickname: String,
        pub own_name: Option<String>,
        pub memo: String,
    }

    /// The main window on its friends tab while this lives; the chat-list
    /// tab comes back on drop when that is where it was.
    struct FriendsTab {
        main: AXUIElement,
        back_to_chats: bool,
    }

    impl Drop for FriendsTab {
        fn drop(&mut self) {
            if self.back_to_chats {
                if let Some(tab) = snapshot_shallow(&self.main)
                    .ok()
                    .and_then(|snap| tab_button(&snap, "chatrooms"))
                {
                    let _ = tab.perform_action(&CFString::new(kAXPressAction));
                }
            }
        }
    }

    fn tab_button(snap: &AxNode, id: &str) -> Option<AXUIElement> {
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        buttons
            .into_iter()
            .find(|b| attr_as_string(&b.element, "AXIdentifier").as_deref() == Some(id))
            .map(|b| b.element.clone())
    }

    fn friends_tab(app: &AXUIElement) -> Result<(FriendsTab, AxNode)> {
        let main = find_or_show_main_window(app)?;
        let snap = snapshot_shallow(&main)?;
        if snap.find_first("AXOutline").is_some() {
            return Ok((
                FriendsTab {
                    main,
                    back_to_chats: false,
                },
                snap,
            ));
        }
        let back_to_chats = snap.find_first("AXTable").is_some();
        let tab = tab_button(&snap, "friends").ok_or_else(|| {
            coded(
                ErrorCode::ChatListUnavailable,
                "KakaoTalk's main window shows no friends tab",
            )
        })?;
        let _ = tab.perform_action(&CFString::new(kAXPressAction));
        let guard = FriendsTab {
            main: main.clone(),
            back_to_chats,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snap = snapshot_shallow(&main)?;
            if snap.find_first("AXOutline").is_some() {
                return Ok((guard, snap));
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::ChatListUnavailable,
                    "the friends tab did not open",
                ));
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// A friend row's cell offers AXShowMenu (채팅하기, 정보 변경, ...);
    /// section headers ("친구 1") do not.
    fn friend_row(row: &AXUIElement) -> Option<FriendRow> {
        let cell = row.children().ok()?.iter().next().map(|c| (*c).clone())?;
        let has_menu = cell
            .action_names()
            .map(|actions| actions.iter().any(|a| a.to_string() == "AXShowMenu"))
            .unwrap_or(false);
        if !has_menu {
            return None;
        }
        let node = snapshot(row).ok()?;
        let mut texts = Vec::new();
        node.find_all("AXStaticText", &mut texts);
        let mut values = texts
            .iter()
            .filter_map(|t| t.value.clone())
            .filter(|v| !v.trim().is_empty());
        Some(FriendRow {
            name: values.next()?,
            status: values.next(),
        })
    }

    /// Friends KakaoTalk's own friends-tab search lists for `query`, read-only.
    pub fn search_friends(query: &str) -> Result<Vec<FriendRow>> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let (tab, snap) = friends_tab(&app)?;
        let (search, rows) = filter_list(&tab.main, &snap, query, "AXOutline").ok_or_else(|| {
            coded(
                ErrorCode::ChatListUnavailable,
                "KakaoTalk's friend search did not behave as measured (is its search field already open?)",
            )
        })?;
        let friends = rows.iter().filter_map(|(row, _)| friend_row(row)).collect();
        drop(search);
        drop(tab);
        Ok(friends)
    }

    /// The 친구 정보 popover, which hangs off the friend row's name text
    /// (row → cell → static text → popover; 26.8, measured 2026-11-07).
    fn find_popover(row: &AXUIElement) -> Option<AXUIElement> {
        fn under(element: &AXUIElement, depth: usize) -> Option<AXUIElement> {
            let children = element.children().ok()?;
            for child in children.iter() {
                if role(&child) == "AXPopover" {
                    return Some((*child).clone());
                }
                if depth > 0 {
                    if let Some(found) = under(&child, depth - 1) {
                        return Some(found);
                    }
                }
            }
            None
        }
        under(row, 3)
    }

    fn read_friend_info(popover: &AXUIElement, name: &str) -> Result<FriendInfo> {
        let node = snapshot(popover)?;
        let mut areas = Vec::new();
        node.find_all("AXTextArea", &mut areas);
        let [nickname, memo] = areas.as_slice() else {
            anyhow::bail!(
                "친구 정보 shows {} text fields instead of name and memo",
                areas.len()
            );
        };
        let mut texts = Vec::new();
        node.find_all("AXStaticText", &mut texts);
        let own_name = texts.iter().find_map(|t| {
            t.value
                .as_deref()?
                .strip_prefix("친구가 설정한 이름:")
                .map(|n| n.trim().to_string())
        });
        Ok(FriendInfo {
            name: name.to_string(),
            nickname: nickname.value.clone().unwrap_or_default(),
            own_name,
            memo: memo.value.clone().unwrap_or_default(),
        })
    }

    /// The one friend named exactly `name`, its 정보 변경 popover open. The
    /// popover hangs off the search result row, so the search stays open
    /// until the guards drop (search first, then the tab).
    fn open_friend_info(
        app: &AXUIElement,
        name: &str,
    ) -> Result<(FriendsTab, ChatSearch, AXUIElement, AXUIElement)> {
        let started = Instant::now();
        let (tab, snap) = friends_tab(app)?;
        trace(started, "friends tab");
        let (search, rows) = filter_list(&tab.main, &snap, name, "AXOutline").ok_or_else(|| {
            coded(
                ErrorCode::ChatListUnavailable,
                "KakaoTalk's friend search did not behave as measured (is its search field already open?)",
            )
        })?;
        trace(started, "friend search filtered");
        let same: Vec<&AXUIElement> = rows
            .iter()
            .filter(|(row, _)| friend_row(row).is_some_and(|f| f.name == name))
            .map(|(row, _)| row)
            .collect();
        let row = match same.as_slice() {
            [row] => (*row).clone(),
            [] => {
                return Err(coded(
                    ErrorCode::ChatNotFound,
                    format!("no friend is named exactly '{name}' (ax-friends lists close names)"),
                ))
            }
            many => {
                return Err(coded(
                    ErrorCode::ChatAmbiguous,
                    format!(
                        "{} friends are named '{name}'; openkakao never guesses",
                        many.len()
                    ),
                ))
            }
        };
        let cell = row
            .children()
            .ok()
            .and_then(|c| c.iter().next().map(|c| (*c).clone()))
            .unwrap_or_else(|| row.clone());
        press_menu_item_or(app, &cell, "정보 변경", ErrorCode::MenuNotFound)?;
        trace(started, "정보 변경 pressed");
        let deadline = Instant::now() + MENU_WAIT;
        loop {
            if let Some(popover) = find_popover(&row) {
                return Ok((tab, search, popover, row));
            }
            if Instant::now() >= deadline {
                return Err(coded(ErrorCode::MenuNotFound, "친구 정보 did not open"));
            }
            sleep(Duration::from_millis(50));
        }
    }

    /// `ax-friend --open-chat`: the friend's 1:1 chat through their menu's
    /// 채팅하기, even when no chat with them exists yet. Returns once a
    /// window titled with their name is open.
    pub fn open_friend_chat_via_ax(name: &str) -> Result<()> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        if find_chat_window(&app, name)?.is_some() {
            return Ok(());
        }
        let (tab, snap) = friends_tab(&app)?;
        let (search, rows) = filter_list(&tab.main, &snap, name, "AXOutline").ok_or_else(|| {
            coded(
                ErrorCode::ChatListUnavailable,
                "KakaoTalk's friend search did not behave as measured (is its search field already open?)",
            )
        })?;
        let same: Vec<&AXUIElement> = rows
            .iter()
            .filter(|(row, _)| friend_row(row).is_some_and(|f| f.name == name))
            .map(|(row, _)| row)
            .collect();
        let [row] = same.as_slice() else {
            return Err(coded(
                if same.is_empty() {
                    ErrorCode::ChatNotFound
                } else {
                    ErrorCode::ChatAmbiguous
                },
                format!("{} friends are named exactly '{name}'", same.len()),
            ));
        };
        let cell = row
            .children()
            .ok()
            .and_then(|c| c.iter().next().map(|c| (*c).clone()))
            .unwrap_or_else(|| (*row).clone());
        press_menu_item_or(&app, &cell, "채팅하기", ErrorCode::MenuNotFound)?;
        drop(search);
        drop(tab);
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            if find_chat_window(&app, name)?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk did not open a chat window titled '{name}' in time");
            }
            sleep(Duration::from_millis(100));
        }
    }

    /// `ax-friend`: read a friend's 친구 정보, and with `memo` / `rename` set
    /// them through KakaoTalk's own popover (확인), then read them back.
    /// The popover is always closed (AXCancel, never Esc).
    pub fn friend_via_ax(
        name: &str,
        memo: Option<&str>,
        rename: Option<&str>,
    ) -> Result<FriendInfo> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        if memo.is_some_and(|m| m.chars().count() > 200) {
            anyhow::bail!("KakaoTalk keeps at most 200 characters of memo");
        }
        if rename.is_some_and(|n| n.chars().count() > 20) {
            anyhow::bail!("KakaoTalk names have at most 20 characters");
        }
        // A new name can make a friend's 1:1 chat answer to an allowlisted
        // chat name, so renaming is approved like a send to an unlisted chat.
        if let Some(new_name) = rename {
            if !super::skips_allowlist() {
                openkakao_cli::human_auth::require_device_owner_auth(&format!(
                    "Approve renaming the KakaoTalk friend ‘{}’ to ‘{}’",
                    crate::util::escape_terminal_text(name),
                    crate::util::escape_terminal_text(new_name)
                ))
                .map_err(|error| coded(ErrorCode::ChatNotAllowed, format!("{error:#}")))?;
            }
        }
        let (tab, search, popover, row) = open_friend_info(&app, name)?;
        let info = read_friend_info(&popover, name);
        // The name the friend's 1:1 chat would take must not be an
        // allowlisted one ("" restores the friend's own name).
        let resulting = match rename {
            Some("") => info.as_ref().ok().and_then(|i| i.own_name.clone()),
            other => other.map(str::to_string),
        };
        if let Some(resulting) = resulting.filter(|n| !super::needs_device_owner_auth(n)) {
            let _ = popover.perform_action(&CFString::new("AXCancel"));
            return Err(coded(
                ErrorCode::ChatNotAllowed,
                format!("the friend would show as '{resulting}', an allowed_send_chats name; nothing was changed"),
            ));
        }
        if memo.is_none() && rename.is_none() {
            let _ = popover.perform_action(&CFString::new("AXCancel"));
            drop(search);
            drop(tab);
            return info;
        }
        let written = (|| -> Result<()> {
            let node = snapshot(&popover)?;
            let mut areas = Vec::new();
            node.find_all("AXTextArea", &mut areas);
            let [nickname, memo_area] = areas.as_slice() else {
                anyhow::bail!("친구 정보 shows no name and memo fields");
            };
            let selected_text: AXAttribute<CFType> =
                AXAttribute::new(&CFString::new("AXSelectedText"));
            for (area, text) in [(nickname, rename), (memo_area, memo)] {
                let Some(text) = text else { continue };
                let clear = || {
                    area.element
                        .set_value(CFString::new("").as_CFType())
                        .map_err(|e| anyhow!("could not clear a 친구 정보 field: {e:?}"))
                };
                clear()?;
                // Only an AXSelectedText insert tells KakaoTalk the text
                // changed (and enables 확인). To clear a field, insert a
                // space for that, then clear the text 확인 will save.
                let typed = if text.is_empty() { " " } else { text };
                area.element
                    .set_attribute(&selected_text, CFString::new(typed).as_CFType())
                    .map_err(|e| anyhow!("could not type into 친구 정보: {e:?}"))?;
                if text.is_empty() {
                    clear()?;
                }
            }
            let after_typing = snapshot(&popover)?;
            let mut buttons = Vec::new();
            after_typing.find_all("AXButton", &mut buttons);
            let confirm = buttons
                .iter()
                .find(|b| b.title.as_deref() == Some("확인"))
                .map(|b| b.element.clone())
                .ok_or_else(|| anyhow!("친구 정보 has no 확인 button"))?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while attr_as_bool(&confirm, "AXEnabled") != Some(true) {
                if Instant::now() >= deadline {
                    anyhow::bail!("KakaoTalk did not enable 확인 (nothing changed?)");
                }
                sleep(Duration::from_millis(50));
            }
            confirm
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|e| anyhow!("could not press 확인: {e:?}"))?;
            Ok(())
        })();
        if let Err(error) = written {
            let _ = popover.perform_action(&CFString::new("AXCancel"));
            return Err(error.context("nothing was changed; the 친구 정보 popover was closed"));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while find_popover(&row).is_some() && Instant::now() < deadline {
            sleep(Duration::from_millis(50));
        }
        drop(search);
        drop(tab);
        // Read back under the name the friend now shows.
        let shown = match rename {
            Some("") => info
                .ok()
                .and_then(|i| i.own_name)
                .unwrap_or_else(|| name.to_string()),
            Some(new_name) => new_name.to_string(),
            None => name.to_string(),
        };
        let (tab, search, popover, _) = open_friend_info(&app, &shown)?;
        let after = read_friend_info(&popover, &shown);
        let _ = popover.perform_action(&CFString::new("AXCancel"));
        drop(search);
        drop(tab);
        let after = after?;
        let memo_ok = memo.is_none_or(|m| after.memo == m);
        let name_ok = rename.is_none_or(|n| after.nickname == n);
        if !(memo_ok && name_ok) {
            return Err(coded(
                ErrorCode::ActionUnverified,
                "KakaoTalk closed 친구 정보 but does not show the new values; check the friend in KakaoTalk",
            ));
        }
        Ok(after)
    }

    /// The chat window's 메뉴 → 채팅방 설정 dialog, focus-free.
    fn open_chat_settings(
        app: &AXUIElement,
        pid: i32,
        window: &AXUIElement,
    ) -> Result<AXUIElement> {
        let started = Instant::now();
        if !menu_windows(pid).is_empty() {
            return Err(coded(
                ErrorCode::UserActive,
                "a KakaoTalk menu is already open; close it by clicking elsewhere (never Esc) and retry",
            ));
        }
        let snap = snapshot_shallow(window)?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let menu_buttons: Vec<_> = buttons
            .iter()
            .filter(|b| b.description.as_deref() == Some("메뉴"))
            .collect();
        let [menu_button] = menu_buttons.as_slice() else {
            return Err(coded(
                ErrorCode::MenuNotFound,
                "the chat window shows no single 메뉴 button",
            ));
        };
        let before = app_windows(app);
        // KakaoTalk answers this press only after its menu closes, so the
        // call would sit out the whole messaging timeout (1.5 s measured)
        // with the menu already open. Wait briefly instead and look for it.
        let _ = menu_button.element.set_messaging_timeout(0.05);
        let _ = menu_button
            .element
            .perform_action(&CFString::new(kAXPressAction));
        trace(started, "메뉴 pressed");
        let deadline = Instant::now() + MENU_WAIT;
        let menu = loop {
            if let Some(menu) = menu_windows(pid)
                .into_iter()
                .find_map(|bounds| menu_in_window(pid, bounds))
            {
                break menu;
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::MenuNotFound,
                    "the chat's 메뉴 did not appear to Accessibility in time",
                ));
            }
            sleep(Duration::from_millis(30));
        };
        trace(started, "메뉴 found");
        let items = menu_items(&menu);
        let Some((settings, _)) = items
            .iter()
            .find(|(_, title)| title.as_deref().map(str::trim) == Some("채팅방 설정"))
        else {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
            return Err(coded(
                ErrorCode::MenuNotFound,
                "the chat's 메뉴 has no 채팅방 설정 item",
            ));
        };
        let _ = settings.perform_action(&CFString::new(kAXPressAction));
        trace(started, "채팅방 설정 pressed");
        if menu_still_open(app) {
            let _ = menu.perform_action(&CFString::new("AXCancel"));
        }
        let has_text = |node: &AxNode, text: &str| {
            node.any(&|n: &AxNode| n.role == "AXStaticText" && n.value.as_deref() == Some(text))
        };
        new_dialog(app, &before, OPEN_CHAT_TIMEOUT, |w| {
            snapshot(w).is_ok_and(|node| has_text(&node, "채팅방 설정"))
        })
        .ok_or_else(|| coded(ErrorCode::MenuNotFound, "채팅방 설정 did not open"))
    }

    /// `local-rename-chat`: the name this account shows for a chat, through
    /// 채팅방 설정 → 채팅방 정보 → 편집 → 채팅방 정보 설정 → 확인, focus-free
    /// (26.8, measured 2026-11-07). A new name can make a chat answer to an
    /// allowlisted name, so it always needs device-owner authentication
    /// (`--dangerously-skip-allowlist` skips it). Verified by the chat window
    /// taking the new title.
    pub fn rename_chat_via_ax(chat_display_name: &str, new_name: &str) -> Result<()> {
        if new_name.trim().is_empty() || new_name.chars().count() > 50 {
            anyhow::bail!("a chat name has 1 to 50 characters");
        }
        if !super::needs_device_owner_auth(new_name) {
            return Err(coded(
                ErrorCode::ChatNotAllowed,
                format!("'{new_name}' is an allowed_send_chats name; renaming a chat to it would let sends reach it without approval"),
            ));
        }
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let mut focus = FocusGuard::new(pid);
        let chat = open_chat(&app, chat_display_name, &mut focus)?;
        focus.restore();
        let window = chat.window;
        clear_leftover_alerts(&window)?;
        if !super::skips_allowlist() {
            openkakao_cli::human_auth::require_device_owner_auth(&format!(
                "Approve renaming the KakaoTalk chat ‘{}’ to ‘{}’",
                crate::util::escape_terminal_text(chat_display_name),
                crate::util::escape_terminal_text(new_name)
            ))
            .map_err(|error| coded(ErrorCode::ChatNotAllowed, format!("{error:#}")))?;
        }
        let dialog = open_chat_settings(&app, pid, &window)?;
        let result = rename_in_settings(&app, &dialog, new_name);
        let closed = close_dialog(&app, &dialog, "채팅방 설정");
        result?;
        closed?;
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            if find_chat_window(&app, new_name)?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::ActionUnverified,
                    format!("KakaoTalk took the name but no chat window is titled '{new_name}' yet; check the chat"),
                ));
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    fn rename_in_settings(app: &AXUIElement, dialog: &AXUIElement, new_name: &str) -> Result<()> {
        let find_edit = |dialog: &AXUIElement| -> Result<Option<AXUIElement>> {
            let mut buttons = Vec::new();
            let snap = snapshot(dialog)?;
            snap.find_all("AXButton", &mut buttons);
            Ok(buttons
                .into_iter()
                .find(|b| b.title.as_deref() == Some("편집"))
                .map(|b| b.element.clone()))
        };
        let mut edit = find_edit(dialog)?;
        if edit.is_none() {
            // 채팅방 정보 is the first of the dialog's two unnamed tabs.
            let snap = snapshot_shallow(dialog)?;
            let blank = |t: &Option<String>| t.as_deref().is_none_or(|t| t.trim().is_empty());
            let mut tabs: Vec<&AxNode> = snap
                .children
                .iter()
                .filter(|n| {
                    n.role == "AXButton"
                        && blank(&n.title)
                        && blank(&n.description)
                        && attr_as_string(&n.element, "AXSubrole").is_none()
                })
                .collect();
            tabs.sort_by(|a, b| {
                a.position_y
                    .partial_cmp(&b.position_y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(tab) = tabs.first() {
                let _ = tab.element.perform_action(&CFString::new(kAXPressAction));
            }
            let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
            while edit.is_none() && Instant::now() < deadline {
                sleep(Duration::from_millis(100));
                edit = find_edit(dialog)?;
            }
        }
        let edit =
            edit.ok_or_else(|| coded(ErrorCode::MenuNotFound, "채팅방 설정 shows no 편집 button"))?;
        let before = app_windows(app);
        let _ = edit.perform_action(&CFString::new(kAXPressAction));
        // 채팅방 정보 설정 is a standard window, not a dialog.
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let editor = loop {
            let found = app_windows(app).into_iter().find(|w| {
                !before.iter().any(|old| same_element(old, w))
                    && snapshot(w).is_ok_and(|node| {
                        node.any(&|n: &AxNode| n.value.as_deref() == Some("채팅방 정보 설정"))
                    })
            });
            if let Some(editor) = found {
                break editor;
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::MenuNotFound,
                    "채팅방 정보 설정 did not open",
                ));
            }
            sleep(Duration::from_millis(100));
        };
        let written = (|| -> Result<()> {
            let node = snapshot(&editor)?;
            let field = node
                .find_first("AXTextArea")
                .map(|n| n.element.clone())
                .ok_or_else(|| anyhow!("채팅방 정보 설정 shows no name field"))?;
            field
                .set_value(CFString::new("").as_CFType())
                .map_err(|e| anyhow!("could not clear the chat name: {e:?}"))?;
            let selected_text: AXAttribute<CFType> =
                AXAttribute::new(&CFString::new("AXSelectedText"));
            field
                .set_attribute(&selected_text, CFString::new(new_name).as_CFType())
                .map_err(|e| anyhow!("could not type the chat name: {e:?}"))?;
            let mut buttons = Vec::new();
            let after = snapshot(&editor)?;
            after.find_all("AXButton", &mut buttons);
            let confirm = buttons
                .iter()
                .find(|b| b.title.as_deref() == Some("확인"))
                .map(|b| b.element.clone())
                .ok_or_else(|| anyhow!("채팅방 정보 설정 has no 확인 button"))?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while attr_as_bool(&confirm, "AXEnabled") != Some(true) {
                if Instant::now() >= deadline {
                    anyhow::bail!("KakaoTalk did not enable 확인");
                }
                sleep(Duration::from_millis(50));
            }
            confirm
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|e| anyhow!("could not press 확인: {e:?}"))?;
            Ok(())
        })();
        if written.is_err() {
            if let Some(close) = attr_as_element(&editor, "AXCloseButton") {
                let _ = close.perform_action(&CFString::new(kAXPressAction));
            }
            return written.map_err(|error| error.context("the chat name was not changed"));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while app_windows(app).iter().any(|w| same_element(w, &editor)) && Instant::now() < deadline
        {
            sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    /// `local-mark-read`: KakaoTalk's own 읽음 처리 for one chat without
    /// opening it, focus-free. The row is found like `--open-if-needed`
    /// finds it (KakaoTalk's chat search, exact name, duplicates refuse or
    /// `--pick`), so a recycled list row can never stand in for it, and
    /// success needs the row's unread badge gone.
    pub fn mark_read_via_ax(chat_display_name: &str) -> Result<u32> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let main_window = find_or_show_main_window(&app)?;
        ensure_chatrooms_tab(&main_window, true)?;
        // Right after another search closed, the list is still restoring and
        // a new search would read its rows (see restored_chat_list).
        let snap = match restored_chat_list(&main_window) {
            Some(snap) => snap,
            None => ensure_chatrooms_tab(&main_window, true)?,
        };
        let (search, rows) =
            filter_chat_list(&main_window, &snap, chat_display_name).ok_or_else(|| {
                coded(
                    ErrorCode::ChatListUnavailable,
                    "KakaoTalk's chat search did not behave as measured (is its search field already open?)",
                )
            })?;
        let names: Vec<Option<String>> = rows.iter().map(|(_, name)| name.clone()).collect();
        let row = match super::match_chat_row(&names, chat_display_name) {
            super::ChatMatch::Found(index) => rows[index].0.clone(),
            super::ChatMatch::NotFound => {
                return Err(coded(
                    ErrorCode::ChatNotFound,
                    format!(
                        "KakaoTalk's chat search lists no chat named exactly '{chat_display_name}'"
                    ),
                ))
            }
            super::ChatMatch::Ambiguous(count) => {
                match super::policy()
                    .pick
                    .and_then(|n| super::picked_row(&names, chat_display_name, Some(n)))
                {
                    Some(index) => rows[index].0.clone(),
                    None => {
                        return Err(coded(
                            ErrorCode::ChatAmbiguous,
                            format!("{count} chats are named '{chat_display_name}'; choose one with --pick <N> (top to bottom)"),
                        ))
                    }
                }
            }
        };
        let unread = snapshot(&row)
            .ok()
            .and_then(|node| chat_row(&node))
            .map_or(0, |r| r.unread);
        if unread == 0 {
            drop(search);
            return Ok(0);
        }
        press_row_menu_item(&app, &row, &["읽음 처리", "Mark as Read"], "읽음 처리")?;
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            let now = snapshot(&row).ok().and_then(|node| chat_row(&node));
            if now
                .as_ref()
                .is_some_and(|r| r.unread == 0 && r.name == chat_display_name)
            {
                drop(search);
                return Ok(u32::try_from(unread).unwrap_or(0));
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::ActionUnverified,
                    format!(
                        "pressed 읽음 처리 but '{chat_display_name}' still shows unread messages"
                    ),
                ));
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    /// `ax-export`: the chat's whole history through KakaoTalk's own
    /// 채팅방 설정 → 대화 내용 저장 → 텍스트 파일로 저장, focus-free: the chat
    /// window's 메뉴 button, 채팅방 설정, the dialog's second tab, 텍스트 파일로
    /// 저장, 저장 in the save sheet, then 확인 on "대화내용 내보내기가
    /// 완료되었습니다.". KakaoTalk writes `<name>.csv` (Date, User, Message,
    /// since the chat began) to the folder the save sheet shows (26.8,
    /// measured 2026-11-07). Returns that file.
    pub fn export_via_ax(chat_display_name: &str) -> Result<std::path::PathBuf> {
        let started = Instant::now();
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let mut focus = FocusGuard::new(pid);
        let chat = open_chat(&app, chat_display_name, &mut focus)?;
        focus.restore();
        let window = chat.window;
        clear_leftover_alerts(&window)?;
        let dialog = open_chat_settings(&app, pid, &window)?;
        trace(started, "채팅방 설정 opened");
        let result = save_export(&dialog);
        // The dialog is closed whatever happened; a failure to close is
        // reported only when the export itself worked.
        let closed = close_dialog(&app, &dialog, "채팅방 설정");
        let path = result?;
        closed?;
        trace(started, "export saved");
        Ok(path)
    }

    /// In an open 채팅방 설정 dialog: 텍스트 파일로 저장, 저장, 확인.
    fn save_export(dialog: &AXUIElement) -> Result<std::path::PathBuf> {
        let started = Instant::now();
        let find_button = |root: &AxNode, title: &str| -> Option<AXUIElement> {
            let mut buttons = Vec::new();
            root.find_all("AXButton", &mut buttons);
            buttons
                .into_iter()
                .find(|b| b.title.as_deref() == Some(title))
                .map(|b| b.element.clone())
        };
        let mut save_text = find_button(&snapshot(dialog)?, "텍스트 파일로 저장");
        if save_text.is_none() {
            // Its tabs are two unnamed buttons at the top; 대화 내용 저장 is
            // on the second (the lower one).
            let snap = snapshot_shallow(dialog)?;
            let mut tabs: Vec<&AxNode> = snap
                .children
                .iter()
                .filter(|n| {
                    let blank =
                        |t: &Option<String>| t.as_deref().is_none_or(|t| t.trim().is_empty());
                    // Not the window's own close/minimize/zoom buttons.
                    n.role == "AXButton"
                        && blank(&n.title)
                        && blank(&n.description)
                        && attr_as_string(&n.element, "AXSubrole").is_none()
                })
                .collect();
            tabs.sort_by(|a, b| {
                a.position_y
                    .partial_cmp(&b.position_y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if crate::util::debug_enabled() {
                eprintln!("[ax] 채팅방 설정 tabs: {}", tabs.len());
            }
            if let Some(tab) = tabs.get(1) {
                let _ = tab.element.perform_action(&CFString::new(kAXPressAction));
            }
            let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
            while save_text.is_none() && Instant::now() < deadline {
                sleep(Duration::from_millis(100));
                save_text = find_button(&snapshot(dialog)?, "텍스트 파일로 저장");
            }
        }
        let save_text = save_text.ok_or_else(|| {
            coded(
                ErrorCode::MenuNotFound,
                "채팅방 설정 shows no 텍스트 파일로 저장 button",
            )
        })?;
        trace(started, "텍스트 파일로 저장 found");
        // Answered only once the save sheet is up; the loop below waits for it.
        let _ = save_text.set_messaging_timeout(0.05);
        let _ = save_text.perform_action(&CFString::new(kAXPressAction));
        trace(started, "텍스트 파일로 저장 pressed");
        let sheet_of = |dialog: &AXUIElement| -> Option<AXUIElement> {
            dialog
                .children()
                .ok()?
                .iter()
                .find(|c| role(c) == "AXSheet")
                .map(|c| (*c).clone())
        };
        fn by_id(root: &AxNode, id: &str) -> Option<AXUIElement> {
            if attr_as_string(&root.element, "AXIdentifier").as_deref() == Some(id) {
                return Some(root.element.clone());
            }
            root.children.iter().find_map(|child| by_id(child, id))
        }
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let panel = loop {
            if let Some(sheet) = sheet_of(dialog) {
                if attr_as_string(&sheet, "AXIdentifier").as_deref() == Some("save-panel") {
                    break sheet;
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk's save sheet did not appear");
            }
            sleep(Duration::from_millis(100));
        };
        trace(started, "save sheet open");
        let panel_node = snapshot(&panel)?;
        let name = by_id(&panel_node, "saveAsNameTextField")
            .and_then(|field| attr_as_string(&field, "AXValue"))
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| anyhow!("the save sheet shows no file name"))?;
        let folder =
            by_id(&panel_node, "where popup").and_then(|popup| attr_as_string(&popup, "AXValue"));
        if name.contains('/') || name.starts_with('.') {
            anyhow::bail!("the save sheet proposes an unexpected file name {name:?}");
        }
        // Only a file written after 저장 is this export, never an older one.
        let saved_after = std::time::SystemTime::now() - Duration::from_secs(2);
        let ok = by_id(&panel_node, "OKButton")
            .ok_or_else(|| anyhow!("the save sheet has no 저장 button"))?;
        let _ = ok.perform_action(&CFString::new(kAXPressAction));
        trace(started, "저장 pressed");
        // A long chat shows a progress sheet ("37%") before the done notice.
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if let Some(sheet) = sheet_of(dialog) {
                let node = snapshot(&sheet)?;
                let mut texts = Vec::new();
                node.find_all("AXStaticText", &mut texts);
                let texts: Vec<String> = texts.iter().filter_map(|t| t.value.clone()).collect();
                let confirm = find_button(&node, "확인");
                if texts.iter().any(|t| t.contains("완료되었습니다")) {
                    if let Some(confirm) = confirm {
                        let _ = confirm.perform_action(&CFString::new(kAXPressAction));
                    }
                    break;
                }
                // The progress sheet ("37%", "대화 내용 내보내기 중입니다.")
                // has no 확인; a sheet with one that is not the done notice
                // is KakaoTalk reporting a failure.
                if let Some(confirm) = confirm {
                    let _ = confirm.perform_action(&CFString::new(kAXPressAction));
                    anyhow::bail!(
                        "KakaoTalk did not finish the export: {}",
                        texts.join(" ").trim()
                    );
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk did not finish the export within 5 minutes");
            }
            sleep(Duration::from_millis(200));
        }
        trace(started, "export done");
        let home = dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))?;
        let folders: Vec<std::path::PathBuf> = match folder.as_deref() {
            Some("다운로드" | "Downloads") => vec![home.join("Downloads")],
            Some("데스크탑" | "바탕화면" | "Desktop") => vec![home.join("Desktop")],
            Some("문서" | "Documents") => vec![home.join("Documents")],
            _ => ["Downloads", "Desktop", "Documents"]
                .iter()
                .map(|d| home.join(d))
                .collect(),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(path) = folders
                .iter()
                .map(|dir| dir.join(format!("{name}.csv")))
                .find(|path| {
                    std::fs::symlink_metadata(path).is_ok_and(|meta| {
                        meta.file_type().is_file()
                            && meta
                                .modified()
                                .is_ok_and(|modified| modified >= saved_after)
                    })
                })
            {
                return Ok(path);
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "KakaoTalk saved the export as '{name}.csv' in {}, which openkakao could not find",
                    folder.as_deref().unwrap_or("the folder its save sheet showed")
                );
            }
            sleep(Duration::from_millis(200));
        }
    }

    pub fn delete_via_ax(
        chat_display_name: &str,
        text: &str,
        scope: super::DeleteScope,
        dry_run: bool,
    ) -> Result<BubbleAction> {
        let started = Instant::now();
        let target = super::BubbleTarget::Text(text.to_string());
        // 모든 대화 상대에게서 needs your own bubble; 나에게서만 takes any.
        let own_only = scope == super::DeleteScope::Everyone;
        let (app, _pid, mut focus, window, elements, bubble, found_in) =
            bubble_target(chat_display_name, &target, own_only)?;
        trace(started, "target bubble found");
        let action = BubbleAction {
            target_text: bubble.text.clone(),
            target_outgoing: bubble.outgoing,
        };
        if dry_run {
            return Ok(action);
        }
        // Only a real action may dismiss a leftover alert.
        clear_leftover_alerts(&window)?;
        focus.watch();
        let prompted = require_send_approval(&window, chat_display_name, "message delete in")?;
        let pairs = |rows: &[(String, bool, bool)]| -> Vec<(String, bool)> {
            rows.iter()
                .map(|(text, outgoing, _)| (text.clone(), *outgoing))
                .collect()
        };
        let before = if prompted {
            text_bubbles(&elements.message_table)?
                .iter()
                .map(|bubble| (bubble.text.clone(), bubble.outgoing))
                .collect()
        } else {
            pairs(&found_in)
        };
        focus.watch();
        press_menu_item_or(
            &app,
            &bubble.text_area,
            scope.menu_item(),
            ErrorCode::DeleteUnavailable,
        )?;
        focus.restore();
        trace(started, "delete item pressed");

        let prepared = (|| -> Result<AXUIElement> {
            let deadline = Instant::now() + Duration::from_secs(3);
            let (confirm, _) = loop {
                if let Some(bar) = selection_bar(&window)? {
                    break bar;
                }
                if Instant::now() >= deadline {
                    return Err(coded(
                        ErrorCode::ActionUnverified,
                        "KakaoTalk's message-selection mode did not appear after the delete item",
                    ));
                }
                sleep(Duration::from_millis(50));
            };
            trace(started, "selection mode");
            // The target's checkbox. KakaoTalk redraws the rows for this mode,
            // so the bubble's text area is usually a new AX element: prefer
            // that identity, else the one row with the exact text and
            // direction (the target was already unique by both). Checkboxes
            // can appear a moment after the bar.
            let center = table_center_x(&elements.message_table);
            let deadline = Instant::now() + Duration::from_secs(2);
            let (checkboxes, checkbox) = loop {
                let mut seen = 0;
                let (rows, _) = tail_rows(&elements.message_table, |_| {
                    seen += 1;
                    seen >= BUBBLE_SEARCH_ROWS
                })?;
                let area = |row: &AxNode| {
                    row.find_first("AXTextArea")
                        .map(|area| area.element.clone())
                };
                let same_element = |row: &AxNode| {
                    area(row).is_some_and(|element| unsafe {
                        CFEqual(element.as_CFTypeRef(), bubble.text_area.as_CFTypeRef()) != 0
                    })
                };
                let same_text = |row: &AxNode| {
                    row.find_first("AXTextArea").is_some_and(|area| {
                        area.value.as_deref() == Some(bubble.text.as_str())
                            && center
                                .zip(area.center_x())
                                .is_some_and(|(table, bubble_x)| {
                                    (bubble_x > table) == bubble.outgoing
                                })
                    })
                };
                let with_box: Vec<(&AxNode, AXUIElement)> = rows
                    .iter()
                    .filter_map(|row| {
                        row.find_first("AXCheckBox")
                            .map(|checkbox| (row, checkbox.element.clone()))
                    })
                    .collect();
                let by_element: Vec<usize> = (0..with_box.len())
                    .filter(|&i| same_element(with_box[i].0))
                    .collect();
                let by_text: Vec<usize> = (0..with_box.len())
                    .filter(|&i| same_text(with_box[i].0))
                    .collect();
                let pick = match (by_element.as_slice(), by_text.as_slice()) {
                    ([index], _) | ([], [index]) => Some(*index),
                    _ => None,
                };
                if let Some(index) = pick {
                    let checkboxes: Vec<(bool, AXUIElement)> = with_box
                        .iter()
                        .enumerate()
                        .map(|(i, (_, checkbox))| (i == index, checkbox.clone()))
                        .collect();
                    let checkbox = with_box[index].1.clone();
                    break (checkboxes, checkbox);
                }
                if Instant::now() >= deadline {
                    return Err(coded(
                        ErrorCode::ActionUnverified,
                        format!(
                            "the selection mode shows {} rows with checkboxes, {} of them the target bubble; refusing to guess",
                            with_box.len(),
                            by_text.len().max(by_element.len())
                        ),
                    ));
                }
                sleep(Duration::from_millis(100));
            };
            checkbox
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("could not tick the target bubble: {error:?}"))?;
            // Exactly the target ticked, and 확인 enabled.
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let ticked: Vec<bool> = checkboxes
                    .iter()
                    .filter(|(_, checkbox)| attr_as_i64(checkbox, "AXValue") == Some(1))
                    .map(|(target, _)| *target)
                    .collect();
                if ticked == [true] && attr_as_bool(&confirm, "AXEnabled") == Some(true) {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(coded(
                        ErrorCode::ActionUnverified,
                        format!(
                            "the selection did not become exactly the target bubble ({} ticked)",
                            ticked.len()
                        ),
                    ));
                }
                sleep(Duration::from_millis(50));
            }
            confirm
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("could not press 확인: {error:?}"))?;
            trace(started, "selection confirmed");
            let deadline = Instant::now() + Duration::from_secs(3);
            let sheet = loop {
                if let Some(sheet) = window_sheet(&window) {
                    break sheet;
                }
                if Instant::now() >= deadline {
                    return Err(coded(
                        ErrorCode::ActionUnverified,
                        "KakaoTalk showed no delete confirmation after 확인",
                    ));
                }
                sleep(Duration::from_millis(50));
            };
            let snap = snapshot_with(&sheet, true)?;
            let mut labels = Vec::new();
            snap.find_all("AXStaticText", &mut labels);
            let texts: Vec<String> = labels
                .iter()
                .filter_map(|label| label.value.clone())
                .collect();
            let buttons = sheet_buttons(&sheet)?;
            let facts: Vec<super::SheetButton> =
                buttons.iter().map(|(_, facts)| facts.clone()).collect();
            let Some(index) = super::pick_delete_confirm(&texts, &facts, scope) else {
                return Err(coded(
                    ErrorCode::ActionUnverified,
                    format!(
                        "KakaoTalk's confirmation does not match '{}' (it says {texts:?})",
                        scope.menu_item()
                    ),
                ));
            };
            Ok(buttons[index].0.clone())
        })();
        let delete = match prepared {
            Ok(delete) => delete,
            Err(error) => {
                leave_selection_mode(&window);
                return Err(
                    error.context("nothing was deleted; KakaoTalk is back in the normal chat")
                );
            }
        };
        let press_error = delete.perform_action(&CFString::new(kAXPressAction)).err();
        trace(started, "delete committed");
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            let back =
                matches!(selection_bar(&window), Ok(None)) && window_sheet(&window).is_none();
            if back {
                if let Ok(after) = text_bubbles(&elements.message_table) {
                    let after: Vec<(String, bool)> = after
                        .iter()
                        .map(|bubble| (bubble.text.clone(), bubble.outgoing))
                        .collect();
                    if super::delete_verified(&before, &after, &bubble.text, bubble.outgoing) {
                        trace(started, "delete verified");
                        return Ok(action);
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::DeliveryUncertain,
                    format!(
                        "the delete may have been applied but '{chat_display_name}' still shows the bubble after {}s; check KakaoTalk; press result: {press_error:?}",
                        VERIFY_TIMEOUT.as_secs()
                    ),
                ));
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    /// One chat-list row scraped from the main window, read-only (never opens
    /// the chat, so its unread state is untouched).
    #[derive(Debug, Clone)]
    pub struct ChatListRow {
        pub name: String,
        pub unread: i32,
        /// A group chat's member count, shown right of its name.
        pub members: Option<u32>,
        pub preview: String,
        // Scraped for completeness but not currently consumed by any caller
        // (ax-watch's event doesn't need the row's own last-message
        // timestamp); keep it available for future use.
        #[allow(dead_code)]
        pub timestamp: String,
    }

    /// Scrape every visible/loaded chat-list row from KakaoTalk's main window.
    /// Uses the same chat-list traversal as `open_chat`
    /// (main window → chatrooms tab → AXTable → AXRow), but only reads each
    /// row instead of selecting it — so nothing is opened and no unread state
    /// changes. Rows with no readable name are skipped.
    pub fn scrape_chat_list() -> Result<Vec<ChatListRow>> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let main_window = find_main_window(&app)?;
        let snap = ensure_chatrooms_tab(&main_window, false)?;
        let table = snap
            .find_first("AXTable")
            .ok_or_else(|| anyhow!("could not find chat list table in KakaoTalk's AX tree"))?;

        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        Ok(rows.into_iter().filter_map(chat_row).collect())
    }

    /// One chat-list row's name, unread badge, time and preview. Rows we
    /// can't name are `None`.
    fn chat_row(row: &AxNode) -> Option<ChatListRow> {
        let mut static_texts = Vec::new();
        row.find_all("AXStaticText", &mut static_texts);
        // The first static text is the chat name (same convention as
        // open_chat).
        let name_node = static_texts.first()?;
        let name = name_node.value.clone()?;
        // Among the remaining static texts, a whole integer is the member
        // count when it sits on the name's line and the unread badge when it
        // sits below it (26.8: same y vs about 20 px lower, measured
        // 2026-11-07); a non-numeric one ("어제", "오후 3:14") is the time.
        let on_name_line = |t: &AxNode| match (t.position_y, name_node.position_y) {
            (Some(y), Some(name_y)) => (y - name_y).abs() <= 4.0,
            _ => false,
        };
        let mut unread = 0;
        let mut members = None;
        let mut timestamp = String::new();
        for t in static_texts.iter().skip(1) {
            let Some(v) = t.value.as_deref() else {
                continue;
            };
            if let Ok(n) = v.trim().parse::<i32>() {
                if on_name_line(t) {
                    members = members.or(u32::try_from(n).ok());
                } else if unread == 0 {
                    unread = n;
                }
            } else if timestamp.is_empty() {
                timestamp = v.to_string();
            }
        }
        // The last-message preview is the row's AXTextArea value.
        let preview = row
            .find_first("AXTextArea")
            .and_then(|t| t.value.clone())
            .unwrap_or_default();
        Some(ChatListRow {
            name,
            unread,
            members,
            preview,
            timestamp,
        })
    }

    /// What tells same-name rows apart for a human: time, preview, pin.
    fn row_summary(row: &AXUIElement) -> String {
        let Ok(node) = snapshot(row) else {
            return "(row could not be read)".to_string();
        };
        let pinned = node.any(&|n: &AxNode| {
            n.role == "AXImage" && n.description.as_deref().is_some_and(|d| d.contains("pin"))
        });
        let facts = chat_row(&node).map_or_else(Vec::new, |row| {
            let mut facts = vec![row.timestamp, crate::util::truncate(&row.preview, 40)];
            if let Some(members) = row.members {
                facts.push(format!("{members} members"));
            }
            if row.unread > 0 {
                facts.push(format!("{} unread", row.unread));
            }
            facts
        });
        let mut parts: Vec<String> = facts.into_iter().filter(|f| !f.is_empty()).collect();
        if pinned {
            parts.push("📌 pinned".to_string());
        }
        crate::util::escape_terminal_text(&parts.join(" · "))
    }

    /// KakaoTalk's normal (layer-0) windows across all Spaces and how many are
    /// on screen now, from the public CoreGraphics window list (owner pid and
    /// layer need no Screen Recording permission; titles would).
    fn cg_window_counts(pid: i32) -> Option<(usize, usize)> {
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::CFNumber;
        use core_graphics::window::{
            copy_window_info, kCGNullWindowID, kCGWindowIsOnscreen, kCGWindowLayer,
            kCGWindowListOptionAll, kCGWindowOwnerPID,
        };
        let windows = copy_window_info(kCGWindowListOptionAll, kCGNullWindowID)?;
        let (mut total, mut onscreen) = (0, 0);
        for window in windows.iter() {
            let info: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(*window as CFDictionaryRef) };
            let value = |key| {
                info.find(unsafe { CFString::wrap_under_get_rule(key) })
                    .map(|v| v.clone())
            };
            let number = |key| {
                value(key)
                    .and_then(|value| value.downcast::<CFNumber>())
                    .and_then(|number| number.to_i64())
            };
            if number(unsafe { kCGWindowOwnerPID }) != Some(pid.into())
                || number(unsafe { kCGWindowLayer }) != Some(0)
            {
                continue;
            }
            total += 1;
            if value(unsafe { kCGWindowIsOnscreen })
                .and_then(|value| value.downcast::<CFBoolean>())
                .map(bool::from)
                == Some(true)
            {
                onscreen += 1;
            }
        }
        Some((total, onscreen))
    }

    /// Record the Dock's "Assign To → All Desktops" for KakaoTalk and restart
    /// the Dock so it takes effect (the Dock blinks once). No private
    /// window-server APIs are used.
    pub fn assign_kakaotalk_to_all_spaces() -> Result<()> {
        run_bounded(
            Command::new("defaults").args([
                "write",
                "com.apple.spaces",
                "app-bindings",
                "-dict-add",
                super::SPACES_BINDING_ID,
                "AllSpaces",
            ]),
            OPEN_CHAT_TIMEOUT,
            "recording KakaoTalk's All Desktops assignment",
        )?;
        run_bounded(
            Command::new("killall").arg("Dock"),
            OPEN_CHAT_TIMEOUT,
            "restarting the Dock",
        )
        .map(drop)
    }

    /// Sign a signed-out KakaoTalk in with an account and password (the
    /// caller reads the password from the keychain), by Accessibility as
    /// measured on 26.8. The process is the signature-verified KakaoTalk
    /// (`find_kakaotalk_pid`) before anything is typed. A logout notice or an
    /// abandoned QR page is dismissed only when its text is recognized. The
    /// password goes in through `AXValue` and is read back by length (the
    /// secure field ignores `AXSelectedText`). The account is inserted with
    /// `AXSelectedText` into the focused account field, which is what enables
    /// 로그인. A rejected password is reported and never retried, because
    /// Kakao locks accounts after repeated failures. Returns `false` when
    /// KakaoTalk was already signed in.
    pub fn login_via_ax(account: &str, password: &str) -> Result<bool> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let mut focus = FocusGuard::new(pid);
        let result = login_steps(&app, account, password, &mut focus);
        focus.restore();
        result
    }

    fn login_window(app: &AXUIElement) -> Option<AXUIElement> {
        app.windows().ok()?.iter().find_map(|window| {
            window
                .title()
                .ok()
                .filter(|title| super::is_login_window_title(&title.to_string()))
                .map(|_| window.clone())
        })
    }

    fn static_texts(node: &AxNode) -> Vec<String> {
        let mut texts = Vec::new();
        node.find_all("AXStaticText", &mut texts);
        texts.into_iter().filter_map(|t| t.value.clone()).collect()
    }

    /// The 로그인 window, raising KakaoTalk (behind the idle wait) when it is
    /// hidden or on another Space. `None` once KakaoTalk shows its main window
    /// instead: it is signed in.
    fn show_login_window(app: &AXUIElement, focus: &mut FocusGuard) -> Result<Option<AXUIElement>> {
        if let Some(window) = login_window(app) {
            return Ok(Some(window));
        }
        if find_main_window(app).is_ok() {
            return Ok(None);
        }
        focus.before_raise()?;
        run_bounded(
            Command::new("open").args(["-b", KAKAOTALK_BUNDLE_ID]),
            OPEN_CHAT_TIMEOUT,
            "showing KakaoTalk's sign-in window",
        )?;
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            if let Some(window) = login_window(app) {
                return Ok(Some(window));
            }
            if find_main_window(app).is_ok() {
                return Ok(None);
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::LoginNeedsHuman,
                    "KakaoTalk showed neither its sign-in nor its main window",
                ));
            }
            sleep(Duration::from_millis(200));
        }
    }

    /// Confirm a recognized sheet on the 로그인 window (`확인`). An unknown
    /// sheet is left alone and reported.
    fn dismiss_login_sheet(window: &AXUIElement) -> Result<bool> {
        let snap = snapshot_shallow(window)?;
        let Some(sheet) = snap.find_first("AXSheet") else {
            return Ok(false);
        };
        let texts = static_texts(sheet);
        if !super::is_dismissable_login_sheet(&texts) {
            return Err(coded(
                ErrorCode::LoginNeedsHuman,
                format!("KakaoTalk's sign-in window shows an unrecognized dialog: {texts:?}"),
            ));
        }
        let mut buttons = Vec::new();
        sheet.find_all("AXButton", &mut buttons);
        let ok = buttons
            .iter()
            .find(|button| button.title.as_deref() == Some("확인"))
            .ok_or_else(|| anyhow!("the sign-in dialog {texts:?} has no 확인 button"))?;
        ok.element
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not confirm the sign-in dialog: {error:?}"))?;
        sleep(Duration::from_millis(600));
        Ok(true)
    }

    fn login_steps(
        app: &AXUIElement,
        account: &str,
        password: &str,
        focus: &mut FocusGuard,
    ) -> Result<bool> {
        focus.watch();
        let Some(mut window) = show_login_window(app, focus)? else {
            return Ok(false);
        };
        // At most: the logout notice, then leaving an abandoned QR page (its
        // close button asks to stop, and confirming closes the window).
        for _ in 0..3 {
            dismiss_login_sheet(&window)?;
            if super::login_screen(&static_texts(&snapshot_shallow(&window)?))
                != super::LoginScreen::Qr
            {
                break;
            }
            let close = attr_as_element(&window, "AXCloseButton")
                .ok_or_else(|| anyhow!("the QR sign-in page has no close button"))?;
            close
                .perform_action(&CFString::new(kAXPressAction))
                .map_err(|error| anyhow!("could not leave the QR sign-in page: {error:?}"))?;
            sleep(Duration::from_millis(600));
            dismiss_login_sheet(&window)?;
            let Some(again) = show_login_window(app, focus)? else {
                return Ok(false);
            };
            window = again;
        }

        let snap = snapshot_shallow(&window)?;
        let mut fields = Vec::new();
        snap.find_all("AXTextField", &mut fields);
        let is_secure = |field: &AxNode| {
            attr_as_string(&field.element, "AXSubrole").as_deref() == Some("AXSecureTextField")
        };
        let (Some(secure), Some(account_field)) = (
            fields.iter().copied().find(|field| is_secure(field)),
            fields.iter().copied().find(|field| !is_secure(field)),
        ) else {
            return Err(coded(
                ErrorCode::LoginNeedsHuman,
                format!(
                    "KakaoTalk's sign-in window shows no account/password form: {:?}",
                    static_texts(&snap)
                ),
            ));
        };
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let login = buttons
            .iter()
            .find(|button| button.title.as_deref() == Some("로그인"))
            .map(|button| button.element.clone())
            .ok_or_else(|| anyhow!("the sign-in form has no 로그인 button"))?;

        secure
            .element
            .set_value(CFString::new(password).as_CFType())
            .map_err(|error| anyhow!("could not enter the password: {error:?}"))?;
        let entered = attr_as_string(&secure.element, "AXValue").map(|v| v.chars().count());
        if entered != Some(password.chars().count()) {
            return Err(coded(
                ErrorCode::LoginNeedsHuman,
                "the password field did not take the password",
            ));
        }
        let focused: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXFocused"));
        let selected_text: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXSelectedText"));
        account_field
            .element
            .set_attribute(&focused, CFBoolean::true_value().as_CFType())
            .map_err(|error| anyhow!("could not focus the account field: {error:?}"))?;
        account_field
            .element
            .set_value(CFString::new("").as_CFType())
            .map_err(|error| anyhow!("could not clear the account field: {error:?}"))?;
        account_field
            .element
            .set_attribute(&selected_text, CFString::new(account).as_CFType())
            .map_err(|error| anyhow!("could not enter the account: {error:?}"))?;
        let deadline = Instant::now() + COMPOSER_VERIFY_TIMEOUT;
        loop {
            let ready = attr_as_string(&account_field.element, "AXValue").as_deref()
                == Some(account)
                && attr_as_bool(&login, "AXEnabled") == Some(true);
            if ready {
                break;
            }
            if Instant::now() >= deadline {
                return Err(coded(
                    ErrorCode::LoginNeedsHuman,
                    "the account did not read back or 로그인 stayed disabled",
                ));
            }
            sleep(COMPOSER_VERIFY_POLL_INTERVAL);
        }

        login
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not press 로그인: {error:?}"))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            sleep(Duration::from_millis(300));
            match login_window(app) {
                None if find_main_window(app).is_ok() => return Ok(true),
                Some(window) => {
                    let texts = snapshot_shallow(&window)
                        .map(|snap| static_texts(&snap))
                        .unwrap_or_default();
                    if super::login_screen(&texts) == super::LoginScreen::Rejected {
                        return Err(coded(
                            ErrorCode::LoginRejected,
                            "KakaoTalk rejected the account or password",
                        ));
                    }
                }
                None => {}
            }
            if Instant::now() >= deadline {
                let texts = login_window(app)
                    .and_then(|window| snapshot_shallow(&window).ok())
                    .map(|snap| static_texts(&snap))
                    .unwrap_or_default();
                return Err(coded(
                    ErrorCode::LoginNeedsHuman,
                    format!("KakaoTalk did not finish signing in within 20s: {texts:?}"),
                ));
            }
        }
    }

    /// `open -g` (background) KakaoTalk by bundle id, wait for its process, and
    /// hand the foreground back if it came forward anyway.
    pub fn launch_kakaotalk_in_background() -> Result<()> {
        let previous = frontmost_pid();
        run_bounded(
            Command::new("open").args(["-g", "-b", KAKAOTALK_BUNDLE_ID]),
            OPEN_CHAT_TIMEOUT,
            "launching KakaoTalk in the background",
        )?;
        let deadline = Instant::now() + Duration::from_secs(15);
        let pid = loop {
            if let Ok(pid) = find_kakaotalk_pid() {
                break pid;
            }
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk did not start within 15s");
            }
            sleep(Duration::from_millis(250));
        };
        let mut focus = FocusGuard::new(pid);
        focus.previous = previous;
        focus.armed = true;
        sleep(Duration::from_secs(1));
        focus.restore();
        Ok(())
    }

    fn check(
        name: &'static str,
        status: super::AxCheckStatus,
        detail: impl Into<String>,
        code: Option<ErrorCode>,
    ) -> super::AxCheck {
        super::AxCheck {
            name,
            status,
            detail: detail.into(),
            code,
        }
    }

    fn failed(name: &'static str, error: &anyhow::Error) -> super::AxCheck {
        check(
            name,
            super::AxCheckStatus::Fail,
            format!("{error:#}"),
            error_code(error),
        )
    }

    /// Read-only readiness checks for `doctor`/`init`: permission, the signed
    /// KakaoTalk process, sign-in, the main window on this Space, the chat
    /// list and its search field, and (with `chat`) what a send to that open
    /// chat window would use. Nothing is pressed, opened or raised.
    pub fn diagnose(chat: Option<&str>) -> Vec<super::AxCheck> {
        use super::AxCheckStatus::{Fail, Ok as Pass, Warn};
        let mut checks = Vec::new();
        if !unsafe { AXIsProcessTrusted() } {
            checks.push(check(
                "Accessibility",
                Fail,
                permission_message(),
                Some(ErrorCode::AccessibilityNotGranted),
            ));
            return checks;
        }
        checks.push(check(
            "Accessibility",
            Pass,
            "granted to this process",
            None,
        ));
        checks.push(match super::kakaotalk_spaces_assignment() {
            Some(true) => check("Spaces", Pass, "KakaoTalk is assigned to All Desktops", None),
            _ => check(
                "Spaces",
                Warn,
                "KakaoTalk is not assigned to All Desktops; its windows on other Spaces are invisible to Accessibility",
                Some(ErrorCode::NotOnAllSpaces),
            ),
        });

        let pid = match find_kakaotalk_pid() {
            Ok(pid) => pid,
            Err(error) => {
                checks.push(failed("KakaoTalk AX target", &error));
                return checks;
            }
        };
        let app = match bounded_application(pid) {
            Ok(app) => app,
            Err(error) => {
                checks.push(failed("KakaoTalk AX target", &error));
                return checks;
            }
        };
        checks.push(check(
            "KakaoTalk AX target",
            Pass,
            "one running process with Kakao's bundle/team signature",
            None,
        ));
        let menus = menu_windows(pid).len();
        if menus > 0 {
            checks.push(check(
                "Open KakaoTalk menus",
                Warn,
                format!(
                    "{menus} KakaoTalk menu window(s) are on screen; close them by clicking elsewhere (never Esc, which closes KakaoTalk windows)"
                ),
                None,
            ));
        }

        match find_main_window(&app) {
            Err(error) if error_code(&error) == Some(ErrorCode::KakaotalkSignedOut) => {
                checks.push(failed("KakaoTalk sign-in", &error));
                return checks;
            }
            Err(error) => {
                checks.push(check("KakaoTalk sign-in", Pass, "signed in", None));
                let mut main = failed("Main window (this Space)", &error);
                let visible = app.windows().map(|windows| windows.len()).unwrap_or(0);
                if let Some((total, onscreen)) = cg_window_counts(pid) {
                    main.detail.push_str(&format!(
                        "; CoreGraphics sees {total} KakaoTalk window(s), {onscreen} on screen, {visible} visible to Accessibility on this Space"
                    ));
                }
                checks.push(main);
            }
            Ok(main_window) => {
                checks.push(check("KakaoTalk sign-in", Pass, "signed in", None));
                checks.push(check(
                    "Main window (this Space)",
                    Pass,
                    "visible and not minimized",
                    None,
                ));
                match snapshot_shallow(&main_window) {
                    Err(error) => checks.push(failed("Chat list", &error)),
                    Ok(snap) => {
                        match snap.find_first("AXTable") {
                            Some(table) => checks.push(check(
                                "Chat list",
                                Pass,
                                format!(
                                    "{} rows loaded (only loaded rows can be opened by name)",
                                    table.element.children().map(|rows| rows.len()).unwrap_or(0)
                                ),
                                None,
                            )),
                            None => checks.push(check(
                                "Chat list",
                                Warn,
                                "the main window is not on the Chats tab",
                                Some(ErrorCode::ChatListUnavailable),
                            )),
                        }
                        checks.push(match search_field_term(&snap) {
                            Some(term) => check(
                                "Chat-list search",
                                Warn,
                                format!("filtered by '{term}'; only matching rows are listed"),
                                Some(ErrorCode::ChatListFiltered),
                            ),
                            None => check("Chat-list search", Pass, "empty", None),
                        });
                    }
                }
            }
        }

        let Some(chat) = chat else {
            return checks;
        };
        let window = match find_chat_window(&app, chat) {
            Ok(Some(window)) => window,
            Ok(None) => {
                checks.push(check(
                    "Chat window",
                    Warn,
                    format!("no open window is titled exactly '{chat}'"),
                    Some(ErrorCode::ChatWindowNotOpen),
                ));
                return checks;
            }
            Err(error) => {
                checks.push(failed("Chat window", &error));
                return checks;
            }
        };
        checks.push(check(
            "Chat window",
            Pass,
            format!("open, titled exactly '{chat}'"),
            None,
        ));
        let elements = match inspect_chat_window(&window) {
            Ok(Some(elements)) => elements,
            Ok(None) => {
                checks.push(check(
                    "Composer",
                    Fail,
                    "no readable composer and message list in this window",
                    None,
                ));
                return checks;
            }
            Err(error) => {
                checks.push(failed("Composer", &error));
                return checks;
            }
        };
        checks.push(
            match attr_as_string(&elements.input_field, "AXValue").as_deref() {
                Some("") => check("Composer", Pass, "found and empty", None),
                Some(draft) => check(
                    "Composer",
                    Warn,
                    format!(
                        "holds a {}-character draft ({}); sends refuse to touch it unless --discard-draft",
                        draft.chars().count(),
                        serde_json::to_string(&crate::util::escape_terminal_text(
                            &crate::util::truncate(draft, 20)
                        ))
                        .unwrap_or_default()
                    ),
                    None,
                ),
                None => check("Composer", Fail, "found but unreadable", None),
            },
        );
        checks.push(match &elements.send_button {
            Some(button) => check(
                "Send button (전송)",
                Pass,
                format!(
                    "found (enabled={:?}); text sends are focus-free",
                    attr_as_bool(button, "AXEnabled")
                ),
                None,
            ),
            None => check(
                "Send button (전송)",
                Warn,
                "not exposed; text sends need --fallback-keys",
                Some(ErrorCode::SendButtonMissing),
            ),
        });
        checks.push(match button_with_shortcut(&window, "⌘O") {
            Ok(Some(_)) => check("File button (⌘O)", Pass, "found", None),
            Ok(None) => check(
                "File button (⌘O)",
                Warn,
                "not found; local-send-file/photo will refuse",
                None,
            ),
            Err(error) => failed("File button (⌘O)", &error),
        });
        checks.push(match read_recent_messages(&window, 1) {
            Ok(messages) => check(
                "Message list",
                Pass,
                format!("readable ({} recent bubble(s) found)", messages.len()),
                None,
            ),
            Err(error) => failed("Message list", &error),
        });
        checks
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn staging_links_without_overwriting_and_cleans_up_only_its_own_entry() {
            let source_dir = tempfile::tempdir().unwrap();
            let downloads = tempfile::tempdir().unwrap();
            let source = source_dir.path().join("보고서.docx");
            std::fs::write(&source, b"original").unwrap();

            let staged = StagedFile::create(&source, downloads.path(), "보고서.docx").unwrap();
            let staged_path = downloads.path().join("보고서.docx");
            assert_eq!(std::fs::read(&staged_path).unwrap(), b"original");
            // A second staging of the same name is a conflict, never an overwrite.
            let conflict = StagedFile::create(&source, downloads.path(), "보고서.docx")
                .err()
                .expect("same name must conflict");
            assert_eq!(error_code(&conflict), Some(ErrorCode::StagingConflict));
            drop(staged);
            assert!(!staged_path.exists());
            assert_eq!(std::fs::read(&source).unwrap(), b"original");

            // Kept on an uncertain send.
            let mut kept = StagedFile::create(&source, downloads.path(), "보고서.docx").unwrap();
            kept.keep = true;
            drop(kept);
            assert!(staged_path.exists());
            std::fs::remove_file(&staged_path).unwrap();

            // If another file took the name meanwhile, it is left alone.
            let replaced = StagedFile::create(&source, downloads.path(), "보고서.docx").unwrap();
            std::fs::remove_file(&staged_path).unwrap();
            std::fs::write(&staged_path, b"someone else's").unwrap();
            drop(replaced);
            assert_eq!(std::fs::read(&staged_path).unwrap(), b"someone else's");
        }

        #[test]
        fn nfc_composes_decomposed_hangul() {
            let decomposed = "\u{1107}\u{1169}\u{1100}\u{1169}\u{1109}\u{1165}.docx";
            assert_eq!(nfc(decomposed), "보고서.docx");
            assert_eq!(nfc("plain.txt"), "plain.txt");
        }

        #[test]
        fn open_chat_timeout_is_bounded() {
            assert!(OPEN_CHAT_TIMEOUT.as_secs() > 0);
        }

        #[test]
        fn verify_poll_interval_is_smaller_than_timeout() {
            assert!(VERIFY_POLL_INTERVAL < VERIFY_TIMEOUT);
        }

        #[test]
        fn return_keycode_matches_macos_carbon_constant() {
            // kVK_Return from Carbon HIToolbox/Events.h — used throughout macOS
            // AX/CGEvent automation tools (also what kakaocli's AXHelpers uses).
            assert_eq!(RETURN_KEYCODE, 36);
        }
    }
} // mod imp

#[cfg(target_os = "macos")]
pub use imp::{
    assign_kakaotalk_to_all_spaces, delete_via_ax, diagnose, edit_via_ax, export_via_ax,
    friend_via_ax, launch_kakaotalk_in_background, login_via_ax, mark_read_via_ax,
    open_friend_chat_via_ax, react_via_ax, read_via_ax, rename_chat_via_ax, reply_via_ax,
    scrape_chat_list, search_friends, send_attachment_via_ax, send_via_ax_classified, BubbleAction,
    ChatListRow,
};

#[cfg(not(target_os = "macos"))]
mod stub {
    use anyhow::{anyhow, Result};

    pub fn send_via_ax_classified(
        _chat_display_name: &str,
        _message: &str,
    ) -> Result<super::AxDeliveryOutcome> {
        Err(anyhow!(
            "local-send (AX automation) is only supported on macOS"
        ))
    }

    pub fn send_attachment_via_ax(
        _chat_display_name: &str,
        _path: &std::path::Path,
        _kind: super::AttachmentKind,
    ) -> Result<super::AttachmentReceipt> {
        Err(anyhow!(
            "local-send-file / local-send-photo (AX automation) are only supported on macOS"
        ))
    }

    /// Mirrors `imp::BubbleAction`; never constructed off macOS.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct BubbleAction {
        pub target_text: String,
        pub target_outgoing: bool,
    }

    pub fn react_via_ax(
        _chat_display_name: &str,
        _target: &super::BubbleTarget,
        _reaction: &str,
        _dry_run: bool,
    ) -> Result<BubbleAction> {
        Err(anyhow!(
            "local-react (AX automation) is only supported on macOS"
        ))
    }

    pub fn reply_via_ax(
        _chat_display_name: &str,
        _target: &super::BubbleTarget,
        _message: &str,
        _dry_run: bool,
    ) -> Result<BubbleAction> {
        Err(anyhow!(
            "local-reply (AX automation) is only supported on macOS"
        ))
    }

    pub fn edit_via_ax(
        _chat_display_name: &str,
        _old: &str,
        _new: &str,
        _dry_run: bool,
    ) -> Result<BubbleAction> {
        Err(anyhow!(
            "local-edit (AX automation) is only supported on macOS"
        ))
    }

    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct FriendRow {
        pub name: String,
        pub status: Option<String>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct FriendInfo {
        pub name: String,
        pub nickname: String,
        pub own_name: Option<String>,
        pub memo: String,
    }

    pub fn open_friend_chat_via_ax(_name: &str) -> Result<()> {
        Err(anyhow!(
            "ax-friend (AX automation) is only supported on macOS"
        ))
    }

    pub fn search_friends(_query: &str) -> Result<Vec<FriendRow>> {
        Err(anyhow!(
            "ax-friends (AX automation) is only supported on macOS"
        ))
    }

    pub fn friend_via_ax(
        _name: &str,
        _memo: Option<&str>,
        _rename: Option<&str>,
    ) -> Result<FriendInfo> {
        Err(anyhow!(
            "ax-friend (AX automation) is only supported on macOS"
        ))
    }

    pub fn rename_chat_via_ax(_chat_display_name: &str, _new_name: &str) -> Result<()> {
        Err(anyhow!(
            "local-rename-chat (AX automation) is only supported on macOS"
        ))
    }

    pub fn mark_read_via_ax(_chat_display_name: &str) -> Result<u32> {
        Err(anyhow!(
            "local-mark-read (AX automation) is only supported on macOS"
        ))
    }

    pub fn export_via_ax(_chat_display_name: &str) -> Result<std::path::PathBuf> {
        Err(anyhow!(
            "ax-export (AX automation) is only supported on macOS"
        ))
    }

    pub fn delete_via_ax(
        _chat_display_name: &str,
        _text: &str,
        _scope: super::DeleteScope,
        _dry_run: bool,
    ) -> Result<BubbleAction> {
        Err(anyhow!(
            "local-delete (AX automation) is only supported on macOS"
        ))
    }

    pub fn assign_kakaotalk_to_all_spaces() -> Result<()> {
        Err(anyhow!("Spaces assignment is only supported on macOS"))
    }

    pub fn launch_kakaotalk_in_background() -> Result<()> {
        Err(anyhow!("KakaoTalk for macOS can only be launched on macOS"))
    }

    pub fn login_via_ax(_account: &str, _password: &str) -> Result<bool> {
        Err(anyhow!("ax-login is only supported on macOS"))
    }

    pub fn diagnose(_chat: Option<&str>) -> Vec<super::AxCheck> {
        vec![super::AxCheck {
            name: "AX automation",
            status: super::AxCheckStatus::Warn,
            detail: "local-send, ax-read and ax-watch are only supported on macOS".to_string(),
            code: None,
        }]
    }

    pub fn read_via_ax(
        _chat_display_name: &str,
        _count: usize,
        _expand: bool,
    ) -> Result<super::ChatView> {
        Err(anyhow!(
            "ax-read (AX automation) is only supported on macOS"
        ))
    }

    /// Mirrors `imp::ChatListRow`. Never constructed off macOS (the fn below
    /// always errors), so its fields would otherwise trip `dead_code`.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct ChatListRow {
        pub name: String,
        pub unread: i32,
        pub members: Option<u32>,
        pub preview: String,
        pub timestamp: String,
    }

    pub fn scrape_chat_list() -> Result<Vec<ChatListRow>> {
        Err(anyhow!(
            "ax-watch (AX automation) is only supported on macOS"
        ))
    }
}

#[cfg(not(target_os = "macos"))]
pub use stub::{
    assign_kakaotalk_to_all_spaces, delete_via_ax, diagnose, edit_via_ax, export_via_ax,
    friend_via_ax, launch_kakaotalk_in_background, login_via_ax, mark_read_via_ax,
    open_friend_chat_via_ax, react_via_ax, read_via_ax, rename_chat_via_ax, reply_via_ax,
    scrape_chat_list, search_friends, send_attachment_via_ax, send_via_ax_classified, BubbleAction,
    ChatListRow,
};

/// Turn a classified outcome into the caller-facing result. `Uncertain` must
/// surface as a hard error — never `Ok` — so the CLI exits non-zero and
/// automation does not blindly retry a send whose commit state is unknown.
fn outcome_to_result(outcome: AxDeliveryOutcome) -> anyhow::Result<()> {
    match outcome {
        AxDeliveryOutcome::Verified => Ok(()),
        AxDeliveryOutcome::Uncertain { reason } => Err(crate::error::coded(
            crate::error::ErrorCode::DeliveryUncertain,
            reason,
        )),
    }
}

pub fn send_via_ax(chat_display_name: &str, message: &str) -> anyhow::Result<()> {
    outcome_to_result(send_via_ax_classified(chat_display_name, message)?)
}
