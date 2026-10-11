use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use hmac::{Hmac, Mac};
use owo_colors::OwoColorize;
use rand::Rng;
use serde_json::Value;
use sha2::Sha256;

use crate::error::OpenKakaoError;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::media::{download_media_file, parse_attachment_url, sanitize_filename};
use crate::state::{
    auth_cooldown_remaining_secs, hook_remaining_secs, mark_hook_attempt, mark_webhook_attempt,
    record_failure, record_guard, record_transport_success, webhook_remaining_secs,
};
use crate::util::{
    color_enabled, get_bson_i64, get_bson_str_array, message_type_label, render_message_content,
    require_permission,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookFormat {
    Raw,
    Slack,
    Discord,
}

impl WebhookFormat {
    pub fn from_str_opt(s: Option<&str>) -> Result<Self> {
        match s {
            None | Some("raw") => Ok(Self::Raw),
            Some("slack") => Ok(Self::Slack),
            Some("discord") => Ok(Self::Discord),
            Some(other) => anyhow::bail!(
                "Unknown webhook format '{}'. Expected: raw, slack, discord",
                other
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchHookConfig {
    pub command: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_headers: Vec<(String, String)>,
    pub webhook_signing_secret: Option<String>,
    pub webhook_format: WebhookFormat,
    pub chat_ids: Vec<i64>,
    pub chat_names: Vec<String>,
    pub keywords: Vec<String>,
    pub message_types: Vec<i32>,
    pub fail_fast: bool,
    pub min_hook_interval_secs: u64,
    pub min_webhook_interval_secs: u64,
    pub hook_timeout_secs: u64,
    pub webhook_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub unattended: bool,
    pub allow_side_effects: bool,
    pub filter_chat_id: Option<i64>,
    pub raw: bool,
    pub read_receipt: bool,
    pub max_reconnect: u32,
    pub reconnect_delay_secs: u64,
    pub reconnect_max_delay_secs: u64,
    pub download_media: bool,
    pub download_dir: String,
    pub hook_cmd: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_headers: Vec<String>,
    pub webhook_signing_secret: Option<String>,
    pub hook_chat_ids: Vec<i64>,
    pub hook_keywords: Vec<String>,
    pub hook_types: Vec<i32>,
    pub hook_fail_fast: bool,
    pub min_hook_interval_secs: u64,
    pub min_webhook_interval_secs: u64,
    pub hook_timeout_secs: u64,
    pub webhook_timeout_secs: u64,
    pub allow_insecure_webhooks: bool,
    pub webhook_format: WebhookFormat,
    pub resume: bool,
    pub json: bool,
    pub capture: bool,
}

#[derive(Debug, Clone)]
pub struct WatchMessageEvent {
    pub event_type: &'static str,
    pub received_at: String,
    pub method: String,
    pub chat_id: i64,
    pub chat_name: String,
    pub log_id: i64,
    pub author_id: i64,
    pub author_nickname: String,
    pub message_type: i32,
    pub message: String,
    pub attachment: String,
    pub unread: i32,
}

impl WatchMessageEvent {
    pub fn as_json(&self) -> Value {
        serde_json::json!({
            "event_type": self.event_type,
            "received_at": self.received_at,
            "method": self.method,
            "chat_id": self.chat_id,
            "chat_name": self.chat_name,
            "log_id": self.log_id,
            "author_id": self.author_id,
            "author_nickname": self.author_nickname,
            "message_type": self.message_type,
            "message": self.message,
            "attachment": self.attachment,
            "unread": self.unread,
        })
    }
}

pub fn watch_hook_matches(config: &WatchHookConfig, event: &WatchMessageEvent) -> bool {
    if !config.chat_ids.is_empty() && !config.chat_ids.contains(&event.chat_id) {
        return false;
    }

    if !config.chat_names.is_empty() && !config.chat_names.contains(&event.chat_name) {
        return false;
    }

    if !config.message_types.is_empty() && !config.message_types.contains(&event.message_type) {
        return false;
    }

    if !config.keywords.is_empty() {
        let haystack = event.message.to_lowercase();
        if !config
            .keywords
            .iter()
            .any(|keyword| haystack.contains(&keyword.to_lowercase()))
        {
            return false;
        }
    }

    true
}

pub fn parse_webhook_header(header: &str) -> Result<(String, String)> {
    let (name, value) = header
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid webhook header, expected 'Name: Value'"))?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        anyhow::bail!("invalid webhook header, expected non-empty name and value");
    }
    Ok((name.to_string(), value.to_string()))
}

pub fn build_webhook_signature(secret: &str, timestamp: &str, payload: &[u8]) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to initialize webhook signer: {}", e))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

pub fn validate_webhook_url(webhook_url: &str, allow_insecure_webhooks: bool) -> Result<()> {
    let url = reqwest::Url::parse(webhook_url)
        .map_err(|e| anyhow::anyhow!("invalid webhook URL '{}': {}", webhook_url, e))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if is_loopback_host(host) || allow_insecure_webhooks {
                Ok(())
            } else {
                anyhow::bail!(
                    "refusing insecure webhook URL '{}'; use https or localhost, or opt in via config safety.allow_insecure_webhooks = true",
                    webhook_url
                )
            }
        }
        other => anyhow::bail!(
            "unsupported webhook URL scheme '{}'; use https or localhost http",
            other
        ),
    }
}

/// Execute a local hook command asynchronously using tokio::process::Command
/// with proper async timeout instead of polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkDelivery {
    Delivered,
    RateLimited { remaining_secs: u64 },
}

pub async fn run_watch_command_hook_async(
    config: &WatchHookConfig,
    event: &WatchMessageEvent,
) -> Result<SinkDelivery> {
    let command = config
        .command
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing hook command"))?;
    if let Some(remaining) = hook_remaining_secs(config.min_hook_interval_secs)? {
        record_guard("hook_rate_limited")?;
        eprintln!(
            "[guard/hook] Skipping local hook for chat {} log {}: {}s rate-limit remaining.",
            event.chat_id, event.log_id, remaining
        );
        return Ok(SinkDelivery::RateLimited {
            remaining_secs: remaining,
        });
    }
    mark_hook_attempt()?;
    let payload = serde_json::to_vec_pretty(&event.as_json())?;

    let mut child = tokio::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("OPENKAKAO_EVENT_TYPE", event.event_type)
        .env("OPENKAKAO_CHAT_ID", event.chat_id.to_string())
        .env("OPENKAKAO_CHAT_NAME", &event.chat_name)
        .env("OPENKAKAO_LOG_ID", event.log_id.to_string())
        .env("OPENKAKAO_AUTHOR_ID", event.author_id.to_string())
        .env("OPENKAKAO_AUTHOR_NICKNAME", &event.author_nickname)
        .env("OPENKAKAO_MESSAGE_TYPE", event.message_type.to_string())
        .env(
            "OPENKAKAO_MESSAGE_TYPE_LABEL",
            message_type_label(event.message_type),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        if let Err(error) = stdin.write_all(&payload).await {
            let _ = child.kill().await;
            return Err(anyhow::anyhow!(
                "failed to write the complete event payload to hook stdin: {error}"
            ));
        }
    }

    let timeout = Duration::from_secs(config.hook_timeout_secs.max(1));
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(SinkDelivery::Delivered),
        Ok(Ok(status)) => Err(anyhow::anyhow!(
            "hook command exited with status {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        )),
        Ok(Err(e)) => Err(anyhow::anyhow!("hook command failed: {}", e)),
        Err(_) => {
            let _ = child.kill().await;
            Err(anyhow::anyhow!(
                "hook command timed out after {}s",
                config.hook_timeout_secs
            ))
        }
    }
}

pub fn run_watch_webhook(
    config: &WatchHookConfig,
    event: &WatchMessageEvent,
) -> Result<SinkDelivery> {
    let webhook_url = config
        .webhook_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing webhook url"))?;
    if let Some(remaining) = webhook_remaining_secs(config.min_webhook_interval_secs)? {
        record_guard("webhook_rate_limited")?;
        eprintln!(
            "[guard/webhook] Skipping webhook for chat {} log {}: {}s rate-limit remaining.",
            event.chat_id, event.log_id, remaining
        );
        return Ok(SinkDelivery::RateLimited {
            remaining_secs: remaining,
        });
    }
    mark_webhook_attempt()?;
    let payload_json = match &config.webhook_format {
        WebhookFormat::Slack => {
            serde_json::json!({
                "text": format!(
                    "*[{}]* {}: {}",
                    event.chat_name, event.author_nickname, event.message
                ),
                "blocks": [
                    {
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!(
                                "*{}* in _{}_\n{}",
                                event.author_nickname, event.chat_name, event.message
                            )
                        }
                    },
                    {
                        "type": "context",
                        "elements": [
                            {
                                "type": "mrkdwn",
                                "text": format!(
                                    "chat:{} | log:{} | type:{}",
                                    event.chat_id, event.log_id,
                                    message_type_label(event.message_type)
                                )
                            }
                        ]
                    }
                ]
            })
        }
        WebhookFormat::Discord => {
            serde_json::json!({
                "content": format!(
                    "**[{}]** {}: {}",
                    event.chat_name, event.author_nickname, event.message
                ),
                "embeds": [
                    {
                        "title": format!("Message in {}", event.chat_name),
                        "description": event.message,
                        "color": 16764229,
                        "fields": [
                            {"name": "Author", "value": &event.author_nickname, "inline": true},
                            {"name": "Type", "value": message_type_label(event.message_type), "inline": true},
                            {"name": "Chat ID", "value": event.chat_id.to_string(), "inline": true}
                        ],
                        "timestamp": &event.received_at
                    }
                ]
            })
        }
        WebhookFormat::Raw => event.as_json(),
    };
    let payload = serde_json::to_vec(&payload_json)?;
    let client = build_webhook_client(config.webhook_timeout_secs)?;
    let mut request = client
        .post(webhook_url)
        .header("Content-Type", "application/json");

    for (name, value) in &config.webhook_headers {
        request = request.header(name, value);
    }

    if let Some(secret) = &config.webhook_signing_secret {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = build_webhook_signature(secret, &timestamp, &payload)?;
        request = request
            .header("X-OpenKakao-Timestamp", &timestamp)
            .header("X-OpenKakao-Signature", signature);
    }

    let response = request.body(payload).send()?;
    if response.status().is_success() {
        Ok(SinkDelivery::Delivered)
    } else {
        Err(anyhow::anyhow!(
            "webhook returned non-success status {}",
            response.status()
        ))
    }
}

fn build_webhook_client(timeout_secs: u64) -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(1)))
        // Validation applies to the configured destination only. Following a
        // redirect could leak the payload or caller-supplied headers to an
        // unvalidated host/scheme, so webhook redirects are fail-closed.
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

