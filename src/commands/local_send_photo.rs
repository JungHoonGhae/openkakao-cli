use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::Result;

use crate::ax_send::AttachmentKind;
use crate::commands::local_send_file::{send_attachment, validate_regular_file, AttachmentSend};
use crate::error::{coded, ErrorCode};
use crate::util::escape_terminal_text;

const MAX_PHOTO_BYTES: u64 = 50 * 1024 * 1024;

pub struct LocalSendPhotoOptions {
    pub chat_name: String,
    pub file: String,
    pub skip_confirm: bool,
    pub dry_run: bool,
    pub json: bool,
}

/// Whether the first bytes are JPEG, PNG, GIF, WebP or HEIF image data.
fn is_image_data(bytes: &[u8]) -> bool {
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
    jpeg || png || gif || webp || heif
}

fn validate_photo(path: &Path) -> Result<crate::commands::local_send_file::ValidatedFile> {
    let validated = validate_regular_file(path, MAX_PHOTO_BYTES, "photo")?;
    let display = escape_terminal_text(&validated.path.to_string_lossy());
    let mut header = [0_u8; 16];
    let read = File::open(&validated.path)
        .and_then(|mut file| file.read(&mut header))
        .map_err(|error| {
            coded(
                ErrorCode::InvalidFile,
                format!("could not read photo: {display} ({error})"),
            )
        })?;
    if !is_image_data(&header[..read]) {
        return Err(coded(
            ErrorCode::InvalidFile,
            format!(
                "unsupported or invalid photo data (expected JPEG, PNG, GIF, WebP, or HEIF): {display}"
            ),
        ));
    }
    Ok(validated)
}

/// `local-send-photo` is `local-send-file` with image-content validation, the
/// 50 MiB photo cap, JPEG/PNG clipboard staging and `[사진]`-only delivery
/// proof.
pub fn cmd_local_send_photo(opts: LocalSendPhotoOptions) -> Result<()> {
    let file = validate_photo(Path::new(&opts.file))?;
    send_attachment(AttachmentSend {
        chat_name: opts.chat_name,
        file,
        kind: AttachmentKind::Photo,
        skip_confirm: opts.skip_confirm,
        dry_run: opts.dry_run,
        json: opts.json,
    })
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
            validate_photo(&path).unwrap().path,
            path.canonicalize().unwrap()
        );
    }

    #[test]
    fn rejects_extension_spoofing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        fs::write(&path, b"not an image").unwrap();
        assert!(validate_photo(&path).is_err());
    }

    #[test]
    fn image_content_sniffing_matches_the_supported_formats() {
        assert!(is_image_data(b"\xff\xd8\xff\xe0rest"));
        assert!(is_image_data(b"GIF89a....."));
        assert!(is_image_data(b"RIFF\0\0\0\0WEBPVP8 "));
        assert!(is_image_data(b"\0\0\0\x18ftypheic"));
        // Office documents are zip containers, not images.
        assert!(!is_image_data(b"PK\x03\x04\x14\0\x06\0"));
        assert!(!is_image_data(b""));
    }
}
