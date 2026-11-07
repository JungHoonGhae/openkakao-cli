//! `notif-watch` — login-free receive detection via the macOS Notification
//! Center database. Polls `usernoted`'s SQLite (read-only) for KakaoTalk
//! notifications and emits them through the shared watch sinks. Works with the
//! KakaoTalk window closed, minimized, or on another Space, and self-sent
//! messages never appear (a message you send posts no notification). No server
//! contact (no ban risk), no SQLCipher decryption, no login — a read-only read
//! of a plaintext local SQLite file.
//!
//! Limits (structural — see docs): muted/notifications-off chats and the chat
//! you are currently focused on post no notification, so they are not seen; the
//! DB retains only notifications still in Notification Center (a forward-only
//! live stream, not history).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use plist::Value;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::commands::watch::{
    dispatch_event, parse_webhook_header, validate_webhook_url, watch_hook_matches, DispatchReport,
    WatchHookConfig, WatchMessageEvent, WebhookFormat,
};
use crate::receive_inbox::{FailureDisposition, ReceiveInbox};
use crate::util::{escape_terminal_text, require_permission};

/// Seconds between the CFAbsoluteTime epoch (2001-01-01) and the Unix epoch.
const CFABSOLUTE_EPOCH_OFFSET: f64 = 978_307_200.0;
/// KakaoTalk's macOS bundle identifier as stored by Notification Center.
const KAKAO_BUNDLE_ID: &str = "com.kakao.kakaotalkmac";
const TERMINAL_LEDGER_GRACE_SECS: u64 = 24 * 60 * 60;

/// One KakaoTalk message pulled from a notification payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifMessage {
    /// `/req/titl` — chat room / sender display name.
    pub chat_name: String,
    /// `/req/body` — message text. Empty for image messages or when the user's
    /// notification-preview setting hides content.
    pub body: String,
    /// `iden` prefix — stable per chat room.
    pub room_id: i64,
    /// `iden` suffix — per-message id (dedup / ordering key).
    pub msg_id: i64,
    /// `/date` — CFAbsoluteTime (seconds since 2001-01-01).
    pub ts: f64,
    /// `/req/atta[0]/pat` — local path of the first attachment, or empty.
    pub attachment_path: String,
}

/// Decode one notification payload (binary plist) into a `NotifMessage`.
///
/// Returns `None` when the payload lacks a parseable `iden` (`<room>_<msg>`,
/// both decimal) — that id is the dedup key, so a notification without it can't
/// be tracked. A missing title/body/attachment is fine (fields default empty).
pub fn parse_notification(payload: &[u8]) -> Option<NotifMessage> {
    let root = Value::from_reader(std::io::Cursor::new(payload)).ok()?;
    let dict = root.as_dictionary()?;
    let req = dict.get("req").and_then(|v| v.as_dictionary());

    let req_str = |k: &str| -> Option<String> {
        req.and_then(|d| d.get(k))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string())
    };

    // iden = "<room_id>_<msg_id>", both decimal. Required.
    let iden = req_str("iden")?;
    let (room_s, msg_s) = iden.split_once('_')?;
    let room_id = room_s.parse::<i64>().ok()?;
    let msg_id = msg_s.parse::<i64>().ok()?;

    let chat_name = req_str("titl").unwrap_or_default();
    let body = req_str("body").unwrap_or_default();

    let ts = dict.get("date").and_then(value_as_f64).unwrap_or(0.0);

    let attachment_path = req
        .and_then(|r| r.get("atta"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("pat"))
        .and_then(|v| v.as_string())
        .unwrap_or_default()
        .to_string();

    Some(NotifMessage {
        chat_name,
        body,
        room_id,
        msg_id,
        ts,
        attachment_path,
    })
}

/// `date` may be a real, an integer, a numeric string, or a plist `Date`
/// (CFDate). All resolve to a CFAbsoluteTime (seconds since 2001-01-01).
fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Real(f) => Some(*f),
        Value::Integer(i) => i
            .as_signed()
            .map(|n| n as f64)
            .or_else(|| i.as_unsigned().map(|n| n as f64)),
        Value::String(s) => s.parse::<f64>().ok(),
        Value::Date(d) => {
            // plist::Date → SystemTime → Unix secs → CFAbsoluteTime
            let unix = std::time::SystemTime::from(*d)
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs_f64();
            Some(unix - CFABSOLUTE_EPOCH_OFFSET)
        }
        _ => None,
    }
}

/// CFAbsoluteTime → RFC3339 UTC string, matching the other watch sources'
/// `received_at` format. A non-positive/absent timestamp falls back to now()
/// (the notification just arrived) rather than 2001-01-01.
fn ts_to_rfc3339(cf: f64) -> String {
    if !cf.is_finite() || cf <= 0.0 {
        return chrono::Utc::now().to_rfc3339();
    }
    let unix = cf + CFABSOLUTE_EPOCH_OFFSET;
    let micros = (unix * 1_000_000.0).round() as i64;
    chrono::DateTime::<chrono::Utc>::from_timestamp_micros(micros)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}

