use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use chrono::{Datelike, Local, TimeZone};
use owo_colors::OwoColorize;

use crate::model::ChatMember;

pub static NO_COLOR: AtomicBool = AtomicBool::new(false);

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SEND_PREFIX: &str = "🤖 [Sent via openkakao]";

pub fn color_enabled() -> bool {
    !NO_COLOR.load(Ordering::Relaxed)
}

pub fn debug_enabled() -> bool {
    std::env::var("OPENKAKAO_CLI_DEBUG").is_ok() || std::env::var("OPENKAKAO_RS_DEBUG").is_ok()
}

pub fn format_outgoing_message(message: &str, no_prefix: bool) -> String {
    if no_prefix {
        message.to_string()
    } else {
        format!("{} {}", SEND_PREFIX, message)
    }
}

pub fn print_section_title(title: &str) {
    if color_enabled() {
        println!("{}", title.bold().cyan());
    } else {
        println!("{}", title);
    }
}

pub fn message_type_label(message_type: i32) -> &'static str {
    match message_type {
        1 => "text",
        2 => "photo",
        3 => "video",
        5 => "contact",
        12 => "voice",
        14 => "emoticon",
        16 => "live",
        18 => "search",
        22 => "map",
        23 => "profile",
        26 => "file",
        27 => "multi-photo",
        71 | 72 => "poll",
        _ => "unknown",
    }
}

pub fn render_message_content(body: &bson::Document, msg_type: i32) -> String {
    let attachment_str = body.get_str("attachment").unwrap_or("");
    let attachment: Option<serde_json::Value> = if attachment_str.is_empty() {
        None
    } else {
        serde_json::from_str(attachment_str).ok()
    };

    match msg_type {
        1 => body
            .get_str("message")
            .or_else(|_| body.get_str("msg"))
            .unwrap_or("")
            .to_string(),
        2 => render_photo_content(&attachment),
        3 => render_video_content(&attachment),
        5 => "연락처를 보냈습니다.".to_string(),
        12 => "음성메시지를 보냈습니다.".to_string(),
        14 => "이모티콘을 보냈습니다.".to_string(),
        16 => "라이브톡".to_string(),
        18 => "샵검색을 보냈습니다.".to_string(),
        22 => "지도를 보냈습니다.".to_string(),
        23 => "프로필을 보냈습니다.".to_string(),
        26 => render_file_content(&attachment),
        27 => render_multi_photo_content(&attachment),
        71 | 72 => "투표를 보냈습니다.".to_string(),
        _ => body
            .get_str("message")
            .or_else(|_| body.get_str("msg"))
            .map(String::from)
            .unwrap_or_else(|_| format!("[type={}]", msg_type)),
    }
}

fn render_photo_content(attachment: &Option<serde_json::Value>) -> String {
    if let Some(att) = attachment {
        let w = att.get("w").and_then(|v| v.as_u64()).unwrap_or(0);
        let h = att.get("h").and_then(|v| v.as_u64()).unwrap_or(0);
        let size = att.get("s").and_then(|v| v.as_u64()).unwrap_or(0);
        if w > 0 && h > 0 {
            return format!("사진 ({}x{}, {})", w, h, format_bytes(size));
        }
    }
    "사진을 보냈습니다.".to_string()
}

fn render_video_content(attachment: &Option<serde_json::Value>) -> String {
    if let Some(att) = attachment {
        let duration = att.get("d").and_then(|v| v.as_u64()).unwrap_or(0);
        if duration > 0 {
            let mins = duration / 60;
            let secs = duration % 60;
            return format!("동영상 ({}:{:02})", mins, secs);
        }
    }
    "동영상을 보냈습니다.".to_string()
}

fn render_file_content(attachment: &Option<serde_json::Value>) -> String {
    if let Some(att) = attachment {
        let name = att.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let size = att.get("s").and_then(|v| v.as_u64()).unwrap_or(0);
        if !name.is_empty() {
            return format!("파일: {} ({})", name, format_bytes(size));
        }
    }
    "파일을 보냈습니다.".to_string()
}

fn render_multi_photo_content(attachment: &Option<serde_json::Value>) -> String {
    if let Some(att) = attachment {
        let count = att
            .get("kl")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        if count > 0 {
            return format!("사진 {}장", count);
        }
    }
    "멀티사진을 보냈습니다.".to_string()
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0B".to_string();
    }
    if bytes < 1024 {
        return format!("{}B", bytes);
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1}KB", bytes as f64 / 1024.0);
    }
    format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
}

pub fn type_label(kind: &str) -> &'static str {
    match kind {
        "DirectChat" => "DM",
        "MultiChat" => "Group",
        "MemoChat" => "Memo",
        "OpenDirectChat" => "OpenDM",
        "OpenMultiChat" => "OpenGroup",
        _ => "Unknown",
    }
}

pub fn is_open_chat(chat_type: &str) -> bool {
    matches!(chat_type, "OpenDirectChat" | "OpenMultiChat")
}

