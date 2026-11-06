//! `local-reply` / `local-edit`: experimental, focus-free message actions on
//! an open chat window (see `ax_send::reply_via_ax` / `ax_send::edit_via_ax`).

use anyhow::Result;

use crate::ax_send::{self, BubbleTarget};
use crate::util::{confirm, escape_terminal_text, truncate, validate_outbound_message};

pub struct BubbleActionOptions {
    pub chat_name: String,
    pub target: BubbleTarget,
    pub text: String,
    pub skip_confirm: bool,
    pub dry_run: bool,
    pub json: bool,
}

fn describe(target: &BubbleTarget) -> String {
    match target {
        BubbleTarget::Last => "the newest text bubble".to_string(),
        BubbleTarget::Text(text) => format!(
            "the bubble {}",
            serde_json::to_string(&escape_terminal_text(text)).unwrap_or_default()
        ),
    }
}

fn run(
    action: &str,
    opts: BubbleActionOptions,
    perform: impl Fn(&str, &BubbleTarget, &str, bool) -> Result<ax_send::BubbleAction>,
) -> Result<()> {
    validate_outbound_message(&opts.text)?;
    if !opts.dry_run {
        eprintln!("Direct AX {action} review (experimental):");
        eprintln!("  chat:   {}", escape_terminal_text(&opts.chat_name));
        eprintln!("  target: {}", describe(&opts.target));
        eprintln!(
            "  text:   {}",
            serde_json::to_string(&escape_terminal_text(&opts.text))?
        );
        if !opts.skip_confirm {
            eprint!("Continue?\n[y/N] ");
            if !confirm()? {
                println!("Cancelled.");
                return Ok(());
            }
        }
    }
    let done = perform(&opts.chat_name, &opts.target, &opts.text, opts.dry_run)?;
    if opts.json {
        crate::util::output_json(&serde_json::json!({
            "dry_run": opts.dry_run,
            "action": format!("local_{action}"),
            "chat_name": opts.chat_name,
            "target": { "text": done.target_text, "outgoing": done.target_outgoing },
            "text": opts.text,
            "status": if opts.dry_run { "previewed" } else { "verified" },
        }))?;
    } else if opts.dry_run {
        eprintln!(
            "[dry-run] Would {action} in \"{}\" targeting {} bubble: {}",
            escape_terminal_text(&opts.chat_name),
            if done.target_outgoing {
                "your"
            } else {
                "an incoming"
            },
            serde_json::to_string(&escape_terminal_text(&truncate(&done.target_text, 80)))?
        );
    } else {
        println!("{action} verified.");
    }
    Ok(())
}

pub fn cmd_local_reply(opts: BubbleActionOptions) -> Result<()> {
    run("reply", opts, |chat, target, text, dry_run| {
        ax_send::reply_via_ax(chat, target, text, dry_run)
    })
}

pub fn cmd_local_edit(opts: BubbleActionOptions) -> Result<()> {
    run("edit", opts, |chat, target, text, dry_run| {
        let BubbleTarget::Text(old) = target else {
            anyhow::bail!("local-edit needs --match \"<old text>\"");
        };
        ax_send::edit_via_ax(chat, old, text, dry_run)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_described_safely_for_the_review() {
        assert_eq!(describe(&BubbleTarget::Last), "the newest text bubble");
        assert_eq!(
            describe(&BubbleTarget::Text("hi \u{1b}[2J".into())),
            format!(
                "the bubble {}",
                serde_json::to_string(&escape_terminal_text("hi \u{1b}[2J")).unwrap()
            )
        );
    }
}
