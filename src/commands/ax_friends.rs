use anyhow::Result;

use crate::ax_send;
use crate::util::{confirm, escape_terminal_text as esc};

/// `ax-friends <query>`: friends KakaoTalk's own search lists, read-only.
pub fn cmd_ax_friends(query: &str, json: bool) -> Result<()> {
    let friends = ax_send::search_friends(query)?;
    if json {
        let items: Vec<_> = friends
            .iter()
            .map(|f| serde_json::json!({"name": f.name, "status": f.status}))
            .collect();
        crate::util::output_json(&serde_json::json!({"query": query, "friends": items}))?;
    } else if friends.is_empty() {
        println!("No friend matches \"{}\".", esc(query));
    } else {
        for f in &friends {
            match &f.status {
                Some(status) => println!("{} · {}", esc(&f.name), esc(status)),
                None => println!("{}", esc(&f.name)),
            }
        }
    }
    Ok(())
}

pub struct AxFriendOptions<'a> {
    pub name: &'a str,
    pub memo: Option<&'a str>,
    pub rename: Option<&'a str>,
    pub skip_confirm: bool,
    pub json: bool,
}

/// `ax-friend <name>`: show a friend's 친구 정보, or set its private memo
/// and the name this account shows for them.
pub fn cmd_ax_friend(opts: AxFriendOptions) -> Result<()> {
    let changing = opts.memo.is_some() || opts.rename.is_some();
    if changing && !opts.skip_confirm {
        eprintln!("Change KakaoTalk friend '{}':", esc(opts.name));
        if let Some(rename) = opts.rename {
            eprintln!("  name: {}", serde_json::to_string(&esc(rename))?);
        }
        if let Some(memo) = opts.memo {
            eprintln!("  memo: {}", serde_json::to_string(&esc(memo))?);
        }
        eprint!("Continue?\n[y/N] ");
        if !confirm()? {
            println!("Cancelled.");
            return Ok(());
        }
    }
    let info = ax_send::friend_via_ax(opts.name, opts.memo, opts.rename)?;
    if opts.json {
        crate::util::output_json(&serde_json::json!({
            "name": info.name,
            "nickname": info.nickname,
            "own_name": info.own_name,
            "memo": info.memo,
            "status": if changing { "verified" } else { "read" },
        }))?;
    } else {
        println!("{}", esc(&info.name));
        if let Some(own) = &info.own_name {
            println!("  friend's own name: {}", esc(own));
        }
        println!(
            "  memo (only you see it): {}",
            if info.memo.is_empty() {
                "-".to_string()
            } else {
                esc(&info.memo)
            }
        );
    }
    Ok(())
}
