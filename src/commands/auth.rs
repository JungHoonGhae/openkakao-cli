use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::auth::{count_authorization_rows, extract_refresh_token, get_credential_candidates};
use crate::auth_flow::{
    attempt_relogin, attempt_renew, credentials_from_auth_response, select_best_credential,
    RecoveryAttempt,
};
use crate::credentials::save_credentials;
use crate::loco;
use crate::loco_helpers::try_renew_token;
use crate::model::KakaoCredentials;
use crate::rest::KakaoRestClient;
use crate::state::recovery_snapshot;
use crate::util::{color_enabled, get_creds, mask_token, print_loco_error_hint};

pub fn cmd_auth(json: bool) -> Result<()> {
    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds.clone())?;
    let valid = client.verify_token()?;

    if json {
        let out = serde_json::json!({
            "user_id": creds.user_id,
            "token_prefix": creds.oauth_token.chars().take(8).collect::<String>(),
            "app_version": creds.app_version,
            "valid": valid,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("  User ID: {}", creds.user_id);
    println!(
        "  Token:   {}...",
        creds.oauth_token.chars().take(8).collect::<String>()
    );
    println!("  Version: {}", creds.app_version);

    if valid {
        if color_enabled() {
            println!("  {}", "Token is valid!".green());
        } else {
            println!("  Token is valid!");
        }
    } else {
        if color_enabled() {
            println!("  {}", "Token is invalid or expired.".red());
        } else {
            println!("  Token is invalid or expired.");
        }
        println!(
            "  Hint: open KakaoTalk, open chat list once, then run 'openkakao-cli login --save'."
        );
    }

    Ok(())
}

pub fn cmd_auth_status(json: bool) -> Result<()> {
    let snapshot = recovery_snapshot()?;

    if json {
        let out = serde_json::json!({
            "path": snapshot.path,
            "last_success_at": snapshot.last_success_at,
            "last_success_transport": snapshot.last_success_transport,
            "last_recovery_source": snapshot.last_recovery_source,
            "last_failure_kind": snapshot.last_failure_kind,
            "last_failure_at": snapshot.last_failure_at,
            "consecutive_failures": snapshot.consecutive_failures,
            "cooldown_until": snapshot.cooldown_until,
            "auth_cooldown_remaining_secs": snapshot.auth_cooldown_remaining_secs,
            "relogin_available_in_secs": snapshot.relogin_available_in_secs,
            "renew_available_in_secs": snapshot.renew_available_in_secs,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("Auth recovery state");
    println!("  State file:            {}", snapshot.path);
    println!(
        "  Last success:          {}",
        snapshot.last_success_at.as_deref().unwrap_or("never")
    );
    println!(
        "  Last transport:        {}",
        snapshot
            .last_success_transport
            .as_deref()
            .unwrap_or("unknown")
    );
    println!(
        "  Last recovery source:  {}",
        snapshot.last_recovery_source.as_deref().unwrap_or("none")
    );
    println!(
        "  Last failure kind:     {}",
        snapshot.last_failure_kind.as_deref().unwrap_or("none")
    );
    println!(
        "  Last failure at:       {}",
        snapshot.last_failure_at.as_deref().unwrap_or("never")
    );
    println!("  Consecutive failures:  {}", snapshot.consecutive_failures);
    println!(
        "  Auth cooldown:         {}",
        format_remaining(
            snapshot.auth_cooldown_remaining_secs,
            snapshot.cooldown_until.as_deref()
        )
    );
    println!(
        "  Relogin available in:  {}",
        format_simple_remaining(snapshot.relogin_available_in_secs)
    );
    println!(
        "  Renew available in:    {}",
        format_simple_remaining(snapshot.renew_available_in_secs)
    );
    Ok(())
}

pub fn format_simple_remaining(value: Option<u64>) -> String {
    match value {
        Some(secs) => format!("{}s", secs),
        None => "now".to_string(),
    }
}

pub fn format_remaining(remaining_secs: Option<u64>, until: Option<&str>) -> String {
    match (remaining_secs, until) {
        (Some(secs), Some(until)) => format!("{}s (until {})", secs, until),
        (Some(secs), None) => format!("{}s", secs),
        _ => "none".to_string(),
    }
}

fn print_login_extraction_hint() {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            println!("Could not extract credentials. (failed to resolve home directory)");
            return;
        }
    };
    let cache_db =
        home.join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Caches/Cache.db");

    if !cache_db.exists() {
        println!("Could not extract credentials.");
        println!("  Cache.db not found at:");
        println!("    {}", cache_db.display());
        println!(
            "  Open the KakaoTalk macOS app, sign in, then click a chat at least once so the app"
        );
        println!("  populates its HTTP cache. Then retry 'openkakao-cli login --save'.");
        return;
    }

    match std::fs::File::open(&cache_db) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            println!("Could not extract credentials.");
            println!("  Cache.db exists but is not readable:");
            println!("    {}", cache_db.display());
            println!("  Grant Full Disk Access to your terminal in:");
            println!("    System Settings → Privacy & Security → Full Disk Access");
            println!("  Then fully quit and reopen the terminal and retry.");
        }
        _ => {
            let auth_rows = count_authorization_rows().ok().flatten();
            match auth_rows {
                Some(0) => {
                    println!("Could not extract credentials.");
                    println!(
                        "  Cache.db is readable but contains zero entries with an Authorization"
                    );
                    println!(
                        "  header. Recent KakaoTalk macOS builds no longer cache authenticated"
                    );
                    println!(
                        "  REST responses to NSURLCache, so 'login --save' cannot recover the"
                    );
                    println!("  token from this path on these versions.");
                    println!();
                    println!(
                        "  Tracking issue: https://github.com/JungHoonGhae/openkakao-cli/issues/15"
                    );
                    println!();
                    println!("  Use email + password login instead — it does not need the cache:");
                    println!("    openkakao-cli login --manual --save");
                }
                _ => {
                    println!("Could not extract credentials.");
                    println!("  Cache.db is readable but no candidates passed parsing:");
                    println!("    {}", cache_db.display());
                    println!(
                        "  Open KakaoTalk, click a chat or refresh the friend list so the app issues a"
                    );
                    println!("  REST call, then retry 'openkakao-cli login --save'.");
                    println!(
                        "  (Set OPENKAKAO_CLI_DEBUG=1 to see which candidates the scan inspected.)"
                    );
                }
            }
        }
    }
}

pub fn cmd_login(save: bool) -> Result<()> {
    let candidates = get_credential_candidates(8)?;
    let Some(_) = candidates.first() else {
        print_login_extraction_hint();
        return Ok(());
    };
    let creds = select_best_credential(candidates)?;

    println!("Credentials extracted!");
    println!("  User ID: {}", creds.user_id);
    println!(
        "  Token:   {}...",
        creds.oauth_token.chars().take(8).collect::<String>()
    );

    let client = KakaoRestClient::new(creds.clone())?;
    if client.verify_token()? {
        println!("  Token verified OK");
    } else {
        println!("  Token may be expired for some operations");
    }

    if save {
        let path = save_credentials(&creds)?;
        println!("Credentials saved to {}", path.display());
    }

    Ok(())
}

/// Fallback app version string used to build the REST `A`/`User-Agent` headers for
/// a from-scratch login when the installed KakaoTalk.app version cannot be read.
///
/// KakaoTalk's macOS REST API rejects logins whose version string is older than the
/// server's minimum with `status=-999` ("최신버전으로 업데이트가 필요합니다"). The real
/// app sends its own bundle version, so we prefer the installed version (see
/// [`installed_kakaotalk_version`]) and only fall back to this constant when the app
/// is missing. Override either with `--app-version`. Keep this in sync with a recent
/// KakaoTalk macOS release.
const DEFAULT_LOGIN_APP_VERSION: &str = "26.5.0";

/// Device name registered for this client and shown in KakaoTalk's device list.
const DEVICE_NAME: &str = "openkakao-cli";

/// `login.json` status returned when the device UUID has never been registered on the
/// account. KakaoTalk requires a passcode-based device registration before it will
/// hand out a token for an unseen device. See [`register_new_device`].
const DEVICE_NOT_REGISTERED: i64 = -100;

/// Read the version string the locally installed KakaoTalk app reports
/// (`CFBundleShortVersionString`). This is exactly what the real app puts in its REST
/// `A`/`User-Agent` headers, so using it keeps `login --manual` working across
/// KakaoTalk updates without a hardcoded version that the server later rejects.
fn installed_kakaotalk_version() -> Option<String> {
    openkakao_cli::kakaotalk_app::installed_app()
        .map(|app| app.version)
        .filter(|version| !version.trim().is_empty() && version != "unknown")
}

/// Resolve the app version string for `login --manual`, in priority order:
/// explicit `--app-version` override → installed KakaoTalk.app version → fallback
/// constant. Blank/whitespace candidates are ignored so they never reach the server.
fn resolve_login_app_version(
    override_version: Option<String>,
    installed_version: Option<String>,
) -> String {
    override_version
        .filter(|s| !s.trim().is_empty())
        .or_else(|| installed_version.filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| DEFAULT_LOGIN_APP_VERSION.to_string())
}

/// Log in with email + password instead of scraping the KakaoTalk cache.
///
/// This path does not depend on `Cache.db`: the device UUID is read from
/// `IOPlatformUUID`, the X-VC header is computed locally, and the access token
/// comes straight from `login.json`. It is the recommended path on recent
/// KakaoTalk builds that no longer cache the bearer token (#15).
pub fn cmd_login_manual(
    save: bool,
    email: Option<String>,
    password: Option<String>,
    app_version: Option<String>,
) -> Result<()> {
    let email = match email {
        Some(e) if !e.trim().is_empty() => e.trim().to_string(),
        _ => prompt_line("Email or phone number: ")?,
    };
    if email.is_empty() {
        anyhow::bail!("email/phone is required");
    }

    let password = match password {
        Some(p) if !p.is_empty() => p,
        _ => rpassword::prompt_password("Password (input hidden): ")
            .context("failed to read password")?,
    };
    if password.is_empty() {
        anyhow::bail!("password is required");
    }

    let device_uuid = crate::local_db::get_platform_uuid()
        .context("could not read device UUID from IOPlatformUUID")?;
    let app_version = resolve_login_app_version(app_version, installed_kakaotalk_version());
    eprintln!("Using app version {} for login headers.", app_version);

    // Minimal credential shell so the REST client can build its headers.
    let base = KakaoCredentials::new(
        String::new(),
        0,
        device_uuid.clone(),
        app_version,
        String::new(),
        String::new(),
    );
    let client = KakaoRestClient::new(base.clone())?;

    eprintln!("Logging in as {} ...", email);
    let response = client.login_with_xvc(&email, &password, &device_uuid, DEVICE_NAME)?;
    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);

    // -100 = this device_uuid is not registered on the account yet. Recent KakaoTalk
    // macOS builds removed the passcode/register_device REST endpoints (they 404), so
    // there is no automated way to register the device from the CLI. Stop here with a
    // clear warning instead of retrying — repeated logins from an unregistered device
    // can get the account's sub-device login blocked (#20, #22).
    if status == DEVICE_NOT_REGISTERED {
        print_device_not_registered_warning();
        anyhow::bail!("login failed (status=-100, device not registered)");
    }

    if status != 0 {
        let message = response
            .get("message")
            .or_else(|| response.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("");
        print_manual_login_failure(status, message);
        anyhow::bail!("login failed (status={})", status);
    }

    let mut creds = credentials_from_auth_response(&base, &response);
    creds.email = Some(email);
    if creds.oauth_token.is_empty() {
        anyhow::bail!("login reported success but the response had no access_token");
    }

    println!("Login OK!");
    println!("  User ID: {}", creds.user_id);
    println!("  Token:   {}...", mask_token(&creds.oauth_token));

    if save {
        let path = save_credentials(&creds)?;
        println!("Credentials saved to {}", path.display());
    } else {
        println!("(not saved — re-run with --save to persist)");
    }

    Ok(())
}

/// Explain `status=-100` (device not registered) and, critically, warn the user not to
/// keep retrying. A passcode/device-registration REST flow once existed in other Kakao
/// clients, but recent KakaoTalk macOS builds no longer expose it (the endpoints 404),
/// so the CLI cannot register a new device. Repeated attempts risk an account block.
fn print_device_not_registered_warning() {
    eprintln!();
    eprintln!("This Mac is not registered on your KakaoTalk account, and KakaoTalk's");
    eprintln!("current macOS builds no longer expose an endpoint to register it from a");
    eprintln!("third-party client (the passcode/register_device routes return 404).");
    eprintln!("openkakao-cli cannot complete this login. (#20, #22)");
    eprintln!();
    eprintln!("  🚨 Do NOT keep retrying. Repeated logins from an unregistered device");
    eprintln!("     can get your account's sub-device login blocked or the account");
    eprintln!("     restricted. This has actually happened to other users.");
    eprintln!();
    eprintln!("  Server login is unfixed on current KakaoTalk builds — but the CLI still");
    eprintln!("  works without it: 'local-send'/'ax-read'/'local-chats' need no login at all.");
}

fn print_manual_login_failure(status: i64, message: &str) {
    eprintln!("Login failed (status={}).", status);
    if !message.is_empty() {
        eprintln!("  Server message: {}", message);
    }
    if status == -999 {
        // The server rejects the version string as too old. We default to the
        // installed KakaoTalk.app version, but if that app is outdated (or missing)
        // the server still refuses. Surface a targeted hint.
        eprintln!("  KakaoTalk reports this client version is too old to log in.");
        eprintln!("  Update KakaoTalk to the latest version, then retry. If you are");
        eprintln!("  already up to date, pass the current app version explicitly:");
        eprintln!("    openkakao-cli login --manual --save --app-version <X.Y.Z>");
        eprintln!("  (find the version in KakaoTalk > About, e.g. 26.5.0)");
        return;
    }
    // -100 is handled separately; anything else here is most likely a wrong
    // email/phone or password.
    eprintln!("  Double-check the email/phone and password and try again.");
}

fn prompt_line(label: &str) -> Result<String> {
    use std::io::Write;
    print!("{}", label);
    std::io::stdout().flush().ok();
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    Ok(input.trim().to_string())
}

pub fn cmd_renew(json: bool) -> Result<()> {
    let creds = get_creds()?;
    eprintln!("Trying refresh_token renewal...");

    match attempt_renew(&creds)? {
        RecoveryAttempt::Unavailable { reason, .. } => {
            eprintln!("  {}.", reason);
            eprintln!("  Hint: Open KakaoTalk app and wait for it to auto-renew, then retry.");
        }
        RecoveryAttempt::Failed {
            source,
            detail,
            response,
        } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "outcome": "failed",
                        "source": source,
                        "detail": detail,
                        "response": response,
                    }))?
                );
            } else {
                eprintln!("Token renewal failed via {}.", source);
                eprintln!("Detail: {}", detail);
                if let Some(response) = response {
                    eprintln!("Response: {}", serde_json::to_string_pretty(&response)?);
                }
            }
        }
        RecoveryAttempt::Recovered {
            source,
            credentials,
            response,
        } => {
            save_credentials(&credentials)?;
            return print_renew_result(json, source, &response);
        }
    }

    Ok(())
}

