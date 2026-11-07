use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenKakaoConfig {
    #[serde(default)]
    pub mode: ModeConfig,
    #[serde(default)]
    pub send: SendConfig,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub ax: AxConfig,
}

/// Login-free AX automation (`local-send*`, `ax-read`, `safe-send approve`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AxConfig {
    /// Seconds of keyboard/mouse idle time required before KakaoTalk may be
    /// raised (default 2, 0 disables). `--wait-idle` overrides it.
    pub wait_idle_secs: Option<u64>,
    /// Open a chat that has no open window from the chat list, raising
    /// KakaoTalk briefly (default false). `--open-if-needed` enables it.
    #[serde(default)]
    pub open_if_needed: bool,
    /// Largest file `local-send-file` accepts, in MiB (default 300).
    pub max_file_mb: Option<u64>,
    /// KakaoTalk login id for `ax-login` (the password stays in the keychain).
    pub login_account: Option<String>,
}

pub const DEFAULT_MAX_FILE_MB: u64 = 300;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModeConfig {
    #[serde(default)]
    pub unattended: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SendConfig {
    #[serde(default)]
    pub allow_non_interactive: bool,
    /// `false` drops the signature (kept from when it was a prefix).
    pub default_prefix: Option<bool>,
    /// The line appended under outgoing messages; `""` drops it.
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WatchConfig {
    #[serde(default)]
    pub allow_side_effects: bool,
    pub default_max_reconnect: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    pub prefer_relogin: Option<bool>,
    pub auto_renew: Option<bool>,
    pub password_cmd: Option<String>,
    pub email_cmd: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SafetyConfig {
    pub min_unattended_send_interval_secs: Option<u64>,
    pub min_hook_interval_secs: Option<u64>,
    pub min_webhook_interval_secs: Option<u64>,
    pub hook_timeout_secs: Option<u64>,
    pub webhook_timeout_secs: Option<u64>,
    #[serde(default)]
    pub allow_insecure_webhooks: bool,
    /// Enable LOCO write operations (send, delete, edit, react).
    /// Disabled by default to protect against account bans.
    #[serde(default)]
    pub allow_loco_write: bool,
    /// Enable AX-automation-based `local-send`/`local-send-photo` (no server
    /// contact, drives the KakaoTalk UI directly). Disabled by default since
    /// they still send real messages from a real KakaoTalk window.
    #[serde(default)]
    pub allow_ax_send: bool,
    /// Chat display names `local-send`/`local-send-photo` are allowed to
    /// target. AX-send matches
    /// chats by display-name text scraped from the UI, not a chat-id (the
    /// local DB it would normally cross-check against is unreadable on
    /// current KakaoTalk builds), so an exact-match allowlist is the only
    /// guard against sending to the wrong chat. Empty means nothing is
    /// allowed to send.
    #[serde(default)]
    pub allowed_send_chats: Vec<String>,
    /// Seconds one successful macOS device-owner authentication (Touch ID or
    /// password) stays valid for later `local-send*`, `local-reply`,
    /// `local-edit` and `safe-send approve` runs, sudo-style. 0 (default)
    /// asks every time; values above 900 are clamped. The approval time is a
    /// file your account can write, so within the window any local process
    /// (a shell, an agent) can send to allowlisted chats or extend the window.
    /// Enable it only on a Mac where every local process is yours.
    #[serde(default)]
    pub device_owner_auth_grace_secs: u64,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            min_unattended_send_interval_secs: Some(10),
            min_hook_interval_secs: Some(2),
            min_webhook_interval_secs: Some(2),
            hook_timeout_secs: Some(20),
            webhook_timeout_secs: Some(10),
            allow_insecure_webhooks: false,
            allow_loco_write: false,
            allow_ax_send: false,
            allowed_send_chats: Vec::new(),
            device_owner_auth_grace_secs: 0,
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    Ok(home.join(".config").join("openkakao").join("config.toml"))
}

pub fn load_config() -> Result<OpenKakaoConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(OpenKakaoConfig::default());
    }

    let data =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let config: OpenKakaoConfig =
        toml::from_str(&data).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_safe() {
        let config = OpenKakaoConfig::default();
        assert!(!config.mode.unattended);
        assert!(!config.send.allow_non_interactive);
        assert!(!config.watch.allow_side_effects);
        assert!(config.auth.password_cmd.is_none());
        assert!(config.auth.email_cmd.is_none());
        assert_eq!(config.safety.min_unattended_send_interval_secs, Some(10));
        assert_eq!(config.safety.min_hook_interval_secs, Some(2));
        assert_eq!(config.safety.min_webhook_interval_secs, Some(2));
        assert_eq!(config.safety.hook_timeout_secs, Some(20));
        assert_eq!(config.safety.webhook_timeout_secs, Some(10));
        assert!(!config.safety.allow_insecure_webhooks);
        assert!(!config.safety.allow_loco_write);
        assert!(!config.ax.open_if_needed);
        assert!(config.ax.wait_idle_secs.is_none());
        assert!(config.ax.max_file_mb.is_none());
    }

    #[test]
    fn ax_section_parses() {
        let config: OpenKakaoConfig =
            toml::from_str("[ax]\nwait_idle_secs = 5\nopen_if_needed = true\nmax_file_mb = 50\n")
                .unwrap();
        assert_eq!(config.ax.wait_idle_secs, Some(5));
        assert!(config.ax.open_if_needed);
        assert_eq!(config.ax.max_file_mb, Some(50));
    }
}
