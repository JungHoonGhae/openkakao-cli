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

// Only `imp::open_chat_row` (macOS-only) actually calls these outside of
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

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn is_verified_delivery(
    before: &[DeliveryObservation],
    after: &[DeliveryObservation],
    target: &str,
    composer_value: Option<&str>,
) -> bool {
    composer_value == Some("") && has_new_exact_outgoing_message(before, after, target)
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
            "new message",
            Some("new message")
        ));
        assert!(is_verified_delivery(
            &before,
            &after,
            "new message",
            Some("")
        ));
        assert!(!is_verified_delivery(&before, &after, "new message", None));
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
        AXUIElementCopyMultipleAttributeValues, AXUIElementRef, AXValueGetTypeID, AXValueGetValue,
        AXValueRef,
    };
    use anyhow::{anyhow, Context, Result};
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFEqual, CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    use core_graphics::event::CGEvent;
    use core_graphics::event::{
        CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, KeyCode,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::{CGPoint, CGSize};

    // Declared on the cross-platform module (see there) so their bounds and
    // exact-match text stay testable wherever the crate's CI runs.
    use super::{FILE_PICKER_READY_TIMEOUT, OPEN_CHAT_TIMEOUT, PHOTO_BUBBLE_PLACEHOLDER};

    const KAKAOTALK_BUNDLE_ID: &str = "com.kakao.KakaoTalkMac";
    const KAKAOTALK_TEAM_ID: &str = "L75WVXX68A";
    const RETURN_KEYCODE: u16 = 36;
    // Scoped delivery verification: a single already-open window's bubbles show
    // the sent text near-instantly, so this can be short (unlike the old
    // app-wide scan). Normally succeeds on the first poll.
    const VERIFY_TIMEOUT: Duration = Duration::from_secs(3);
    const VERIFY_POLL_INTERVAL: Duration = Duration::from_millis(200);
    const COMPOSER_VERIFY_TIMEOUT: Duration = Duration::from_secs(1);
    const COMPOSER_VERIFY_POLL_INTERVAL: Duration = Duration::from_millis(50);
    const SNAPSHOT_MAX_DEPTH: usize = 128;
    const SNAPSHOT_MAX_NODES: usize = 20_000;
    const SNAPSHOT_MAX_DURATION: Duration = Duration::from_secs(5);
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
            anyhow::bail!(
                "KakaoTalk is not running (or `{KAKAOTALK_BUNDLE_ID}` not found) — open it and log in first"
            );
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
            Err(anyhow!(
                "Accessibility permission is not granted to this terminal app.\n\
                 Open System Settings → Privacy & Security → Accessibility,\n\
                 and enable it for your terminal (Terminal.app, iTerm2, etc.),\n\
                 then re-run this command."
            ))
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

    fn focus_window_and_verify(app: &AXUIElement, window: &AXUIElement) -> Result<()> {
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

    /// HID events are delivered at global screen coordinates, so a
    /// coordinate-based click on a row is only safe while KakaoTalk is the
    /// frontmost application and its main chat-list window — the window whose
    /// row coordinates were read — is the focused window. Unlike
    /// `focus_window_and_verify`, this never raises or activates anything:
    /// when the check fails, the caller refuses to click rather than deliver
    /// the click to whatever unrelated window actually covers that point.
    fn verify_main_window_frontmost(app: &AXUIElement, main_window: &AXUIElement) -> Result<()> {
        let frontmost = attr_as_bool(app, "AXFrontmost") == Some(true);
        let focused_is_main =
            attr_as_element(app, "AXFocusedWindow").is_some_and(|focused| unsafe {
                CFEqual(focused.as_CFTypeRef(), main_window.as_CFTypeRef()) != 0
            });
        if !frontmost || !focused_is_main {
            anyhow::bail!(
                "KakaoTalk is not the frontmost app with its main chat-list window focused, so a \
                 coordinate-based chat-row click could land in another window. Bring KakaoTalk's \
                 main window to the front yourself (this check never raises windows itself) or \
                 open the chat by hand, then retry."
            );
        }
        Ok(())
    }

    /// Find KakaoTalk's main chat-list window, as opposed to any individual
    /// open-chat windows (which are separate `AXWindow`s titled with the
    /// other party's — or your own, for the self chat — display name).
    fn find_main_window(app: &AXUIElement) -> Result<AXUIElement> {
        let windows = app
            .windows()
            .map_err(|e| anyhow!("AXWindows read failed: {e:?}"))?;
        let window = windows
            .iter()
            .find(|w| attr_as_string(w, "AXIdentifier").as_deref() == Some("Main Window"))
            .map(|w| w.clone())
            .ok_or_else(|| {
                anyhow!(
                    "could not find KakaoTalk's main chat-list window. Make sure it's open, not \
                     minimized, and on the Space (virtual desktop) you're currently viewing — the \
                     Accessibility API only sees windows that are visible on the active Space, \
                     and restoring a minimized/off-Space window automatically risks stealing \
                     your foreground focus, which this tool does not do for you. One-time fix \
                     if this keeps happening: right-click the KakaoTalk Dock icon → Options → \
                     Assign To → All Desktops."
                )
            })?;

        // Note: a minimized window still shows up here (unlike one on another
        // Space, which disappears from `windows()` entirely), but restoring
        // it via AXMinimized=false was observed to sometimes bring KakaoTalk
        // to the foreground — which this tool must never do — so we
        // deliberately do NOT auto-restore. The caller gets the same "not
        // found" error and a manual fix, same as the off-Space case.
        let minimized_attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXMinimized"));
        let is_minimized = window
            .attribute(&minimized_attr)
            .ok()
            .and_then(|v| v.downcast::<CFBoolean>())
            .map(bool::from)
            == Some(true);
        if is_minimized {
            return Err(anyhow!(
                "KakaoTalk's main chat-list window is minimized. Restoring it automatically risks \
                 stealing your foreground focus, which this tool does not do for you — please \
                 un-minimize it yourself (click its Dock icon) and retry."
            ));
        }

        Ok(window)
    }

    /// A single recursive snapshot of an AX subtree, capturing each node's
    /// role/value/help/description once so later lookups (`find_first`,
    /// `find_all`) run entirely in memory instead of re-walking the tree via
    /// AX's cross-process IPC on every call. Building this snapshot costs
    /// roughly the same as one `find_descendants_by_role` call; the win is
    /// not calling `find_descendants_by_role` dozens of times against
    /// overlapping subtrees, which is what made `open_chat_row` take ~9s
    /// against an 84-row chat list before this change.
    struct AxNode {
        element: AXUIElement,
        role: String,
        value: Option<String>,
        help: Option<String>,
        description: Option<String>,
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
        exhausted: bool,
    }

    /// Build an `AxNode` tree rooted at `root` with one recursive walk,
    /// fetching every node's role, children, value, help, and description in
    /// a **single** `AXUIElementCopyMultipleAttributeValues` IPC round-trip
    /// instead of 2–5 separate `AXUIElementCopyAttributeValue` calls. Each AX
    /// call is a cross-process round-trip to KakaoTalk, so on its ~700-node
    /// main window collapsing five calls into one roughly halves the wall
    /// time of the walk (measured ~4.3ms/node with the old per-attribute
    /// approach). Attributes a node doesn't carry come back as error
    /// placeholders in the same call (`options = 0`, i.e. don't stop on the
    /// first missing one), so they cost no extra round-trip and simply fail
    /// the `downcast` to `None`.
    fn snapshot(root: &AXUIElement) -> Result<AxNode> {
        let mut budget = SnapshotBudget {
            started_at: Instant::now(),
            nodes: 0,
            visited: HashSet::new(),
            exhausted: false,
        };
        let node = snapshot_bounded(root, 0, &mut budget);
        if budget.exhausted {
            anyhow::bail!(
                "KakaoTalk AX snapshot exceeded its depth/node/time safety budget; refusing incomplete UI data"
            );
        }
        Ok(node)
    }

    fn snapshot_bounded(root: &AXUIElement, depth: usize, budget: &mut SnapshotBudget) -> AxNode {
        let identity = root.as_concrete_TypeRef() as usize;
        if !budget.visited.insert(identity) {
            return AxNode {
                element: root.clone(),
                role: String::new(),
                value: None,
                help: None,
                description: None,
                position_x: None,
                position_y: None,
                width: None,
                height: None,
                children: Vec::new(),
            };
        }
        if depth > SNAPSHOT_MAX_DEPTH
            || budget.nodes >= SNAPSHOT_MAX_NODES
            || budget.started_at.elapsed() >= SNAPSHOT_MAX_DURATION
        {
            budget.exhausted = true;
            return AxNode {
                element: root.clone(),
                role: String::new(),
                value: None,
                help: None,
                description: None,
                position_x: None,
                position_y: None,
                width: None,
                height: None,
                children: Vec::new(),
            };
        }
        budget.nodes += 1;
        // Order matters: these indices are read back positionally below.
        let names = CFArray::from_CFTypes(&[
            CFString::new("AXRole").as_CFType(),
            CFString::new("AXChildren").as_CFType(),
            CFString::new("AXValue").as_CFType(),
            CFString::new("AXHelp").as_CFType(),
            CFString::new("AXDescription").as_CFType(),
            CFString::new("AXPosition").as_CFType(),
            CFString::new("AXSize").as_CFType(),
        ]);

        let mut values_ref: CFArrayRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyMultipleAttributeValues(
                root.as_concrete_TypeRef(),
                names.as_concrete_TypeRef(),
                0, // don't stop on error — missing attrs return placeholders
                &mut values_ref,
            )
        };
        if budget.started_at.elapsed() >= SNAPSHOT_MAX_DURATION {
            budget.exhausted = true;
        }
        if err != 0 || values_ref.is_null() {
            // Rare: the batch call failed for this element. Fall back to a
            // leaf node carrying just the role via the slow per-attr path,
            // so one failed call doesn't drop the whole subtree.
            return AxNode {
                element: root.clone(),
                role: role(root),
                value: None,
                help: None,
                description: None,
                position_x: None,
                position_y: None,
                width: None,
                height: None,
                children: Vec::new(),
            };
        }
        let values = unsafe { CFArray::<CFType>::wrap_under_create_rule(values_ref) };

        let string_at = |i: isize| -> Option<String> {
            values
                .get(i)
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
        };

        let point_at = |i: isize| -> Option<CGPoint> {
            let value = values.get(i)?;
            if value.type_of() != unsafe { AXValueGetTypeID() } {
                return None;
            }
            let mut point = CGPoint::new(0.0, 0.0);
            if unsafe {
                AXValueGetValue(
                    value.as_CFTypeRef() as AXValueRef,
                    kAXValueTypeCGPoint,
                    (&mut point as *mut CGPoint).cast(),
                )
            } {
                Some(point)
            } else {
                None
            }
        };
        let size_at = |i: isize| -> Option<CGSize> {
            let value = values.get(i)?;
            if value.type_of() != unsafe { AXValueGetTypeID() } {
                return None;
            }
            let mut size = CGSize::new(0.0, 0.0);
            if unsafe {
                AXValueGetValue(
                    value.as_CFTypeRef() as AXValueRef,
                    kAXValueTypeCGSize,
                    (&mut size as *mut CGSize).cast(),
                )
            } {
                Some(size)
            } else {
                None
            }
        };

        // Slot 1 is the AXChildren array. `ConcreteCFType` is only implemented
        // for the untyped `CFArray<*const c_void>`, so downcast to that and
        // wrap each raw element ref as an `AXUIElement` under the get rule
        // (retain), the same +1 retain semantics the typed `.children()`
        // accessor gives. A node with no children yields an error placeholder
        // that fails the array downcast → empty Vec.
        let node_children = values
            .get(1)
            .and_then(|v| v.downcast::<CFArray<*const std::ffi::c_void>>())
            .map(|arr| {
                let mut children = Vec::new();
                for child_ref in arr.iter() {
                    if budget.exhausted {
                        break;
                    }
                    let child =
                        unsafe { AXUIElement::wrap_under_get_rule(*child_ref as AXUIElementRef) };
                    children.push(snapshot_bounded(&child, depth + 1, budget));
                }
                children
            })
            .unwrap_or_default();

        AxNode {
            element: root.clone(),
            role: string_at(0).unwrap_or_default(),
            value: string_at(2),
            help: string_at(3),
            description: string_at(4),
            position_x: point_at(5).map(|point| point.x),
            position_y: point_at(5).map(|point| point.y),
            width: size_at(6).map(|size| size.width),
            height: size_at(6).map(|size| size.height),
            children: node_children,
        }
    }

    impl AxNode {
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

    fn press_return(pid: i32) -> Result<()> {
        post_key_to_pid(pid, RETURN_KEYCODE, true)?;
        post_key_to_pid(pid, RETURN_KEYCODE, false)?;
        Ok(())
    }

    fn press_command_v(_pid: i32) -> Result<()> {
        let script = r#"tell application "System Events" to keystroke "v" using command down"#;
        let mut child = Command::new("osascript")
            .args(["-e", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to deliver the verified clipboard image to KakaoTalk")?;
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    anyhow::bail!(
                        "could not paste the verified clipboard image ({status}): {}",
                        stderr.trim()
                    );
                }
                Err(error) => return Err(error).context("failed while waiting for image paste"),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("verified clipboard image paste timed out");
                }
                Ok(None) => sleep(Duration::from_millis(50)),
            }
        }
    }

    fn copy_image_to_clipboard(path: &std::path::Path) -> Result<()> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "jpg" | "jpeg" | "png" => {}
            _ => anyhow::bail!(
                "native picker was unusable and clipboard fallback supports only JPEG or PNG"
            ),
        }
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
}
"#;
        let mut child = Command::new("osascript")
            .args(["-l", "JavaScript", "-e", script, "--"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to stage the verified photo on the macOS clipboard")?;
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    anyhow::bail!(
                        "could not stage the verified photo on the macOS clipboard ({status}): {}",
                        stderr.trim()
                    );
                }
                Err(error) => {
                    return Err(error).context("failed while waiting for macOS clipboard staging");
                }
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("macOS clipboard staging timed out");
                }
                Ok(None) => sleep(Duration::from_millis(50)),
            }
        }
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

    fn double_click(point: CGPoint) -> Result<()> {
        for click_state in [1_i64, 2_i64] {
            for event_type in [CGEventType::LeftMouseDown, CGEventType::LeftMouseUp] {
                let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                    .map_err(|_| anyhow!("failed to create CGEventSource"))?;
                let event =
                    CGEvent::new_mouse_event(source, event_type, point, CGMouseButton::Left)
                        .map_err(|_| anyhow!("failed to create mouse CGEvent"))?;
                event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, click_state);
                event.post(CGEventTapLocation::HID);
            }
            sleep(Duration::from_millis(80));
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
    /// active; the Friends tab renders an `AXOutline` instead. Left over from an
    /// earlier manual tab switch during development, this makes `open_chat_row`
    /// resilient to whatever tab the window happens to be on.
    ///
    /// Returns the AxNode snapshot to use afterward — either the one just
    /// taken (if the table was already visible) or a fresh one (if the tab
    /// was just pressed, since that changes the UI). Returning the snapshot
    /// instead of re-taking it in the caller avoids a second full tree walk
    /// in the common case (already on the right tab), which previously
    /// doubled open_chat_row's cost.
    fn ensure_chatrooms_tab(main_window: &AXUIElement) -> Result<AxNode> {
        let snap = snapshot(main_window)?;
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
            return snapshot(main_window);
        }
        Ok(snap)
    }

    fn open_chat_row(app: &AXUIElement, chat_display_name: &str) -> Result<()> {
        let debug = std::env::var("OPENKAKAO_CLI_DEBUG").is_ok();
        let start = Instant::now();

        let main_window = find_main_window(app)?;
        let snap = ensure_chatrooms_tab(&main_window)?;
        if debug {
            eprintln!(
                "[ax_send] open_chat_row: snapshot took {:?}",
                start.elapsed()
            );
        }
        let table = snap
            .find_first("AXTable")
            .ok_or_else(|| anyhow!("could not find chat list table in KakaoTalk's AX tree"))?;

        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        // Match on the row's first AXStaticText (the chat name) exactly, not
        // a substring — e.g. "Alice" must not accidentally match an
        // "Alice & Bob" group chat. If more than one row has the exact same
        // display name, refuse to guess rather than silently picking one
        // (there is no chat-id to disambiguate with — see
        // SafetyConfig::allowed_send_chats). The matching decision itself is
        // `super::match_chat_row`, a pure function tested outside this
        // macOS-only module.
        let row_names: Vec<Option<String>> = rows
            .iter()
            .map(|row| row.find_first("AXStaticText").and_then(|t| t.value.clone()))
            .collect();

        let row = match super::match_chat_row(&row_names, chat_display_name) {
            super::ChatMatch::NotFound => {
                return Err(anyhow!(
                    "chat '{chat_display_name}' not found in visible/loaded chat list"
                ))
            }
            super::ChatMatch::Found(idx) => rows[idx],
            super::ChatMatch::Ambiguous(count) => {
                return Err(anyhow!(
                    "chat name '{chat_display_name}' matches {count} chats in the visible list — ambiguous, refusing to guess"
                ))
            }
        };

        // We still resolve the target through the chat list first so duplicate
        // display names remain an error. Once that uniqueness check succeeds,
        // an already-open exact-title window is the safest fast path: current
        // KakaoTalk builds expose neither AXPress nor AXConfirm on some chat
        // rows, and a process-wide Return could land in an unrelated control.
        if find_chat_window(app, chat_display_name)?.is_some() {
            return Ok(());
        }

        // KakaoTalk may restore an already-open chat on another macOS Space,
        // where AXWindows cannot see it. After the visible chat list has
        // already established that the recipient name is unique, use the
        // app's own Window menu as a semantic (non-coordinate) way to select
        // exactly one existing window with that title. Pass the title as an
        // argv value so chat text is never interpolated into AppleScript.
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
        let mut script_child = Command::new("osascript")
            .args(["-e", script, chat_display_name])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to inspect KakaoTalk's Window menu")?;
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let mut clicked = false;
        loop {
            match script_child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    let mut stdout = String::new();
                    if let Some(mut pipe) = script_child.stdout.take() {
                        let _ = pipe.read_to_string(&mut stdout);
                    }
                    clicked = stdout.trim() == "clicked";
                    break;
                }
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() >= deadline => {
                    if debug {
                        eprintln!(
                            "[ax_send] open_chat_row: Window-menu probe timed out after \
                             {OPEN_CHAT_TIMEOUT:?}; falling back to chat-row selection"
                        );
                    }
                    let _ = script_child.kill();
                    let _ = script_child.wait();
                    break;
                }
                Ok(None) => sleep(Duration::from_millis(50)),
            }
        }
        if clicked {
            let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
            loop {
                if find_chat_window(app, chat_display_name)?.is_some() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "exact chat selected from KakaoTalk's Window menu did not become AX-visible"
                    );
                }
                sleep(Duration::from_millis(100));
            }
        }

        // Select via AX attribute (works even for off-screen rows — this is the
        // fix kakaocli landed for its off-screen-row regression) rather than a
        // coordinate-based double click. `AXSelectedRows` is a settable
        // attribute of the *table*, not the row (has no typed accessor in the
        // `accessibility` crate either way, so it's addressed by raw name).
        let selected_rows_attr: AXAttribute<CFType> =
            AXAttribute::new(&CFString::new("AXSelectedRows"));
        let one_row = CFArray::from_CFTypes(std::slice::from_ref(&row.element));
        table
            .element
            .set_attribute(&selected_rows_attr, one_row.as_CFType())
            .map_err(|e| anyhow!("failed to select chat row: {e:?}"))?;
        let selected_rows = table
            .element
            .attribute(&selected_rows_attr)
            .map_err(|error| anyhow!("failed to verify selected chat row: {error:?}"))?
            .downcast::<CFArray<*const std::ffi::c_void>>()
            .ok_or_else(|| anyhow!("selected chat row could not be read back"))?;
        let selected_exactly_once = selected_rows.len() == 1
            && selected_rows.get(0).is_some_and(|selected| unsafe {
                CFEqual(*selected, row.element.as_CFTypeRef()) != 0
            });
        if !selected_exactly_once {
            anyhow::bail!(
                "selected chat row did not match the exact requested row; refusing Return"
            );
        }

        // Open the exact selected row with an element-scoped AX action. A
        // process-wide synthetic Return here could land in a composer if focus
        // changed, so keyboard events are reserved for the post-authenticated
        // final send commit only.
        let press = CFString::new(kAXPressAction);
        let confirm = CFString::new("AXConfirm");
        let row_actions = row
            .element
            .action_names()
            .unwrap_or_else(|_| CFArray::from_CFTypes(&[]));
        let table_actions = table
            .element
            .action_names()
            .unwrap_or_else(|_| CFArray::from_CFTypes(&[]));
        let row_supports_press = row_actions
            .iter()
            .any(|action| action.to_string() == "AXPress");
        let row_supports_confirm = row_actions
            .iter()
            .any(|action| action.to_string() == "AXConfirm");
        let table_supports_confirm = table_actions
            .iter()
            .any(|action| action.to_string() == "AXConfirm");
        if row_supports_press {
            row.element.perform_action(&press).map_err(|error| {
                anyhow!("failed to open exact chat row with AXPress: {error:?}")
            })?;
        } else if row_supports_confirm {
            row.element.perform_action(&confirm).map_err(|error| {
                anyhow!("failed to open exact chat row with AXConfirm: {error:?}")
            })?;
        } else if table_supports_confirm {
            table.element.perform_action(&confirm).map_err(|error| {
                anyhow!("failed to confirm the exact selected chat row: {error:?}")
            })?;
        } else {
            // KakaoTalk 26.7 exposes no element-scoped action for some chat
            // rows. A bounded double-click at the exact selected row is safer
            // than a process-wide Return: it targets the verified row itself,
            // and the caller still requires the resulting exact-title window
            // and readable composer before it can continue.
            verify_main_window_frontmost(app, &main_window)?;
            let point = row
                .position_x
                .zip(row.position_y)
                .zip(row.width.zip(row.height))
                .map(|((x, y), (width, height))| CGPoint::new(x + width / 2.0, y + height / 2.0))
                .ok_or_else(|| {
                    anyhow!("exact selected chat row has no bounded AX frame; refusing click")
                })?;
            double_click(point)?;
        }

        if debug {
            eprintln!("[ax_send] open_chat_row: total {:?}", start.elapsed());
        }
        Ok(())
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
        let snap = snapshot(window)?;
        Ok(snap
            .find_first("AXSheet")
            .map(|sheet| sheet.element.clone()))
    }

    fn button_with_shortcut(root: &AXUIElement, shortcut: &str) -> Result<Option<AXUIElement>> {
        let snap = snapshot(root)?;
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        let matches: Vec<_> = buttons
            .into_iter()
            .filter(|button| {
                button
                    .help
                    .as_deref()
                    .is_some_and(|help| help.ends_with(shortcut))
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
                        || attr_as_string(&button.element, "AXTitle").as_deref() == Some("Open"))
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
        let matches = labels
            .into_iter()
            .filter(|label| label.value.as_deref() == Some(filename))
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

    fn wait_for_clipboard_image_preview(
        window: &AXUIElement,
    ) -> Result<(AXUIElement, AXUIElement)> {
        let deadline = Instant::now() + FILE_PICKER_READY_TIMEOUT;
        loop {
            if let Some(preview) = find_file_picker_sheet(window)? {
                if let Some(button) = find_enabled_button_with_title(&preview, "Send 1 files")? {
                    return Ok((preview, button));
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk did not expose a single-image clipboard preview in time");
            }
            sleep(Duration::from_millis(100));
        }
    }

    struct ChatWindowElements {
        input_field: AXUIElement,
        message_table: AXUIElement,
    }

    /// Resolve the composer and message table from one window snapshot so the
    /// send path does not recursively walk the same AX tree twice before the
    /// commit point.
    fn inspect_chat_window(root: &AXUIElement) -> Result<Option<ChatWindowElements>> {
        let snap = snapshot(root)?;
        let Some(input_field) = find_input_field(&snap) else {
            return Ok(None);
        };
        let Some(message_table) = snap.find_first("AXTable") else {
            return Ok(None);
        };
        Ok(Some(ChatWindowElements {
            input_field,
            message_table: message_table.element.clone(),
        }))
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

    /// Scrape every message bubble currently rendered in a chat window's
    /// message list, in on-screen (chronological) order. A row with an
    /// `AXTextArea` is a text message; a row with no `AXTextArea` but an
    /// `AXImage` descendant becomes the placeholder "[사진]"; a row with a
    /// share-labeled `AXButton` ("공유") becomes "[파일]". Rows matching none
    /// of these (date separators, system notices) are skipped, same as
    /// before.
    fn read_visible_messages(window: &AXUIElement) -> Result<Vec<AxMessage>> {
        let snap = snapshot(window)?;
        let Some(table) = snap.find_first("AXTable") else {
            return Ok(Vec::new());
        };
        Ok(read_visible_messages_from_node(table))
    }

    fn read_visible_messages_from_table(table: &AXUIElement) -> Result<Vec<AxMessage>> {
        let snap = snapshot(table)?;
        Ok(read_visible_messages_from_node(&snap))
    }

    fn delivery_observations(table: &AXUIElement) -> Result<Vec<super::DeliveryObservation>> {
        Ok(read_visible_messages_from_table(table)?
            .into_iter()
            .map(|message| super::DeliveryObservation {
                text: message.text,
                outgoing: message.outgoing,
            })
            .collect())
    }

    fn read_visible_messages_from_node(table: &AxNode) -> Vec<AxMessage> {
        let table_center_x = table
            .position_x
            .zip(table.width)
            .map(|(position_x, width)| position_x + width / 2.0);
        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        rows.iter()
            .filter_map(|row| {
                let (text, bubble) = message_row_text(row)?;
                let outgoing = table_center_x
                    .zip(bubble.position_x.zip(bubble.width))
                    .is_some_and(|(table_center, (position_x, width))| {
                        position_x + width / 2.0 > table_center
                    });

                let time = row
                    .find_first("AXStaticText")
                    .and_then(|t| t.help.clone().or_else(|| t.value.clone()));

                Some(AxMessage {
                    time,
                    text,
                    outgoing,
                })
            })
            .collect()
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
            return Some(("[파일]".to_string(), button));
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
            super::ChatMatch::Ambiguous(count) => Err(anyhow!(
                "chat name '{chat_display_name}' matches {count} open windows exactly — ambiguous, refusing to guess"
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

    /// Send one image through the signed KakaoTalk app's native file picker.
    /// Errors are pre-commit; `Uncertain` means KakaoTalk's final Send action
    /// may have started an upload and callers must inspect before retrying.
    pub fn send_photo_via_ax_classified(
        chat_display_name: &str,
        photo_path: &std::path::Path,
    ) -> Result<super::AxDeliveryOutcome> {
        let _display_wake = hold_display_awake()?;
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        open_chat_row(&app, chat_display_name)?;

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let (window, elements) = loop {
            if let Some(window) = find_chat_window(&app, chat_display_name)? {
                if let Some(elements) = inspect_chat_window(&window)? {
                    break (window, elements);
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "exact chat window '{chat_display_name}' did not open with a readable composer in time"
                );
            }
            sleep(Duration::from_millis(150));
        };

        let existing_draft = attr_as_string(&elements.input_field, "AXValue").ok_or_else(|| {
            anyhow!("could not read the exact target's composer before opening the file picker")
        })?;
        if !existing_draft.is_empty() {
            anyhow::bail!(
                "the exact target's composer already contains a draft; refusing to disturb it"
            );
        }
        delivery_observations(&elements.message_table)
            .context("could not read the exact target's messages before opening the file picker")?;
        let filename = photo_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("photo filename is not valid UTF-8"))?;

        let send_files = button_with_shortcut(&window, "⌘O")?
            .ok_or_else(|| anyhow!("could not find KakaoTalk's unique Send Files (⌘O) button"))?;
        send_files
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not open KakaoTalk's file picker: {error:?}"))?;

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let sheet = loop {
            if let Some(sheet) = find_file_picker_sheet(&window)? {
                break sheet;
            }
            if Instant::now() >= deadline {
                anyhow::bail!("KakaoTalk's file picker did not appear in time");
            }
            sleep(Duration::from_millis(100));
        };

        // KakaoTalk 26.7's native open panel does not reliably expose its
        // Go-to-Folder field or enable the Open button. Cancel the pre-commit
        // panel and paste the already content-validated image bytes into the
        // exact, empty composer. KakaoTalk must still expose one enabled
        // single-image preview, which is revalidated below before the only
        // action that can send the photo.
        let cancel = find_enabled_button_with_title(&sheet, "Cancel")?
            .ok_or_else(|| anyhow!("file picker did not expose a unique enabled Cancel button"))?;
        cancel
            .perform_action(&CFString::new(kAXPressAction))
            .map_err(|error| anyhow!("could not cancel KakaoTalk's file picker: {error:?}"))?;
        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        loop {
            if find_file_picker_sheet(&window)?.is_none() {
                break;
            }
            if Instant::now() >= deadline {
                anyhow::bail!("file picker did not close; clipboard photo paste was not attempted");
            }
            sleep(Duration::from_millis(100));
        }
        copy_image_to_clipboard(photo_path)?;
        focus_window_and_verify(&app, &window)
            .context("could not focus exact target for clipboard photo paste")?;
        focus_and_verify(
            &elements.input_field,
            "exact target composer for photo paste",
        )?;
        press_command_v(pid)?;
        let (preview_sheet, send_button) = wait_for_clipboard_image_preview(&window)?;
        let preview_has_filename = false;

        let authenticated_target =
            crate::util::escape_terminal_text(&crate::util::truncate(chat_display_name, 80));
        openkakao_cli::human_auth::require_device_owner_auth(&format!(
            "Approve one KakaoTalk photo to ‘{authenticated_target}’ after reviewing the terminal preview"
        ))?;

        // Revalidate the exact recipient and previewed basename immediately
        // before the first action that can upload the photo.
        find_chat_window(&app, chat_display_name)?
            .ok_or_else(|| anyhow!("exact target window disappeared before photo commit"))?;
        if preview_has_filename && !sheet_has_exact_filename(&preview_sheet, filename)? {
            anyhow::bail!("previewed photo changed before commit; Send was not pressed");
        }
        let baseline = delivery_observations(&elements.message_table)
            .context("could not re-read the exact target's messages before photo commit")?;
        // KakaoTalk 26.7 can return AX error -25200 even after accepting the
        // Send action. Always verify the bubble before classifying the result.
        let commit_error = send_button
            .perform_action(&CFString::new(kAXPressAction))
            .err();

        let deadline = Instant::now() + VERIFY_TIMEOUT;
        loop {
            match find_chat_window(&app, chat_display_name) {
                Ok(Some(_)) => {
                    let current = match delivery_observations(&elements.message_table) {
                        Ok(observations) => observations,
                        Err(error) => {
                            return Ok(super::AxDeliveryOutcome::Uncertain {
                                reason: format!(
                                    "photo may have been sent but AX verification failed: {error:#}; commit action result: {commit_error:?}"
                                ),
                            });
                        }
                    };
                    if super::has_new_exact_outgoing_message(
                        &baseline,
                        &current,
                        PHOTO_BUBBLE_PLACEHOLDER,
                    ) {
                        return Ok(super::AxDeliveryOutcome::Verified);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    return Ok(super::AxDeliveryOutcome::Uncertain {
                        reason: format!(
                            "photo may have been sent but target-window verification became ambiguous: {error:#}; commit action result: {commit_error:?}"
                        ),
                    });
                }
            }
            if Instant::now() >= deadline {
                return Ok(super::AxDeliveryOutcome::Uncertain {
                    reason: format!(
                        "photo may have been sent but no additional outgoing photo bubble appeared in '{chat_display_name}' within {}s — inspect KakaoTalk before retrying; commit action result: {commit_error:?}",
                        VERIFY_TIMEOUT.as_secs(),
                    ),
                });
            }
            sleep(VERIFY_POLL_INTERVAL);
        }
    }

    /// Read the most recent `count` messages visible in a chat's AX message list,
    /// opening the chat first if it isn't already open. No local SQLCipher DB
    /// access, so this works even when `local_db.rs`'s key derivation is stale
    /// for the installed KakaoTalk build (see README deprecation notice). Only
    /// messages already rendered on screen are returned — older history requires
    /// scrolling up in KakaoTalk first.
    pub fn read_via_ax(chat_display_name: &str, count: usize) -> Result<Vec<AxMessage>> {
        let debug = std::env::var("OPENKAKAO_CLI_DEBUG").is_ok();
        let start = Instant::now();
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;

        open_chat_row(&app, chat_display_name)?;

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let mut messages = loop {
            if let Some(window) = find_chat_window(&app, chat_display_name)? {
                let msgs = read_visible_messages(&window)?;
                if !msgs.is_empty() {
                    break msgs;
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!("chat window did not open (or has no visible messages) in time");
            }
            sleep(Duration::from_millis(150));
        };
        if messages.len() > count {
            messages = messages.split_off(messages.len() - count);
        }
        if debug {
            eprintln!("[ax_send] read_via_ax: total {:?}", start.elapsed());
        }
        Ok(messages)
    }

    /// Send to exactly one chat and classify the result by commit point.
    /// Errors returned from this function happened before the final Return key,
    /// while `Uncertain` means Return may have been delivered but a new message
    /// bubble could not be proven.
    pub fn send_via_ax_classified(
        chat_display_name: &str,
        message: &str,
    ) -> Result<super::AxDeliveryOutcome> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;

        // Always resolve the target through the chat list, even when an exact-
        // title window is already open. Otherwise one of two same-named rooms
        // could bypass the list-level ambiguity check through the old fast path.
        open_chat_row(&app, chat_display_name)?;

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let elements = loop {
            if let Some(window) = find_chat_window(&app, chat_display_name)? {
                if let Some(elements) = inspect_chat_window(&window)? {
                    break elements;
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "exact chat window '{chat_display_name}' did not open with a readable composer in time"
                );
            }
            sleep(Duration::from_millis(150));
        };

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

        // Keep the OS-mediated human-presence boundary inside the one native
        // transport implementation. This makes it impossible for a future
        // command caller to reach a real AX send merely by forgetting its own
        // prompt. At this point the signed process, unique target row, exact
        // window, empty composer, and pre-send bubble baseline are all known;
        // authentication happens immediately before the composer is changed.
        let authenticated_target =
            crate::util::escape_terminal_text(&crate::util::truncate(chat_display_name, 80));
        openkakao_cli::human_auth::require_device_owner_auth(&format!(
            "Approve one KakaoTalk message to ‘{authenticated_target}’ after reviewing the terminal preview"
        ))?;

        focus_and_verify(&field, "exact target composer")?;
        field
            .set_value(CFString::new(message).as_CFType())
            .map_err(|error| anyhow!("could not set the exact target composer: {error:?}"))?;
        if !wait_for_composer_value(&field, message) {
            anyhow::bail!(
                "could not verify the exact message in the target composer; Return was not pressed"
            );
        }

        // Re-establish and verify focus immediately before the irreversible
        // PID-scoped Return event. The composer read-back loop above may wait
        // for up to a second, during which another KakaoTalk control could
        // otherwise gain focus.
        focus_and_verify(&field, "exact target composer immediately before commit")?;
        if attr_as_string(&field, "AXValue").as_deref() != Some(message) {
            anyhow::bail!(
                "the exact target composer changed before commit; Return was not pressed"
            );
        }

        if let Err(error) = press_return(pid) {
            return Ok(super::AxDeliveryOutcome::Uncertain {
                reason: format!(
                    "Return delivery failed after commit began: {error:#}; inspect KakaoTalk before retrying"
                ),
            });
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
                    if super::is_verified_delivery(
                        &baseline,
                        &current,
                        message,
                        attr_as_string(&field, "AXValue").as_deref(),
                    ) {
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
                        "sent the message but no additional exact-text bubble appeared in the '{chat_display_name}' chat window within {}s — check KakaoTalk before retrying to avoid duplicates",
                        VERIFY_TIMEOUT.as_secs()
                    ),
                });
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
    /// Uses the same single-snapshot chat-list traversal as `open_chat_row`
    /// (main window → chatrooms tab → AXTable → AXRow), but only reads each
    /// row instead of selecting it — so nothing is opened and no unread state
    /// changes. Rows with no readable name are skipped.
    pub fn scrape_chat_list() -> Result<Vec<ChatListRow>> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = bounded_application(pid)?;
        let main_window = find_main_window(&app)?;
        let snap = ensure_chatrooms_tab(&main_window)?;
        let table = snap
            .find_first("AXTable")
            .ok_or_else(|| anyhow!("could not find chat list table in KakaoTalk's AX tree"))?;

        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut static_texts = Vec::new();
            row.find_all("AXStaticText", &mut static_texts);
            // The first static text is the chat name (same convention as
            // open_chat_row). Skip rows we can't name.
            let Some(name) = static_texts.first().and_then(|t| t.value.clone()) else {
                continue;
            };
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

            out.push(ChatListRow {
                name,
                unread,
                preview,
                timestamp,
            });
        }
        Ok(out)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

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
    read_via_ax, scrape_chat_list, send_photo_via_ax_classified, send_via_ax_classified,
    ChatListRow,
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

    pub fn send_photo_via_ax_classified(
        _chat_display_name: &str,
        _photo_path: &std::path::Path,
    ) -> Result<super::AxDeliveryOutcome> {
        Err(anyhow!(
            "local-send-photo (AX automation) is only supported on macOS"
        ))
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
    read_via_ax, scrape_chat_list, send_photo_via_ax_classified, send_via_ax_classified,
    ChatListRow,
};

/// Turn a classified outcome into the caller-facing result. `Uncertain` must
/// surface as a hard error — never `Ok` — so the CLI exits non-zero and
/// automation does not blindly retry a send whose commit state is unknown.
fn outcome_to_result(outcome: AxDeliveryOutcome) -> anyhow::Result<()> {
    match outcome {
        AxDeliveryOutcome::Verified => Ok(()),
        AxDeliveryOutcome::Uncertain { reason } => Err(anyhow::anyhow!(reason)),
    }
}

pub fn send_via_ax(chat_display_name: &str, message: &str) -> anyhow::Result<()> {
    outcome_to_result(send_via_ax_classified(chat_display_name, message)?)
}

pub fn send_photo_via_ax(
    chat_display_name: &str,
    photo_path: &std::path::Path,
) -> anyhow::Result<()> {
    outcome_to_result(send_photo_via_ax_classified(chat_display_name, photo_path)?)
}
