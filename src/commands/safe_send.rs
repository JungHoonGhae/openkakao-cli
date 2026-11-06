use std::io::{IsTerminal, Write};

use anyhow::Result;

use crate::safe_send::{ProposeSend, SafeSendOutbox, SendTransport, TransportOutcome};
use crate::util::{
    confirm, escape_terminal_text, output_json, truncate, validate_outbound_message,
};

pub struct ProposeOptions {
    pub chat_name: String,
    pub message: String,
    pub reply_to: Option<(i64, i64)>,
    pub idempotency_key: Option<String>,
    pub json: bool,
}

pub struct ApproveOptions {
    pub intent_id: String,
    pub allow_ax_send: bool,
    pub allowed_chats: Vec<String>,
    pub unattended: bool,
    pub json: bool,
}

/// Keeps the adapter's pre-commit error so `approve` can surface its stable
/// `code` and `fix` instead of a bare "not sent".
#[derive(Default)]
struct AxSendTransport {
    not_sent: Option<anyhow::Error>,
}

impl SendTransport for AxSendTransport {
    fn send(&mut self, chat_name: &str, message: &str) -> TransportOutcome {
        match crate::ax_send::send_via_ax_classified(chat_name, message) {
            Ok(crate::ax_send::AxDeliveryOutcome::Verified) => TransportOutcome::Verified,
            Ok(crate::ax_send::AxDeliveryOutcome::Uncertain { reason }) => {
                TransportOutcome::Uncertain { reason }
            }
            // An error coded as possibly committed must never reopen the proposal.
            Err(error) if crate::error::may_have_delivered(&error) => TransportOutcome::Uncertain {
                reason: format!("{error:#}"),
            },
            Err(error) => {
                let reason = error.to_string();
                self.not_sent = Some(error);
                TransportOutcome::NotSent { reason }
            }
        }
    }
}

fn require_approval_session(
    unattended: bool,
    stdin_is_terminal: bool,
    allow_ax_send: bool,
) -> Result<()> {
    if unattended || !stdin_is_terminal {
        anyhow::bail!(
            "safe-send approval requires an interactive terminal; there is no unattended bypass"
        );
    }
    if !allow_ax_send {
        anyhow::bail!(
            "AX sending is disabled; set safety.allow_ax_send = true only after reviewing the safe-send proposal"
        );
    }
    Ok(())
}

pub fn cmd_propose(options: ProposeOptions) -> Result<()> {
    validate_outbound_message(&options.message)?;
    let outbox = SafeSendOutbox::open()?;
    let intent = outbox.propose(ProposeSend {
        chat_name: &options.chat_name,
        message: &options.message,
        reply_to: options.reply_to,
        idempotency_key: options.idempotency_key.as_deref(),
    })?;

    if options.json {
        output_json(&intent)?;
    } else {
        println!("Safe-send intent stored; this command sent no message.");
        println!(
            "  state:   {}",
            serde_json::to_value(intent.state)?
                .as_str()
                .unwrap_or("unknown")
        );
        println!("  intent:  {}", intent.intent_id);
        println!("  target:  {}", escape_terminal_text(&intent.chat_name));
        println!(
            "  message: {}",
            serde_json::to_string(&escape_terminal_text(&truncate(&intent.message, 120)))?
        );
        println!("  code:    {}", intent.approval_code);
        if intent.state == crate::safe_send::SendState::Proposed {
            println!(
                "Review it, then run: openkakao-cli safe-send approve {}",
                intent.intent_id
            );
        }
    }
    Ok(())
}

pub fn cmd_list(json: bool) -> Result<()> {
    let outbox = SafeSendOutbox::open()?;
    let intents = outbox.list_active()?;
    if json {
        output_json(&intents)?;
    } else if intents.is_empty() {
        println!("No active safe-send intents.");
    } else {
        for intent in intents {
            println!(
                "{} [{}] {}: {} (code {})",
                intent.intent_id,
                serde_json::to_value(intent.state)?
                    .as_str()
                    .unwrap_or("unknown"),
                escape_terminal_text(&intent.chat_name),
                serde_json::to_string(&escape_terminal_text(&truncate(&intent.message, 80)))?,
                intent.approval_code
            );
        }
    }
    Ok(())
}

pub fn cmd_approve(options: ApproveOptions) -> Result<()> {
    require_approval_session(
        options.unattended,
        std::io::stdin().is_terminal(),
        options.allow_ax_send,
    )?;

    let mut outbox = SafeSendOutbox::open()?;
    let intent = outbox.get(&options.intent_id)?;

    validate_outbound_message(&intent.message)?;
    eprintln!("Safe-send approval review:");
    eprintln!("  intent:  {}", intent.intent_id);
    eprintln!("  target:  {}", escape_terminal_text(&intent.chat_name));
    if !crate::ax_send::is_allowlisted(&options.allowed_chats, &intent.chat_name) {
        eprintln!(
            "  ⚠ NOT in [safety] allowed_send_chats: macOS device-owner authentication follows the code"
        );
    }
    eprintln!(
        "  message: {}",
        serde_json::to_string(&escape_terminal_text(&intent.message))?
    );
    if let Some((chat_id, log_id)) = intent.reply_to {
        eprintln!("  reply:   chat {chat_id}, log {log_id}");
    }
    eprint!("Type the 12-character approval code: ");
    std::io::stderr().flush()?;
    let mut approval_code = String::new();
    std::io::stdin().read_line(&mut approval_code)?;

    let mut transport = AxSendTransport::default();
    let sent = outbox
        .approve_and_send(&intent.intent_id, approval_code.trim(), &mut transport)
        .map_err(|error| match transport.not_sent.take() {
            Some(cause) => cause.context(error.to_string()),
            None => error,
        })?;
    if options.json {
        output_json(&sent)?;
    } else {
        println!("Safe-send completed for intent {}.", sent.intent_id);
    }
    Ok(())
}

pub fn cmd_cancel(intent_id: &str, skip_confirm: bool, json: bool) -> Result<()> {
    let outbox = SafeSendOutbox::open()?;
    let intent = outbox.get(intent_id)?;
    if !skip_confirm {
        if !std::io::stdin().is_terminal() {
            anyhow::bail!("safe-send cancel requires an interactive terminal or --yes");
        }
        eprint!(
            "Cancel safe-send intent {} for {:?}?\n[y/N] ",
            intent.intent_id, intent.chat_name
        );
        std::io::stderr().flush()?;
        if !confirm()? {
            println!("Cancelled nothing.");
            return Ok(());
        }
    }

    let cancelled = outbox.cancel(intent_id)?;
    if json {
        output_json(&cancelled)?;
    } else {
        println!("Safe-send intent {} cancelled.", cancelled.intent_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_session_requires_a_human_terminal_and_ax_opt_in() {
        let unattended = require_approval_session(true, true, true).unwrap_err();
        assert!(unattended.to_string().contains("no unattended bypass"));

        let non_terminal = require_approval_session(false, false, true).unwrap_err();
        assert!(non_terminal.to_string().contains("interactive terminal"));

        let disabled = require_approval_session(false, true, false).unwrap_err();
        assert!(disabled.to_string().contains("AX sending is disabled"));

        require_approval_session(false, true, true).unwrap();
    }
}
