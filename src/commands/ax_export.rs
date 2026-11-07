use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::ax_send;
use crate::util::{escape_terminal_text as esc, truncate};

/// One exported message: KakaoTalk's CSV columns `Date, User, Message`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExportedMessage {
    pub date: String,
    pub user: String,
    pub message: String,
}

/// Read a KakaoTalk chat export (`Date,User,Message`, UTF-8 with a BOM).
pub fn read_export(path: &Path) -> Result<Vec<ExportedMessage>> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(data.trim_start_matches('\u{feff}').as_bytes());
    let mut messages = Vec::new();
    for record in reader.records() {
        let record =
            record.with_context(|| format!("{} is not a KakaoTalk export", path.display()))?;
        let field = |index| record.get(index).unwrap_or_default().to_string();
        messages.push(ExportedMessage {
            date: field(0),
            user: field(1),
            message: field(2),
        });
    }
    Ok(messages)
}

/// Messages whose text or sender contains `query` (case-insensitive), the
/// newest `limit` of them, oldest first.
pub fn search(messages: &[ExportedMessage], query: &str, limit: usize) -> Vec<ExportedMessage> {
    let query = query.to_lowercase();
    let hits: Vec<&ExportedMessage> = messages
        .iter()
        .filter(|m| {
            m.message.to_lowercase().contains(&query) || m.user.to_lowercase().contains(&query)
        })
        .collect();
    hits[hits.len().saturating_sub(limit)..]
        .iter()
        .map(|m| (*m).clone())
        .collect()
}

/// Move the export to `output` without overwriting anything there.
fn move_export(from: &Path, output: &Path) -> Result<PathBuf> {
    let target = if output.is_dir() {
        output.join(from.file_name().unwrap_or_default())
    } else {
        output.to_path_buf()
    };
    if target.exists() {
        anyhow::bail!(
            "{} already exists; the export stays at {}",
            target.display(),
            from.display()
        );
    }
    if std::fs::rename(from, &target).is_err() {
        // Another volume: copy, then remove the original.
        std::fs::copy(from, &target)
            .with_context(|| format!("could not write {}", target.display()))?;
        std::fs::remove_file(from)?;
    }
    Ok(target)
}

pub fn cmd_ax_export(chat_name: &str, output: Option<&Path>, json: bool) -> Result<()> {
    let mut path = ax_send::export_via_ax(chat_name)?;
    if let Some(output) = output {
        path = move_export(&path, output)?;
    }
    let messages = read_export(&path)?;
    let (first, last) = (messages.first(), messages.last());
    if json {
        crate::util::output_json(&serde_json::json!({
            "chat_name": chat_name,
            "path": path,
            "messages": messages.len(),
            "first": first.map(|m| &m.date),
            "last": last.map(|m| &m.date),
        }))?;
    } else {
        println!(
            "Saved {} messages ({} → {}) to {}",
            messages.len(),
            first.map_or("-", |m| m.date.as_str()),
            last.map_or("-", |m| m.date.as_str()),
            path.display()
        );
    }
    Ok(())
}

pub struct AxSearchOptions<'a> {
    pub chat_name: &'a str,
    pub query: &'a str,
    pub limit: usize,
    /// Search this export instead of exporting the chat again.
    pub from: Option<&'a Path>,
    /// Keep the fresh export (it is removed after the search otherwise).
    pub keep: bool,
    pub json: bool,
}

pub fn cmd_ax_search(opts: AxSearchOptions) -> Result<()> {
    let (path, fresh) = match opts.from {
        Some(path) => (path.to_path_buf(), false),
        None => (ax_send::export_via_ax(opts.chat_name)?, true),
    };
    let messages = read_export(&path);
    if fresh && !opts.keep {
        let _ = std::fs::remove_file(&path);
    }
    let messages = messages?;
    let hits = search(&messages, opts.query, opts.limit);
    if opts.json {
        crate::util::output_json(&serde_json::json!({
            "chat_name": opts.chat_name,
            "query": opts.query,
            "searched": messages.len(),
            "matches": hits,
            "export": (opts.keep || !fresh).then_some(&path),
        }))?;
    } else if hits.is_empty() {
        println!(
            "No message in \"{}\" contains \"{}\".",
            esc(opts.chat_name),
            esc(opts.query)
        );
    } else {
        for m in &hits {
            let text = m.message.split_whitespace().collect::<Vec<_>>().join(" ");
            println!(
                "[{}] {}: {}",
                esc(&m.date),
                esc(&m.user),
                esc(&truncate(&text, 200))
            );
        }
        println!("\n{} of {} messages match", hits.len(), messages.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_are_read_and_searched_newest_last() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("KakaoTalk_Chat_test.csv");
        std::fs::write(
            &path,
            "\u{feff}Date,User,Message\n\
             2019-08-05 15:30:58,\"정훈\",\"브런치 어디?\"\n\
             2026-11-07 10:23:00,\"Kristin\",\"Brunch at 37.5\n고산점\"\n\
             2026-11-07 16:33:52,\"정훈\",\"테스트\"\n",
        )
        .unwrap();
        let messages = read_export(&path).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1].message, "Brunch at 37.5\n고산점");

        let hits = search(&messages, "BRUNCH", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].user, "Kristin");
        assert_eq!(search(&messages, "브런치", 10).len(), 1);
        assert_eq!(search(&messages, "정훈", 1)[0].date, "2026-11-07 16:33:52");
        assert!(search(&messages, "없는 말", 10).is_empty());
    }
}
