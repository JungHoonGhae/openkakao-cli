//! Read-side message envelopes observed in the signed 26.7 apps.
//! See docs/research/message-envelopes-26.7.md for evidence and compatibility policy.

use anyhow::{bail, Context, Result};
use bson::{Bson, Document};

use super::packet::LocoPacket;

#[derive(Debug)]
pub struct ReceivedMessage<'a> {
    pub chat_id: i64,
    pub log_id: i64,
    pub author_id: i64,
    pub message_type: i32,
    pub send_at: i64,
    pub nickname: &'a str,
    pub attachment: &'a str,
    pub body: &'a Document,
}

fn integer(body: &Document, key: &str) -> Option<i64> {
    match body.get(key) {
        Some(Bson::Int64(value)) => Some(*value),
        Some(Bson::Int32(value)) => Some(i64::from(*value)),
        _ => None,
    }
}

impl<'a> ReceivedMessage<'a> {
    fn parse(body: &'a Document, envelope: &'a Document) -> Result<Self> {
        let chat_id = integer(body, "chatId")
            .or_else(|| integer(envelope, "chatId"))
            .filter(|id| *id > 0)
            .context("Message has no positive chatId")?;
        if envelope.contains_key("chatId") && integer(envelope, "chatId") != Some(chat_id) {
            bail!("Message chatId conflicts with its envelope");
        }
        // A malformed explicit inner identity must not fall back to the envelope.
        if body.contains_key("chatId") && integer(body, "chatId") != Some(chat_id) {
            bail!("Message has an invalid chatId");
        }
        let log_id = integer(body, "logId")
            .filter(|id| *id > 0)
            .context("Message has no positive logId")?;
        let message_type = integer(body, "type")
            .and_then(|value| i32::try_from(value).ok())
            .context("Message has no valid type")?;
        let nickname = envelope
            .get_str("authorNickname")
            .or_else(|_| body.get_str("authorNickname"))
            .ok()
            .or_else(|| {
                body.get_document("author")
                    .ok()
                    .and_then(|author| author.get_str("nickName").ok())
            })
            .unwrap_or("???");
        Ok(Self {
            chat_id,
            log_id,
            author_id: integer(body, "authorId").unwrap_or(0),
            message_type,
            send_at: integer(body, "sendAt").unwrap_or(0),
            nickname,
            attachment: body.get_str("attachment").unwrap_or(""),
            body,
        })
    }
}

/// Decode the whole envelope before any event, hook, cache or cursor mutation.
/// Flat messages remain supported for compatibility. An explicit nested field
/// must be valid; it never silently falls back to unrelated outer message data.
pub fn received_messages(packet: &LocoPacket) -> Result<Vec<ReceivedMessage<'_>>> {
    if !matches!(packet.method.as_str(), "MSG" | "SYNCMSG") {
        bail!("Not a message packet");
    }
    if packet.status() != 0 {
        bail!("Message packet returned status {}", packet.status());
    }
    let envelope = &packet.body;
    if packet.method == "SYNCMSG" && envelope.contains_key("chatLogs") {
        return envelope
            .get_array("chatLogs")
            .context("SYNCMSG chatLogs must be an array")?
            .iter()
            .map(|value| {
                let body = value
                    .as_document()
                    .context("chatLogs entry must be a document")?;
                ReceivedMessage::parse(body, envelope)
            })
            .collect();
    }
    let body = if envelope.contains_key("chatLog") {
        envelope
            .get_document("chatLog")
            .context("chatLog must be a document")?
    } else {
        envelope
    };
    Ok(vec![ReceivedMessage::parse(body, envelope)?])
}