/// Map a decoded notification to the shared watch event.
pub fn build_event(m: &NotifMessage) -> WatchMessageEvent {
    WatchMessageEvent {
        event_type: "notif_message",
        received_at: ts_to_rfc3339(m.ts),
        method: "notif".to_string(),
        chat_id: m.room_id,
        chat_name: m.chat_name.clone(),
        log_id: m.msg_id,
        author_id: 0,
        author_nickname: String::new(),
        // 1 = text, 2 = has attachment (mirrors the LOCO message_type convention
        // closely enough for hook filtering).
        message_type: if m.attachment_path.is_empty() { 1 } else { 2 },
        message: m.body.clone(),
        attachment: m.attachment_path.clone(),
        unread: 0,
    }
}

/// Tracks one Notification Center snapshot and returns notifications newly
/// present in the next snapshot. Keeping only the previous snapshot bounds
/// memory by Notification Center's retention instead of watch-process uptime.
///
/// The first observation establishes a baseline so notifications that predate
/// startup do not flood the sinks. Duplicate records in one DB snapshot are
/// emitted at most once.
#[derive(Default)]
struct NotificationTracker {
    previous: HashSet<(i64, i64)>,
    initialized: bool,
}

impl NotificationTracker {
    /// Construct a tracker whose first observation is emitted instead of used
    /// only as a baseline. This is opt-in because retained notifications may
    /// predate the current process invocation.
    fn replay_existing() -> Self {
        Self {
            previous: HashSet::new(),
            initialized: true,
        }
    }

    fn observe<'a>(&mut self, messages: &'a [NotifMessage]) -> Vec<&'a NotifMessage> {
        let mut current = HashSet::with_capacity(messages.len());
        let mut fresh = Vec::new();

        for message in messages {
            let key = (message.room_id, message.msg_id);
            if current.insert(key) && self.initialized && !self.previous.contains(&key) {
                fresh.push(message);
            }
        }

        self.previous = current;
        self.initialized = true;
        fresh
    }
}

/// Options for `notif-watch` (mirrors `AxWatchOptions`).
pub struct NotifWatchOptions {
    pub interval_secs: u64,
    pub replay_existing: bool,
    pub durable: bool,
    pub hook_cmd: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_headers: Vec<String>,
    pub webhook_signing_secret: Option<String>,
    pub webhook_format: WebhookFormat,
    pub hook_chats: Vec<String>,
    pub hook_keywords: Vec<String>,
    pub fail_fast: bool,
    pub allow_insecure_webhooks: bool,
    pub min_hook_interval_secs: u64,
    pub min_webhook_interval_secs: u64,
    pub hook_timeout_secs: u64,
    pub webhook_timeout_secs: u64,
    pub json: bool,
    pub unattended: bool,
    pub allow_side_effects: bool,
}

struct NotifSink<'a> {
    hook_config: &'a WatchHookConfig,
    has_sinks: bool,
    json: bool,
}

impl NotifSink<'_> {
    async fn emit(&self, message: &NotifMessage) -> Result<DispatchReport> {
        let event = build_event(message);
        let human_line = format!(
            "[notif-watch] {}: {}",
            escape_terminal_text(&event.chat_name),
            if event.message.is_empty() {
                "(attachment)".to_string()
            } else {
                escape_terminal_text(&event.message)
            }
        );
        dispatch_event(
            &event,
            self.hook_config,
            self.has_sinks,
            self.json,
            &human_line,
            "notif-watch",
        )
        .await
    }

    async fn drain(&self, inbox: &ReceiveInbox) -> Result<()> {
        let lease_secs = delivery_lease_secs(self.hook_config);
        for _ in 0..1_000 {
            let Some(claim) = inbox.claim_next(lease_secs)? else {
                break;
            };
            let pending = &claim.event;
            let message: NotifMessage = match serde_json::from_value(pending.payload.clone()) {
                Ok(message) => message,
                Err(error) => {
                    let summary = format!("decode durable event {}: {error}", pending.event_id);
                    inbox.mark_claim_quarantined(&claim, &summary)?;
                    eprintln!(
                        "[notif-watch] quarantined malformed durable event {}: {}",
                        pending.event_id, error
                    );
                    continue;
                }
            };
            let report = match self.emit(&message).await {
                Ok(report) => report,
                Err(error) => {
                    let disposition = inbox.mark_failed(&claim, &error.to_string())?;
                    report_retry_disposition(&pending.event_id, disposition);
                    return Err(error).with_context(|| {
                        format!("deliver durable notification event {}", pending.event_id)
                    });
                }
            };
            if report.is_success() {
                inbox.mark_delivered(&claim)?;
                if self.uses_throttled_sink(&message) {
                    break;
                }
            } else {
                let summary = report.error_summary();
                let disposition = inbox.mark_failed(&claim, &summary)?;
                report_retry_disposition(&pending.event_id, disposition);
                break;
            }
        }
        Ok(())
    }

    fn uses_throttled_sink(&self, message: &NotifMessage) -> bool {
        if !watch_hook_matches(self.hook_config, &build_event(message)) {
            return false;
        }
        (self.hook_config.command.is_some() && self.hook_config.min_hook_interval_secs > 0)
            || (self.hook_config.webhook_url.is_some()
                && self.hook_config.min_webhook_interval_secs > 0)
    }
}

