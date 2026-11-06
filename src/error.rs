use thiserror::Error;

/// Primary error type for openkakao-cli operations.
#[derive(Error, Debug)]
pub enum OpenKakaoError {
    #[error("LOCO command {command} failed (status={status})")]
    LocoStatus {
        command: String,
        status: i64,
        body: Option<bson::Document>,
    },

    #[error("Token expired or invalid (status=-950)")]
    TokenExpired,

    #[error("Network error: {message}")]
    Network { message: String, is_transient: bool },

    #[error("REST API error (status={status}): {message}")]
    RestApi { status: i64, message: String },

    #[error("Safety block: {0}")]
    SafetyBlock(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl OpenKakaoError {
    /// Whether this error is transient and the operation should be retried.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::LocoStatus { status, .. } => matches!(status, -300 | -500),
            Self::TokenExpired => true,
            Self::Network { is_transient, .. } => *is_transient,
            _ => false,
        }
    }

    /// Create a LOCO status error from a command name and response status.
    pub fn loco(command: impl Into<String>, status: i64) -> Self {
        if status == -950 {
            Self::TokenExpired
        } else {
            Self::LocoStatus {
                command: command.into(),
                status,
                body: None,
            }
        }
    }

    /// Create a LOCO status error with the response body attached.
    pub fn loco_with_body(command: impl Into<String>, status: i64, body: bson::Document) -> Self {
        if status == -950 {
            Self::TokenExpired
        } else {
            Self::LocoStatus {
                command: command.into(),
                status,
                body: Some(body),
            }
        }
    }
}

impl From<reqwest::Error> for OpenKakaoError {
    fn from(e: reqwest::Error) -> Self {
        let is_transient = e.is_timeout() || e.is_connect();
        Self::Network {
            message: e.to_string(),
            is_transient,
        }
    }
}

impl From<std::io::Error> for OpenKakaoError {
    fn from(e: std::io::Error) -> Self {
        let is_transient = matches!(
            e.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::TimedOut
        );
        Self::Network {
            message: e.to_string(),
            is_transient,
        }
    }
}