fn reconnect_delay(attempt: u32, initial_secs: u64, max_secs: u64) -> Duration {
    let base_secs = std::cmp::min(
        initial_secs.saturating_mul(2u64.pow(attempt.saturating_sub(1))),
        max_secs,
    );
    let jitter = if base_secs > 0 {
        rand::thread_rng().gen_range(0..=base_secs / 2)
    } else {
        0
    };
    Duration::from_secs(base_secs + jitter)
}

fn watch_state_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve home directory"))?;
    Ok(home
        .join(".config")
        .join("openkakao")
        .join("watch_state.json"))
}

fn save_watch_state(last_log_ids: &HashMap<i64, i64>) -> Result<()> {
    let path = watch_state_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(last_log_ids)?;
    std::fs::write(&path, json)?;
    Ok(())
}

fn load_watch_state() -> Result<HashMap<i64, i64>> {
    let path = watch_state_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = std::fs::read_to_string(&path)?;
    let map: HashMap<i64, i64> = serde_json::from_str(&data)?;
    Ok(map)
}

struct WatchContext<'a> {
    chat_names: &'a HashMap<i64, String>,
    options: &'a WatchOptions,
    hook_config: &'a Option<WatchHookConfig>,
    last_log_ids: &'a mut HashMap<i64, i64>,
    message_db: Option<&'a crate::message_db::MessageDb>,
}

