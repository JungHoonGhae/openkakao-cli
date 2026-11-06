use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rand::RngCore;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
pub struct ProposeSend<'a> {
    pub chat_name: &'a str,
    pub message: &'a str,
    pub reply_to: Option<(i64, i64)>,
    pub idempotency_key: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SendState {
    Proposed,
    Sending,
    Sent,
    Uncertain,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SendIntent {
    pub intent_id: String,
    pub chat_name: String,
    pub message: String,
    pub reply_to: Option<(i64, i64)>,
    pub state: SendState,
    pub approval_code: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeSendLimits {
    pub min_interval_secs: u64,
    pub max_per_chat_per_hour: u32,
    pub max_global_per_hour: u32,
    pub max_global_per_day: u32,
    pub sending_lease_secs: u64,
    pub proposal_ttl_secs: u64,
}

impl Default for SafeSendLimits {
    fn default() -> Self {
        Self {
            min_interval_secs: 10,
            max_per_chat_per_hour: 3,
            max_global_per_hour: 10,
            max_global_per_day: 20,
            sending_lease_secs: 5 * 60,
            proposal_ttl_secs: 15 * 60,
        }
    }
}

pub struct SafeSendOutbox {
    conn: Connection,
    limits: SafeSendLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportOutcome {
    /// A new message was observed in the exact target after commit.
    Verified,
    /// The transport failed before the commit action, so no message was sent.
    NotSent { reason: String },
    /// The commit action may have happened, but delivery could not be proven.
    Uncertain { reason: String },
}

pub trait SendTransport {
    fn send(&mut self, chat_name: &str, message: &str) -> TransportOutcome;
}

impl SafeSendOutbox {
    pub fn open() -> Result<Self> {
        Self::open_at(&outbox_path()?)
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        Self::open_at_with_limits(path, SafeSendLimits::default())
    }

    pub fn open_at_with_limits(path: &Path, limits: SafeSendLimits) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create safe-send directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open safe-send outbox {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("secure safe-send outbox {}", path.display()))?;
        }

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS safe_send_outbox (
                 intent_id        TEXT PRIMARY KEY,
                 idempotency_key  TEXT NOT NULL UNIQUE,
                 chat_name        TEXT NOT NULL,
                 message          TEXT NOT NULL,
                 reply_chat_id    INTEGER,
                 reply_log_id     INTEGER,
                 state            TEXT NOT NULL,
                 nonce            TEXT NOT NULL,
                 created_at       INTEGER NOT NULL,
                 expires_at       INTEGER NOT NULL,
                 claimed_at       INTEGER,
                 last_error       TEXT,
                 sent_at          INTEGER
             );
             CREATE INDEX IF NOT EXISTS idx_safe_send_outbox_state
                 ON safe_send_outbox(state, created_at, intent_id);
             CREATE INDEX IF NOT EXISTS idx_safe_send_outbox_claimed_at
                 ON safe_send_outbox(claimed_at);
             CREATE INDEX IF NOT EXISTS idx_safe_send_outbox_chat_claimed_at
                 ON safe_send_outbox(chat_name, claimed_at);",
        )?;
        let stale_before = chrono::Utc::now()
            .timestamp()
            .saturating_sub(limits.sending_lease_secs.min(i64::MAX as u64) as i64);
        conn.execute(
            "UPDATE safe_send_outbox
             SET state = 'uncertain',
                 last_error = COALESCE(last_error, 'execution interrupted after send claim')
             WHERE state = 'sending' AND (claimed_at IS NULL OR claimed_at <= ?1)",
            [stale_before],
        )?;
        conn.execute(
            "UPDATE safe_send_outbox
             SET state = 'expired', last_error = 'approval window expired'
             WHERE state = 'proposed' AND expires_at <= ?1",
            [chrono::Utc::now().timestamp()],
        )?;
        Ok(Self { conn, limits })
    }

    pub fn propose(&self, request: ProposeSend<'_>) -> Result<SendIntent> {
        if request.chat_name.trim().is_empty() {
            anyhow::bail!("safe-send chat name must not be blank");
        }
        if request.message.trim().is_empty() {
            anyhow::bail!("safe-send message must not be blank");
        }
        if request.chat_name.chars().any(is_display_spoofing_control) {
            anyhow::bail!(
                "safe-send chat name contains a control or bidirectional formatting character"
            );
        }
        if request.message.chars().any(|character| {
            is_display_spoofing_control(character) && !matches!(character, '\n' | '\t')
        }) {
            anyhow::bail!(
                "safe-send message contains an unsafe control character or bidirectional formatting character"
            );
        }

        let intent_id = random_hex(16);
        let nonce = random_hex(16);
        let idempotency_key = request
            .idempotency_key
            .map(str::to_owned)
            .unwrap_or_else(|| format!("manual:{intent_id}"));
        let created_at = chrono::Utc::now().timestamp();
        let expires_at =
            created_at.saturating_add(self.limits.proposal_ttl_secs.min(i64::MAX as u64) as i64);
        let (reply_chat_id, reply_log_id) =
            request.reply_to.map_or((None, None), |(chat_id, log_id)| {
                (Some(chat_id), Some(log_id))
            });

        self.conn.execute(
            "INSERT OR IGNORE INTO safe_send_outbox
             (intent_id, idempotency_key, chat_name, message, reply_chat_id,
              reply_log_id, state, nonce, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'proposed', ?7, ?8, ?9)",
            params![
                intent_id,
                idempotency_key,
                request.chat_name,
                request.message,
                reply_chat_id,
                reply_log_id,
                nonce,
                created_at,
                expires_at,
            ],
        )?;

        let stored = self.find_by_idempotency_key(&idempotency_key)?;
        if stored.chat_name != request.chat_name
            || stored.message != request.message
            || stored.reply_to != request.reply_to
        {
            anyhow::bail!(
                "safe-send idempotency key already belongs to different content: {}",
                idempotency_key
            );
        }
        Ok(stored)
    }

    pub fn list_active(&self) -> Result<Vec<SendIntent>> {
        let mut stmt = self.conn.prepare(
            "SELECT intent_id, chat_name, message, reply_chat_id, reply_log_id,
                    state, nonce, created_at, expires_at, last_error
             FROM safe_send_outbox
             WHERE state IN ('proposed', 'sending', 'uncertain')
             ORDER BY created_at, intent_id",
        )?;
        let rows = stmt.query_map([], decode_intent_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get(&self, intent_id: &str) -> Result<SendIntent> {
        self.find_by_intent_id(intent_id)
    }

    pub fn approve_and_send(
        &mut self,
        intent_id: &str,
        approval_code: &str,
        transport: &mut impl SendTransport,
    ) -> Result<SendIntent> {
        let mut intent = self.find_by_intent_id(intent_id)?;
        if intent.state != SendState::Proposed {
            anyhow::bail!(
                "safe-send intent {} is {}, not proposed",
                intent_id,
                state_name(intent.state)
            );
        }
        if !constant_time_eq(approval_code.as_bytes(), intent.approval_code.as_bytes()) {
            anyhow::bail!("safe-send approval code does not match the proposed content");
        }
        if chrono::Utc::now().timestamp() >= intent.expires_at {
            self.conn.execute(
                "UPDATE safe_send_outbox
                 SET state = 'expired', last_error = 'approval window expired'
                 WHERE intent_id = ?1 AND state = 'proposed'",
                [intent_id],
            )?;
            anyhow::bail!("safe-send proposal has expired; create and review a new proposal");
        }

        let now = chrono::Utc::now().timestamp();
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        enforce_limits(&transaction, self.limits, &intent.chat_name, now)?;
        let claimed = transaction.execute(
            "UPDATE safe_send_outbox
             SET state = 'sending', claimed_at = ?2
             WHERE intent_id = ?1 AND state = 'proposed'",
            params![intent_id, now],
        )?;
        if claimed != 1 {
            anyhow::bail!("safe-send intent was already claimed; refusing a duplicate send");
        }
        transaction.commit()?;

        match transport.send(&intent.chat_name, &intent.message) {
            TransportOutcome::Verified => {}
            TransportOutcome::NotSent { reason } => {
                self.conn.execute(
                    "UPDATE safe_send_outbox
                     SET state = 'proposed', claimed_at = NULL, last_error = ?2
                     WHERE intent_id = ?1 AND state = 'sending'",
                    params![intent_id, reason],
                )?;
                anyhow::bail!(
                    "safe-send message was not sent; the proposal remains available for review"
                );
            }
            TransportOutcome::Uncertain { reason } => {
                self.conn.execute(
                    "UPDATE safe_send_outbox
                     SET state = 'uncertain', last_error = ?2
                     WHERE intent_id = ?1 AND state = 'sending'",
                    params![intent_id, reason],
                )?;
                return Err(crate::error::coded(
                    crate::error::ErrorCode::DeliveryUncertain,
                    "safe-send outcome is uncertain; inspect KakaoTalk and do not retry automatically",
                ));
            }
        }

        let completed = self.conn.execute(
            "UPDATE safe_send_outbox
             SET state = 'sent', sent_at = ?2, last_error = NULL
             WHERE intent_id = ?1 AND state = 'sending'",
            params![intent_id, chrono::Utc::now().timestamp()],
        )?;
        if completed != 1 {
            anyhow::bail!(
                "safe-send completion state changed unexpectedly; inspect KakaoTalk before any new proposal"
            );
        }
        intent.state = SendState::Sent;
        Ok(intent)
    }

    pub fn cancel(&self, intent_id: &str) -> Result<SendIntent> {
        let cancelled = self.conn.execute(
            "UPDATE safe_send_outbox
             SET state = 'cancelled',
                 last_error = COALESCE(last_error, 'cancelled by operator')
             WHERE intent_id = ?1 AND state IN ('proposed', 'uncertain')",
            [intent_id],
        )?;
        if cancelled != 1 {
            anyhow::bail!(
                "safe-send intent is not cancellable; only proposed or uncertain intents can be cancelled"
            );
        }
        self.find_by_intent_id(intent_id)
    }

    fn find_by_idempotency_key(&self, key: &str) -> Result<SendIntent> {
        let intent = self.conn.query_row(
            "SELECT intent_id, chat_name, message, reply_chat_id, reply_log_id,
                    state, nonce, created_at, expires_at, last_error
             FROM safe_send_outbox
             WHERE idempotency_key = ?1",
            [key],
            decode_intent_row,
        )?;
        Ok(intent)
    }

    fn find_by_intent_id(&self, intent_id: &str) -> Result<SendIntent> {
        let intent = self.conn.query_row(
            "SELECT intent_id, chat_name, message, reply_chat_id, reply_log_id,
                    state, nonce, created_at, expires_at, last_error
             FROM safe_send_outbox
             WHERE intent_id = ?1",
            [intent_id],
            decode_intent_row,
        )?;
        Ok(intent)
    }
}

fn enforce_limits(
    transaction: &Transaction<'_>,
    limits: SafeSendLimits,
    chat_name: &str,
    now: i64,
) -> Result<()> {
    let last_claimed_at: Option<i64> = transaction.query_row(
        "SELECT max(claimed_at) FROM safe_send_outbox WHERE claimed_at IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if let Some(last_claimed_at) = last_claimed_at {
        let available_at = last_claimed_at.saturating_add(limits.min_interval_secs as i64);
        if now < available_at {
            anyhow::bail!(
                "safe-send global cooldown active for {}s",
                available_at - now
            );
        }
    }

    let per_chat_hour: u32 = transaction.query_row(
        "SELECT count(*) FROM safe_send_outbox
         WHERE chat_name = ?1 AND claimed_at >= ?2",
        params![chat_name, now - 60 * 60],
        |row| row.get(0),
    )?;
    if per_chat_hour >= limits.max_per_chat_per_hour {
        anyhow::bail!(
            "safe-send per-chat hourly budget exhausted ({}/{})",
            per_chat_hour,
            limits.max_per_chat_per_hour
        );
    }

    let global_hour: u32 = transaction.query_row(
        "SELECT count(*) FROM safe_send_outbox WHERE claimed_at >= ?1",
        [now - 60 * 60],
        |row| row.get(0),
    )?;
    if global_hour >= limits.max_global_per_hour {
        anyhow::bail!(
            "safe-send global hourly budget exhausted ({}/{})",
            global_hour,
            limits.max_global_per_hour
        );
    }

    let global_day: u32 = transaction.query_row(
        "SELECT count(*) FROM safe_send_outbox WHERE claimed_at >= ?1",
        [now - 24 * 60 * 60],
        |row| row.get(0),
    )?;
    if global_day >= limits.max_global_per_day {
        anyhow::bail!(
            "safe-send global daily budget exhausted ({}/{})",
            global_day,
            limits.max_global_per_day
        );
    }
    Ok(())
}

fn decode_intent_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SendIntent> {
    let intent_id: String = row.get(0)?;
    let chat_name: String = row.get(1)?;
    let message: String = row.get(2)?;
    let reply_chat_id: Option<i64> = row.get(3)?;
    let reply_log_id: Option<i64> = row.get(4)?;
    let state: String = row.get(5)?;
    let nonce: String = row.get(6)?;
    let created_at: i64 = row.get(7)?;
    let expires_at: i64 = row.get(8)?;
    let last_error: Option<String> = row.get(9)?;

    let state = match state.as_str() {
        "proposed" => SendState::Proposed,
        "sending" => SendState::Sending,
        "sent" => SendState::Sent,
        "uncertain" => SendState::Uncertain,
        "cancelled" => SendState::Cancelled,
        "expired" => SendState::Expired,
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                format!("unknown safe-send state: {other}").into(),
            ))
        }
    };
    let reply_to = match (reply_chat_id, reply_log_id) {
        (Some(chat_id), Some(log_id)) => Some((chat_id, log_id)),
        _ => None,
    };
    let approval_code = approval_code(
        &intent_id, &chat_name, &message, reply_to, created_at, expires_at, &nonce,
    );

    Ok(SendIntent {
        intent_id,
        chat_name,
        message,
        reply_to,
        state,
        approval_code,
        created_at,
        expires_at,
        last_error,
    })
}

fn state_name(state: SendState) -> &'static str {
    match state {
        SendState::Proposed => "proposed",
        SendState::Sending => "sending",
        SendState::Sent => "sent",
        SendState::Uncertain => "uncertain",
        SendState::Cancelled => "cancelled",
        SendState::Expired => "expired",
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn is_display_spoofing_control(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

fn approval_code(
    intent_id: &str,
    chat_name: &str,
    message: &str,
    reply_to: Option<(i64, i64)>,
    created_at: i64,
    expires_at: i64,
    nonce: &str,
) -> String {
    let canonical = serde_json::to_vec(&(
        intent_id, chat_name, message, reply_to, created_at, expires_at, nonce,
    ))
    .expect("safe-send approval fields are serializable");
    let digest = Sha256::digest(canonical);
    hex::encode(digest)[..12].to_string()
}

fn random_hex(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::thread_rng().fill_bytes(&mut value);
    hex::encode(value)
}

fn outbox_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory for safe-send outbox")?;
    Ok(home
        .join(".config")
        .join("openkakao")
        .join("safe_send_outbox.db"))
}
