use std::process::Command;

use anyhow::{Context, Result};

use crate::ax_send;
use crate::error::{coded, ErrorCode};

/// Login-keychain service holding the KakaoTalk password. The human saves it
/// once with `security add-generic-password -s openkakao.kakaotalk -a <id> -w`.
const KEYCHAIN_SERVICE: &str = "openkakao.kakaotalk";

/// Sign a signed-out KakaoTalk back in from the keychain (see
/// `ax_send::login_via_ax`). The password never reaches argv, stdout or logs.
pub fn cmd_ax_login(account: Option<String>, json: bool) -> Result<()> {
    let account = account.filter(|id| !id.trim().is_empty()).ok_or_else(|| {
        coded(
            ErrorCode::LoginSecretMissing,
            "no KakaoTalk login id: pass --account or set [ax] login_account",
        )
    })?;
    let password = keychain_password(&account)?;
    let status = if ax_send::login_via_ax(&account, &password)? {
        "signed_in"
    } else {
        "already_signed_in"
    };
    if json {
        crate::util::output_json(&serde_json::json!({ "status": status }))?;
    } else {
        println!("KakaoTalk: {}", status.replace('_', " "));
    }
    Ok(())
}

/// The item for this account, else the service's only item.
fn keychain_password(account: &str) -> Result<String> {
    for args in [
        vec![
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            account,
            "-w",
        ],
        vec!["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"],
    ] {
        let output = Command::new("security")
            .args(&args)
            .output()
            .context("could not run /usr/bin/security")?;
        if !output.status.success() {
            continue;
        }
        let password =
            String::from_utf8(output.stdout).context("the keychain password is not UTF-8")?;
        let password = password.strip_suffix('\n').unwrap_or(&password);
        if !password.is_empty() {
            return Ok(password.to_string());
        }
    }
    Err(coded(
        ErrorCode::LoginSecretMissing,
        format!("no '{KEYCHAIN_SERVICE}' password in the login keychain"),
    ))
}
