use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ax_send::{self, AttachmentKind};
use crate::error::{coded, ErrorCode};
use crate::util::{confirm, escape_terminal_text};

const MIB: u64 = 1024 * 1024;

pub struct LocalSendFileOptions {
    pub chat_name: String,
    pub file: String,
    pub max_file_mb: u64,
    pub skip_confirm: bool,
    pub dry_run: bool,
    pub json: bool,
}

/// A path that passed validation before any KakaoTalk automation runs.
pub(crate) struct ValidatedFile {
    pub path: PathBuf,
    pub size: u64,
}

pub(crate) fn validate_utf8_path(path: &Path, display: &str, noun: &str) -> Result<()> {
    if path.to_str().is_none() {
        return Err(coded(
            ErrorCode::InvalidFile,
            format!("{noun} path is not valid UTF-8: {display}"),
        ));
    }
    Ok(())
}

/// Resolve symlinks and require an existing, non-empty regular file of at
/// most `max_bytes`, with a UTF-8 path (it is passed to KakaoTalk as a file
/// URL and matched by name afterwards).
pub(crate) fn validate_regular_file(
    path: &Path,
    max_bytes: u64,
    noun: &str,
) -> Result<ValidatedFile> {
    let invalid = |message: String| coded(ErrorCode::InvalidFile, message);
    let requested = escape_terminal_text(&path.to_string_lossy());
    let canonical = path.canonicalize().map_err(|error| {
        invalid(format!(
            "{noun} does not exist or is unreadable: {requested} ({error})"
        ))
    })?;
    let display = escape_terminal_text(&canonical.to_string_lossy());
    let metadata = canonical
        .metadata()
        .map_err(|error| invalid(format!("could not inspect {noun}: {display} ({error})")))?;
    if !metadata.is_file() {
        return Err(invalid(format!(
            "{noun} path is not a regular file: {display}"
        )));
    }
    validate_utf8_path(&canonical, &display, noun)?;
    if !size_allowed(metadata.len(), max_bytes) {
        return Err(invalid(format!(
            "{noun} must be between 1 byte and {} MiB: {display}",
            max_bytes / MIB
        )));
    }
    Ok(ValidatedFile {
        path: canonical,
        size: metadata.len(),
    })
}

fn size_allowed(size: u64, max_bytes: u64) -> bool {
    (1..=max_bytes).contains(&size)
}

pub fn cmd_local_send_file(opts: LocalSendFileOptions) -> Result<()> {
    let max_bytes = opts.max_file_mb.saturating_mul(MIB);
    let file = validate_regular_file(Path::new(&opts.file), max_bytes, "file")?;
    send_attachment(AttachmentSend {
        chat_name: opts.chat_name,
        file,
        kind: AttachmentKind::File,
        skip_confirm: opts.skip_confirm,
        dry_run: opts.dry_run,
        json: opts.json,
    })
}

pub(crate) struct AttachmentSend {
    pub chat_name: String,
    pub file: ValidatedFile,
    pub kind: AttachmentKind,
    pub skip_confirm: bool,
    pub dry_run: bool,
    pub json: bool,
}