pub fn print_renew_result(json: bool, source: &str, response: &Value) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "outcome": "recovered",
                "source": source,
                "response": response,
            }))?
        );
    } else {
        eprintln!("  Token renewed successfully via {}!", source);
        if let Some(access) = response.get("access_token").and_then(Value::as_str) {
            println!("New access_token: {}...", &access[..8.min(access.len())]);
        }
        if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
            println!("New refresh_token: {}...", &refresh[..8.min(refresh.len())]);
        }
        if let Some(obj) = response.as_object() {
            for (k, v) in obj {
                if k == "access_token" || k == "refresh_token" || k == "status" {
                    continue;
                }
                eprintln!("  {}: {}", k, v);
            }
        }
    }
    Ok(())
}

pub fn cmd_relogin(
    json: bool,
    fresh_xvc: bool,
    password_override: Option<String>,
    email_override: Option<String>,
) -> Result<()> {
    let creds = get_creds()?;
    eprintln!("Resolving login parameters...");
    match attempt_relogin(
        &creds,
        fresh_xvc,
        password_override.as_deref(),
        email_override.as_deref(),
    )? {
        RecoveryAttempt::Unavailable { reason, .. } => {
            eprintln!("  {}.", reason);
        }
        RecoveryAttempt::Failed {
            source,
            detail,
            response,
        } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "outcome": "failed",
                        "source": source,
                        "detail": detail,
                        "response": response,
                    }))?
                );
                return Ok(());
            }

            eprintln!("  Relogin failed via {}.", source);
            eprintln!("  Detail: {}", detail);
            if let Some(response) = response {
                let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
                let msg = response
                    .get("message")
                    .or_else(|| response.get("msg"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !msg.is_empty() {
                    eprintln!("  Server message: {} (status={})", msg, status);
                }
            }
        }
        RecoveryAttempt::Recovered {
            source,
            credentials,
            response,
        } => {
            save_credentials(&credentials)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "outcome": "recovered",
                        "source": source,
                        "response": response,
                    }))?
                );
                return Ok(());
            }

            let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
            eprintln!("  Status: {}", status);
            if let Some(access) = response.get("access_token").and_then(Value::as_str) {
                eprintln!("  access_token: {}", mask_token(access));
            }
            if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
                eprintln!("  refresh_token: {}", mask_token(refresh));
            }
            eprintln!("  Credentials saved via {}.", source);
        }
    }

    Ok(())
}