async fn handle_msg_packet(
    packet: &crate::loco::packet::LocoPacket,
    ctx: &mut WatchContext<'_>,
    client: &mut crate::loco::client::LocoClient,
) -> Result<()> {
    let messages = match crate::loco::message::received_messages(packet) {
        Ok(messages) => messages,
        Err(err) => {
            eprintln!("[watch] Ignoring malformed MSG: {err}");
            return Ok(());
        }
    };
    let Some(message) = messages.first() else {
        return Ok(());
    };
    let chat_id = message.chat_id;
    if let Some(filter) = ctx.options.filter_chat_id {
        if chat_id != filter {
            return Ok(());
        }
    }
    let chat_label = ctx
        .chat_names
        .get(&chat_id)
        .cloned()
        .unwrap_or_else(|| chat_id.to_string());
    let nick = message.nickname.to_string();
    let msg_type = message.message_type;
    let content = render_message_content(message.body, msg_type);
    let log_id = message.log_id;
    let author_id = message.author_id;
    let attachment = message.attachment.to_string();
    let event = WatchMessageEvent {
        event_type: "message",
        received_at: chrono::Utc::now().to_rfc3339(),
        method: packet.method.clone(),
        chat_id,
        chat_name: chat_label.clone(),
        log_id,
        author_id,
        author_nickname: nick.clone(),
        message_type: msg_type,
        message: content.clone(),
        attachment: attachment.clone(),
        unread: 0,
    };

    if ctx.options.json {
        println!(
            "{}",
            serde_json::to_string(&event.as_json()).unwrap_or_default()
        );
    } else {
        let now = chrono::Local::now().format("%H:%M:%S");
        if color_enabled() {
            println!(
                "{} {} {}: {}",
                format!("[{}]", now).dimmed(),
                format!("[{}]", chat_label).cyan(),
                nick.bold(),
                content
            );
        } else {
            println!("[{}] [{}] {}: {}", now, chat_label, nick, content);
        }
    }

    if log_id > 0 {
        ctx.last_log_ids
            .entry(chat_id)
            .and_modify(|current| *current = (*current).max(log_id))
            .or_insert(log_id);
    }

    // Cache message to local SQLite DB
    if let Some(db) = &ctx.message_db {
        if log_id > 0 {
            let send_at = message.send_at;
            let cached = crate::message_db::CachedMessage {
                chat_id,
                log_id,
                author_id,
                author_name: nick.clone(),
                message_type: msg_type,
                message: content.clone(),
                attachment: attachment.clone(),
                send_at,
            };
            if let Err(e) = db.upsert_messages(std::slice::from_ref(&cached)) {
                eprintln!("[watch] Cache write failed: {}", e);
            }
        }
    }

    if let Some(config) = ctx.hook_config {
        if watch_hook_matches(config, &event) {
            if config.command.is_some() {
                match run_watch_command_hook_async(config, &event).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[watch] Hook failed: {}", e);
                        if config.fail_fast {
                            return Err(e);
                        }
                    }
                }
            }
            if config.webhook_url.is_some() {
                match tokio::task::spawn_blocking({
                    let config = config.clone();
                    let event = event.clone();
                    move || run_watch_webhook(&config, &event)
                })
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        eprintln!("[watch] Webhook failed: {}", e);
                        if config.fail_fast {
                            return Err(e);
                        }
                    }
                    Err(e) => {
                        let err = anyhow::anyhow!("webhook task join error: {}", e);
                        eprintln!("[watch] Webhook failed: {}", err);
                        if config.fail_fast {
                            return Err(err);
                        }
                    }
                }
            }
        }
    }

    if ctx.options.read_receipt && log_id > 0 {
        let _ = client
            .send_packet(
                "NOTIREAD",
                bson::doc! {
                    "chatId": chat_id,
                    "watermark": log_id,
                },
            )
            .await;
    }

    if ctx.options.download_media
        && matches!(msg_type, 2 | 3 | 12 | 14 | 26 | 27)
        && !attachment.is_empty()
    {
        let dl_creds = client.credentials.clone();
        let dl_dir = ctx.options.download_dir.clone();
        tokio::task::spawn_blocking(move || {
            if let Some((url, filename)) = parse_attachment_url(&attachment, msg_type) {
                let dir = Path::new(&dl_dir).join(chat_id.to_string());
                let save_name = format!("{}_{}", log_id, sanitize_filename(&filename));
                let save_path = dir.join(&save_name);
                match download_media_file(&dl_creds, &url, &save_path) {
                    Ok(bytes) => {
                        eprintln!(
                            "[watch] Downloaded {} ({} bytes)",
                            save_path.display(),
                            bytes
                        );
                    }
                    Err(e) => {
                        eprintln!("[watch] Download failed for {}: {}", save_name, e);
                    }
                }
            }
        });
    }

    Ok(())
}

async fn handle_syncmsg_packet(
    packet: &crate::loco::packet::LocoPacket,
    ctx: &mut WatchContext<'_>,
) -> Result<()> {
    let messages = match crate::loco::message::received_messages(packet) {
        Ok(messages) => messages,
        Err(err) => {
            eprintln!("[watch] Ignoring malformed SYNCMSG: {err}");
            return Ok(());
        }
    };
    for message in &messages {
        handle_sync_message(message, ctx)?;
    }
    Ok(())
}

