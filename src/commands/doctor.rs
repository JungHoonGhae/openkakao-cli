use anyhow::Result;
use owo_colors::OwoColorize;

use crate::auth::get_credential_candidates;
use crate::ax_send::AxCheckStatus;
use crate::config::OpenKakaoConfig;
use crate::credentials::load_credentials;
use crate::error::ErrorCode;
use crate::model::KakaoCredentials;
use crate::rest::KakaoRestClient;
use crate::state::{recovery_snapshot, safety_snapshot};
use crate::util::{color_enabled, VERSION};

struct Check {
    name: String,
    status: CheckStatus,
    detail: String,
    code: Option<&'static str>,
    fix: Option<String>,
}

impl Default for Check {
    fn default() -> Self {
        Self {
            name: String::new(),
            status: CheckStatus::Ok,
            detail: String::new(),
            code: None,
            fix: None,
        }
    }
}

enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

const HOMEBREW_FORMULA: &str = "junghoonghae/openkakao/openkakao-cli";
const HOMEBREW_TAP: &str = "junghoonghae/openkakao";

/// Whether Homebrew's trust store (`brew trust`, Homebrew 7+) trusts this
/// formula or its tap. Unparseable data counts as untrusted.
fn homebrew_trusts_formula(trust_json: &str) -> bool {
    let Ok(trust) = serde_json::from_str::<serde_json::Value>(trust_json) else {
        return false;
    };
    let listed = |key: &str, wanted: &str| {
        trust[key]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| entry.as_str() == Some(wanted)))
    };
    listed("trustedformulae", HOMEBREW_FORMULA) || listed("trustedtaps", HOMEBREW_TAP)
}

/// `brew trust` guidance, only when this binary was installed from the tap.
fn homebrew_check() -> Option<Check> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if !exe.to_string_lossy().contains("/Cellar/openkakao-cli/") {
        return None;
    }
    let trust_path = std::env::var_os("XDG_CONFIG_HOME")
        .map(|dir| std::path::PathBuf::from(dir).join("homebrew/trust.json"))
        .or_else(|| dirs::home_dir().map(|home| home.join(".homebrew/trust.json")))?;
    let trusted = std::fs::read_to_string(&trust_path)
        .map(|data| homebrew_trusts_formula(&data))
        .unwrap_or(false);
    Some(Check {
        name: "Homebrew tap trust".into(),
        status: if trusted {
            CheckStatus::Ok
        } else {
            CheckStatus::Warn
        },
        detail: if trusted {
            format!("{HOMEBREW_FORMULA} is trusted")
        } else {
            format!(
                "{HOMEBREW_FORMULA} is not in {}; Homebrew 7+ refuses to install or upgrade untrusted tap formulae",
                trust_path.display()
            )
        },
        code: (!trusted).then_some("homebrew_untrusted_tap"),
        fix: (!trusted).then(|| format!("brew trust --formula {HOMEBREW_FORMULA}")),
    })
}

fn format_remaining(remaining_secs: Option<u64>, until: Option<&str>) -> String {
    match (remaining_secs, until) {
        (Some(secs), Some(until)) => format!("{}s (until {})", secs, until),
        (Some(secs), None) => format!("{}s", secs),
        _ => "none".to_string(),
    }
}

/// AX readiness checks shared with `init`.
fn ax_checks(chat: Option<&str>) -> Vec<Check> {
    crate::ax_send::diagnose(chat)
        .into_iter()
        .map(|check| Check {
            name: check.name.into(),
            status: match check.status {
                AxCheckStatus::Ok => CheckStatus::Ok,
                AxCheckStatus::Warn => CheckStatus::Warn,
                AxCheckStatus::Fail => CheckStatus::Fail,
            },
            detail: check.detail,
            code: check.code.map(ErrorCode::as_str),
            fix: check.code.map(|code| code.fix().to_string()),
        })
        .collect()
}

/// Print AX readiness for `init` and report whether any check failed.
pub(crate) fn print_ax_checks(json: bool) -> (bool, Vec<serde_json::Value>) {
    let checks = ax_checks(None);
    let failed = checks
        .iter()
        .any(|check| matches!(check.status, CheckStatus::Fail));
    if !json {
        print_checks(&checks);
    }
    (failed, checks.iter().map(check_json).collect())
}

fn check_json(c: &Check) -> serde_json::Value {
    let mut item = serde_json::json!({
        "check": c.name,
        "status": match c.status {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        },
        "detail": c.detail,
    });
    if let Some(code) = c.code {
        item["code"] = code.into();
    }
    if let Some(fix) = &c.fix {
        item["fix"] = fix.clone().into();
    }
    item
}