pub fn cmd_loco_test() -> Result<()> {
    let creds = get_creds()?;

    eprintln!("Testing LOCO connection for user {}...", creds.user_id);
    eprintln!(
        "  Token: {}...",
        creds.oauth_token.chars().take(8).collect::<String>()
    );

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds.clone());
        let login_data = client.full_connect_with_retry(3).await?;

        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        let user_id = login_data
            .get_i64("userId")
            .or_else(|_| login_data.get_i32("userId").map(|v| v as i64))
            .unwrap_or(0);

        if status == 0 && user_id > 0 {
            println!("LOCO connection successful!");
            println!("  User ID: {}", user_id);

            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                println!("  Chat rooms: {}", chat_datas.len());
                for cd in chat_datas.iter() {
                    if let Some(doc) = cd.as_document() {
                        let cid = doc
                            .get_i64("c")
                            .or_else(|_| doc.get_i32("c").map(|v| v as i64))
                            .unwrap_or(0);
                        let ctype = doc.get_str("t").unwrap_or("?");
                        let members = doc.get_array("m").map(|a| a.len()).unwrap_or(0);
                        let li = doc.get_i64("ll").unwrap_or(0);
                        println!(
                            "    {} (type={}, members={}, lastLog={})",
                            cid, ctype, members, li
                        );
                    }
                }
            }
        } else {
            println!("LOCO login returned status={}", status);
            print_loco_error_hint(status);

            // Print the full response for debugging
            eprintln!("\nFull LOGINLIST response:");
            for (k, v) in login_data.iter() {
                if k != "chatDatas" && k != "revision" {
                    eprintln!("  {}: {:?}", k, v);
                }
            }
        }

        Ok(())
    })
}