fn handle_sync_message(
    message: &crate::loco::message::ReceivedMessage<'_>,
    ctx: &mut WatchContext<'_>,
) -> Result<()> {
    let chat_id = message.chat_id;
    if let Some(filter) = ctx.options.filter_chat_id {
        if chat_id != filter {
            return Ok(());
        }
    }
    let chat_label = ctx
        .chat_names
        .get(&chat_id)
        .cloned()
        .unwrap_or_else(|| format!("{}", chat_id));
    let log_id = message.log_id;
    let msg_type = message.message_type;
    let content = render_message_content(message.body, msg_type);
    let nick = message.nickname.to_string();

    if ctx.options.json {
        let sync_event = serde_json::json!({
            "event_type": "sync",
            "received_at": chrono::Utc::now().to_rfc3339(),
            "method": "SYNCMSG",
            "chat_id": chat_id,
            "chat_name": chat_label,
            "log_id": log_id,
            "author_id": message.author_id,
            "attachment": message.attachment,
            "author_nickname": nick,
            "message_type": msg_type,
            "message": content,
        });
        println!("{}", serde_json::to_string(&sync_event).unwrap_or_default());
    } else {
        let now = chrono::Local::now().format("%H:%M:%S");
        if color_enabled() {
            println!(
                "{} {} {} {}: {}",
                format!("[{}]", now).dimmed(),
                "[sync]".dimmed(),
                format!("[{}]", chat_label).cyan(),
                nick.bold(),
                content
            );
        } else {
            println!("[{}] [sync] [{}] {}: {}", now, chat_label, nick, content);
        }
    }

    if log_id > 0 {
        ctx.last_log_ids
            .entry(chat_id)
            .and_modify(|current| *current = (*current).max(log_id))
            .or_insert(log_id);
    }

    // Cache SYNCMSG to local SQLite DB
    if let Some(db) = &ctx.message_db {
        if log_id > 0 {
            let author_id = message.author_id;
            let send_at = message.send_at;
            let attachment = message.attachment.to_string();
            let cached = crate::message_db::CachedMessage {
                chat_id,
                log_id,
                author_id,
                author_name: nick,
                message_type: msg_type,
                message: content,
                attachment,
                send_at,
            };
            if let Err(e) = db.upsert_messages(std::slice::from_ref(&cached)) {
                eprintln!("[watch] Cache write failed: {}", e);
            }
        }
    }

    Ok(())
}

