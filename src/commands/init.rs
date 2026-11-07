//! `openkakao-cli init`: first-run setup for the login-free AX path.
//!
//! Shows the read-only AX readiness checks, writes the send allowlist only
//! from names the user typed or passed with `--allow-chat`, and installs the
//! agent skill. Re-running is safe: existing config is merged (comments kept),
//! and a skill that is already installed is left alone.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::ValueEnum;
use toml_edit::{value, Array, DocumentMut, Item, Table};

use crate::util::{confirm, escape_terminal_text};

const SKILL_SOURCE: &str = "JungHoonGhae/openkakao-cli@openkakao-cli";
const SKILL_NAME: &str = "openkakao-cli";
const SKILL_URL: &str =
    "https://raw.githubusercontent.com/JungHoonGhae/openkakao-cli/main/skills/openkakao-cli/SKILL.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SkillTarget {
    Claude,
    Codex,
    All,
    None,
}

pub struct InitOptions {
    pub yes: bool,
    pub skill: Option<SkillTarget>,
    pub allow_chats: Vec<String>,
    pub all_spaces: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agent {
    Claude,
    Codex,
}

impl Agent {
    /// Agent id understood by `npx skills add -a`.
    fn npx_id(self) -> &'static str {
        match self {
            Self::Claude => "claude-code",
            Self::Codex => "codex",
        }
    }

    /// The agent's home, honoring the same overrides as the `skills` CLI.
    fn home(self, home: &Path, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
        let (var, default) = match self {
            Self::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
            Self::Codex => ("CODEX_HOME", ".codex"),
        };
        env(var)
            .map(|dir| dir.trim().to_string())
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(default))
    }

    fn skill_dir(self, home: &Path, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
        self.home(home, env).join("skills").join(SKILL_NAME)
    }
}

fn agents_for(target: SkillTarget) -> Vec<Agent> {
    match target {
        SkillTarget::Claude => vec![Agent::Claude],
        SkillTarget::Codex => vec![Agent::Codex],
        SkillTarget::All => vec![Agent::Claude, Agent::Codex],
        SkillTarget::None => Vec::new(),
    }
}