/// Stable, machine-readable failure codes for the login-free AX commands.
/// Agents branch on `code`; humans read `fix`. Renaming a code is a breaking
/// change for automation, so add new ones instead.
// Most codes are raised only by the macOS AX driver, which the non-macOS
// stub never compiles.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    AccessibilityNotGranted,
    KakaotalkNotRunning,
    KakaotalkSignedOut,
    MainWindowNotVisible,
    MainWindowMinimized,
    NotOnAllSpaces,
    ChatListUnavailable,
    ChatWindowNotOpen,
    ChatNotFound,
    ChatListFiltered,
    ChatAmbiguous,
    SendButtonMissing,
    MenuNotFound,
    StagingConflict,
    UserActive,
    AxSendDisabled,
    ChatNotAllowed,
    InvalidFile,
    AttachmentUnverified,
    DeliveryUncertain,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccessibilityNotGranted => "accessibility_not_granted",
            Self::KakaotalkNotRunning => "kakaotalk_not_running",
            Self::KakaotalkSignedOut => "kakaotalk_signed_out",
            Self::MainWindowNotVisible => "main_window_not_visible",
            Self::MainWindowMinimized => "main_window_minimized",
            Self::NotOnAllSpaces => "kakaotalk_not_on_all_spaces",
            Self::ChatListUnavailable => "chat_list_unavailable",
            Self::ChatWindowNotOpen => "chat_window_not_open",
            Self::ChatNotFound => "chat_not_found",
            Self::ChatListFiltered => "chat_list_filtered",
            Self::ChatAmbiguous => "chat_ambiguous",
            Self::SendButtonMissing => "send_button_missing",
            Self::MenuNotFound => "menu_not_found",
            Self::StagingConflict => "staging_conflict",
            Self::UserActive => "user_active",
            Self::AxSendDisabled => "ax_send_disabled",
            Self::ChatNotAllowed => "chat_not_allowed",
            Self::InvalidFile => "invalid_file",
            Self::AttachmentUnverified => "attachment_unverified",
            Self::DeliveryUncertain => "delivery_uncertain",
        }
    }

    pub fn fix(self) -> &'static str {
        match self {
            Self::AccessibilityNotGranted => {
                "System Settings → Privacy & Security → Accessibility: enable the app that runs \
                 openkakao-cli (Terminal, iTerm2, Orca, ...). Over SSH, add \
                 /usr/libexec/sshd-keygen-wrapper (press ⌘⇧G in the + dialog). Then re-run."
            }
            Self::KakaotalkNotRunning => {
                "Launch it in the background with `open -g -b com.kakao.KakaoTalkMac` (or run \
                 `openkakao-cli init`), make sure it is signed in, then re-run."
            }
            Self::KakaotalkSignedOut => {
                "KakaoTalk is showing its sign-in window. Sign in on this Mac, then re-run."
            }
            Self::MainWindowNotVisible => {
                "Open KakaoTalk's main window on the Space you are viewing. If it lives on another \
                 Space, assign KakaoTalk to all desktops once: `openkakao-cli init --all-spaces` \
                 (the same as Dock → Options → Assign To → All Desktops)."
            }
            Self::MainWindowMinimized => {
                "Un-minimize KakaoTalk's main window yourself (click its Dock icon); restoring it \
                 automatically could steal your focus."
            }
            Self::NotOnAllSpaces => {
                "Run `openkakao-cli init --all-spaces` once. It records the Dock's Assign To → All \
                 Desktops setting for KakaoTalk and restarts the Dock, so KakaoTalk windows stay \
                 visible to Accessibility on every Space."
            }
            Self::ChatListUnavailable => {
                "Switch KakaoTalk's main window to the Chats tab and re-run `openkakao-cli doctor`."
            }
            Self::ChatWindowNotOpen => {
                "Open the chat window in KakaoTalk once and keep it open: open windows are read and \
                 sent to without touching your focus. Or re-run with --open-if-needed, which opens \
                 it from the chat list's context menu without raising KakaoTalk (this marks the \
                 chat as read)."
            }
            Self::ChatNotFound => {
                "Only rows KakaoTalk has loaded are visible, and the list can stay stale until the \
                 main window repaints: scroll the chat list or switch tabs (Friends → Chats) in \
                 KakaoTalk, or open the chat window once (an open window is used directly). The \
                 name must match the row exactly."
            }
            Self::ChatListFiltered => {
                "Clear the search field in KakaoTalk's chat list, then scroll or switch tabs so the \
                 list repaints, and re-run."
            }
            Self::ChatAmbiguous => {
                "Several chats show this exact name. Rename one in KakaoTalk or close the extra \
                 chat windows; openkakao-cli never guesses."
            }
            Self::SendButtonMissing => {
                "This KakaoTalk build exposes no 전송 button to Accessibility (verified on \
                 KakaoTalk 26.9.0). Re-run with --fallback-keys to raise KakaoTalk briefly and \
                 press Return instead."
            }
            Self::MenuNotFound => {
                "KakaoTalk did not expose the expected menu in time, so nothing else was done (if a \
                 menu is showing, click elsewhere to dismiss it; never press Esc, which closes \
                 KakaoTalk windows). Retry, open the chat window yourself once, or use \
                 --fallback-keys (raises KakaoTalk and double-clicks the row)."
            }
            Self::StagingConflict => {
                "A different file with this name is already in ~/Downloads. Move or copy your file \
                 to the top level of ~/Downloads, ~/Desktop or ~/Documents and send it from there, \
                 or rename it."
            }
            Self::UserActive => {
                "You were using the keyboard or mouse, so KakaoTalk was not raised. Retry when idle, \
                 or lower the wait with --wait-idle <secs> (0 disables it)."
            }
            Self::AxSendDisabled => {
                "Run `openkakao-cli init --allow-chat \"<exact chat name>\"`, or set \
                 [safety] allow_ax_send = true in ~/.config/openkakao/config.toml."
            }
            Self::ChatNotAllowed => {
                "Run `openkakao-cli init --allow-chat \"<exact chat name>\"`, or add the name to \
                 [safety] allowed_send_chats in ~/.config/openkakao/config.toml."
            }
            Self::InvalidFile => {
                "Pass an existing, non-empty regular file with a UTF-8 path. local-send-file takes \
                 any type up to [ax] max_file_mb (default 300 MiB); local-send-photo takes JPEG or \
                 PNG image data up to 50 MiB (send other images with local-send-file)."
            }
            Self::AttachmentUnverified => {
                "Nothing was sent. Check the chat window in KakaoTalk (close any leftover preview), \
                 then re-run with OPENKAKAO_CLI_DEBUG=1 and report the output."
            }
            Self::DeliveryUncertain => {
                "The send may have happened. Look at the chat in KakaoTalk before retrying; never \
                 retry blindly."
            }
        }
    }
}

