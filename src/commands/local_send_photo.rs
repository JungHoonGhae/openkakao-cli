use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::ax_send;
use crate::util::{confirm, escape_terminal_text};

const MAX_PHOTO_BYTES: u64 = 50 * 1024 * 1024;

pub struct LocalSendPhotoOptions {
    pub chat_name: String,
    pub file: String,
    pub skip_confirm: bool,
    pub dry_run: bool,
    pub json: bool,
}

fn validate_photo_utf8_path(path: &Path, display: &str) -> Result<()> {
    if path.to_str().is_none() {
        anyhow::bail!("photo path is not valid UTF-8: {display}");
    }
    Ok(())
}

pub(crate) fn validate_photo_path(path: &Path) -> Result<PathBuf> {
    let requested = escape_terminal_text(&path.to_string_lossy());
    let canonical = path
        .canonicalize()
        .with_context(|| format!("photo does not exist or is unreadable: {requested}"))?;
    let display = escape_terminal_text(&canonical.to_string_lossy());
    let metadata = canonical
        .metadata()
        .with_context(|| format!("could not inspect photo: {display}"))?;
    if !metadata.is_file() {
        anyhow::bail!("photo path is not a regular file: {display}");
    }
    validate_photo_utf8_path(&canonical, &display)?;
    if metadata.len() == 0 || metadata.len() > MAX_PHOTO_BYTES {
        anyhow::bail!("photo must be between 1 byte and 50 MiB: {display}");
    }

    let mut header = [0_u8; 16];
    let read = File::open(&canonical)
        .and_then(|mut file| file.read(&mut header))
        .with_context(|| format!("could not read photo: {display}"))?;
    let bytes = &header[..read];
    let jpeg = bytes.starts_with(&[0xff, 0xd8, 0xff]);
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    let gif = bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a");
    let webp = bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP";
    let heif = bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(
            &bytes[8..12],
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1"
        );
    if !(jpeg || png || gif || webp || heif) {
        anyhow::bail!(
            "unsupported or invalid photo data (expected JPEG, PNG, GIF, WebP, or HEIF): {display}"
        );
    }
    Ok(canonical)
}

pub fn cmd_local_send_photo(opts: LocalSendPhotoOptions) -> Result<()> {
    let path = validate_photo_path(Path::new(&opts.file))?;
    let display_path = escape_terminal_text(&path.to_string_lossy());

    if opts.dry_run {
        eprintln!(
            "[dry-run] Would AX-send photo to chat \"{}\": {}",
            escape_terminal_text(&opts.chat_name),
            display_path
        );
        if opts.json {
            crate::util::output_json(&serde_json::json!({
                "dry_run": true,
                "action": "local_send_photo",
                "chat_name": opts.chat_name,
                "file": path,
            }))?;
        }
        return Ok(());
    }

    eprintln!("Direct AX photo send review:");
    eprintln!("  target: {}", escape_terminal_text(&opts.chat_name));
    eprintln!("  file:   {display_path}");
    if !opts.skip_confirm {
        eprint!("Continue to macOS device-owner authentication?\n[y/N] ");
        if !confirm()? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    ax_send::send_photo_via_ax(&opts.chat_name, &path)?;
    if opts.json {
        crate::util::output_json(&serde_json::json!({
            "chat_name": opts.chat_name,
            "file": path,
            "status": "sent",
        }))?;
    } else {
        println!("Photo sent!");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn accepts_png_by_content_and_returns_canonical_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("photo.bin");
        fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();
        assert_eq!(
            validate_photo_path(&path).unwrap(),
            path.canonicalize().unwrap()
        );
    }

    #[test]
    fn rejects_extension_spoofing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        fs::write(&path, b"not an image").unwrap();
        assert!(validate_photo_path(&path).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn rejects_non_utf8_filename_before_any_automation() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(OsStr::from_bytes(b"\xff\xfe.png"));
        let error = validate_photo_utf8_path(path, "test").unwrap_err();
        assert!(error.to_string().contains("not valid UTF-8"));
    }

    #[test]
    #[cfg(unix)]
    fn rejects_non_utf8_parent_directory_before_any_automation() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new(OsStr::from_bytes(b"/tmp/\xffdir/photo.png"));
        let error = validate_photo_utf8_path(path, "test").unwrap_err();
        assert!(error.to_string().contains("not valid UTF-8"));
    }
}