fn delivery_lease_secs(config: &WatchHookConfig) -> u64 {
    // Hook and webhook delivery are sequential. Lease for their combined
    // worst-case runtime so a second watcher cannot reclaim a still-live
    // delivery between sinks.
    let hook_budget = config
        .command
        .as_ref()
        .map(|_| config.hook_timeout_secs.max(1))
        .unwrap_or(0);
    let webhook_budget = config
        .webhook_url
        .as_ref()
        .map(|_| config.webhook_timeout_secs.max(1))
        .unwrap_or(0);
    hook_budget
        .saturating_add(webhook_budget)
        .saturating_add(60)
}

fn report_retry_disposition(event_id: &str, disposition: FailureDisposition) {
    match disposition {
        FailureDisposition::RetryScheduled { delay_secs } => eprintln!(
            "[notif-watch] delivery deferred for {event_id}; retrying in about {delay_secs}s"
        ),
        FailureDisposition::Quarantined => eprintln!(
            "[notif-watch] delivery quarantined for {event_id} after repeated failures; later events can continue"
        ),
    }
}

fn notification_event_id(message: &NotifMessage) -> String {
    format!("notif:{}:{}", message.room_id, message.msg_id)
}

/// Path to the macOS Notification Center database.
fn notif_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("No home directory")?;
    Ok(home.join("Library/Group Containers/group.com.apple.usernoted/db2/db"))
}

/// Open the notification DB read-only. Uses plain `mode=ro` (not `immutable=1`)
/// so SQLite reads the `-wal` too — usernoted is WAL-mode and actively written,
/// and the newest notifications sit in the WAL before they're checkpointed;
/// ignoring it drops them. `mode=ro` also gives a consistent WAL snapshot, so a
/// read that races the writer doesn't see a torn image. Single source of truth
/// for how every code path opens this DB.
fn open_notif_db(db_path: &Path) -> Result<Connection> {
    let uri = format!("file:{}?mode=ro", db_path.display());
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open notification DB: {}", db_path.display()))
}

/// Read the current KakaoTalk notification payloads, read-only.
fn read_kakao_payloads(db_path: &Path) -> Result<Vec<Vec<u8>>> {
    let conn = open_notif_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT r.data FROM record r JOIN app a ON r.app_id = a.app_id \
         WHERE lower(a.identifier) = ?1 AND r.data IS NOT NULL \
         ORDER BY COALESCE(r.request_last_date, r.request_date, r.delivered_date, 0), r.rec_id",
    )?;
    let rows = stmt.query_map([KAKAO_BUNDLE_ID], |row| row.get::<_, Vec<u8>>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The KakaoTalk notifications Notification Center still holds, read-only.
pub fn retained_messages() -> Result<Vec<NotifMessage>> {
    Ok(read_kakao_notifications(&notif_db_path()?)?.messages)
}

/// What the retained notifications say about one chat window's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatIdMatch {
    /// One chat id, backed by this many visible messages.
    Found {
        chat_id: i64,
        evidence: usize,
    },
    Unknown,
    Ambiguous(Vec<i64>),
}

