use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("openkakao-cli").unwrap()
}

#[test]
fn help_exits_zero() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OpenKakao Rust CLI"));
}

#[test]
fn help_lists_expected_subcommands() {
    let output = cmd().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    for subcmd in &[
        "auth",
        "chats",
        "read",
        "send",
        "watch",
        "doctor",
        "members",
        "delete",
        "mark-read",
        "safe-send",
    ] {
        assert!(
            stdout.contains(subcmd),
            "--help output should list '{}' subcommand",
            subcmd
        );
    }
}

#[test]
fn version_prints_correct_version() {
    let version = env!("CARGO_PKG_VERSION");
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(version));
}

#[test]
fn invalid_subcommand_exits_nonzero() {
    cmd()
        .arg("nonexistent-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn send_without_args_fails() {
    cmd().arg("send").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn read_without_chat_id_fails() {
    cmd().arg("read").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn json_flag_is_global() {
    // --json should be accepted before any subcommand
    // doctor doesn't require credentials for basic checks
    cmd().args(["--json", "--help"]).assert().success();
}

#[test]
fn no_color_flag_is_global() {
    cmd().args(["--no-color", "--help"]).assert().success();
}

#[test]
fn watch_accepts_capture_flag() {
    cmd()
        .args(["watch", "--capture", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("capture"));
}

#[test]
fn probe_accepts_capture_pushes_flag() {
    cmd()
        .args(["probe", "PING", "--capture-pushes", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("capture-pushes"));
}

#[test]
fn delete_help_works() {
    cmd()
        .args(["delete", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Delete a message"));
}

#[test]
fn delete_without_args_fails() {
    cmd().arg("delete").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn mark_read_help_works() {
    cmd()
        .args(["mark-read", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mark messages as read"));
}

#[test]
fn mark_read_without_args_fails() {
    cmd().arg("mark-read").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn doctor_json_outputs_valid_json() {
    let output = cmd().args(["--json", "doctor"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --json output should be valid JSON");
    assert!(
        parsed.get("checks").is_some(),
        "doctor --json output should have 'checks' key"
    );
    assert!(
        parsed["checks"].is_array(),
        "doctor --json 'checks' should be an array"
    );
}

#[test]
fn auth_status_json_outputs_valid_json() {
    let output = cmd().args(["--json", "auth-status"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth-status --json output should be valid JSON");
    assert!(
        parsed.get("consecutive_failures").is_some(),
        "auth-status --json output should have 'consecutive_failures' key"
    );
    assert!(
        parsed.get("path").is_some(),
        "auth-status --json output should have 'path' key"
    );
}

#[test]
fn cache_stats_json_outputs_valid_json() {
    let output = cmd().args(["--json", "cache-stats"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("cache-stats --json output should be valid JSON");
    assert!(
        parsed.get("total_messages").is_some(),
        "cache-stats --json output should have 'total_messages' key"
    );
    assert!(
        parsed.get("chats").is_some(),
        "cache-stats --json output should have 'chats' key"
    );
    assert!(
        parsed["chats"].is_array(),
        "cache-stats --json 'chats' should be an array"
    );
}

#[test]
fn safe_send_propose_and_list_are_local_only() {
    let home = tempfile::tempdir().unwrap();
    let output = cmd()
        .env("HOME", home.path())
        .args([
            "--json",
            "--no-prefix",
            "safe-send",
            "propose",
            "허용된 방",
            "검토할 답장",
            "--reply-chat-id",
            "42",
            "--reply-log-id",
            "99",
            "--idempotency-key",
            "reply:42:99:policy-v1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "safe-send propose failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let proposed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(proposed["state"], "proposed");
    assert_eq!(proposed["chat_name"], "허용된 방");
    assert_eq!(proposed["message"], "검토할 답장");
    assert_eq!(proposed["reply_to"], serde_json::json!([42, 99]));
    assert_eq!(proposed["approval_code"].as_str().unwrap().len(), 12);

    let listed = cmd()
        .env("HOME", home.path())
        .args(["--json", "safe-send", "list"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let active: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(active.as_array().unwrap().len(), 1);
    assert_eq!(active[0]["intent_id"], proposed["intent_id"]);

    cmd()
        .env("HOME", home.path())
        .args([
            "--unattended",
            "safe-send",
            "approve",
            proposed["intent_id"].as_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no unattended bypass"));
}

fn write_png(path: &std::path::Path) {
    // 1x1 RGBA PNG; only the magic bytes matter for validation, but keep it a
    // real image so the fixture stays honest.
    let png = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89,
    ];
    std::fs::write(path, png).unwrap();
}

#[test]
fn local_send_photo_dry_run_emits_json_with_canonicalized_path() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("vacation.png");
    write_png(&photo);
    let link = dir.path().join("latest-link.png");
    std::os::unix::fs::symlink(&photo, &link).unwrap();

    let output = cmd()
        .env("HOME", dir.path())
        .args([
            "--json",
            "local-send-photo",
            "Kim Chiang",
            link.to_str().unwrap(),
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["dry_run"], serde_json::json!(true));
    assert_eq!(parsed["action"], "local_send_photo");
    assert_eq!(parsed["chat_name"], "Kim Chiang");
    // The symlink must be reported as its canonical target, not the link path.
    assert_eq!(
        parsed["file"].as_str().unwrap(),
        photo.canonicalize().unwrap().to_str().unwrap()
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Would AX-send photo"),
        "dry-run summary should go to stderr so stdout stays pure JSON"
    );
}

#[test]
fn local_send_photo_dry_run_rejects_spoofed_and_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let spoofed = dir.path().join("photo.jpg");
    std::fs::write(&spoofed, b"just text wearing a .jpg extension").unwrap();

    cmd()
        .env("HOME", dir.path())
        .args(["local-send-photo", "Kim Chiang"])
        .arg(&spoofed)
        .args(["--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported or invalid photo data",
        ));

    let missing = dir.path().join("missing.png");
    cmd()
        .env("HOME", dir.path())
        .args(["local-send-photo", "Kim Chiang"])
        .arg(&missing)
        .args(["--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("photo does not exist"));
}

#[test]
fn local_send_photo_real_send_is_blocked_until_allowlisted_and_confirmed() {
    let home = tempfile::tempdir().unwrap();
    let config_dir = home.path().join(".config").join("openkakao");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        "[safety]\nallow_ax_send = true\nallowed_send_chats = [\"Family\"]\n",
    )
    .unwrap();
    let photo = home.path().join("vacation.png");
    write_png(&photo);

    // Not allowlisted: refuse before any KakaoTalk interaction.
    cmd()
        .env("HOME", home.path())
        .args(["local-send-photo", "Kim Chiang"])
        .arg(&photo)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not in the local-send allowlist"));

    // Allowlisted but declined at the confirmation prompt: cancel cleanly
    // without ever reaching AX automation (safe on any runner OS).
    std::fs::write(
        config_dir.join("config.toml"),
        "[safety]\nallow_ax_send = true\nallowed_send_chats = [\"Kim Chiang\"]\n",
    )
    .unwrap();
    cmd()
        .env("HOME", home.path())
        .args(["local-send-photo", "Kim Chiang"])
        .arg(&photo)
        .write_stdin("n\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cancelled."))
        .stderr(predicate::str::contains("Direct AX photo send review"));
}
