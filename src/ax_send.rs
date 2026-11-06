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

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn has_new_exact_outgoing_message(
    before: &[DeliveryObservation],
    after: &[DeliveryObservation],
    target: &str,
) -> bool {
    let matches_target =
        |observation: &&DeliveryObservation| observation.outgoing && observation.text == target;
    let before_count = before.iter().filter(matches_target).count();
    let after_count = after.iter().filter(matches_target).count();
    after_count > before_count
}

/// A cleared composer plus a new outgoing exact-text bubble. With the
/// baseline's newest row found again (`appended_start`, see
/// [`appended_rows_start`]), one such bubble among the rows appended after it
/// is enough, so an older identical bubble that scrolls out of the observed
/// tail cannot turn a real send into `delivery_uncertain`. Without the anchor
/// the counting rule applies.
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
            Some(start) => after
                .get(start..)
                .unwrap_or_default()
                .iter()
                .any(|row| row.outgoing && row.text == target),
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
/// verify a send. Without that anchor, the counting rule applies only if the
/// table actually gained rows.
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
}

pub(crate) const DEFAULT_WAIT_IDLE: std::time::Duration = std::time::Duration::from_secs(2);

impl Default for AxPolicy {
    fn default() -> Self {
        Self {
            wait_idle: DEFAULT_WAIT_IDLE,
            open_if_needed: false,
            fallback_keys: false,
            pick: None,
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
                .filter(|&index| bubbles[index].0 == *text)
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
            .filter(|(body, outgoing, _)| *outgoing && body == text)
            .count()
    };
    let edited_now = after
        .iter()
        .any(|(body, outgoing, edited)| *outgoing && *edited && body == new);
    edited_now && outgoing_with(after, old) + 1 == outgoing_with(before, old)
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
        ] {
            assert!(is_clock_label(label), "{label}");
        }
        for text in [
            "openkakao-시험.txt",
            "용량: 42.0bytes",
            "유효기간: ~2026. 11. 20.",
            "2026. 11. 6.",
            "1:2",
            "123:45",
            "",
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
        // the display was asleep but the user session remained unlocked.
        sleep(Duration::from_millis(300));
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
        let rows = chat_list_rows(&table.element)?;

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
                    // Safety-relevant, so always shown, not only under debug.
                    eprintln!(
                        "--pick {}: opening '{}' ({})",
                        super::policy().pick.unwrap_or_default(),
                        crate::util::escape_terminal_text(chat_display_name),
                        row_summary(&rows[index].0)
                    );
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
    fn menu_in_window((x, y, width, _height): (f64, f64, f64, f64)) -> Option<AXUIElement> {
        let system = unsafe { AXUIElement::wrap_under_create_rule(AXUIElementCreateSystemWide()) };
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
        if let Err(error) = element.perform_action(&CFString::new("AXShowMenu")) {
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
            if let Some(menu) = pid
                .into_iter()
                .flat_map(menu_windows)
                .find_map(menu_in_window)
            {
                return Ok(Some(menu));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            sleep(Duration::from_millis(100));
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
                    .is_some_and(|title| OPEN_CHAT_MENU_TITLES.contains(&title.trim()))
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
                    "the chat-list row's context menu has no unique 채팅방 열기 item (items: {:?}){stray}",
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
                "the mouse cursor is over the chat-list row's context menu; 채팅방 열기 was not pressed",
            ));
        }
        let pressed = item
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not press 채팅방 열기: {error:?}"));
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
    fn find_input_field(snap: &AxNode) -> Option<AXUIElement> {
        let mut scroll_areas = Vec::new();
        snap.find_all("AXScrollArea", &mut scroll_areas);

        for area in scroll_areas {
            if area.find_first("AXTable").is_some() {
                continue; // this scroll area is the message list, not the composer
            }
            if let Some(field) = area.find_first("AXTextArea") {
                return Some(field.element.clone());
            }
        }
        None
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
    fn require_send_approval(chat_display_name: &str, action: &str) -> Result<()> {
        if !super::needs_device_owner_auth(chat_display_name) {
            return Ok(());
        }
        let target =
            crate::util::escape_terminal_text(&crate::util::truncate(chat_display_name, 80));
        openkakao_cli::human_auth::require_device_owner_auth(&format!(
            "Approve one KakaoTalk {action} ‘{target}’, which is NOT in allowed_send_chats, after reviewing the terminal preview"
        ))
        .map_err(|error| {
            coded(
                ErrorCode::ChatNotAllowed,
                format!("'{chat_display_name}' is not in allowed_send_chats and {error:#}"),
            )
        })
    }

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

    /// One message bubble scraped from a chat window's AX message list.
    #[derive(Debug, Clone)]
    pub struct AxMessage {
        /// The time label's `AXHelp` text (full date, e.g. "2026. 6. 17.") if
        /// present, else its plain displayed value (e.g. "14:32").
        pub time: Option<String>,
        pub text: String,
        pub outgoing: bool,
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
        Ok(rows
            .iter()
            .filter_map(|row| message_from_row(row, center))
            .collect())
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
        match (
            bubble.role.as_str(),
            time_label(row).and_then(AxNode::center_x),
        ) {
            ("AXTextArea", _) => bubble_center.is_some_and(|x| x > table_center),
            (_, Some(time_center)) => time_center < table_center,
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
        let time = time_label(row)
            .or_else(|| {
                (bubble.role == "AXTextArea")
                    .then(|| row.find_first("AXStaticText"))
                    .flatten()
            })
            .and_then(|t| t.help.clone().or_else(|| t.value.clone()));
        Some(AxMessage {
            time,
            text,
            outgoing,
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
            let before_names: Option<Vec<Option<String>>> = match initial_list {
                Some(list) => Some(
                    rows_with_label(&list, &["AXTextField"], "open panel file list (initial)")?
                        .into_iter()
                        .map(|(_, name)| name.as_deref().map(nfc))
                        .collect(),
                ),
                None => None,
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
            anyhow::bail!(
                "the exact target's composer already contains a draft; refusing to disturb it"
            );
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
            require_send_approval(chat_display_name, &format!("{noun} to"))?;
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
                                trace(started, &format!("delivery verified by {verified_by}"));
                                // KakaoTalk reads the staged copy while it uploads;
                                // remove it only once the bubble stops saying so.
                                if let Some(staged) = staged.as_mut() {
                                    if !wait_for_upload_to_finish(&elements.message_table) {
                                        staged.keep = true;
                                        eprintln!(
                                            "KakaoTalk is still uploading after {}s; the staged copy at {} was kept, delete it once the upload finishes",
                                            UPLOAD_FINISH_TIMEOUT.as_secs(),
                                            staged.path.display()
                                        );
                                    }
                                    trace(started, "upload finished (staged copy may go)");
                                }
                                return Ok(super::AttachmentReceipt {
                                    verified_by,
                                    preview_shows_filename,
                                    staged: staged.is_some(),
                                });
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

    /// `true` once no row in the message tail shows an upload in progress,
    /// `false` after [`UPLOAD_FINISH_TIMEOUT`] or when the tail cannot be read.
    fn wait_for_upload_to_finish(table: &AXUIElement) -> bool {
        let deadline = Instant::now() + UPLOAD_FINISH_TIMEOUT;
        loop {
            let mut seen = 0;
            let uploading = tail_rows(table, |_| {
                seen += 1;
                seen >= DELIVERY_TAIL_ROWS
            })
            .map(|(rows, _)| rows.iter().any(|row| uploading_label(row).is_some()));
            match uploading {
                Ok(false) => return true,
                Ok(true) if Instant::now() < deadline => sleep(Duration::from_secs(1)),
                _ => return false,
            }
        }
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
    pub fn read_via_ax(chat_display_name: &str, count: usize) -> Result<Vec<AxMessage>> {
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
            let messages = read_recent_messages(&chat.window, count)?;
            // A window that was already open is read once; a just-opened one
            // may need a moment to render its rows.
            if chat.was_open || !messages.is_empty() {
                trace(started, "messages read");
                return Ok(messages);
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
        let (_window, elements) = wait_for_chat_elements(&app, chat_display_name)?;
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
        if !existing_draft.is_empty() {
            anyhow::bail!(
                "the exact target's composer already contains a draft; refusing to overwrite or append to it"
            );
        }

        let baseline = delivery_observations(&message_table)?;
        trace(started, "delivery baseline read");

        // Keep the OS-mediated human-presence boundary inside the one native
        // transport implementation. This makes it impossible for a future
        // command caller to reach a real AX send merely by forgetting its own
        // prompt. At this point the signed process, exact window, empty
        // composer, and pre-send bubble baseline are all known; authentication
        // happens immediately before the composer is changed.
        focus.watch();
        require_send_approval(chat_display_name, "message to")?;

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
            String::new()
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
                ErrorCode::MenuNotFound,
                format!(
                    "the message's context menu has no unique '{item}' item (items: {:?})",
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
        let bubble = bubbles.swap_remove(index);
        Ok((app, pid, focus, window, elements, bubble))
    }

    /// Reply to one bubble without raising KakaoTalk (verified on 26.9.0):
    /// `AXShowMenu` on it, `AXPress` `답장`, require reply mode (composer
    /// `답장 메시지 입력` and a `…에게 답장` banner) **before inserting any
    /// text**, then the `AXSelectedText` + `전송` commit and the usual
    /// exact-text verification. Experimental.
    pub fn reply_via_ax(
        chat_display_name: &str,
        target: &super::BubbleTarget,
        message: &str,
        dry_run: bool,
    ) -> Result<BubbleAction> {
        let (app, _pid, mut focus, window, elements, bubble) =
            bubble_target(chat_display_name, target, false)?;
        let action = BubbleAction {
            target_text: bubble.text.clone(),
            target_outgoing: bubble.outgoing,
        };
        if dry_run {
            return Ok(action);
        }
        focus.watch();
        require_send_approval(chat_display_name, "reply in")?;
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
        let target = super::BubbleTarget::Text(old.to_string());
        let (app, _pid, mut focus, window, elements, bubble) =
            bubble_target(chat_display_name, &target, true)?;
        let action = BubbleAction {
            target_text: bubble.text.clone(),
            target_outgoing: bubble.outgoing,
        };
        if dry_run {
            return Ok(action);
        }
        focus.watch();
        require_send_approval(chat_display_name, "message edit in")?;
        let rows = |bubbles: &[Bubble]| -> Vec<(String, bool, bool)> {
            bubbles
                .iter()
                .map(|bubble| (bubble.text.clone(), bubble.outgoing, bubble.edited))
                .collect()
        };
        let before = rows(&text_bubbles(&elements.message_table)?);
        focus.watch();
        press_bubble_menu_item(&app, &bubble.text_area, "수정")?;
        let prepared = (|| {
            let state = wait_for_mode(&window, super::ComposerMode::Edit)?;
            if !wait_for_composer_value(&state.composer, old) {
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
        let press_error = commit.perform_action(&CFString::new(kAXPressAction)).err();
        focus.restore();
        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            let cleared = attr_as_string(&state.composer, "AXValue").as_deref() == Some("");
            if let Ok(after) = text_bubbles(&elements.message_table) {
                if cleared && super::edit_verified(&before, &rows(&after), old, new) {
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

    /// One chat-list row scraped from the main window, read-only (never opens
    /// the chat, so its unread state is untouched).
    #[derive(Debug, Clone)]
    pub struct ChatListRow {
        pub name: String,
        pub unread: i32,
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
        let name = static_texts.first().and_then(|t| t.value.clone())?;
        // Among the remaining static texts, the unread badge is the one
        // whose whole value parses as an integer (e.g. "5"); a
        // non-numeric one (e.g. "어제", "오후 3:14") is the timestamp.
        let mut unread = 0;
        let mut timestamp = String::new();
        for t in static_texts.iter().skip(1) {
            let Some(v) = t.value.as_deref() else {
                continue;
            };
            if let Ok(n) = v.trim().parse::<i32>() {
                if unread == 0 {
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
                Some(_) => check(
                    "Composer",
                    Warn,
                    "holds a draft; sends refuse to touch it",
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
    assign_kakaotalk_to_all_spaces, diagnose, edit_via_ax, launch_kakaotalk_in_background,
    read_via_ax, reply_via_ax, scrape_chat_list, send_attachment_via_ax, send_via_ax_classified,
    BubbleAction, ChatListRow,
};

#[cfg(not(target_os = "macos"))]
mod stub {
    use anyhow::{anyhow, Result};

    /// Mirrors `imp::AxMessage`'s shape so callers don't need cfg-gating.
    /// Never actually constructed here — `read_via_ax` below always errors
    /// on this platform — so its fields would otherwise trip `dead_code`.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct AxMessage {
        pub time: Option<String>,
        pub text: String,
        pub outgoing: bool,
    }

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

    pub fn assign_kakaotalk_to_all_spaces() -> Result<()> {
        Err(anyhow!("Spaces assignment is only supported on macOS"))
    }

    pub fn launch_kakaotalk_in_background() -> Result<()> {
        Err(anyhow!("KakaoTalk for macOS can only be launched on macOS"))
    }

    pub fn diagnose(_chat: Option<&str>) -> Vec<super::AxCheck> {
        vec![super::AxCheck {
            name: "AX automation",
            status: super::AxCheckStatus::Warn,
            detail: "local-send, ax-read and ax-watch are only supported on macOS".to_string(),
            code: None,
        }]
    }

    pub fn read_via_ax(_chat_display_name: &str, _count: usize) -> Result<Vec<AxMessage>> {
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
    assign_kakaotalk_to_all_spaces, diagnose, edit_via_ax, launch_kakaotalk_in_background,
    read_via_ax, reply_via_ax, scrape_chat_list, send_attachment_via_ax, send_via_ax_classified,
    BubbleAction, ChatListRow,
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