/// A chat window's KakaoTalk chat id, from notifications whose text equals
/// an incoming message the window shows and whose title is that message's
/// sender (group chats) or the window title (1:1 chats). Only one id among
/// all matches counts; none or several stay unknown or ambiguous. Message
/// text can be copied into another chat, so this is a label to show a
/// person, never evidence for approving a send.
pub fn resolve_chat_id(
    window_title: &str,
    visible: &[crate::ax_send::AxMessage],
    retained: &[NotifMessage],
) -> ChatIdMatch {
    let mut ids: Vec<i64> = Vec::new();
    let mut evidence = 0;
    for message in visible
        .iter()
        .filter(|m| !m.outgoing && !m.text.trim().is_empty())
    {
        let matched: Vec<i64> = retained
            .iter()
            .filter(|n| {
                n.body == message.text
                    && (n.chat_name == window_title
                        || message.sender.as_deref() == Some(n.chat_name.as_str()))
            })
            .map(|n| n.room_id)
            .collect();
        if !matched.is_empty() {
            evidence += 1;
        }
        for id in matched {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    match ids.as_slice() {
        [] => ChatIdMatch::Unknown,
        [chat_id] => ChatIdMatch::Found {
            chat_id: *chat_id,
            evidence,
        },
        _ => ChatIdMatch::Ambiguous(ids),
    }
}

/// One decoded Notification Center snapshot plus per-record diagnostics.
/// Malformed records must not poison all later notifications, but they also
/// must not disappear silently because they can signal KakaoTalk schema drift.
struct NotificationSnapshot {
    messages: Vec<NotifMessage>,
    warnings: Vec<String>,
}

/// Read and decode the current KakaoTalk notification snapshot. Non-message
/// KakaoTalk notifications intentionally have no `<room>_<message>` identity
/// and are skipped. Individual malformed records are reported in `warnings`
/// while structurally invalid SQLite queries still fail the whole read.
fn read_kakao_notifications(db_path: &Path) -> Result<NotificationSnapshot> {
    let mut messages = Vec::new();
    let mut warnings = Vec::new();
    for (index, payload) in read_kakao_payloads(db_path)?.into_iter().enumerate() {
        let root = match Value::from_reader(std::io::Cursor::new(&payload)) {
            Ok(root) => root,
            Err(error) => {
                warnings.push(format!(
                    "KakaoTalk notification payload row {index} is malformed: {error}"
                ));
                continue;
            }
        };
        let has_message_identity = root
            .as_dictionary()
            .and_then(|dict| dict.get("req"))
            .and_then(Value::as_dictionary)
            .is_some_and(|req| req.contains_key("iden"));
        if !has_message_identity {
            continue;
        }
        match parse_notification(&payload) {
            Some(message) => messages.push(message),
            None => warnings.push(format!(
                "KakaoTalk notification payload row {index} has an identity but no parseable <room>_<message> id; notification schema may have changed"
            )),
        }
    }
    Ok(NotificationSnapshot { messages, warnings })
}

fn report_schema_warnings(snapshot: &NotificationSnapshot, previous_summary: &mut Option<String>) {
    if snapshot.warnings.is_empty() {
        *previous_summary = None;
        return;
    }
    let summary = format!(
        "{} malformed KakaoTalk notification record(s); continuing with valid records. First error: {}",
        snapshot.warnings.len(),
        snapshot.warnings[0]
    );
    if previous_summary.as_deref() != Some(summary.as_str()) {
        eprintln!("[notif-watch] schema warning: {summary}");
        *previous_summary = Some(summary);
    }
}

/// Whether KakaoTalk is registered as a notifier in an open notification DB.
/// Split out from [`check_access`] so it can be tested against an in-memory DB.
fn kakao_notifier_registered(conn: &Connection) -> rusqlite::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM app WHERE lower(identifier) = ?1",
        [KAKAO_BUNDLE_ID],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Notification-receive readiness, for `doctor`.
pub struct NotifAccess {
    /// Whether the notification DB exists and could be opened read-only.
    pub db_readable: bool,
    /// Whether KakaoTalk is registered as a notifier (needs notifications on).
    pub kakao_registered: bool,
    /// Whether current Kakao payloads and DB columns match the reader.
    pub schema_compatible: bool,
    /// Human-readable summary for the doctor row.
    pub detail: String,
}

/// Probe whether `notif-watch` can work on this machine (read-only, no writes).
pub fn check_access() -> NotifAccess {
    let path = match notif_db_path() {
        Ok(p) => p,
        Err(e) => {
            return NotifAccess {
                db_readable: false,
                kakao_registered: false,
                schema_compatible: false,
                detail: format!("cannot resolve home directory: {e}"),
            }
        }
    };
    if !path.exists() {
        return NotifAccess {
            db_readable: false,
            kakao_registered: false,
            schema_compatible: false,
            detail: format!("notification DB not found at {}", path.display()),
        };
    }
    match open_notif_db(&path) {
        Ok(conn) => {
            let kakao = kakao_notifier_registered(&conn).unwrap_or(false);
            let schema_check = if kakao {
                read_kakao_notifications(&path)
            } else {
                Ok(NotificationSnapshot {
                    messages: Vec::new(),
                    warnings: Vec::new(),
                })
            };
            let schema_compatible = schema_check
                .as_ref()
                .is_ok_and(|snapshot| snapshot.warnings.is_empty());
            NotifAccess {
                db_readable: true,
                kakao_registered: kakao,
                schema_compatible,
                detail: if let Err(error) = &schema_check {
                    format!("notification DB is readable, but its KakaoTalk schema is incompatible: {error}")
                } else if let Some(snapshot) = schema_check
                    .as_ref()
                    .ok()
                    .filter(|snapshot| !snapshot.warnings.is_empty())
                {
                    format!(
                        "notification DB is readable, but {} KakaoTalk payload(s) are malformed; valid records remain readable",
                        snapshot.warnings.len()
                    )
                } else if kakao {
                    "notification DB readable; KakaoTalk registered as notifier".to_string()
                } else {
                    "notification DB readable, but KakaoTalk is not a registered notifier — \
                     enable KakaoTalk notifications in System Settings > Notifications"
                        .to_string()
                },
            }
        }
        Err(e) => NotifAccess {
            db_readable: false,
            kakao_registered: false,
            schema_compatible: false,
            detail: format!("cannot open notification DB read-only: {e}"),
        },
    }
}

/// Poll the Notification Center DB and fire hooks/webhooks on new KakaoTalk
/// messages. Runs until interrupted (Ctrl-C).
pub fn cmd_notif_watch(options: NotifWatchOptions) -> Result<()> {
    if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        require_permission(
            options.unattended && options.allow_side_effects,
            "notif-watch side effects (hooks or webhooks)",
            "Re-run with --unattended --allow-watch-side-effects, or set both in ~/.config/openkakao/config.toml.",
        )?;
    }

    if let Some(url) = &options.webhook_url {
        validate_webhook_url(url, options.allow_insecure_webhooks)?;
    }
    let webhook_headers = options
        .webhook_headers
        .iter()
        .map(|h| parse_webhook_header(h))
        .collect::<Result<Vec<_>>>()?;

    let hook_config = WatchHookConfig {
        command: options.hook_cmd.clone(),
        webhook_url: options.webhook_url.clone(),
        webhook_headers,
        webhook_signing_secret: options.webhook_signing_secret.clone(),
        webhook_format: options.webhook_format,
        chat_ids: vec![],
        chat_names: options.hook_chats.clone(),
        keywords: options.hook_keywords.clone(),
        message_types: vec![],
        fail_fast: options.fail_fast,
        min_hook_interval_secs: options.min_hook_interval_secs,
        min_webhook_interval_secs: options.min_webhook_interval_secs,
        hook_timeout_secs: options.hook_timeout_secs,
        webhook_timeout_secs: options.webhook_timeout_secs,
    };
    let has_sinks = hook_config.command.is_some() || hook_config.webhook_url.is_some();

    let db_path = notif_db_path()?;
    // A 0s interval would busy-loop and hammer the DB; keep at least 1s.
    let interval_secs = options.interval_secs.max(1);
    let durable_inbox = if options.durable {
        Some(ReceiveInbox::open()?)
    } else {
        None
    };
    // Fail once, with context, for permanent startup problems such as a missing
    // DB, Full Disk Access denial, or an incompatible schema. Once a valid
    // baseline exists, transient read failures are retried inside the loop.
    let initial_read = read_kakao_notifications(&db_path);
    let rt = tokio::runtime::Runtime::new()?;
    let mut last_schema_warning = None;
    let initial_messages = match initial_read {
        Ok(snapshot) => {
            report_schema_warnings(&snapshot, &mut last_schema_warning);
            snapshot.messages
        }
        Err(error) => {
            if let Some(inbox) = durable_inbox.as_ref() {
                rt.block_on(
                    NotifSink {
                        hook_config: &hook_config,
                        has_sinks,
                        json: options.json,
                    }
                    .drain(inbox),
                )?;
            }
            return Err(error).context(
                "notif-watch could not read the macOS Notification Center database; \
                 grant Full Disk Access to this terminal and run `openkakao-cli doctor --json`",
            );
        }
    };
    let retained_count = initial_messages.len();
    let replay_retained = options.replay_existing || options.durable;
    let mut tracker = if replay_retained {
        NotificationTracker::replay_existing()
    } else {
        NotificationTracker::default()
    };
    let mut pending_messages = Some(initial_messages);

    rt.block_on(async {
        let sink = NotifSink {
            hook_config: &hook_config,
            has_sinks,
            json: options.json,
        };

        if options.durable {
            eprintln!(
                "[notif-watch] durable inbox enabled; reconciling {retained_count} retained \
                 message(s); polling every {interval_secs}s (Ctrl-C to stop)"
            );
        } else if options.replay_existing {
            eprintln!(
                "[notif-watch] replaying {retained_count} retained message(s); polling every \
                 {interval_secs}s (Ctrl-C to stop)"
            );
        } else {
            eprintln!(
                "[notif-watch] baseline: {retained_count} retained message(s); polling every \
                 {interval_secs}s (Ctrl-C to stop)"
            );
        }

        loop {
            if let Some(messages) = pending_messages.take() {
                let retained_event_ids: HashSet<String> =
                    messages.iter().map(notification_event_id).collect();
                for message in tracker.observe(&messages) {
                    if let Some(inbox) = durable_inbox.as_ref() {
                        let event_id = notification_event_id(message);
                        inbox.capture(&event_id, "notif", message)?;
                    } else {
                        sink.emit(message).await?;
                    }
                }
                if let Some(inbox) = durable_inbox.as_ref() {
                    inbox.reconcile_terminal(
                        "notif",
                        &retained_event_ids,
                        TERMINAL_LEDGER_GRACE_SECS,
                    )?;
                }
            }

            if let Some(inbox) = durable_inbox.as_ref() {
                sink.drain(inbox).await?;
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            match read_kakao_notifications(&db_path) {
                Ok(snapshot) => {
                    report_schema_warnings(&snapshot, &mut last_schema_warning);
                    pending_messages = Some(snapshot.messages);
                }
                Err(e) => eprintln!("[notif-watch] read failed (retrying next poll): {e}"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::{Dictionary, Value};

    fn test_hook_config() -> WatchHookConfig {
        WatchHookConfig {
            command: None,
            webhook_url: None,
            webhook_headers: vec![],
            webhook_signing_secret: None,
            webhook_format: WebhookFormat::Raw,
            chat_ids: vec![],
            chat_names: vec![],
            keywords: vec![],
            message_types: vec![],
            fail_fast: false,
            min_hook_interval_secs: 0,
            min_webhook_interval_secs: 0,
            hook_timeout_secs: 10,
            webhook_timeout_secs: 10,
        }
    }

    fn test_message(room_id: i64, msg_id: i64) -> NotifMessage {
        NotifMessage {
            chat_name: "방".into(),
            body: format!("message-{msg_id}"),
            room_id,
            msg_id,
            ts: 1.0,
            attachment_path: String::new(),
        }
    }

    /// Build a synthetic notification payload matching KakaoTalk's structure,
    /// so tests carry no real message content.
    fn make_payload(
        titl: &str,
        body: Option<&str>,
        iden: &str,
        date: f64,
        attachment: Option<&str>,
    ) -> Vec<u8> {
        let mut req = Dictionary::new();
        req.insert("titl".into(), Value::String(titl.into()));
        req.insert("iden".into(), Value::String(iden.into()));
        if let Some(b) = body {
            req.insert("body".into(), Value::String(b.into()));
        }
        if let Some(p) = attachment {
            let mut atta = Dictionary::new();
            atta.insert("pat".into(), Value::String(p.into()));
            req.insert("atta".into(), Value::Array(vec![Value::Dictionary(atta)]));
        }
        let mut root = Dictionary::new();
        root.insert("app".into(), Value::String("com.kakao.KakaoTalkMac".into()));
        root.insert("date".into(), Value::Real(date));
        root.insert("req".into(), Value::Dictionary(req));

        let mut buf = Vec::new();
        Value::Dictionary(root)
            .to_writer_binary(&mut buf)
            .expect("write bplist");
        buf
    }

    #[test]
    fn chat_ids_need_one_room_among_all_matches() {
        let notif = |room_id, title: &str, body: &str| NotifMessage {
            chat_name: title.to_string(),
            body: body.to_string(),
            room_id,
            msg_id: 1,
            ts: 0.0,
            attachment_path: String::new(),
        };
        let seen = |sender: Option<&str>, text: &str| crate::ax_send::AxMessage {
            sender: sender.map(str::to_string),
            text: text.to_string(),
            ..Default::default()
        };
        let retained = vec![
            notif(7, "귀몬❤️", "Omg sooo many people"),
            notif(9, "친구 | 구자원", "민쥐이이"),
            notif(10, "친구 | 구자원", "ㅋㅋ"),
        ];
        // 1:1: the notification title is the window title.
        assert_eq!(
            resolve_chat_id(
                "귀몬❤️",
                &[seen(Some("귀몬❤️"), "Omg sooo many people")],
                &retained
            ),
            ChatIdMatch::Found {
                chat_id: 7,
                evidence: 1
            }
        );
        // Group: the sender's name; the same sender's other room never counts.
        assert_eq!(
            resolve_chat_id(
                "우리친구",
                &[seen(Some("친구 | 구자원"), "민쥐이이")],
                &retained
            ),
            ChatIdMatch::Found {
                chat_id: 9,
                evidence: 1
            }
        );
        assert_eq!(
            resolve_chat_id(
                "우리친구",
                &[
                    seen(Some("친구 | 구자원"), "민쥐이이"),
                    seen(Some("친구 | 구자원"), "ㅋㅋ")
                ],
                &retained
            ),
            ChatIdMatch::Ambiguous(vec![9, 10])
        );
        assert_eq!(
            resolve_chat_id("정훈", &[seen(None, "다른 말")], &retained),
            ChatIdMatch::Unknown
        );
    }

    #[test]
    fn parses_text_message() {
        let p = make_payload(
            "테스트방",
            Some("안녕하세요"),
            "424242_999001",
            800000000.25,
            None,
        );
        let m = parse_notification(&p).expect("should parse");
        assert_eq!(m.chat_name, "테스트방");
        assert_eq!(m.body, "안녕하세요");
        assert_eq!(m.room_id, 424242);
        assert_eq!(m.msg_id, 999001);
        assert_eq!(m.attachment_path, "");
    }

    #[test]
    fn parses_image_message_with_empty_body_and_attachment() {
        let p = make_payload(
            "테스트방",
            None,
            "424242_999002",
            800000100.75,
            Some("/tmp/photo.jpeg"),
        );
        let m = parse_notification(&p).expect("should parse");
        assert_eq!(m.body, "");
        assert_eq!(m.attachment_path, "/tmp/photo.jpeg");
        let ev = build_event(&m);
        assert_eq!(ev.message, "");
        assert_eq!(ev.attachment, "/tmp/photo.jpeg");
        assert_eq!(ev.message_type, 2);
    }

    #[test]
    fn same_room_shares_room_id_distinct_msg_id() {
        let a =
            parse_notification(&make_payload("방", Some("첫"), "424242_100", 1.0, None)).unwrap();
        let b =
            parse_notification(&make_payload("방", Some("둘"), "424242_200", 2.0, None)).unwrap();
        assert_eq!(a.room_id, b.room_id);
        assert_ne!(a.msg_id, b.msg_id);
    }

    #[test]
    fn rejects_payload_without_parseable_iden() {
        // no iden at all
        let no_iden = {
            let mut req = Dictionary::new();
            req.insert("titl".into(), Value::String("방".into()));
            req.insert("body".into(), Value::String("hi".into()));
            let mut root = Dictionary::new();
            root.insert("req".into(), Value::Dictionary(req));
            let mut buf = Vec::new();
            Value::Dictionary(root).to_writer_binary(&mut buf).unwrap();
            buf
        };
        assert!(parse_notification(&no_iden).is_none());
        // non-numeric iden
        let bad = make_payload("방", Some("hi"), "abc_def", 1.0, None);
        assert!(parse_notification(&bad).is_none());
    }

    #[test]
    fn build_event_maps_iden_and_timestamp() {
        // Fractional CFAbsoluteTime is preserved instead of truncated.
        let m = parse_notification(&make_payload("방", Some("hi"), "42_99", 1.25, None)).unwrap();
        let ev = build_event(&m);
        assert_eq!(ev.chat_id, 42);
        assert_eq!(ev.log_id, 99);
        assert_eq!(ev.method, "notif");
        assert_eq!(ev.event_type, "notif_message");
        assert!(ev.received_at.starts_with("2001-01-01T00:00:01.250"));
    }

    #[test]
    fn absent_or_zero_timestamp_falls_back_to_now_not_2001() {
        let m = parse_notification(&make_payload("방", Some("hi"), "42_99", 0.0, None)).unwrap();
        let ev = build_event(&m);
        // must NOT collapse to the CFAbsolute epoch
        assert!(!ev.received_at.starts_with("2001-01-01"));
    }

    #[test]
    fn detects_kakao_notifier_registration() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE app(app_id INTEGER PRIMARY KEY, identifier TEXT);")
            .unwrap();
        // no kakao yet
        assert!(!kakao_notifier_registered(&conn).unwrap());
        conn.execute(
            "INSERT INTO app(identifier) VALUES ('com.example.kakao-helper')",
            [],
        )
        .unwrap();
        assert!(!kakao_notifier_registered(&conn).unwrap());
        conn.execute(
            "INSERT INTO app(identifier) VALUES ('com.kakao.KakaoTalkMac')",
            [],
        )
        .unwrap();
        assert!(kakao_notifier_registered(&conn).unwrap());
    }

    #[test]
    fn db_reader_filters_exact_bundle_and_orders_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notifications.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app(app_id INTEGER PRIMARY KEY, identifier TEXT); \
             CREATE TABLE record( \
                 rec_id INTEGER PRIMARY KEY, app_id INTEGER, data BLOB, \
                 request_date REAL, request_last_date REAL, delivered_date REAL \
             ); \
             INSERT INTO app(app_id, identifier) \
                 VALUES (1, 'com.kakao.KakaoTalkMac'), \
                        (2, 'com.example.kakao-helper');",
        )
        .unwrap();
        let older = make_payload("방", Some("older"), "10_1", 1.0, None);
        let newer = make_payload("방", Some("newer"), "10_2", 2.0, None);
        let unrelated = make_payload("방", Some("unrelated"), "10_3", 3.0, None);
        conn.execute(
            "INSERT INTO record(rec_id, app_id, data, request_date) VALUES (1, 1, ?1, 2)",
            [&newer],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO record(rec_id, app_id, data, request_date) VALUES (2, 1, ?1, 1)",
            [&older],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO record(rec_id, app_id, data, request_date) VALUES (3, 2, ?1, 0)",
            [&unrelated],
        )
        .unwrap();
        drop(conn);

        let snapshot = read_kakao_notifications(&path).unwrap();
        assert!(snapshot.warnings.is_empty());
        let bodies: Vec<_> = snapshot
            .messages
            .iter()
            .map(|message| message.body.as_str())
            .collect();
        assert_eq!(bodies, vec!["older", "newer"]);
    }

    #[test]
    fn malformed_payload_does_not_block_later_valid_notifications() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notifications.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app(app_id INTEGER PRIMARY KEY, identifier TEXT); \
             CREATE TABLE record( \
                 rec_id INTEGER PRIMARY KEY, app_id INTEGER, data BLOB, \
                 request_date REAL, request_last_date REAL, delivered_date REAL \
             ); \
             INSERT INTO app(app_id, identifier) VALUES (1, 'com.kakao.kakaotalkmac');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO record(rec_id, app_id, data, request_date) VALUES (1, 1, ?1, 1)",
            [vec![0xff, 0x00, 0xff]],
        )
        .unwrap();
        let valid = make_payload("방", Some("later"), "10_2", 2.0, None);
        conn.execute(
            "INSERT INTO record(rec_id, app_id, data, request_date) VALUES (2, 1, ?1, 2)",
            [&valid],
        )
        .unwrap();
        drop(conn);

        let snapshot = read_kakao_notifications(&path).unwrap();
        assert_eq!(snapshot.warnings.len(), 1);
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.messages[0].body, "later");
    }

    #[test]
    fn delivery_lease_covers_sequential_hook_and_webhook_timeouts() {
        let mut config = test_hook_config();
        config.command = Some("hook".to_string());
        config.webhook_url = Some("https://example.invalid/hook".to_string());
        config.hook_timeout_secs = 20;
        config.webhook_timeout_secs = 10;

        assert_eq!(delivery_lease_secs(&config), 90);
    }

    #[test]
    fn db_reader_propagates_row_type_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notifications.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app(app_id INTEGER PRIMARY KEY, identifier TEXT); \
             CREATE TABLE record( \
                 rec_id INTEGER PRIMARY KEY, app_id INTEGER, data BLOB, \
                 request_date REAL, request_last_date REAL, delivered_date REAL \
             ); \
             INSERT INTO app(app_id, identifier) VALUES (1, 'com.kakao.kakaotalkmac'); \
             INSERT INTO record(rec_id, app_id, data, request_date) \
                 VALUES (1, 1, 'not a blob', 1);",
        )
        .unwrap();
        drop(conn);

        assert!(read_kakao_payloads(&path).is_err());
    }

    #[test]
    fn tracker_baselines_then_emits_each_new_key_once() {
        let baseline = vec![NotifMessage {
            chat_name: "방".into(),
            body: "old".into(),
            room_id: 10,
            msg_id: 1,
            ts: 1.0,
            attachment_path: String::new(),
        }];
        let mut next = baseline.clone();
        next.push(NotifMessage {
            body: "new".into(),
            msg_id: 2,
            ..baseline[0].clone()
        });
        // Duplicate DB rows for the same message still produce one event.
        next.push(next[1].clone());

        let mut tracker = NotificationTracker::default();
        assert!(tracker.observe(&baseline).is_empty());
        let fresh = tracker.observe(&next);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].msg_id, 2);
        assert!(tracker.observe(&next).is_empty());
    }

    #[test]
    fn replay_tracker_emits_retained_messages_once() {
        let retained = vec![
            NotifMessage {
                chat_name: "방".into(),
                body: "first".into(),
                room_id: 10,
                msg_id: 1,
                ts: 1.0,
                attachment_path: String::new(),
            },
            NotifMessage {
                chat_name: "방".into(),
                body: "duplicate".into(),
                room_id: 10,
                msg_id: 1,
                ts: 1.0,
                attachment_path: String::new(),
            },
        ];

        let mut tracker = NotificationTracker::replay_existing();
        let replayed = tracker.observe(&retained);
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].body, "first");
        assert!(tracker.observe(&retained).is_empty());
    }

    #[test]
    fn tracker_is_bounded_to_the_current_notification_snapshot() {
        let message = |room_id, msg_id| NotifMessage {
            chat_name: "방".into(),
            body: String::new(),
            room_id,
            msg_id,
            ts: 1.0,
            attachment_path: String::new(),
        };
        let mut tracker = NotificationTracker::default();
        tracker.observe(&[message(10, 1), message(10, 2)]);
        assert_eq!(tracker.previous.len(), 2);

        tracker.observe(&[message(20, 1)]);
        assert_eq!(tracker.previous, HashSet::from([(20, 1)]));
    }

    #[tokio::test]
    async fn durable_drain_acknowledges_every_successful_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = ReceiveInbox::open_at(&dir.path().join("receive.db")).unwrap();
        let first = test_message(10, 1);
        let second = test_message(10, 2);
        inbox
            .capture(&notification_event_id(&first), "notif", &first)
            .unwrap();
        inbox
            .capture(&notification_event_id(&second), "notif", &second)
            .unwrap();

        let hook_config = test_hook_config();
        NotifSink {
            hook_config: &hook_config,
            has_sinks: false,
            json: false,
        }
        .drain(&inbox)
        .await
        .unwrap();

        assert!(inbox.claim_next(60).unwrap().is_none());
    }

    #[test]
    fn durable_drain_stops_only_for_a_matching_throttled_sink() {
        let message = test_message(10, 1);
        let mut hook_config = test_hook_config();
        hook_config.command = Some("true".into());
        hook_config.min_hook_interval_secs = 10;
        let sink = NotifSink {
            hook_config: &hook_config,
            has_sinks: true,
            json: false,
        };

        assert!(sink.uses_throttled_sink(&message));

        hook_config.chat_names = vec!["different room".into()];
        let filtered_sink = NotifSink {
            hook_config: &hook_config,
            has_sinks: true,
            json: false,
        };
        assert!(!filtered_sink.uses_throttled_sink(&message));

        hook_config.chat_names.clear();
        hook_config.min_hook_interval_secs = 0;
        let unthrottled_sink = NotifSink {
            hook_config: &hook_config,
            has_sinks: true,
            json: false,
        };
        assert!(!unthrottled_sink.uses_throttled_sink(&message));
    }
}