pub fn extract_chat_type(room_info: &bson::Document) -> String {
    room_info
        .get_document("chatInfo")
        .ok()
        .and_then(|ci| ci.get_str("type").ok())
        .or_else(|| room_info.get_str("t").ok())
        .unwrap_or("Unknown")
        .to_string()
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated = s.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

/// Escape characters that can forge terminal/log output while preserving
/// ordinary Unicode text. Structured JSON and webhook payloads should keep the
/// original value; this is only for human-readable terminal rendering.
pub fn escape_terminal_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '\u{061c}'
                        | '\u{200e}'
                        | '\u{200f}'
                        | '\u{202a}'..='\u{202e}'
                        | '\u{2066}'..='\u{2069}'
                )
            {
                character.escape_unicode().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

pub fn parse_since_date(since: Option<&str>) -> Result<Option<i64>> {
    let Some(s) = since else { return Ok(None) };
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Invalid --since date '{}'. Expected YYYY-MM-DD.", s))?;
    let dt = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
    let local_dt = Local
        .from_local_datetime(&dt)
        .single()
        .ok_or_else(|| anyhow::anyhow!("Ambiguous local time for {}", s))?;
    Ok(Some(local_dt.timestamp()))
}

pub fn format_time(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }

    let Some(dt) = Local.timestamp_opt(epoch, 0).single() else {
        return String::new();
    };

    let now = Local::now();
    if dt.date_naive() == now.date_naive() {
        return dt.format("%H:%M").to_string();
    }

    if dt.year() == now.year() {
        return dt.format("%m/%d %H:%M").to_string();
    }

    dt.format("%Y/%m/%d").to_string()
}

pub fn build_member_name_map_from_bson(members: &[bson::Bson]) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    for m in members {
        if let Some(doc) = m.as_document() {
            let uid = get_bson_i64(doc, &["userId"]);
            let nick = get_bson_str(doc, &["nickName", "nickname"]);
            if uid > 0 && !nick.is_empty() {
                map.insert(uid, nick);
            }
        }
    }
    map
}

pub fn member_name_map(members: &[ChatMember], my_user_id: i64) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    for m in members {
        out.insert(m.user_id, m.display_name());
    }
    out.insert(my_user_id, "Me".to_string());
    out
}