fn print_checks(checks: &[Check]) {
    for c in checks {
        let (icon, color_fn): (&str, fn(&str) -> String) = match c.status {
            CheckStatus::Ok => {
                if color_enabled() {
                    ("OK", |s: &str| format!("{}", s.green()))
                } else {
                    ("OK", |s: &str| s.to_string())
                }
            }
            CheckStatus::Warn => {
                if color_enabled() {
                    ("WARN", |s: &str| format!("{}", s.yellow()))
                } else {
                    ("WARN", |s: &str| s.to_string())
                }
            }
            CheckStatus::Fail => {
                if color_enabled() {
                    ("FAIL", |s: &str| format!("{}", s.red()))
                } else {
                    ("FAIL", |s: &str| s.to_string())
                }
            }
        };
        println!("  [{}] {}: {}", color_fn(icon), c.name, c.detail);
        if let Some(fix) = &c.fix {
            println!("         fix: {fix}");
        }
    }
}

pub fn cmd_doctor(
    json: bool,
    test_loco: bool,
    chat: Option<&str>,
    config: &OpenKakaoConfig,
) -> Result<()> {
    let mut checks: Vec<Check> = Vec::new();
    let mut installed_version: Option<String> = None;
    let mut saved_app_version: Option<String> = None;
    let recovery = recovery_snapshot()?;
    let safety = safety_snapshot(
        config
            .safety
            .min_unattended_send_interval_secs
            .unwrap_or(10),
        config.safety.min_hook_interval_secs.unwrap_or(2),
        config.safety.min_webhook_interval_secs.unwrap_or(2),
    )?;

    checks.push(Check {
        name: "State file".into(),
        status: CheckStatus::Ok,
        detail: recovery.path.clone(),
        ..Default::default()
    });
    checks.push(Check {
        name: "Auth recovery state".into(),
        status: if recovery.auth_cooldown_remaining_secs.is_some()
            || recovery.consecutive_failures > 0
        {
            CheckStatus::Warn
        } else {
            CheckStatus::Ok
        },
        detail: format!(
            "failures={}, last_failure={}, auth_cooldown={}, last_success={} via {}",
            recovery.consecutive_failures,
            recovery.last_failure_kind.as_deref().unwrap_or("none"),
            format_remaining(
                recovery.auth_cooldown_remaining_secs,
                recovery.cooldown_until.as_deref()
            ),
            recovery
                .last_success_transport
                .as_deref()
                .unwrap_or("never"),
            recovery.last_recovery_source.as_deref().unwrap_or("none")
        ),
        ..Default::default()
    });
    checks.push(Check {
        name: "Safety guards".into(),
        status: if safety.last_guard_reason.is_some() {
            CheckStatus::Warn
        } else {
            CheckStatus::Ok
        },
        detail: format!(
            "send={}s, hook={}s, webhook={}s, hook_timeout={}s, webhook_timeout={}s, insecure_webhooks={}, last_guard={}",
            config.safety.min_unattended_send_interval_secs.unwrap_or(10),
            config.safety.min_hook_interval_secs.unwrap_or(2),
            config.safety.min_webhook_interval_secs.unwrap_or(2),
            config.safety.hook_timeout_secs.unwrap_or(20),
            config.safety.webhook_timeout_secs.unwrap_or(10),
            if config.safety.allow_insecure_webhooks { "allowed" } else { "blocked" },
            safety.last_guard_reason.as_deref().unwrap_or("none")
        ),
        ..Default::default()
    });

    // 1. KakaoTalk.app installed version
    if let Some(app) = openkakao_cli::kakaotalk_app::installed_app() {
        installed_version = Some(app.version.clone());
        checks.push(Check {
            name: "KakaoTalk.app".into(),
            status: CheckStatus::Ok,
            detail: format!(
                "v{} ({}) at {}",
                app.version,
                app.bundle_id,
                app.path.display()
            ),
            ..Default::default()
        });
    } else {
        checks.push(Check {
            name: "KakaoTalk.app".into(),
            status: CheckStatus::Fail,
            detail: "No KakaoTalk bundle with the expected identifier in /Applications or ~/Applications".into(),
            ..Default::default()
        });
    }

    // 2. KakaoTalk process running
    let pgrep_output = std::process::Command::new("pgrep")
        .args(["-x", "KakaoTalk"])
        .output();
    match pgrep_output {
        Ok(output) if output.status.success() => {
            let pids = String::from_utf8_lossy(&output.stdout).trim().to_string();
            checks.push(Check {
                name: "KakaoTalk process".into(),
                status: CheckStatus::Ok,
                detail: format!("Running (PID: {})", pids.replace('\n', ", ")),
                ..Default::default()
            });
        }
        _ => {
            checks.push(Check {
                name: "KakaoTalk process".into(),
                status: CheckStatus::Warn,
                detail: "Not running. Start KakaoTalk to refresh tokens.".into(),
                ..Default::default()
            });
        }
    }

    // 2b. Login-free AX path: permission, sign-in, main window on this Space,
    // chat-list search field, and (with --chat) the open chat window.
    // Read-only: nothing is pressed, opened, or raised.
    checks.extend(ax_checks(chat));
    checks.push(Check {
        name: "AX send gate".into(),
        status: if config.safety.allow_ax_send
            && !config.safety.allowed_send_chats.is_empty()
            && config.safety.device_owner_auth_grace_secs == 0
        {
            CheckStatus::Ok
        } else {
            // A grace window lets local processes send without a prompt.
            CheckStatus::Warn
        },
        detail: {
            let mut detail = format!(
                "allow_ax_send={}, {} allowlisted chat(s), device-owner auth grace {}s",
                config.safety.allow_ax_send,
                config.safety.allowed_send_chats.len(),
                config.safety.device_owner_auth_grace_secs
            );
            if let Some(left) = openkakao_cli::human_auth::device_owner_auth_grace_left() {
                detail.push_str(&format!(
                    " ({}s left before the next prompt)",
                    left.as_secs()
                ));
            }
            detail
        },
        fix: if !config.safety.allow_ax_send || config.safety.allowed_send_chats.is_empty() {
            Some("openkakao-cli init --allow-chat \"<exact chat name>\"".to_string())
        } else if config.safety.device_owner_auth_grace_secs > 0 {
            Some(
                "device_owner_auth_grace_secs is on: within the window any local process can send to allowlisted chats without a prompt. Set it to 0 unless every process on this Mac is yours."
                    .to_string(),
            )
        } else {
            None
        },
        ..Default::default()
    });
    checks.extend(homebrew_check());

    // 3. Cache.db existence and freshness
    let home = dirs::home_dir().unwrap_or_default();
    let cache_db =
        home.join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Caches/Cache.db");
    if cache_db.exists() {
        match std::fs::metadata(&cache_db) {
            Ok(meta) => {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| {
                        if d.as_secs() < 60 {
                            format!("{}s ago", d.as_secs())
                        } else if d.as_secs() < 3600 {
                            format!("{}m ago", d.as_secs() / 60)
                        } else if d.as_secs() < 86400 {
                            format!("{}h ago", d.as_secs() / 3600)
                        } else {
                            format!("{}d ago", d.as_secs() / 86400)
                        }
                    })
                    .unwrap_or_else(|| "unknown".into());
                let size_kb = meta.len() / 1024;
                let status = if meta
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d.as_secs() > 86400)
                {
                    CheckStatus::Warn
                } else {
                    CheckStatus::Ok
                };
                checks.push(Check {
                    name: "Cache.db".into(),
                    status,
                    detail: format!("{}KB, modified {}", size_kb, modified),
                    ..Default::default()
                });
            }
            Err(e) => {
                checks.push(Check {
                    name: "Cache.db".into(),
                    status: CheckStatus::Warn,
                    detail: format!("Exists but unreadable: {}", e),
                    ..Default::default()
                });
            }
        }
    } else {
        checks.push(Check {
            name: "Cache.db".into(),
            status: CheckStatus::Fail,
            detail: "Not found. Has KakaoTalk been used on this Mac?".into(),
            ..Default::default()
        });
    }

    // 4. Saved credentials file
    match crate::credentials::credentials_path() {
        Ok(path) => {
            if path.exists() {
                match load_credentials() {
                    Ok(Some(creds)) => {
                        saved_app_version = Some(creds.app_version.clone());
                        checks.push(Check {
                            name: "Saved credentials".into(),
                            status: CheckStatus::Ok,
                            detail: format!(
                                "user_id={}, version={}, token={}...",
                                creds.user_id,
                                creds.app_version,
                                creds.oauth_token.chars().take(8).collect::<String>()
                            ),
                            ..Default::default()
                        });
                    }
                    Ok(None) => {
                        checks.push(Check {
                            name: "Saved credentials".into(),
                            status: CheckStatus::Warn,
                            detail: "File exists but empty/invalid".into(),
                            ..Default::default()
                        });
                    }
                    Err(e) => {
                        checks.push(Check {
                            name: "Saved credentials".into(),
                            status: CheckStatus::Warn,
                            detail: format!("Parse error: {}", e),
                            ..Default::default()
                        });
                    }
                }
            } else {
                checks.push(Check {
                    name: "Saved credentials".into(),
                    status: CheckStatus::Warn,
                    detail: format!(
                        "Not found. Run 'openkakao-cli login --save'. ({})",
                        path.display()
                    ),
                    ..Default::default()
                });
            }
        }
        Err(e) => {
            checks.push(Check {
                name: "Saved credentials".into(),
                status: CheckStatus::Fail,
                detail: format!("Cannot determine path: {}", e),
                ..Default::default()
            });
        }
    }

    // 4b. Version drift
    if let (Some(installed), Some(saved)) = (&installed_version, &saved_app_version) {
        if installed == saved {
            checks.push(Check {
                name: "Version match".into(),
                status: CheckStatus::Ok,
                detail: format!("Installed and saved both v{}", installed),
                ..Default::default()
            });
        } else {
            checks.push(Check {
                name: "Version drift".into(),
                status: CheckStatus::Warn,
                detail: format!(
                    "Installed v{} != saved v{}. Run `relogin --fresh-xvc` to re-authenticate.",
                    installed, saved
                ),
                ..Default::default()
            });
        }
    }

    // 5. Token validity via REST API
    let creds_result: Result<KakaoCredentials> = {
        if let Ok(Some(saved)) = load_credentials() {
            Ok(saved)
        } else {
            let candidates = get_credential_candidates(4).unwrap_or_default();
            candidates
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No credentials found"))
        }
    };
    match &creds_result {
        Ok(creds) => match KakaoRestClient::new(creds.clone()) {
            Ok(client) => match client.verify_token() {
                Ok(true) => {
                    checks.push(Check {
                        name: "REST API token".into(),
                        status: CheckStatus::Ok,
                        detail: format!("Valid (user_id={})", creds.user_id),
                        ..Default::default()
                    });
                }
                Ok(false) => {
                    checks.push(Check {
                        name: "REST API token".into(),
                        status: CheckStatus::Fail,
                        detail: "Token rejected. Open KakaoTalk, browse chats, then re-login."
                            .into(),
                        ..Default::default()
                    });
                }
                Err(e) => {
                    checks.push(Check {
                        name: "REST API token".into(),
                        status: CheckStatus::Fail,
                        detail: format!("Request failed: {}", e),
                        ..Default::default()
                    });
                }
            },
            Err(e) => {
                checks.push(Check {
                    name: "REST API token".into(),
                    status: CheckStatus::Fail,
                    detail: format!("Client init failed: {}", e),
                    ..Default::default()
                });
            }
        },
        Err(e) => {
            checks.push(Check {
                name: "REST API token".into(),
                status: CheckStatus::Fail,
                detail: format!("No credentials: {}", e),
                ..Default::default()
            });
        }
    }

    // 6. LOCO booking connectivity (optional)
    if test_loco {
        if let Ok(creds) = &creds_result {
            let rt = tokio::runtime::Runtime::new()?;
            let loco_creds = creds.clone();
            match rt.block_on(async {
                let client = crate::loco::client::LocoClient::new(loco_creds);
                client.booking().await
            }) {
                Ok(config) => {
                    let hosts = config
                        .get_document("ticket")
                        .ok()
                        .and_then(|t| t.get_array("lsl").ok())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "none".into());
                    let ports = config
                        .get_document("wifi")
                        .ok()
                        .and_then(|w| w.get_array("ports").ok())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_i32())
                                .map(|p| p.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "none".into());
                    checks.push(Check {
                        name: "LOCO booking (GETCONF)".into(),
                        status: CheckStatus::Ok,
                        detail: format!("hosts=[{}], ports=[{}]", hosts, ports),
                        ..Default::default()
                    });
                }
                Err(e) => {
                    checks.push(Check {
                        name: "LOCO booking (GETCONF)".into(),
                        status: CheckStatus::Fail,
                        detail: format!("Connection failed: {}", e),
                        ..Default::default()
                    });
                }
            }
        } else {
            checks.push(Check {
                name: "LOCO booking (GETCONF)".into(),
                status: CheckStatus::Fail,
                detail: "Skipped (no credentials)".into(),
                ..Default::default()
            });
        }
    }

    // 7. Local database (SQLCipher) access
    match crate::local_db::LocalDbReader::check_access() {
        Ok(status) => {
            let (db_status, detail) = if status.decryptable {
                (
                    CheckStatus::Ok,
                    format!(
                        "Decryptable. Path: {}",
                        status.db_path.as_deref().unwrap_or("unknown")
                    ),
                )
            } else if !status.container_exists {
                (
                    CheckStatus::Fail,
                    "KakaoTalk container directory not found".into(),
                )
            } else if !status.uuid_available {
                (
                    CheckStatus::Fail,
                    "IOPlatformUUID not available (ioreg failed)".into(),
                )
            } else if !status.user_id_available {
                (
                    CheckStatus::Fail,
                    "User ID not found in KakaoTalk preferences".into(),
                )
            } else if !status.db_file_found {
                (
                    CheckStatus::Warn,
                    "Database file not found (key derivation may differ)".into(),
                )
            } else {
                (
                    CheckStatus::Warn,
                    format!(
                        "File found but decryption failed. Path: {}",
                        status.db_path.as_deref().unwrap_or("unknown")
                    ),
                )
            };
            checks.push(Check {
                name: "Local DB (SQLCipher)".into(),
                status: db_status,
                detail,
                ..Default::default()
            });
        }
        Err(e) => {
            checks.push(Check {
                name: "Local DB (SQLCipher)".into(),
                status: CheckStatus::Warn,
                detail: format!("Check failed: {}", e),
                ..Default::default()
            });
        }
    }

    // 7a2. Notification access (notif-watch receive path)
    let notif = crate::commands::notif_watch::check_access();
    checks.push(Check {
        name: "Notification access (notif-watch)".into(),
        // Warn (not Fail) either way — notif-watch is one optional receive path.
        status: if notif.db_readable && notif.kakao_registered && notif.schema_compatible {
            CheckStatus::Ok
        } else {
            CheckStatus::Warn
        },
        detail: notif.detail,
        ..Default::default()
    });

    // 7b. LOCO write safety
    checks.push(Check {
        name: "LOCO write operations".into(),
        status: if config.safety.allow_loco_write {
            CheckStatus::Warn
        } else {
            CheckStatus::Ok
        },
        detail: if config.safety.allow_loco_write {
            "ENABLED — send/delete/edit/react allowed (account ban risk)".into()
        } else {
            "Disabled (safe). Enable with safety.allow_loco_write = true".into()
        },
        ..Default::default()
    });

    // 8. Protocol constants
    checks.push(Check {
        name: "Protocol constants".into(),
        status: CheckStatus::Ok,
        detail: format!(
            "handshake_key_type=16, encrypt_type=3 (AES-128-GCM), RSA=2048-bit e=3, booking={}:{}",
            "booking-loco.kakao.com", 443
        ),
        ..Default::default()
    });

    // Output
    if json {
        let items: Vec<serde_json::Value> = checks.iter().map(check_json).collect();
        let out = serde_json::json!({
            "checks": items,
            "recovery_state": recovery,
            "safety_state": safety,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("openkakao-cli doctor (v{})", VERSION);
        println!();
        print_checks(&checks);

        if !test_loco {
            println!();
            println!("  Tip: run with --loco to also test LOCO booking connectivity.");
        }
        println!(
            "  Tip: run 'openkakao-cli auth-status --json' for the raw persisted recovery state."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homebrew_trust_accepts_the_formula_or_its_tap() {
        assert!(homebrew_trusts_formula(
            r#"{"trustedformulae": ["junghoonghae/openkakao/openkakao-cli"], "trustedtaps": []}"#
        ));
        assert!(homebrew_trusts_formula(
            r#"{"trustedformulae": [], "trustedtaps": ["junghoonghae/openkakao"]}"#
        ));
        assert!(!homebrew_trusts_formula(
            r#"{"trustedformulae": ["someone/else/openkakao-cli"]}"#
        ));
        assert!(!homebrew_trusts_formula("not json"));
    }

    #[test]
    fn check_json_includes_code_and_fix_only_when_present() {
        let plain = check_json(&Check {
            name: "x".into(),
            detail: "fine".into(),
            ..Default::default()
        });
        assert!(plain.get("code").is_none() && plain.get("fix").is_none());
        let coded = check_json(&Check {
            name: "y".into(),
            status: CheckStatus::Warn,
            code: Some("chat_list_filtered"),
            fix: Some("clear it".into()),
            ..Default::default()
        });
        assert_eq!(coded["status"], "warn");
        assert_eq!(coded["code"], "chat_list_filtered");
        assert_eq!(coded["fix"], "clear it");
    }
}