/// The default skill target: the agents whose home directory exists.
fn detected_target(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> SkillTarget {
    match (
        Agent::Claude.home(home, env).is_dir(),
        Agent::Codex.home(home, env).is_dir(),
    ) {
        (true, true) => SkillTarget::All,
        (true, false) => SkillTarget::Claude,
        (false, true) => SkillTarget::Codex,
        (false, false) => SkillTarget::None,
    }
}

const NEW_CONFIG_HEADER: &str = "\
# openkakao-cli configuration (created by `openkakao-cli init`).
# Login-free sends stay off until exact chat names are allowlisted, e.g.
#   openkakao-cli init --allow-chat \"<exact chat name>\"
";

/// The config text after allowlisting `chats`, or `None` when nothing would
/// change. Chats are only ever added from `chats` (the user's own input);
/// allowlisting at least one also sets `allow_ax_send = true`, since an
/// allowlist is useless without it. Existing settings and comments are kept.
fn apply_allowlist(existing: Option<&str>, chats: &[String]) -> Result<Option<String>> {
    let mut doc = match existing {
        Some(text) => text
            .parse::<DocumentMut>()
            .context("could not parse the existing config")?,
        None => format!(
            "{NEW_CONFIG_HEADER}\n[safety]\nallow_ax_send = false\nallowed_send_chats = []\n"
        )
        .parse::<DocumentMut>()?,
    };
    let created = existing.is_none();
    if chats.is_empty() {
        return Ok(created.then(|| doc.to_string()));
    }

    let safety = doc
        .entry("safety")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .context("[safety] in the config is not a table")?;
    let mut changed = created;
    if safety.get("allow_ax_send").and_then(Item::as_bool) != Some(true) {
        safety["allow_ax_send"] = value(true);
        changed = true;
    }
    let allowed = safety
        .entry("allowed_send_chats")
        .or_insert_with(|| value(Array::new()))
        .as_array_mut()
        .context("[safety] allowed_send_chats in the config is not an array")?;
    for chat in chats {
        if !allowed
            .iter()
            .any(|existing| existing.as_str() == Some(chat))
        {
            allowed.push(chat.as_str());
            changed = true;
        }
    }
    Ok(changed.then(|| doc.to_string()))
}

fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    let dir = path
        .parent()
        .context("config path has no parent directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(contents.as_bytes())?;
    file.persist(path)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

fn looks_like_skill(body: &str) -> bool {
    body.starts_with("---") && body.contains(&format!("name: {SKILL_NAME}"))
}

fn download_skill(dir: &Path) -> Result<()> {
    let body = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(SKILL_URL)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .with_context(|| format!("could not download {SKILL_URL}"))?;
    if !looks_like_skill(&body) {
        anyhow::bail!("{SKILL_URL} did not return the openkakao-cli SKILL.md");
    }
    write_atomically(&dir.join("SKILL.md"), &body)
}

fn npx_available() -> bool {
    std::process::Command::new("npx")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// `npx skills add` for all `agents` at once. Its output goes to stderr so
/// `--json` stdout stays one JSON document.
fn npx_install(agents: &[Agent]) -> Result<()> {
    let mut command = std::process::Command::new("npx");
    command.args(["-y", "skills", "add", SKILL_SOURCE, "-g", "-y", "-a"]);
    command.args(agents.iter().map(|agent| agent.npx_id()));
    let status = command
        .stdin(std::process::Stdio::null())
        .stdout(std::io::stderr())
        .status()
        .context("could not run npx")?;
    if !status.success() {
        anyhow::bail!("`npx skills add {SKILL_SOURCE}` failed ({status})");
    }
    Ok(())
}

struct SkillResult {
    agent: Agent,
    path: PathBuf,
    status: &'static str,
    method: &'static str,
    detail: Option<String>,
}

fn install_skills(
    agents: &[Agent],
    home: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<SkillResult> {
    let mut results: Vec<SkillResult> = agents
        .iter()
        .map(|&agent| {
            let path = agent.skill_dir(home, env);
            let present = path.join("SKILL.md").is_file();
            SkillResult {
                agent,
                path,
                status: if present {
                    "already_installed"
                } else {
                    "missing"
                },
                method: "none",
                detail: None,
            }
        })
        .collect();
    let missing: Vec<Agent> = results
        .iter()
        .filter(|result| result.status == "missing")
        .map(|result| result.agent)
        .collect();
    if missing.is_empty() {
        return results;
    }

    let npx_error = if npx_available() {
        npx_install(&missing).err()
    } else {
        Some(anyhow::anyhow!("npx is not installed"))
    };
    for result in results
        .iter_mut()
        .filter(|result| result.status == "missing")
    {
        if npx_error.is_none() && result.path.join("SKILL.md").is_file() {
            result.status = "installed";
            result.method = "npx";
            continue;
        }
        // Fallback: fetch the published SKILL.md directly.
        match download_skill(&result.path) {
            Ok(()) => {
                result.status = "installed";
                result.method = "download";
                result.detail = npx_error.as_ref().map(|error| format!("{error:#}"));
            }
            Err(error) => {
                result.status = "failed";
                result.detail = Some(match &npx_error {
                    Some(npx) => format!("{npx:#}; {error:#}"),
                    None => format!("{error:#}"),
                });
            }
        }
    }
    results
}

fn prompt_line(question: &str) -> Result<String> {
    eprint!("{question}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn parse_skill_answer(answer: &str, default: SkillTarget) -> Option<SkillTarget> {
    if answer.is_empty() {
        return Some(default);
    }
    SkillTarget::from_str(answer, true).ok()
}

fn target_name(target: SkillTarget) -> &'static str {
    match target {
        SkillTarget::Claude => "claude",
        SkillTarget::Codex => "codex",
        SkillTarget::All => "all",
        SkillTarget::None => "none",
    }
}

pub fn cmd_init(opts: InitOptions) -> Result<()> {
    let interactive = !opts.yes;
    if interactive && !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "init asks questions; for scripts pass --yes (plus --allow-chat \"<exact name>\" and --skill claude|codex|all|none)"
        );
    }
    let home = dirs::home_dir().context("could not resolve the home directory")?;
    let say = |line: String| {
        if !opts.json {
            println!("{line}");
        }
    };

    say("openkakao-cli init: login-free KakaoTalk setup\n\n1. AX readiness (read-only)".into());
    let (mut checks_failed, mut checks) = crate::commands::doctor::print_ax_checks(opts.json);
    let has_code =
        |checks: &[serde_json::Value], code: &str| checks.iter().any(|check| check["code"] == code);
    let mut fixes = Vec::new();

    if has_code(&checks, "kakaotalk_not_running") {
        let launch = if interactive {
            eprint!("   KakaoTalk is not running. Launch it in the background now? [y/N] ");
            confirm()?
        } else {
            say("   KakaoTalk is not running; a non-interactive run does not launch it (open -g -b com.kakao.KakaoTalkMac, then re-run).".into());
            false
        };
        if launch {
            crate::ax_send::launch_kakaotalk_in_background()?;
            fixes.push("launched KakaoTalk in the background");
        }
    }
    if crate::ax_send::kakaotalk_spaces_assignment() != Some(true) {
        let assign = opts.all_spaces
            || (interactive
                && {
                    eprint!(
                    "   Assign KakaoTalk to All Desktops so its windows stay visible on every Space?\n   (same as Dock → Options → Assign To → All Desktops; the Dock restarts once) [y/N] "
                );
                    confirm()?
                });
        if assign {
            crate::ax_send::assign_kakaotalk_to_all_spaces()?;
            fixes.push("assigned KakaoTalk to All Desktops (Dock restarted)");
        }
    }
    if !fixes.is_empty() {
        say(format!("   Fixed: {}. Re-checking:", fixes.join("; ")));
        (checks_failed, checks) = crate::commands::doctor::print_ax_checks(opts.json);
    }

    // 2. Allowlist: only names the user typed or passed explicitly.
    let mut chats = opts.allow_chats.clone();
    if interactive && chats.is_empty() {
        eprintln!(
            "\n2. Send allowlist: type the exact KakaoTalk chat names local-send may send to,\n   one per line (empty line to finish, or just Enter to skip)."
        );
        loop {
            let name = prompt_line("   chat name: ")?;
            if name.is_empty() {
                break;
            }
            chats.push(name);
        }
    }
    let mut seen = std::collections::HashSet::new();
    chats.retain(|chat| seen.insert(chat.clone()));
    let config_path = crate::config::config_path()?;
    let existing = match std::fs::read_to_string(&config_path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", config_path.display()))
        }
    };
    if interactive && !chats.is_empty() {
        eprint!(
            "   Write allow_ax_send = true and allowlist {} chat(s) to {}? [y/N] ",
            chats.len(),
            config_path.display()
        );
        if !confirm()? {
            chats.clear();
        }
    }
    let updated = apply_allowlist(existing.as_deref(), &chats)?;
    if let Some(text) = &updated {
        write_atomically(&config_path, text)?;
    }
    let effective: crate::config::OpenKakaoConfig =
        toml::from_str(updated.as_deref().or(existing.as_deref()).unwrap_or(""))
            .context("the resulting config does not parse")?;
    say(format!(
        "\n2. Config {}: {} (allow_ax_send={}, allowlisted: {})",
        if updated.is_some() {
            "written"
        } else {
            "unchanged"
        },
        config_path.display(),
        effective.safety.allow_ax_send,
        if effective.safety.allowed_send_chats.is_empty() {
            "none".to_string()
        } else {
            effective
                .safety
                .allowed_send_chats
                .iter()
                .map(|chat| format!("\"{}\"", escape_terminal_text(chat)))
                .collect::<Vec<_>>()
                .join(", ")
        }
    ));

    // 3. Agent skill.
    let env = |name: &str| std::env::var(name).ok();
    let detected = detected_target(&home, &env);
    let target = match opts.skill {
        Some(target) => target,
        None if interactive => loop {
            let answer = prompt_line(&format!(
                "\n3. Install the openkakao-cli agent skill for [claude/codex/all/none] (default: {}): ",
                target_name(detected)
            ))?;
            if let Some(target) = parse_skill_answer(&answer, detected) {
                break target;
            }
        },
        // Non-interactive runs install nothing unless --skill says so: a
        // script must not download and run npm code on a guess.
        None => SkillTarget::None,
    };
    let skills = install_skills(&agents_for(target), &home, &env);
    if skills.is_empty() {
        say(format!(
            "\n3. Agent skill: skipped (install later: npx skills add {SKILL_SOURCE})"
        ));
    }
    for skill in &skills {
        say(format!(
            "\n3. Agent skill ({}): {} at {}{}{}",
            skill.agent.npx_id(),
            skill.status.replace('_', " "),
            skill.path.display(),
            if skill.method == "none" {
                String::new()
            } else {
                format!(" via {}", skill.method)
            },
            skill
                .detail
                .as_ref()
                .map(|detail| format!(" ({detail})"))
                .unwrap_or_default()
        ));
    }

    let next = [
        "openkakao-cli doctor --chat \"<chat name>\"   # read-only check of an open chat window",
        "openkakao-cli ax-read \"<chat name>\" -n 5      # focus-free read of an open chat window",
        "openkakao-cli local-send \"<chat name>\" \"hi\" --dry-run",
    ];
    if opts.json {
        crate::util::output_json(&serde_json::json!({
            "ready": !checks_failed,
            "checks": checks,
            "fixes": fixes,
            "config": {
                "path": config_path,
                "changed": updated.is_some(),
                "allow_ax_send": effective.safety.allow_ax_send,
                "allowed_send_chats": effective.safety.allowed_send_chats,
            },
            "skills": skills.iter().map(|skill| serde_json::json!({
                "agent": skill.agent.npx_id(),
                "path": skill.path,
                "status": skill.status,
                "method": skill.method,
                "detail": skill.detail,
            })).collect::<Vec<_>>(),
            "next": next,
        }))?;
    } else {
        println!("\nNext:");
        for step in next {
            println!("  {step}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn new_config_is_safe_until_a_chat_is_named() {
        let text = apply_allowlist(None, &[]).unwrap().expect("creates a file");
        let config: crate::config::OpenKakaoConfig = toml::from_str(&text).unwrap();
        assert!(!config.safety.allow_ax_send);
        assert!(config.safety.allowed_send_chats.is_empty());
        assert!(text.contains("init --allow-chat"));
    }

    #[test]
    fn named_chats_enable_ax_send_and_are_written_exactly() {
        let chats = vec!["[ilwall] Remodule".to_string(), "정훈".to_string()];
        let text = apply_allowlist(None, &chats).unwrap().unwrap();
        let config: crate::config::OpenKakaoConfig = toml::from_str(&text).unwrap();
        assert!(config.safety.allow_ax_send);
        assert_eq!(config.safety.allowed_send_chats, chats);
    }

    #[test]
    fn merge_keeps_comments_and_other_settings_and_is_idempotent() {
        let existing = "# my notes\n[send]\ndefault_prefix = false\n\n[safety]\n# keep me\nallowed_send_chats = [\"Family\"]\n";
        let chats = vec!["Family".to_string(), "Work".to_string()];
        let merged = apply_allowlist(Some(existing), &chats).unwrap().unwrap();
        assert!(merged.contains("# my notes") && merged.contains("# keep me"));
        let config: crate::config::OpenKakaoConfig = toml::from_str(&merged).unwrap();
        assert_eq!(config.send.default_prefix, Some(false));
        assert!(config.safety.allow_ax_send);
        assert_eq!(config.safety.allowed_send_chats, ["Family", "Work"]);
        assert_eq!(apply_allowlist(Some(&merged), &chats).unwrap(), None);
    }

    #[test]
    fn existing_config_is_never_touched_without_named_chats() {
        let existing = "[safety]\nallow_ax_send = false\n";
        assert_eq!(apply_allowlist(Some(existing), &[]).unwrap(), None);
    }

    #[test]
    fn malformed_allowlist_is_an_error_not_a_rewrite() {
        let existing = "[safety]\nallowed_send_chats = \"Family\"\n";
        assert!(apply_allowlist(Some(existing), &["Work".to_string()]).is_err());
    }

    #[test]
    fn skill_dirs_follow_agent_home_overrides() {
        let home = Path::new("/Users/me");
        assert_eq!(
            Agent::Claude.skill_dir(home, &no_env),
            Path::new("/Users/me/.claude/skills/openkakao-cli")
        );
        assert_eq!(
            Agent::Codex.skill_dir(home, &no_env),
            Path::new("/Users/me/.codex/skills/openkakao-cli")
        );
        let env = |name: &str| match name {
            "CODEX_HOME" => Some("/opt/codex".to_string()),
            "CLAUDE_CONFIG_DIR" => Some("  ".to_string()),
            _ => None,
        };
        assert_eq!(
            Agent::Codex.skill_dir(home, &env),
            Path::new("/opt/codex/skills/openkakao-cli")
        );
        // A blank override falls back to the default, like the skills CLI.
        assert_eq!(
            Agent::Claude.skill_dir(home, &env),
            Path::new("/Users/me/.claude/skills/openkakao-cli")
        );
    }

    #[test]
    fn skill_targets_and_detection() {
        assert_eq!(agents_for(SkillTarget::All), [Agent::Claude, Agent::Codex]);
        assert!(agents_for(SkillTarget::None).is_empty());
        let home = tempfile::tempdir().unwrap();
        assert_eq!(detected_target(home.path(), &no_env), SkillTarget::None);
        std::fs::create_dir(home.path().join(".codex")).unwrap();
        assert_eq!(detected_target(home.path(), &no_env), SkillTarget::Codex);
        std::fs::create_dir(home.path().join(".claude")).unwrap();
        assert_eq!(detected_target(home.path(), &no_env), SkillTarget::All);
    }

    #[test]
    fn already_installed_skills_are_left_alone() {
        let home = tempfile::tempdir().unwrap();
        let dir = Agent::Claude.skill_dir(home.path(), &|_| None);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: openkakao-cli\n---\n").unwrap();
        let results = install_skills(&[Agent::Claude], home.path(), &no_env);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "already_installed");
        assert_eq!(results[0].method, "none");
    }

    #[test]
    fn skill_answers_and_downloads_are_validated() {
        assert_eq!(
            parse_skill_answer("", SkillTarget::Codex),
            Some(SkillTarget::Codex)
        );
        assert_eq!(
            parse_skill_answer("ALL", SkillTarget::None),
            Some(SkillTarget::All)
        );
        assert_eq!(parse_skill_answer("vim", SkillTarget::None), None);
        assert!(looks_like_skill(
            "---\nname: openkakao-cli\ndescription: x\n---\n"
        ));
        assert!(!looks_like_skill("<html>404</html>"));
    }
}