/// An error carrying a stable [`ErrorCode`]. `Display` is the plain message so
/// existing `to_string()` callers and stored reasons stay unchanged; the fix
/// hint is printed separately.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct CodedError {
    pub code: ErrorCode,
    pub message: String,
}

pub fn coded(code: ErrorCode, message: impl Into<String>) -> anyhow::Error {
    CodedError {
        code,
        message: message.into(),
    }
    .into()
}

/// The first [`ErrorCode`] anywhere in the error's context chain.
pub fn error_code(error: &anyhow::Error) -> Option<ErrorCode> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CodedError>())
        .map(|coded| coded.code)
}

/// `{"error": {"code", "message", "fix"}}` for `--json` failures. Uncoded
/// errors use `"unclassified"` and a `null` fix.
pub fn error_json(error: &anyhow::Error) -> serde_json::Value {
    let code = error_code(error);
    serde_json::json!({
        "error": {
            "code": code.map_or("unclassified", ErrorCode::as_str),
            "message": format!("{error:#}"),
            "fix": code.map(ErrorCode::fix),
        }
    })
}

#[cfg(test)]
mod coded_tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn code_survives_added_context_and_maps_to_json() {
        let error = Err::<(), _>(coded(ErrorCode::ChatListFiltered, "chat 'x' not found"))
            .context("ax-read failed")
            .unwrap_err();
        assert_eq!(error_code(&error), Some(ErrorCode::ChatListFiltered));
        let json = error_json(&error);
        assert_eq!(json["error"]["code"], "chat_list_filtered");
        assert_eq!(
            json["error"]["message"],
            "ax-read failed: chat 'x' not found"
        );
        assert!(json["error"]["fix"]
            .as_str()
            .unwrap()
            .contains("search field"));
    }

    #[test]
    fn display_is_the_plain_message() {
        let error = coded(ErrorCode::DeliveryUncertain, "may have been sent");
        assert_eq!(error.to_string(), "may have been sent");
    }

    #[test]
    fn uncoded_errors_are_unclassified() {
        let json = error_json(&anyhow::anyhow!("boom"));
        assert_eq!(json["error"]["code"], "unclassified");
        assert!(json["error"]["fix"].is_null());
    }

    #[test]
    fn codes_are_stable_snake_case_and_hints_name_the_known_fixes() {
        assert_eq!(
            ErrorCode::AccessibilityNotGranted.as_str(),
            "accessibility_not_granted"
        );
        assert!(ErrorCode::AccessibilityNotGranted
            .fix()
            .contains("sshd-keygen-wrapper"));
        assert!(ErrorCode::MainWindowNotVisible
            .fix()
            .contains("All Desktops"));
        assert!(ErrorCode::ChatNotFound.fix().contains("repaints"));
        assert!(ErrorCode::UserActive.fix().contains("--wait-idle"));
    }
}