pub fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in &rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= widths.len() {
                widths.push(cell.chars().count());
            } else {
                widths[idx] = widths[idx].max(cell.chars().count());
            }
        }
    }

    if color_enabled() {
        let header_line = headers
            .iter()
            .enumerate()
            .map(|(idx, h)| format!("{:width$}", h.bold(), width = widths[idx]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{header_line}");
    } else {
        let header_line = headers
            .iter()
            .enumerate()
            .map(|(idx, h)| format!("{:width$}", h, width = widths[idx]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{header_line}");
    }

    let separator = widths
        .iter()
        .map(|w| "-".repeat(*w))
        .collect::<Vec<_>>()
        .join("  ");
    if color_enabled() {
        println!("{}", separator.dimmed());
    } else {
        println!("{separator}");
    }

    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(idx, cell)| format!("{:width$}", cell, width = widths[idx]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{line}");
    }
}

pub fn confirm() -> Result<bool> {
    use std::io::{self, Write};
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

pub fn require_permission(enabled: bool, purpose: &str, hint: &str) -> Result<()> {
    if enabled {
        return Ok(());
    }

    anyhow::bail!("{} requires explicit opt-in. {}", purpose, hint)
}

pub fn validate_outbound_message(message: &str) -> Result<()> {
    if message.trim().is_empty() {
        anyhow::bail!("refusing to send an empty or whitespace-only message");
    }
    Ok(())
}

pub fn print_loco_error_hint(status: i64) {
    match status {
        -950 => {
            eprintln!("  Error: Authentication rejected (-950).");
            eprintln!("  Likely causes:");
            eprintln!("    1. Token expired: open KakaoTalk, browse chats, then 'openkakao-cli login --save'.");
            eprintln!("    2. Session conflict: another client may have invalidated this session.");
            eprintln!("  Will attempt auto-refresh if possible.");
        }
        -999 => {
            eprintln!("  Error: Upgrade required (-999).");
            eprintln!(
                "  The app version string is too old. Update KakaoTalk and re-extract credentials."
            );
        }
        -400 => {
            eprintln!("  Error: Bad request (-400). Missing required parameter.");
        }
        -300 => {
            eprintln!("  Error: Unsupported request or device mismatch (-300).");
            eprintln!(
                "  This method/body combination is likely not valid for the macOS LOCO surface."
            );
        }
        -203 => {
            eprintln!("  Error: Missing required parameter (-203).");
            eprintln!(
                "  This LOCO method likely exists, but the required body shape is incomplete."
            );
        }
        -301 => {
            eprintln!("  Error: Account restricted (-301). Your account may be under review.");
            eprintln!("  WARNING: Do not retry aggressively. Wait and check KakaoTalk app.");
        }
        -1 => {
            eprintln!("  Error: Connection failed or no status in response.");
            eprintln!("  Run 'openkakao-cli doctor --loco' to check connectivity.");
        }
        _ => {
            eprintln!(
                "  Unknown LOCO error (status={}). Run 'openkakao-cli doctor' for diagnostics.",
                status
            );
        }
    }
}

pub fn parse_loco_status_from_error(message: &str) -> Option<i64> {
    let lower = message.to_lowercase();
    if let Some(pos) = lower.find("status=") {
        let rest = &lower[pos + 7..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '-')
            .unwrap_or(rest.len());
        rest[..end].parse::<i64>().ok()
    } else {
        None
    }
}

// --- BSON helpers ---

pub fn get_bson_i64(doc: &bson::Document, keys: &[&str]) -> i64 {
    for k in keys {
        if let Ok(v) = doc.get_i64(k) {
            return v;
        }
        if let Ok(v) = doc.get_i32(k) {
            return v as i64;
        }
    }
    0
}

pub fn get_bson_i32(doc: &bson::Document, keys: &[&str]) -> i32 {
    for k in keys {
        if let Ok(v) = doc.get_i32(k) {
            return v;
        }
        if let Ok(v) = doc.get_i64(k) {
            return v as i32;
        }
    }
    0
}

pub fn get_bson_bool(doc: &bson::Document, keys: &[&str]) -> bool {
    for k in keys {
        if let Ok(v) = doc.get_bool(k) {
            return v;
        }
    }
    false
}

pub fn get_bson_str(doc: &bson::Document, keys: &[&str]) -> String {
    for k in keys {
        if let Ok(v) = doc.get_str(k) {
            return v.to_string();
        }
    }
    String::new()
}

pub fn get_bson_i64_array(doc: &bson::Document, keys: &[&str]) -> Vec<i64> {
    for k in keys {
        if let Ok(arr) = doc.get_array(k) {
            return arr
                .iter()
                .filter_map(|v| v.as_i64().or_else(|| v.as_i32().map(|n| n as i64)))
                .collect();
        }
    }
    Vec::new()
}

pub fn get_bson_i32_array(doc: &bson::Document, keys: &[&str]) -> Vec<i32> {
    for k in keys {
        if let Ok(arr) = doc.get_array(k) {
            return arr
                .iter()
                .filter_map(|v| v.as_i32().or_else(|| v.as_i64().map(|n| n as i32)))
                .collect();
        }
    }
    Vec::new()
}

pub fn get_bson_str_array(doc: &bson::Document, keys: &[&str]) -> Vec<String> {
    for k in keys {
        if let Ok(arr) = doc.get_array(k) {
            return arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
    }
    Vec::new()
}

/// Mask a token for safe debug output: first 8 chars + "..." + last 4 chars.
/// Tokens shorter than 16 chars show first 4 + "..." + last 2.
pub fn mask_token(token: &str) -> String {
    let len = token.len();
    if len <= 8 {
        return "*".repeat(len);
    }
    if len < 16 {
        format!("{}...{}", &token[..4], &token[len - 2..])
    } else {
        format!("{}...{}", &token[..8], &token[len - 4..])
    }
}

pub fn get_rest_client() -> Result<crate::rest::KakaoRestClient> {
    crate::auth_flow::get_rest_ready_client()
}

pub fn output_json<T: serde::Serialize>(data: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(data)?);
    Ok(())
}

pub fn get_creds() -> Result<crate::model::KakaoCredentials> {
    crate::auth_flow::resolve_base_credentials()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn message_key_takes_precedence_even_when_empty_or_type_unknown() {
        for msg_type in [1, 999] {
            let body = doc! {"message": "", "msg": "must not leak into empty text"};
            assert_eq!(render_message_content(&body, msg_type), "");
            let body = doc! {"message": "future content", "msg": "legacy"};
            assert_eq!(render_message_content(&body, msg_type), "future content");
            let legacy = doc! {"msg": "legacy text"};
            assert_eq!(render_message_content(&legacy, msg_type), "legacy text");
        }
    }

    #[test]
    fn test_mask_token_short() {
        assert_eq!(mask_token("abc"), "***");
        assert_eq!(mask_token("12345678"), "********");
    }

    #[test]
    fn test_mask_token_medium() {
        assert_eq!(mask_token("123456789"), "1234...89");
        assert_eq!(mask_token("abcdefghijklmno"), "abcd...no");
    }

    #[test]
    fn test_mask_token_long() {
        assert_eq!(mask_token("abcdefghijklmnop"), "abcdefgh...mnop");
        let token = "a".repeat(65);
        assert_eq!(mask_token(&token), "aaaaaaaa...aaaa");
    }

    #[test]
    fn test_mask_token_empty() {
        assert_eq!(mask_token(""), "");
    }

    #[test]
    fn terminal_text_escapes_line_and_direction_controls() {
        assert_eq!(
            escape_terminal_text("정상\n위조\u{1b}[2J\u{202e}txt"),
            "정상\\u{a}위조\\u{1b}[2J\\u{202e}txt"
        );
    }
}