pub fn cmd_watch_cache(interval: u64) -> Result<()> {
    eprintln!(
        "Watching Cache.db for fresh tokens (interval={}s)...",
        interval
    );
    eprintln!("Open KakaoTalk and use it normally. Press Ctrl-C to stop.");

    let mut last_token = extract_refresh_token()?.unwrap_or_default();
    let mut last_oauth = get_credential_candidates(1)?
        .first()
        .map(|c| c.oauth_token.clone())
        .unwrap_or_default();

    if !last_token.is_empty() {
        eprintln!(
            "  Current refresh_token: {}...",
            last_token.chars().take(8).collect::<String>()
        );
    }
    if !last_oauth.is_empty() {
        eprintln!(
            "  Current oauth_token:   {}...",
            last_oauth.chars().take(8).collect::<String>()
        );
    }

    loop {
        std::thread::sleep(std::time::Duration::from_secs(interval));

        // Check refresh_token
        if let Ok(Some(rt)) = extract_refresh_token() {
            if rt != last_token {
                if color_enabled() {
                    eprintln!("{}", "NEW refresh_token detected!".green().bold());
                } else {
                    eprintln!("NEW refresh_token detected!");
                }
                eprintln!("  {}...", rt.chars().take(60).collect::<String>());
                last_token = rt.clone();

                // Try renewal immediately
                if let Ok(creds) = get_creds() {
                    match try_renew_token(&creds, &rt) {
                        Ok(Some(new_token)) => {
                            if color_enabled() {
                                eprintln!("{}", "Token renewal SUCCEEDED!".green().bold());
                            } else {
                                eprintln!("Token renewal SUCCEEDED!");
                            }
                            eprintln!(
                                "  New access_token: {}...",
                                new_token.chars().take(8).collect::<String>()
                            );
                            // Save the new credentials
                            let mut new_creds = creds.clone();
                            new_creds.oauth_token = new_token;
                            new_creds.refresh_token = Some(rt);
                            if let Ok(path) = save_credentials(&new_creds) {
                                eprintln!("  Saved to {}", path.display());
                            }
                        }
                        Ok(None) => eprintln!("  Renewal returned no access_token."),
                        Err(e) => eprintln!("  Renewal failed: {}", e),
                    }
                }
            }
        }

        // Check oauth_token
        if let Ok(candidates) = get_credential_candidates(1) {
            if let Some(cand) = candidates.first() {
                if cand.oauth_token != last_oauth {
                    if color_enabled() {
                        eprintln!("{}", "NEW oauth_token detected!".green().bold());
                    } else {
                        eprintln!("NEW oauth_token detected!");
                    }
                    eprintln!(
                        "  {}...",
                        cand.oauth_token.chars().take(8).collect::<String>()
                    );
                    last_oauth = cand.oauth_token.clone();
                }
            }
        }

        eprint!(".");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_installed_and_default() {
        let resolved =
            resolve_login_app_version(Some("9.9.9".to_string()), Some("26.5.0".to_string()));
        assert_eq!(resolved, "9.9.9");
    }

    #[test]
    fn installed_used_when_no_override() {
        // Regression for #18: a from-scratch login must send the real installed
        // version, not the stale hardcoded one, or the server replies status=-999.
        let resolved = resolve_login_app_version(None, Some("26.5.0".to_string()));
        assert_eq!(resolved, "26.5.0");
    }

    #[test]
    fn blank_candidates_fall_through_to_default() {
        let resolved = resolve_login_app_version(Some("   ".to_string()), Some("".to_string()));
        assert_eq!(resolved, DEFAULT_LOGIN_APP_VERSION);
        assert_eq!(
            resolve_login_app_version(None, None),
            DEFAULT_LOGIN_APP_VERSION
        );
    }

    #[test]
    fn default_is_not_the_rejected_legacy_version() {
        // The old default "3.7.0" is now rejected by KakaoTalk's REST API (#18).
        assert_ne!(DEFAULT_LOGIN_APP_VERSION, "3.7.0");
    }

    #[test]
    fn device_not_registered_code_is_stable() {
        // -100 drives the deprecation warning path in cmd_login_manual (#20, #22).
        assert_eq!(DEVICE_NOT_REGISTERED, -100);
    }
}
