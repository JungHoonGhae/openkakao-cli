//! End-to-end recovery of the active KakaoTalk user ID from 26.7-style
//! preferences, driven through the public `LocalDbReader::check_access()` —
//! the same entry point behind `openkakao-cli doctor`'s "Local DB (SQLCipher)"
//! check and every `local` read command.
//!
//! KakaoTalk 26.7.0 replaced the numeric `FSChatWindowFrame_` preference
//! suffixes with opaque hashes, so the plist only identifies the active
//! account through its `DESIGNATEDFRIENDSREVISION:<sha512(userId)>` marker.
//! These tests build a simulated KakaoTalk home (binary plist + saved
//! credentials from `login --save`) and observe whether user-ID recovery
//! succeeds, without touching any real KakaoTalk data or sending anything.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use openkakao_cli::local_db::{LocalDbReader, LocalDbStatus};
use sha2::Digest;

/// `dirs::home_dir()` reads $HOME, and the tests share one process.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn sha512_hex(value: &str) -> String {
    hex::encode(sha2::Sha512::digest(value.as_bytes()))
}

/// Write a KakaoTalk 26.7-style container preference plist: opaque (non-numeric)
/// `FSChatWindowFrame_` suffixes, an optional `DESIGNATEDFRIENDSREVISION:<sha512>`
/// active-account marker, and an optional legacy direct `KAKAO_USER_ID` key.
fn write_kt267_plist(
    home: &Path,
    marker_id: Option<i64>,
    marker_value: i64,
    direct_user_id: Option<i64>,
) {
    let prefs = home.join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Preferences");
    fs::create_dir_all(&prefs).unwrap();
    // Container directory that `check_access` also probes.
    fs::create_dir_all(home.join(
        "Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac",
    ))
    .unwrap();

    let mut dict = plist::Dictionary::new();
    // 26.7 replaced the numeric userId suffix with per-window opaque hashes,
    // so the exact-suffix strategy cannot recover anything here.
    dict.insert(
        "NSWindow Frame FSChatWindowFrame_a7f3c9d1e5b24608".into(),
        plist::Value::String("649 377 510 400 0 0 1512 982".into()),
    );
    dict.insert(
        "NSWindow Frame FSChatWindowFrame_3b8e2f6a9c0d1475".into(),
        plist::Value::String("649 377 510 400 0 0 1512 982".into()),
    );
    if let Some(id) = direct_user_id {
        dict.insert("KAKAO_USER_ID".into(), plist::Value::Integer(id.into()));
    }
    if let Some(id) = marker_id {
        dict.insert(
            format!("DESIGNATEDFRIENDSREVISION:{}", sha512_hex(&id.to_string())),
            plist::Value::Integer(marker_value.into()),
        );
    }
    let path = prefs.join("com.kakao.KakaoTalkMac.9c31e0ab2d47f658.plist");
    plist::Value::Dictionary(dict)
        .to_file_binary(&path)
        .unwrap();
}

/// Write the credentials file that `login --save` produces.
fn write_saved_credentials(home: &Path, user_id: i64) {
    let dir = home.join(".config").join("openkakao");
    fs::create_dir_all(&dir).unwrap();
    let creds = serde_json::json!({
        "oauth_token": "local-recovery-fixture-token",
        "user_id": user_id,
        "device_uuid": "LOCAL-FIXTURE-UUID",
        "device_name": "openkakao-cli",
        "app_version": "7.8.2",
        "user_agent": "KT/7.8.2 Mac/26.7.0 ko",
        "a_header": "mac/7.8.2/ko"
    });
    let path = dir.join("credentials.json");
    fs::write(&path, creds.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// Run `check_access()` with $HOME pointed at the simulated KakaoTalk home.
fn check_access_as(home: &Path, label: &str) -> LocalDbStatus {
    let _guard = HOME_LOCK.lock().unwrap();
    let previous = std::env::var_os("HOME");
    std::env::set_var("HOME", home);

    let started = Instant::now();
    let status = LocalDbReader::check_access().expect("check_access should not error");
    let elapsed = started.elapsed();

    if let Some(prev) = previous {
        std::env::set_var("HOME", prev);
    } else {
        std::env::remove_var("HOME");
    }

    eprintln!(
        "[{label}] took {:.2}s -> {}",
        elapsed.as_secs_f32(),
        serde_json::to_string(&status).unwrap()
    );
    status
}

/// The fix under test: a saved user ID whose SHA-512 matches the plist's
/// non-zero active-account marker is recovered on a 26.7-style plist where no
/// numeric suffix survives. The ID is large enough that the bounded brute-force
/// fallback alone cannot reach it inside its 15s budget, so a pass here means
/// the saved-ID path itself answered.
#[test]
fn kt267_saved_user_id_matching_active_marker_is_recovered() {
    let home = tempfile::tempdir().unwrap();
    write_kt267_plist(home.path(), Some(199_453_377), 1, None);
    write_saved_credentials(home.path(), 199_453_377);

    let status = check_access_as(home.path(), "26.7 plist + matching saved ID");

    assert!(
        status.user_id_available,
        "saved ID whose SHA-512 matches the active marker must be recovered: {status:?}"
    );
}

/// Stale credentials: a different account is active, so the saved ID must be
/// rejected rather than blindly reused. The active marker's pre-image sits far
/// beyond the brute-force budget, so recovery must fail outright.
#[test]
fn kt267_stale_saved_user_id_is_rejected() {
    let home = tempfile::tempdir().unwrap();
    write_kt267_plist(home.path(), Some(8_888_888_888), 1, None);
    write_saved_credentials(home.path(), 111_111_111);

    let status = check_access_as(home.path(), "26.7 plist + stale saved ID");

    assert!(
        !status.user_id_available,
        "saved ID for a non-active account must be rejected: {status:?}"
    );
}

/// Inactive credentials: the marker's revision counter is zero, so even a
/// hash-matching saved ID must not be trusted.
#[test]
fn kt267_inactive_marker_rejects_hash_matching_saved_user_id() {
    let home = tempfile::tempdir().unwrap();
    write_kt267_plist(home.path(), Some(199_453_377), 0, None);
    write_saved_credentials(home.path(), 199_453_377);

    let status = check_access_as(home.path(), "26.7 plist + zero-value marker");

    assert!(
        !status.user_id_available,
        "saved ID matching a zero-value (inactive) marker must be rejected: {status:?}"
    );
}

/// Fallback preserved: with no saved credentials at all, the bounded SHA-512
/// brute force still recovers a small user ID from the revision marker.
#[test]
fn kt267_brute_force_fallback_survives_without_saved_credentials() {
    let home = tempfile::tempdir().unwrap();
    write_kt267_plist(home.path(), Some(12_345), 1, None);

    let status = check_access_as(home.path(), "26.7 plist + no saved credentials");

    assert!(
        status.user_id_available,
        "bounded brute-force fallback must keep working without credentials: {status:?}"
    );
}

/// Fallback preserved: a legacy direct `KAKAO_USER_ID` key still wins without
/// any saved credentials or revision marker.
#[test]
fn direct_user_id_key_still_wins_without_saved_credentials() {
    let home = tempfile::tempdir().unwrap();
    write_kt267_plist(home.path(), None, 0, Some(199_453_377));

    let status = check_access_as(home.path(), "plist with direct KAKAO_USER_ID key");

    assert!(
        status.user_id_available,
        "direct-key lookup must keep working: {status:?}"
    );
}
