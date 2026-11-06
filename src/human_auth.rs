//! OS-mediated user-presence verification for irreversible local actions.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Upper bound for `[safety] device_owner_auth_grace_secs` (15 minutes). The
/// stamp below is a file the account can write, so while the window is open
/// any local process can extend it: the window turns the OS-enforced presence
/// check into trust of the Mac's local processes, hence the short cap.
pub const MAX_DEVICE_OWNER_AUTH_GRACE_SECS: u64 = 900;

// ponytail: one process-wide value set once from the loaded config, so the
// four native send paths keep calling `require_device_owner_auth(reason)`.
static GRACE_SECS: AtomicU64 = AtomicU64::new(0);

/// Seconds a successful device-owner authentication keeps satisfying later
/// prompts (0 = ask every time). Clamped to [`MAX_DEVICE_OWNER_AUTH_GRACE_SECS`].
pub fn set_device_owner_auth_grace(secs: u64) {
    GRACE_SECS.store(
        secs.min(MAX_DEVICE_OWNER_AUTH_GRACE_SECS),
        Ordering::Relaxed,
    );
}

pub fn device_owner_auth_grace() -> u64 {
    GRACE_SECS.load(Ordering::Relaxed)
}

/// `~/.config/openkakao/device_owner_auth.stamp`: its mtime is the last
/// successful authentication (sudo-style timestamp file, mode 0600).
fn grace_stamp_path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".config")
            .join("openkakao")
            .join("device_owner_auth.stamp"),
    )
}

/// Time left in the grace window for a stamp written at `approved_at`, or
/// `None` when the window is off, elapsed, or the stamp is from the future.
fn grace_remaining(approved_at: SystemTime, now: SystemTime, grace: Duration) -> Option<Duration> {
    if grace.is_zero() {
        return None;
    }
    let age = now.duration_since(approved_at).ok()?;
    grace.checked_sub(age).filter(|left| !left.is_zero())
}

/// Time left before the next send asks for device-owner authentication again,
/// `None` when it will ask.
pub fn device_owner_auth_grace_left() -> Option<Duration> {
    let grace = Duration::from_secs(device_owner_auth_grace());
    let approved_at = std::fs::metadata(grace_stamp_path()?)
        .ok()?
        .modified()
        .ok()?;
    grace_remaining(approved_at, SystemTime::now(), grace)
}

#[cfg(target_os = "macos")]
fn touch_grace_stamp() -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let path = grace_stamp_path().ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    writeln!(file, "{now}")?;
    file.set_modified(SystemTime::now())?;
    Ok(())
}

#[cfg(any(test, target_os = "macos"))]
fn process_validation_error(code: i32) -> Option<&'static str> {
    match code {
        0 => None,
        10 => Some("could not bind a macOS code-signing object to the KakaoTalk process"),
        11 => Some("the running KakaoTalk process failed Apple-anchored dynamic code validation"),
        12 => Some("the running process does not have KakaoTalk's expected bundle/team identity"),
        _ => Some("the running KakaoTalk process failed code-signing validation unexpectedly"),
    }
}

#[cfg(any(test, target_os = "macos"))]
fn authentication_error(code: i32) -> Option<&'static str> {
    match code {
        0 => None,
        1 => Some(
            "macOS device-owner authentication is unavailable; configure Touch ID or a login password before approving sends",
        ),
        2 => Some("macOS device-owner authentication was cancelled or denied; no message was sent"),
        3 => Some("macOS device-owner authentication timed out; no message was sent"),
        _ => Some("macOS device-owner authentication failed unexpectedly; no message was sent"),
    }
}

#[cfg(target_os = "macos")]
pub fn require_device_owner_auth(reason: &str) -> Result<()> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    unsafe extern "C" {
        fn openkakao_authenticate_device_owner(reason_utf8: *const c_char) -> c_int;
    }

    if let Some(left) = device_owner_auth_grace_left() {
        eprintln!(
            "Device-owner authentication reused: approved within the last {}s ({}s left)",
            device_owner_auth_grace(),
            left.as_secs()
        );
        return Ok(());
    }
    let reason = CString::new(reason)?;
    let code = unsafe { openkakao_authenticate_device_owner(reason.as_ptr()) };
    if let Some(message) = authentication_error(code) {
        anyhow::bail!(message);
    }
    if device_owner_auth_grace() > 0 {
        if let Err(error) = touch_grace_stamp() {
            eprintln!(
                "warning: could not record the authentication time for the grace window: {error}"
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn validate_kakaotalk_process(pid: i32) -> Result<()> {
    use std::os::raw::c_int;

    unsafe extern "C" {
        fn openkakao_validate_kakaotalk_process(pid: c_int) -> c_int;
    }

    let code = unsafe { openkakao_validate_kakaotalk_process(pid) };
    if let Some(message) = process_validation_error(code) {
        anyhow::bail!(message);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn require_device_owner_auth(_reason: &str) -> Result<()> {
    anyhow::bail!("safe-send approval with device-owner authentication is only supported on macOS")
}

#[cfg(not(target_os = "macos"))]
pub fn validate_kakaotalk_process(_pid: i32) -> Result<()> {
    anyhow::bail!("KakaoTalk process validation is only supported on macOS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grace_window_decisions() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let grace = Duration::from_secs(900);
        // off
        assert_eq!(
            grace_remaining(t0, t0 + Duration::from_secs(1), Duration::ZERO),
            None
        );
        // inside the window
        assert_eq!(
            grace_remaining(t0, t0 + Duration::from_secs(100), grace),
            Some(Duration::from_secs(800))
        );
        // exactly elapsed, and elapsed
        assert_eq!(grace_remaining(t0, t0 + grace, grace), None);
        assert_eq!(
            grace_remaining(t0, t0 + Duration::from_secs(5_000), grace),
            None
        );
        // a stamp from the future (clock moved back) never counts
        assert_eq!(
            grace_remaining(t0 + Duration::from_secs(10), t0, grace),
            None
        );
        // the setter clamps
        set_device_owner_auth_grace(10 * MAX_DEVICE_OWNER_AUTH_GRACE_SECS);
        assert_eq!(device_owner_auth_grace(), MAX_DEVICE_OWNER_AUTH_GRACE_SECS);
        set_device_owner_auth_grace(0);
    }

    #[test]
    fn native_authentication_codes_fail_closed() {
        assert_eq!(authentication_error(0), None);
        assert!(authentication_error(1).unwrap().contains("unavailable"));
        assert!(authentication_error(2).unwrap().contains("denied"));
        assert!(authentication_error(3).unwrap().contains("timed out"));
        assert!(authentication_error(99).unwrap().contains("unexpectedly"));
    }

    #[test]
    fn process_validation_codes_fail_closed() {
        assert_eq!(process_validation_error(0), None);
        assert!(process_validation_error(10).unwrap().contains("bind"));
        assert!(process_validation_error(11)
            .unwrap()
            .contains("Apple-anchored"));
        assert!(process_validation_error(12)
            .unwrap()
            .contains("bundle/team"));
        assert!(process_validation_error(99)
            .unwrap()
            .contains("unexpectedly"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_validation_rejects_this_non_kakao_test_process() {
        assert!(validate_kakaotalk_process(std::process::id() as i32).is_err());
    }
}