/// The review, confirmation, send and report shared by `local-send-file` and
/// `local-send-photo`.
pub(crate) fn send_attachment(send: AttachmentSend) -> Result<()> {
    let noun = send.kind.noun();
    let action = match send.kind {
        AttachmentKind::Photo => "local_send_photo",
        AttachmentKind::File => "local_send_file",
    };
    let display_path = escape_terminal_text(&send.file.path.to_string_lossy());

    if send.dry_run {
        eprintln!(
            "[dry-run] Would AX-send {noun} to chat \"{}\": {display_path} ({} bytes)",
            escape_terminal_text(&send.chat_name),
            send.file.size
        );
        if send.json {
            crate::util::output_json(&serde_json::json!({
                "dry_run": true,
                "action": action,
                "chat_name": send.chat_name,
                "file": send.file.path,
                "size_bytes": send.file.size,
            }))?;
        }
        return Ok(());
    }

    eprintln!("Direct AX {noun} send review:");
    eprintln!("  target: {}", escape_terminal_text(&send.chat_name));
    eprintln!("  file:   {display_path} ({} bytes)", send.file.size);
    if !send.skip_confirm {
        eprint!("Continue to macOS device-owner authentication?\n[y/N] ");
        if !confirm()? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let receipt = ax_send::send_attachment_via_ax(&send.chat_name, &send.file.path, send.kind)?;
    if send.json {
        crate::util::output_json(&serde_json::json!({
            "chat_name": send.chat_name,
            "file": send.file.path,
            "size_bytes": send.file.size,
            "status": "sent",
            "verified_by": receipt.verified_by,
            "preview_shows_filename": receipt.preview_shows_filename,
            "staged_in_downloads": receipt.staged,
        }))?;
    } else {
        let mut noun = noun.to_string();
        noun[..1].make_ascii_uppercase();
        println!("{noun} sent! (verified by {})", receipt.verified_by);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn accepts_any_regular_file_and_resolves_symlinks() {
        let dir = tempdir().unwrap();
        let doc = dir.path().join("report.docx");
        let bytes = b"PK\x03\x04 not really a zip, and that is fine";
        fs::write(&doc, bytes).unwrap();
        let link = dir.path().join("latest.docx");
        std::os::unix::fs::symlink(&doc, &link).unwrap();
        let validated = validate_regular_file(&link, MIB, "file").unwrap();
        assert_eq!(validated.path, doc.canonicalize().unwrap());
        assert_eq!(validated.size, bytes.len() as u64);
    }

    #[test]
    fn rejects_missing_empty_directory_and_oversized_files() {
        let dir = tempdir().unwrap();
        let code = |result: Result<ValidatedFile>| {
            crate::error::error_code(&result.err().expect("should be rejected"))
        };
        assert_eq!(
            code(validate_regular_file(
                &dir.path().join("missing.pdf"),
                MIB,
                "file"
            )),
            Some(ErrorCode::InvalidFile)
        );
        assert_eq!(
            code(validate_regular_file(dir.path(), MIB, "file")),
            Some(ErrorCode::InvalidFile)
        );
        let empty = dir.path().join("empty.txt");
        fs::write(&empty, b"").unwrap();
        assert_eq!(
            code(validate_regular_file(&empty, MIB, "file")),
            Some(ErrorCode::InvalidFile)
        );
        let big = dir.path().join("big.zip");
        fs::write(&big, vec![0_u8; 11]).unwrap();
        assert!(validate_regular_file(&big, 11, "file").is_ok());
        let error = validate_regular_file(&big, 10, "file").err().unwrap();
        assert!(error.to_string().contains("must be between 1 byte and"));
    }

    #[test]
    fn size_cap_is_inclusive_and_rejects_empty() {
        assert!(!size_allowed(0, 300 * MIB));
        assert!(size_allowed(1, 300 * MIB));
        assert!(size_allowed(300 * MIB, 300 * MIB));
        assert!(!size_allowed(300 * MIB + 1, 300 * MIB));
    }

    #[test]
    #[cfg(unix)]
    fn rejects_non_utf8_filename_before_any_automation() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(OsStr::from_bytes(b"\xff\xfe.png"));
        let error = validate_utf8_path(path, "test", "photo").unwrap_err();
        assert!(error.to_string().contains("not valid UTF-8"));
    }

    #[test]
    #[cfg(unix)]
    fn rejects_non_utf8_parent_directory_before_any_automation() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(OsStr::from_bytes(b"/tmp/\xffdir/photo.png"));
        let error = validate_utf8_path(path, "test", "file").unwrap_err();
        assert!(error.to_string().contains("not valid UTF-8"));
    }
}
