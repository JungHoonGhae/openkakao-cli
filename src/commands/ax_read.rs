use anyhow::Result;

use crate::ax_send;

pub struct AxReadOptions {
    pub chat_name: String,
    pub count: usize,
    pub json: bool,
}

/// Read recent messages via AX automation (scrapes the open KakaoTalk chat
/// window) — no local SQLCipher DB access required. See `src/ax_send.rs`.
pub fn cmd_ax_read(opts: AxReadOptions) -> Result<()> {
    let AxReadOptions {
        ref chat_name,
        count,
        json,
    } = opts;

    let view = ax_send::read_via_ax(chat_name, count)?;
    let messages = &view.messages;

    if json {
        let items: Vec<_> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "time": m.time,
                    "sender": m.sender,
                    "outgoing": m.outgoing,
                    "text": m.text,
                })
            })
            .collect();
        crate::util::output_json(&serde_json::json!({
            "chat_name": chat_name,
            "members": view.members,
            "messages": items,
        }))?;
    } else if messages.is_empty() {
        println!("No visible messages found in chat \"{chat_name}\".");
    } else {
        // Other people's names and messages can carry terminal escapes.
        use crate::util::escape_terminal_text as esc;
        if let Some(members) = view.members {
            println!("{} · {members} members\n", esc(chat_name));
        }
        for m in messages {
            let who = match (&m.sender, m.outgoing) {
                (_, true) => "me: ".to_string(),
                (Some(sender), false) => format!("{}: ", esc(sender)),
                (None, false) => String::new(),
            };
            let text: Vec<String> = m.text.lines().map(esc).collect();
            let text = text.join("\n");
            match &m.time {
                Some(t) => println!("[{}] {who}{text}", esc(t)),
                None => println!("{who}{text}"),
            }
        }
        println!(
            "\n{} messages (AX scrape, no server contact)",
            messages.len()
        );
    }

    Ok(())
}