async fn handle_syncdlmsg_packet(
    packet: &crate::loco::packet::LocoPacket,
    ctx: &mut WatchContext<'_>,
) -> Result<()> {
    let chat_id = get_bson_i64(&packet.body, &["chatId"]);
    if let Some(filter) = ctx.options.filter_chat_id {
        if chat_id != filter {
            return Ok(());
        }
    }
    let chat_label = ctx
        .chat_names
        .get(&chat_id)
        .cloned()
        .unwrap_or_else(|| format!("{}", chat_id));
    let log_id = get_bson_i64(&packet.body, &["logId"]);

    let event = WatchMessageEvent {
        event_type: "delete",
        received_at: chrono::Utc::now().to_rfc3339(),
        method: "SYNCDLMSG".to_string(),
        chat_id,
        chat_name: chat_label.clone(),
        log_id,
        author_id: 0,
        author_nickname: String::new(),
        message_type: 0,
        message: String::new(),
        attachment: String::new(),
        unread: 0,
    };

    if ctx.options.json {
        let delete_event = serde_json::json!({
            "event": "delete",
            "chat_id": chat_id,
            "log_id": log_id,
            "chat_name": chat_label,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        println!(
            "{}",
            serde_json::to_string(&delete_event).unwrap_or_default()
        );
    } else {
        let now = chrono::Local::now().format("%H:%M:%S");
        if color_enabled() {
            println!(
                "{} {} Chat {}: message {} deleted",
                format!("[{}]", now).dimmed(),
                "[deleted]".dimmed(),
                chat_label.cyan(),
                log_id
            );
        } else {
            println!(
                "[{}] [deleted] Chat {}: message {} deleted",
                now, chat_label, log_id
            );
        }
    }

    if let Some(config) = ctx.hook_config {
        if watch_hook_matches(config, &event) {
            if config.command.is_some() {
                match run_watch_command_hook_async(config, &event).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[watch] Hook failed: {}", e);
                        if config.fail_fast {
                            return Err(e);
                        }
                    }
                }
            }
            if config.webhook_url.is_some() {
                match tokio::task::spawn_blocking({
                    let config = config.clone();
                    let event = event.clone();
                    move || run_watch_webhook(&config, &event)
                })
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => {
                        eprintln!("[watch] Webhook failed: {}", e);
                        if config.fail_fast {
                            return Err(e);
                        }
                    }
                    Err(e) => {
                        let err = anyhow::anyhow!("webhook task join error: {}", e);
                        eprintln!("[watch] Webhook failed: {}", err);
                        if config.fail_fast {
                            return Err(err);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_syncaction_packet(
    packet: &crate::loco::packet::LocoPacket,
    ctx: &mut WatchContext<'_>,
) -> Result<()> {
    let chat_id = get_bson_i64(&packet.body, &["chatId"]);
    if let Some(filter) = ctx.options.filter_chat_id {
        if chat_id != filter {
            return Ok(());
        }
    }
    let chat_label = ctx
        .chat_names
        .get(&chat_id)
        .cloned()
        .unwrap_or_else(|| format!("{}", chat_id));
    let user_id = get_bson_i64(&packet.body, &["userId"]);
    let log_id = get_bson_i64(&packet.body, &["logId"]);
    let action_type = get_bson_i64(&packet.body, &["type"]) as i32;

    if ctx.options.json {
        let react_event = serde_json::json!({
            "event": "reaction",
            "chat_id": chat_id,
            "log_id": log_id,
            "user_id": user_id,
            "reaction_type": action_type,
            "chat_name": chat_label,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        println!(
            "{}",
            serde_json::to_string(&react_event).unwrap_or_default()
        );
    } else {
        let now = chrono::Local::now().format("%H:%M:%S");
        if color_enabled() {
            println!(
                "{} {} Chat {}: user {} reacted (type={}) to message {}",
                format!("[{}]", now).dimmed(),
                "[reaction]".dimmed(),
                chat_label.cyan(),
                user_id,
                action_type,
                log_id
            );
        } else {
            println!(
                "[{}] [reaction] Chat {}: user {} reacted (type={}) to message {}",
                now, chat_label, user_id, action_type, log_id
            );
        }
    }

    Ok(())
}

async fn handle_syncrewr_packet(
    packet: &crate::loco::packet::LocoPacket,
    ctx: &mut WatchContext<'_>,
) -> Result<()> {
    let chat_id = get_bson_i64(&packet.body, &["chatId"]);
    if let Some(filter) = ctx.options.filter_chat_id {
        if chat_id != filter {
            return Ok(());
        }
    }
    let chat_label = ctx
        .chat_names
        .get(&chat_id)
        .cloned()
        .unwrap_or_else(|| format!("{}", chat_id));
    let log_id = get_bson_i64(&packet.body, &["logId"]);

    if ctx.options.json {
        let edit_event = serde_json::json!({
            "event": "edit",
            "chat_id": chat_id,
            "log_id": log_id,
            "chat_name": chat_label,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        println!("{}", serde_json::to_string(&edit_event).unwrap_or_default());
    } else {
        let now = chrono::Local::now().format("%H:%M:%S");
        if color_enabled() {
            println!(
                "{} {} Chat {}: message {} edited",
                format!("[{}]", now).dimmed(),
                "[edited]".dimmed(),
                chat_label.cyan(),
                log_id
            );
        } else {
            println!(
                "[{}] [edited] Chat {}: message {} edited",
                now, chat_label, log_id
            );
        }
    }

    Ok(())
}

fn handle_unknown_push_packet(packet: &crate::loco::packet::LocoPacket, options: &WatchOptions) {
    if options.json {
        let body_json = serde_json::to_value(&packet.body).unwrap_or(serde_json::Value::Null);
        let capture_event = serde_json::json!({
            "event": "unknown_push",
            "method": packet.method,
            "status": packet.status(),
            "body": body_json,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        println!(
            "{}",
            serde_json::to_string(&capture_event).unwrap_or_default()
        );
    } else {
        // capture is true but json is false — human-readable capture output
        let now = chrono::Local::now().format("%H:%M:%S");
        let body_json = serde_json::to_value(&packet.body).unwrap_or(serde_json::Value::Null);
        if color_enabled() {
            println!(
                "{} {} {} (status={}) body: {}",
                format!("[{}]", now).dimmed(),
                "[capture]".dimmed(),
                packet.method.bold(),
                packet.status(),
                body_json
            );
        } else {
            println!(
                "[{}] [capture] {} (status={}) body: {}",
                now,
                packet.method,
                packet.status(),
                body_json
            );
        }
    }
}

pub fn cmd_watch(options: WatchOptions) -> Result<()> {
    if options.read_receipt || options.hook_cmd.is_some() || options.webhook_url.is_some() {
        require_permission(
            options.unattended && options.allow_side_effects,
            "watch side effects (read receipts, hooks, or webhooks)",
            "Re-run with --unattended --allow-watch-side-effects, or set both in ~/.config/openkakao/config.toml.",
        )?;
    }

    if let Some(webhook_url) = &options.webhook_url {
        validate_webhook_url(webhook_url, options.allow_insecure_webhooks)?;
    }

    let creds = crate::util::get_creds()?;
    let parsed_webhook_headers = options
        .webhook_headers
        .iter()
        .map(|header| parse_webhook_header(header))
        .collect::<Result<Vec<_>>>()?;
    let hook_config = if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        Some(WatchHookConfig {
            command: options.hook_cmd.clone(),
            webhook_url: options.webhook_url.clone(),
            webhook_headers: parsed_webhook_headers,
            webhook_signing_secret: options.webhook_signing_secret.clone(),
            webhook_format: options.webhook_format.clone(),
            chat_ids: options.hook_chat_ids.clone(),
            chat_names: vec![],
            keywords: options.hook_keywords.clone(),
            message_types: options.hook_types.clone(),
            fail_fast: options.hook_fail_fast,
            min_hook_interval_secs: options.min_hook_interval_secs,
            min_webhook_interval_secs: options.min_webhook_interval_secs,
            hook_timeout_secs: options.hook_timeout_secs,
            webhook_timeout_secs: options.webhook_timeout_secs,
        })
    } else {
        None
    };

    // Load resume state if requested
    let mut last_log_ids: HashMap<i64, i64> = if options.resume {
        match load_watch_state() {
            Ok(state) if !state.is_empty() => {
                eprintln!(
                    "[watch] Resuming with {} chat cursors from previous session",
                    state.len()
                );
                state
            }
            _ => HashMap::new(),
        }
    } else {
        HashMap::new()
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds);
        let mut reconnect_count: u32 = 0;

        // Open local message cache for persisting watched messages
        let watch_message_db = match crate::message_db::MessageDb::open() {
            Ok(db) => Some(db),
            Err(e) => {
                eprintln!("[watch] Warning: could not open message cache: {}", e);
                None
            }
        };

        'reconnect: loop {
            let login_data = match loco_connect_with_auto_refresh(&mut client).await {
                Ok(data) => data,
                Err(e) => {
                    let err_msg = e.to_string();
                    let is_auth_error = err_msg.contains("cooling down")
                        || err_msg.contains("-950")
                        || err_msg.contains("-999");
                    let is_retryable = e.downcast_ref::<OpenKakaoError>()
                        .map(|oke| oke.is_retryable())
                        .unwrap_or(false);

                    if is_auth_error {
                        record_failure("auth_relogin_needed")?;
                        let delay = auth_cooldown_remaining_secs()?.unwrap_or(30);
                        eprintln!(
                            "[watch] Auth recovery not ready: {}. Retrying in {}s...",
                            e, delay
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        client.disconnect();
                        continue 'reconnect;
                    }
                    record_failure("network")?;
                    if !is_retryable && options.max_reconnect == 0 {
                        return Err(e);
                    }
                    if options.max_reconnect == 0 || reconnect_count >= options.max_reconnect {
                        return Err(e);
                    }
                    reconnect_count += 1;
                    let delay = reconnect_delay(reconnect_count, options.reconnect_delay_secs, options.reconnect_max_delay_secs);
                    if options.json {
                        println!("{}", serde_json::json!({"type": "reconnecting", "attempt": reconnect_count, "delay_secs": delay.as_secs(), "reason": e.to_string()}));
                    } else {
                        eprintln!(
                            "[watch] Connect failed: {}. Reconnecting in {:.0}s ({}/{})...",
                            e, delay.as_secs_f64(), reconnect_count, options.max_reconnect
                        );
                    }
                    tokio::time::sleep(delay).await;
                    client.disconnect();
                    continue 'reconnect;
                }
            };

            // Build chat_id → name map from LOGINLIST chatDatas
            let mut chat_names: HashMap<i64, String> = HashMap::new();
            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                for cd in chat_datas {
                    if let Some(doc) = cd.as_document() {
                        let cid = get_bson_i64(doc, &["c", "chatId"]);
                        if cid != 0 {
                            let name = doc
                                .get_document("chatInfo")
                                .ok()
                                .and_then(|ci| ci.get_str("name").ok())
                                .map(String::from)
                                .unwrap_or_default();
                            let name = if name.is_empty() {
                                get_bson_str_array(doc, &["k"]).join(", ")
                            } else {
                                name
                            };
                            if !name.is_empty() {
                                chat_names.insert(cid, name);
                            }
                        }
                    }
                }
            }

            let chat_count = chat_names.len();
            if reconnect_count > 0 {
                eprintln!(
                    "[watch] Reconnected! ({} chats loaded)",
                    chat_count
                );
            } else {
                eprintln!(
                    "[watch] Connected! Listening for messages... ({} chats loaded)",
                    chat_count
                );
                if let Some(cid) = options.filter_chat_id {
                    eprintln!("[watch] Filtering chat_id={}", cid);
                }
                if let Some(config) = &hook_config {
                    if let Some(command) = &config.command {
                        eprintln!("[watch] Hook command enabled: {}", command);
                    }
                    if let Some(webhook_url) = &config.webhook_url {
                        eprintln!("[watch] Webhook enabled: {}", webhook_url);
                    }
                }
                eprintln!("[watch] Press Ctrl-C to stop.");
            }
            reconnect_count = 0;
            record_transport_success("watch")?;

            let mut ping_interval =
                tokio::time::interval(std::time::Duration::from_secs(60));
            ping_interval.tick().await;

            loop {
                tokio::select! {
                    packet_result = client.recv_packet() => {
                        match packet_result {
                            Ok(packet) => {
                                let method = &packet.method;

                                if method == "CHANGESVR" {
                                    eprintln!("[watch] Server requested reconnect (CHANGESVR)");
                                    client.disconnect();
                                    continue 'reconnect;
                                }

                                if options.raw {
                                    let now = chrono::Local::now().format("%H:%M:%S");
                                    println!("[{}] {} {:?}", now, method, packet.body);
                                    continue;
                                }

                                let mut watch_ctx = WatchContext {
                                    chat_names: &chat_names,
                                    options: &options,
                                    hook_config: &hook_config,
                                    last_log_ids: &mut last_log_ids,
                                    message_db: watch_message_db.as_ref(),
                                };
                                match method.as_str() {
                                    "MSG" => {
                                        handle_msg_packet(&packet, &mut watch_ctx, &mut client).await?;
                                    }
                                    "SYNCMSG" => {
                                        handle_syncmsg_packet(&packet, &mut watch_ctx).await?;
                                    }
                                    "SYNCDLMSG" => {
                                        handle_syncdlmsg_packet(&packet, &mut watch_ctx).await?;
                                    }
                                    "SYNCREWR" => {
                                        handle_syncrewr_packet(&packet, &mut watch_ctx).await?;
                                    }
                                    "SYNCACTION" => {
                                        handle_syncaction_packet(&packet, &mut watch_ctx).await?;
                                    }
                                    "DECUNREAD" | "NOTIREAD" | "SYNCLINKCR" | "SYNCLINKUP" => {
                                        // Known push events, silently ignore
                                    }
                                    _ => {
                                        if options.capture || options.json {
                                            handle_unknown_push_packet(&packet, &options);
                                        } else {
                                            eprintln!("[watch] Push: {} (status={})", method, packet.status());
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let err_msg = e.to_string();
                                let is_auth = err_msg.contains("-950") || err_msg.contains("-999");
                                if is_auth {
                                    record_failure("auth_relogin_needed")?;
                                    let delay = auth_cooldown_remaining_secs()?.unwrap_or(30);
                                    eprintln!(
                                        "[watch] Auth error: {}. Retrying in {}s...",
                                        e, delay
                                    );
                                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                                    client.disconnect();
                                    continue 'reconnect;
                                }

                                let is_retryable = e.downcast_ref::<OpenKakaoError>()
                                    .map(|oke| oke.is_retryable())
                                    .unwrap_or(false);

                                record_failure("network")?;
                                if options.max_reconnect == 0 && !is_retryable {
                                    eprintln!("[watch] Connection lost: {}", e);
                                    return Err(e);
                                }
                                if options.max_reconnect == 0 {
                                    eprintln!("[watch] Connection lost: {}", e);
                                    return Err(e);
                                }
                                reconnect_count += 1;
                                if reconnect_count > options.max_reconnect {
                                    eprintln!(
                                        "[watch] Connection lost after {} reconnect attempts: {}",
                                        options.max_reconnect, e
                                    );
                                    return Err(e);
                                }
                                let delay = reconnect_delay(reconnect_count, options.reconnect_delay_secs, options.reconnect_max_delay_secs);
                                if options.json {
                                    println!("{}", serde_json::json!({"type": "reconnecting", "attempt": reconnect_count, "delay_secs": delay.as_secs(), "reason": e.to_string()}));
                                } else {
                                    eprintln!(
                                        "[watch] Connection lost: {}. Reconnecting in {:.0}s ({}/{})...",
                                        e, delay.as_secs_f64(), reconnect_count, options.max_reconnect
                                    );
                                }
                                tokio::time::sleep(delay).await;
                                client.disconnect();
                                continue 'reconnect;
                            }
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(e) = client.send_packet("PING", bson::doc! {}).await {
                            record_failure("network")?;
                            eprintln!("[watch] PING failed: {}", e);
                            if options.max_reconnect == 0 {
                                return Err(anyhow::anyhow!("PING failed: {}", e));
                            }
                            reconnect_count += 1;
                            if reconnect_count > options.max_reconnect {
                                return Err(anyhow::anyhow!(
                                    "PING failed after {} reconnects: {}", options.max_reconnect, e
                                ));
                            }
                            let delay = reconnect_delay(reconnect_count, options.reconnect_delay_secs, options.reconnect_max_delay_secs);
                            if options.json {
                                println!("{}", serde_json::json!({"type": "reconnecting", "attempt": reconnect_count, "delay_secs": delay.as_secs(), "reason": format!("PING failed: {}", e)}));
                            } else {
                                eprintln!(
                                    "[watch] PING failed: {}. Reconnecting in {:.0}s ({}/{})...",
                                    e, delay.as_secs_f64(), reconnect_count, options.max_reconnect
                                );
                            }
                            tokio::time::sleep(delay).await;
                            client.disconnect();
                            continue 'reconnect;
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        eprintln!("\n[watch] Shutting down...");
                        client.disconnect_graceful().await;
                        // Persist last_log_ids for resume
                        if !last_log_ids.is_empty() {
                            if let Err(e) = save_watch_state(&last_log_ids) {
                                eprintln!("[watch] Failed to save resume state: {}", e);
                            } else {
                                eprintln!("[watch] Saved resume state ({} chats). Use --resume to continue.", last_log_ids.len());
                            }
                        }
                        return Ok(());
                    }
                }
            }
        }
    })
}

/// Emit one local-watch event to every configured sink, then apply the shared
/// emit→hook→webhook→fail_fast policy used by `ax-watch` and `notif-watch`.
///
/// - `json`: print the event as NDJSON to stdout; otherwise write `human_line`
///   (already formatted by the caller) to stderr.
/// - `has_sinks`: whether a command hook or webhook is configured.
/// - `source_label`: prefixes error lines, e.g. `[ax-watch] hook failed: …`.
///
/// Returns `Err` only when a sink fails **and** `fail_fast` is set. Otherwise
/// failures are reported to callers so durable receivers can defer their
/// acknowledgement while ordinary watch loops keep running.
#[derive(Debug, Default)]
pub struct DispatchReport {
    pub sink_errors: Vec<String>,
}

impl DispatchReport {
    pub fn is_success(&self) -> bool {
        self.sink_errors.is_empty()
    }

    pub fn error_summary(&self) -> String {
        self.sink_errors.join("; ")
    }
}

pub async fn dispatch_event(
    event: &WatchMessageEvent,
    hook_config: &WatchHookConfig,
    has_sinks: bool,
    json: bool,
    human_line: &str,
    source_label: &str,
) -> Result<DispatchReport> {
    let mut report = DispatchReport::default();
    if json {
        println!("{}", event.as_json());
    } else {
        eprintln!("{human_line}");
    }

    if has_sinks && watch_hook_matches(hook_config, event) {
        if hook_config.command.is_some() {
            match run_watch_command_hook_async(hook_config, event).await {
                Ok(SinkDelivery::Delivered) => {}
                Ok(SinkDelivery::RateLimited { remaining_secs }) => {
                    report
                        .sink_errors
                        .push(format!("hook rate-limited for {remaining_secs}s"));
                }
                Err(e) => {
                    eprintln!("[{source_label}] hook failed: {e}");
                    if hook_config.fail_fast {
                        return Err(e);
                    }
                    report.sink_errors.push(format!("hook: {e}"));
                }
            }
        }
        if hook_config.webhook_url.is_some() {
            let cfg = hook_config.clone();
            let ev = event.clone();
            match tokio::task::spawn_blocking(move || run_watch_webhook(&cfg, &ev))
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
            {
                Ok(SinkDelivery::Delivered) => {}
                Ok(SinkDelivery::RateLimited { remaining_secs }) => {
                    report
                        .sink_errors
                        .push(format!("webhook rate-limited for {remaining_secs}s"));
                }
                Err(e) => {
                    eprintln!("[{source_label}] webhook failed: {e}");
                    if hook_config.fail_fast {
                        return Err(e);
                    }
                    report.sink_errors.push(format!("webhook: {e}"));
                }
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn message_test_options() -> WatchOptions {
        WatchOptions {
            unattended: false,
            allow_side_effects: false,
            filter_chat_id: Some(101),
            raw: false,
            read_receipt: false,
            max_reconnect: 0,
            reconnect_delay_secs: 1,
            reconnect_max_delay_secs: 1,
            download_media: false,
            download_dir: String::new(),
            hook_cmd: None,
            webhook_url: None,
            webhook_headers: vec![],
            webhook_signing_secret: None,
            hook_chat_ids: vec![],
            hook_keywords: vec![],
            hook_types: vec![],
            hook_fail_fast: false,
            min_hook_interval_secs: 0,
            min_webhook_interval_secs: 0,
            hook_timeout_secs: 1,
            webhook_timeout_secs: 1,
            allow_insecure_webhooks: false,
            webhook_format: WebhookFormat::Raw,
            resume: false,
            json: true,
            capture: false,
        }
    }

    fn test_chat_log(chat_id: i64, log_id: i64) -> bson::Document {
        bson::doc! {
            "chatId": chat_id, "logId": log_id, "authorId": 202_i64,
            "type": 1, "message": "nested text", "sendAt": 1788760000,
            "attachment": "{\"example\":true}",
        }
    }

    #[tokio::test]
    async fn nested_msg_reaches_cache_and_preserves_resume_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::message_db::MessageDb::open_at(&dir.path().join("cache.db")).unwrap();
        let options = message_test_options();
        let names = HashMap::from([(101, "Example chat".to_string())]);
        let mut cursors = HashMap::from([(101, 500)]);
        let mut ctx = WatchContext {
            chat_names: &names,
            options: &options,
            hook_config: &None,
            last_log_ids: &mut cursors,
            message_db: Some(&db),
        };
        // No socket, saved credentials or external service is used by this test.
        let creds = crate::model::KakaoCredentials::new(
            "fake".into(),
            1,
            "fake".into(),
            "26.7.0".into(),
            "fake".into(),
            "fake".into(),
        );
        let mut client = crate::loco::client::LocoClient::new(creds);
        let builder = crate::loco::packet::PacketBuilder::new();
        let packet = builder.build(
            "MSG",
            bson::doc! {
                "chatId": 101_i64, "authorNickname": "Tester", "chatLog": test_chat_log(101, 400),
            },
        );
        handle_msg_packet(&packet, &mut ctx, &mut client)
            .await
            .unwrap();
        let cached = db.get_messages(101, 10).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].log_id, 400);
        assert_eq!(cached[0].author_id, 202);
        assert_eq!(cached[0].author_name, "Tester");
        assert_eq!(cached[0].message, "nested text");
        assert_eq!(cached[0].attachment, "{\"example\":true}");
        assert_eq!(cached[0].send_at, 1788760000);
        assert_eq!(ctx.last_log_ids[&101], 500);

        // Filtering uses the decoded chat identity, including nested-only envelopes.
        let filtered = builder.build("MSG", bson::doc! {"chatLog": test_chat_log(303, 700)});
        handle_msg_packet(&filtered, &mut ctx, &mut client)
            .await
            .unwrap();
        assert!(db.get_messages(303, 10).unwrap().is_empty());
        assert!(!ctx.last_log_ids.contains_key(&303));

        let malformed = builder.build("MSG", bson::doc! {"chatId": 101_i64, "chatLog": {}});
        handle_msg_packet(&malformed, &mut ctx, &mut client)
            .await
            .unwrap();
        assert_eq!(db.get_messages(101, 10).unwrap().len(), 1);
        assert_eq!(ctx.last_log_ids[&101], 500);
        let newer = builder.build("MSG", bson::doc! {"chatLog": test_chat_log(101, 600)});
        handle_msg_packet(&newer, &mut ctx, &mut client)
            .await
            .unwrap();
        assert_eq!(ctx.last_log_ids[&101], 600);
    }

    #[tokio::test]
    async fn sync_batch_caches_each_matching_log_without_regressing_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::message_db::MessageDb::open_at(&dir.path().join("cache.db")).unwrap();
        let options = message_test_options();
        let names = HashMap::new();
        let mut cursors = HashMap::new();
        let mut ctx = WatchContext {
            chat_names: &names,
            options: &options,
            hook_config: &None,
            last_log_ids: &mut cursors,
            message_db: Some(&db),
        };
        let builder = crate::loco::packet::PacketBuilder::new();
        let packet = builder.build("SYNCMSG", bson::doc! {
            "isOK": true,
            "chatLogs": [test_chat_log(101, 600), test_chat_log(303, 700), test_chat_log(101, 400)],
        });
        handle_syncmsg_packet(&packet, &mut ctx).await.unwrap();
        assert_eq!(db.get_messages(101, 10).unwrap().len(), 2);
        assert!(db.get_messages(303, 10).unwrap().is_empty());
        assert_eq!(*ctx.last_log_ids, HashMap::from([(101, 600)]));

        // A valid first entry cannot partially update state when a later entry is invalid.
        let bad = builder.build(
            "SYNCMSG",
            bson::doc! {"chatLogs": [test_chat_log(101, 800), bson::doc! {}]},
        );
        handle_syncmsg_packet(&bad, &mut ctx).await.unwrap();
        let empty = builder.build("SYNCMSG", bson::doc! {"isOK": true, "chatLogs": []});
        handle_syncmsg_packet(&empty, &mut ctx).await.unwrap();
        assert_eq!(db.get_messages(101, 10).unwrap().len(), 2);
        assert_eq!(*ctx.last_log_ids, HashMap::from([(101, 600)]));
    }

    fn default_test_hook_config() -> WatchHookConfig {
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

    fn default_test_event() -> WatchMessageEvent {
        WatchMessageEvent {
            event_type: "test",
            received_at: String::new(),
            method: "ax".to_string(),
            chat_id: 0,
            chat_name: String::new(),
            log_id: 0,
            author_id: 0,
            author_nickname: String::new(),
            message_type: 1,
            message: String::new(),
            attachment: String::new(),
            unread: 0,
        }
    }

    #[test]
    fn chat_names_filter_matches_exact_name() {
        let mut config = default_test_hook_config();
        config.chat_names = vec!["정훈".to_string()];
        let mut event = default_test_event();
        event.chat_name = "정훈".to_string();
        assert!(watch_hook_matches(&config, &event));
        event.chat_name = "누군가".to_string();
        assert!(!watch_hook_matches(&config, &event));
    }

    #[test]
    fn empty_chat_names_filter_passes_everything() {
        let config = default_test_hook_config(); // chat_names empty
        let mut event = default_test_event();
        event.chat_name = "아무개".to_string();
        assert!(watch_hook_matches(&config, &event));
    }

    #[test]
    fn webhook_client_does_not_follow_redirects() {
        let redirect_target = TcpListener::bind("127.0.0.1:0").unwrap();
        redirect_target.set_nonblocking(true).unwrap();
        let target_url = format!("http://{}/leak", redirect_target.local_addr().unwrap());

        let redirect_server = TcpListener::bind("127.0.0.1:0").unwrap();
        let redirect_url = format!("http://{}/hook", redirect_server.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = redirect_server.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {target_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });

        let response = build_webhook_client(1)
            .unwrap()
            .post(redirect_url)
            .body("sensitive payload")
            .send()
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        server.join().unwrap();

        thread::sleep(Duration::from_millis(50));
        let error = redirect_target.accept().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    }
}
