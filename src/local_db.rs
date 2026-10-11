use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use base64::Engine;
use rusqlite::Connection;
use serde::Serialize;
use sha2::Digest;

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LocalChat {
    pub chat_id: i64,
    pub chat_type: i32,
    pub chat_name: String,
    pub active_members_count: i32,
    pub last_log_id: i64,
    pub last_updated_at: i64,
    pub unread_count: i64,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalMessage {
    pub log_id: i64,
    pub chat_id: i64,
    pub author_id: i64,
    pub sender_name: String,
    pub message: String,
    pub message_type: i32,
    pub sent_at: i64,
}

// ---------------------------------------------------------------------------
// Device info extraction
// ---------------------------------------------------------------------------

pub fn get_platform_uuid() -> Result<String> {
    let output = Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .context("Failed to run ioreg")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            // ioreg outputs: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
            // Split on '"' gives: ["", "IOPlatformUUID", " = ", "UUID-VALUE", ""]
            // parts[3] is always the UUID value regardless of whether parts[1] is the key name.
            let parts: Vec<&str> = line.split('"').collect();
            if parts.len() >= 4 {
                let uuid = parts[3].trim();
                if uuid.len() >= 36 && uuid != "IOPlatformUUID" {
                    return Ok(uuid.to_string());
                }
            }
        }
    }
    anyhow::bail!("IOPlatformUUID not found in ioreg output")
}

fn get_user_id_from_plist() -> Result<i64> {
    let home = dirs::home_dir().context("No home directory")?;

    // Strategy 1: Container preferences with hex suffix
    let container_prefs =
        home.join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Preferences");
    if container_prefs.exists() {
        if let Ok(entries) = std::fs::read_dir(&container_prefs) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("com.kakao.KakaoTalkMac.") && name.ends_with(".plist") {
                    let path = entry.path();
                    if let Ok(user_id) = extract_user_id_from_plist(&path) {
                        return Ok(user_id);
                    }
                }
            }
        }
    }

    // Strategy 2: Global preferences
    let global_plist = home.join("Library/Preferences/com.kakao.KakaoTalkMac.plist");
    if global_plist.exists() {
        if let Ok(user_id) = extract_user_id_from_plist(&global_plist) {
            return Ok(user_id);
        }
    }

    anyhow::bail!(
        "Could not extract KakaoTalk user ID from preferences. \
         Is KakaoTalk installed and logged in?"
    )
}

fn dict_has_active_user_id(dict: &plist::Dictionary, user_id: i64) -> bool {
    let expected = hex::encode(sha2::Sha512::digest(user_id.to_string().as_bytes()));
    extract_active_account_hash(dict).as_deref() == Some(expected.as_str())
}

fn extract_user_id_from_plist(path: &std::path::Path) -> Result<i64> {
    let dict: plist::Dictionary = plist::from_file(path).context("Failed to parse plist")?;

    // Strategy A: FSChatWindowTransparency keys → longest common suffix
    let prefix = "FSChatWindowTransparency";
    let suffixes: Vec<String> = dict
        .keys()
        .filter(|k| k.starts_with(prefix) && k.len() > prefix.len())
        .map(|k| k[prefix.len()..].to_string())
        .collect();

    if suffixes.len() >= 2 {
        // Each suffix is chatId + userId. Find the common tail.
        if let Some(common) = longest_common_suffix(&suffixes) {
            if let Ok(id) = common.parse::<i64>() {
                return Ok(id);
            }
        }
    }

    // Strategy B: Direct key lookup
    for key in &["userId", "user_id", "KAKAO_USER_ID", "userID"] {
        if let Some(val) = dict.get(key) {
            if let Some(n) = val.as_signed_integer() {
                return Ok(n);
            }
            if let Some(s) = val.as_string() {
                if let Ok(n) = s.parse::<i64>() {
                    return Ok(n);
                }
            }
        }
    }

    // Strategy C: FSChatWindowFrame_ keys → shared userId (newer KT versions).
    // Cheap and exact, so it runs before the brute-force fallback below.
    let frame_prefix = "NSWindow Frame FSChatWindowFrame_";
    let frame_suffixes: Vec<String> = dict
        .keys()
        .filter(|k| k.starts_with(frame_prefix) && k.len() > frame_prefix.len())
        .map(|k| k[frame_prefix.len()..].to_string())
        .collect();
    // Every FSChatWindowFrame_ key for one account carries the SAME userId, so
    // require the suffixes to be identical rather than merely share a tail —
    // a shared tail (e.g. 199453377 vs 23377 -> 3377) would parse into a wrong,
    // smaller userId and silently derive the wrong DB key.
    if let Some(id) = unique_user_id(&frame_suffixes) {
        return Ok(id);
    }

    // Strategy D: KakaoTalk 26.7 uses opaque FSChatWindowFrame_* suffixes.
    // Reuse a previously saved user ID only when it hashes to this plist's
    // active-account marker. Run this before brute force so larger IDs do not
    // exhaust the bounded search first.
    if let Ok(Some(credentials)) = crate::credentials::load_credentials() {
        if credentials.user_id > 0 && dict_has_active_user_id(&dict, credentials.user_id) {
            return Ok(credentials.user_id);
        }
    }

    // Strategy E: SHA-512 brute-force from revision key suffixes (last resort).
    // Bounded by a wall-clock deadline so a missing/foreign hash cannot hang the CLI.
    if let Some(hash) = extract_active_account_hash(&dict) {
        if let Some(id) = recover_user_id_from_sha512(&hash) {
            return Ok(id);
        }
    }

    anyhow::bail!("No userId found in plist")
}

/// SHA-512 of "0" — the default/empty account hash.
const EMPTY_ACCOUNT_HASH: &str =
    "31bca02094eb78126a517b206a88c73cfa9ec6f704c7030d18212cace820f025f00bf0ea68dbf3f3a5436ca63b53bf7bf80ad8d5de7d8359d0b7fed9dbc3ab99";

/// Extract the active account's SHA-512 hash from revision keys.
/// Keys like `DESIGNATEDFRIENDSREVISION:<sha512hex>` appear with non-zero values
/// for the active account. SHA-512("0") is the default/empty account (skipped).
fn extract_active_account_hash(dict: &plist::Dictionary) -> Option<String> {
    let prefix = "DESIGNATEDFRIENDSREVISION:";
    for (key, val) in dict {
        if !key.starts_with(prefix) {
            continue;
        }
        let hash = &key[prefix.len()..];
        if hash == EMPTY_ACCOUNT_HASH {
            continue;
        }
        // Only trust an integer revision counter. A float here would be coerced
        // with a saturating `as i64` cast (NaN -> 0, 1e300 -> i64::MAX), which
        // could wrongly select a hash and trigger the expensive brute force.
        let non_zero = matches!(val, plist::Value::Integer(n) if n.as_signed().unwrap_or(0) != 0);
        if non_zero {
            return Some(hash.to_string());
        }
    }
    None
}

/// Wall-clock budget for the SHA-512 pre-image search. The search is a last
/// resort and runs on the main thread, so it must not hang the CLI when the
/// hash has no small pre-image (logged-out account, foreign hash, or a userId
/// outside the scanned range).
const SHA512_BRUTE_FORCE_BUDGET: std::time::Duration = std::time::Duration::from_secs(15);

/// Recover a userId by brute-forcing the SHA-512 pre-image.
/// KakaoTalk stores SHA-512(userId) in plist revision keys. userIds are small
/// positive integers, so the real value is normally found quickly — but if it
/// is not present in the scanned range the loop stops at the time budget rather
/// than burning minutes of CPU.
fn recover_user_id_from_sha512(hex_hash: &str) -> Option<i64> {
    use sha2::Digest;

    if hex_hash.len() != 128 {
        return None;
    }

    let mut target = [0u8; 64];
    for (i, chunk) in hex_hash.as_bytes().chunks(2).enumerate() {
        if i >= 64 {
            break;
        }
        let s = std::str::from_utf8(chunk).ok()?;
        target[i] = u8::from_str_radix(s, 16).ok()?;
    }

    let start = std::time::Instant::now();
    let mut i: i64 = 1;
    while i <= 10_000_000_000 {
        let mut hasher = sha2::Sha512::new();
        hasher.update(i.to_string().as_bytes());
        let result = hasher.finalize();
        if result.as_slice() == target {
            if std::env::var("OPENKAKAO_CLI_DEBUG").is_ok() {
                eprintln!("[local-db] SHA-512 preimage found: userId={i}");
            }
            return Some(i);
        }
        // Check the deadline periodically to keep the hot loop tight.
        if i % 1_000_000 == 0 && start.elapsed() >= SHA512_BRUTE_FORCE_BUDGET {
            if std::env::var("OPENKAKAO_CLI_DEBUG").is_ok()
                || std::env::var("OPENKAKAO_RS_DEBUG").is_ok()
            {
                eprintln!(
                    "[local-db] SHA-512 userId search hit {}s budget at i={}, giving up",
                    SHA512_BRUTE_FORCE_BUDGET.as_secs(),
                    i
                );
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Return the userId shared by every `FSChatWindowFrame_` suffix, but only when
/// all suffixes are identical and parse as an integer. Returns `None` if the
/// suffixes disagree (which would otherwise collapse to a wrong shared tail).
fn unique_user_id(suffixes: &[String]) -> Option<i64> {
    let first = suffixes.first()?;
    if suffixes.iter().all(|s| s == first) {
        first.parse::<i64>().ok()
    } else {
        None
    }
}

fn longest_common_suffix(strings: &[String]) -> Option<String> {
    if strings.is_empty() {
        return None;
    }
    let reversed: Vec<Vec<char>> = strings.iter().map(|s| s.chars().rev().collect()).collect();
    let min_len = reversed.iter().map(|r| r.len()).min().unwrap_or(0);
    let mut common_len = 0;
    for i in 0..min_len {
        let ch = reversed[0][i];
        if reversed.iter().all(|r| r[i] == ch) {
            common_len = i + 1;
        } else {
            break;
        }
    }
    if common_len == 0 {
        return None;
    }
    Some(reversed[0][..common_len].iter().rev().collect())
}

// ---------------------------------------------------------------------------
// Key derivation (matches kakaocli KeyDerivation.swift)
// ---------------------------------------------------------------------------

fn hashed_device_uuid(uuid: &str) -> String {
    let sha1_hash = sha1::Sha1::digest(uuid.as_bytes());
    let sha256_hash = sha2::Sha256::digest(uuid.as_bytes());
    let mut combined = Vec::with_capacity(52);
    combined.extend_from_slice(&sha1_hash);
    combined.extend_from_slice(&sha256_hash);
    base64::engine::general_purpose::STANDARD.encode(&combined)
}

/// Derive the database file name from userId and UUID.
fn derive_database_name(user_id: i64, uuid: &str) -> String {
    let reversed_uuid: String = uuid.chars().rev().collect();
    let hawawa = format!("..F.{}.A.F.{}..|", user_id, reversed_uuid);

    // Salt: reversed base64(SHA1 || SHA256) of UUID
    let hashed = hashed_device_uuid(uuid);
    let salt: String = hashed.chars().rev().collect();

    let derived = pbkdf2_sha256(hawawa.as_bytes(), salt.as_bytes(), 100_000, 128);
    let hex_str = hex::encode(&derived);

    // Extract substring [28..106] (78 chars)
    hex_str[28..106].to_string()
}

/// Derive the SQLCipher encryption key.
fn derive_secure_key(user_id: i64, uuid: &str) -> String {
    let hashed = hashed_device_uuid(uuid);

    let uuid_prefix5: String = uuid.chars().take(5).collect();
    let uuid_drop7: String = uuid.chars().skip(7).collect();

    let parts = [
        "A",
        &hashed,
        "|",
        "F",
        &uuid_prefix5,
        "H",
        &user_id.to_string(),
        "|",
        &uuid_drop7,
    ];
    let hawawa: String = parts.join("F");
    let reversed_hawawa: String = hawawa.chars().rev().collect();

    // Salt: UUID from 30% offset to end
    let offset = (uuid.len() as f64 * 0.3) as usize;
    let salt = &uuid[offset..];

    let derived = pbkdf2_sha256(reversed_hawawa.as_bytes(), salt.as_bytes(), 100_000, 128);
    hex::encode(&derived)
}

/// PBKDF2-HMAC-SHA256
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, key_len: usize) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;

    let hash_len = 32; // SHA-256 output
    let blocks_needed = key_len.div_ceil(hash_len);
    let mut output = Vec::with_capacity(blocks_needed * hash_len);

    for block_num in 1..=blocks_needed as u32 {
        // U1 = PRF(password, salt || INT_32_BE(block_num))
        let mut mac = HmacSha256::new_from_slice(password).expect("HMAC accepts any key length");
        mac.update(salt);
        mac.update(&block_num.to_be_bytes());
        let mut u = mac.finalize().into_bytes().to_vec();
        let mut result = u.clone();

        for _ in 1..iterations {
            let mut mac =
                HmacSha256::new_from_slice(password).expect("HMAC accepts any key length");
            mac.update(&u);
            u = mac.finalize().into_bytes().to_vec();
            for (r, ui) in result.iter_mut().zip(u.iter()) {
                *r ^= ui;
            }
        }
        output.extend_from_slice(&result);
    }

    output.truncate(key_len);
    output
}

// ---------------------------------------------------------------------------
// Database path resolution
// ---------------------------------------------------------------------------

fn find_database_path(db_name: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("No home directory")?;
    let container_dir = home.join(
        "Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac",
    );

    if !container_dir.exists() {
        anyhow::bail!(
            "KakaoTalk container directory not found: {}",
            container_dir.display()
        );
    }

    // Look for a FILE (not directory) matching the derived database name.
    // Must check it's a regular file — KakaoTalk also creates hex-named directories.
    if let Ok(entries) = std::fs::read_dir(&container_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if (name == db_name || name.starts_with(db_name))
                && entry.metadata().map(|m| m.is_file()).unwrap_or(false)
            {
                return Ok(entry.path());
            }
        }
    }

    // Fallback: look for any 78-char hex-named FILE (the DB naming convention).
    // Exclude -shm and -wal sidecar files.
    if let Ok(entries) = std::fs::read_dir(&container_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() == 78
                && name.chars().all(|c| c.is_ascii_hexdigit())
                && entry.metadata().map(|m| m.is_file()).unwrap_or(false)
            {
                return Ok(entry.path());
            }
        }
    }

    anyhow::bail!(
        "KakaoTalk database not found in {}. Derived name: {}",
        container_dir.display(),
        db_name
    )
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct LocalDbReader {
    conn: Connection,
}

impl LocalDbReader {
    pub fn open() -> Result<Self> {
        let uuid = get_platform_uuid().context("Failed to get IOPlatformUUID")?;
        let user_id = get_user_id_from_plist().context("Failed to get KakaoTalk user ID")?;

        let db_name = derive_database_name(user_id, &uuid);
        if std::env::var("OPENKAKAO_CLI_DEBUG").is_ok() {
            eprintln!("[local-db] uuid={uuid} user_id={user_id} derived_db_name={db_name}");
        }
        let db_path =
            find_database_path(&db_name).context("Failed to locate KakaoTalk local database")?;

        let secure_key = derive_secure_key(user_id, &uuid);

        let conn = Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        // Configure SQLCipher
        conn.pragma_update(None, "cipher_compatibility", 3)?;
        conn.pragma_update(None, "key", &secure_key)?;

        // Verify the key works
        conn.execute_batch("SELECT count(*) FROM sqlite_master")
            .context(
                "Failed to decrypt KakaoTalk database. Key derivation may have failed. \
                 Ensure KakaoTalk is installed and logged in.",
            )?;

        Ok(Self { conn })
    }

    /// Check if the local database is accessible (for doctor command).
    pub fn check_access() -> Result<LocalDbStatus> {
        let home = dirs::home_dir().context("No home directory")?;
        let container_dir = home.join(
            "Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac",
        );

        let uuid_ok = get_platform_uuid().is_ok();
        let user_id_ok = get_user_id_from_plist().is_ok();
        let container_exists = container_dir.exists();

        let db_file = if uuid_ok && user_id_ok {
            let uuid = get_platform_uuid().ok();
            let uid = get_user_id_from_plist().ok();
            if let (Some(u), Some(id)) = (uuid, uid) {
                let name = derive_database_name(id, &u);
                find_database_path(&name).ok()
            } else {
                None
            }
        } else {
            None
        };

        let decryptable = if db_file.is_some() {
            Self::open().is_ok()
        } else {
            false
        };

        Ok(LocalDbStatus {
            uuid_available: uuid_ok,
            user_id_available: user_id_ok,
            container_exists,
            db_file_found: db_file.is_some(),
            db_path: db_file.map(|p| p.to_string_lossy().to_string()),
            decryptable,
        })
    }

    pub fn list_chats(&self, limit: usize) -> Result<Vec<LocalChat>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.chatId, r.type, r.chatName, r.activeMembersCount,
                    r.lastLogId, r.lastUpdatedAt, r.countOfNewMessage,
                    COALESCE(u.displayName, u.friendNickName, u.nickName, '') as displayName
             FROM NTChatRoom r
             LEFT JOIN NTUser u ON r.directChatMemberUserId = u.userId AND u.linkId = 0
             ORDER BY r.lastUpdatedAt DESC
             LIMIT ?",
        )?;

        let rows = stmt
            .query_map([limit as i64], |row| {
                let chat_name: String = row.get::<_, String>(2).unwrap_or_default();
                let display_name: String = row.get::<_, String>(7).unwrap_or_default();
                let title = if chat_name.is_empty() {
                    display_name.clone()
                } else {
                    chat_name
                };
                Ok(LocalChat {
                    chat_id: row.get(0)?,
                    chat_type: row.get(1)?,
                    chat_name: title,
                    active_members_count: row.get(3).unwrap_or(0),
                    last_log_id: row.get(4).unwrap_or(0),
                    last_updated_at: row.get(5).unwrap_or(0),
                    unread_count: row.get(6).unwrap_or(0),
                    display_name,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn read_messages(
        &self,
        chat_id: i64,
        limit: usize,
        since_ts: Option<i64>,
    ) -> Result<Vec<LocalMessage>> {
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(ts) = since_ts {
                (
                    "SELECT m.logId, m.chatId, m.authorId,
                        COALESCE(u.displayName, u.friendNickName, u.nickName, '') as senderName,
                        COALESCE(m.message, '') as message, m.type, m.sentAt
                 FROM NTChatMessage m
                 LEFT JOIN NTUser u ON m.authorId = u.userId AND u.linkId = 0
                 WHERE m.chatId = ? AND m.sentAt >= ?
                 ORDER BY m.sentAt DESC
                 LIMIT ?"
                        .to_string(),
                    vec![Box::new(chat_id), Box::new(ts), Box::new(limit as i64)],
                )
            } else {
                (
                    "SELECT m.logId, m.chatId, m.authorId,
                        COALESCE(u.displayName, u.friendNickName, u.nickName, '') as senderName,
                        COALESCE(m.message, '') as message, m.type, m.sentAt
                 FROM NTChatMessage m
                 LEFT JOIN NTUser u ON m.authorId = u.userId AND u.linkId = 0
                 WHERE m.chatId = ?
                 ORDER BY m.sentAt DESC
                 LIMIT ?"
                        .to_string(),
                    vec![Box::new(chat_id), Box::new(limit as i64)],
                )
            };

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(LocalMessage {
                    log_id: row.get(0)?,
                    chat_id: row.get(1)?,
                    author_id: row.get(2).unwrap_or(0),
                    sender_name: row.get(3).unwrap_or_default(),
                    message: row.get(4).unwrap_or_default(),
                    message_type: row.get(5).unwrap_or(0),
                    sent_at: row.get(6).unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn search_messages(&self, query: &str, limit: usize) -> Result<Vec<LocalMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.logId, m.chatId, m.authorId,
                    COALESCE(u.displayName, u.friendNickName, u.nickName, '') as senderName,
                    COALESCE(m.message, '') as message, m.type, m.sentAt
             FROM NTChatMessage m
             LEFT JOIN NTUser u ON m.authorId = u.userId AND u.linkId = 0
             WHERE m.message LIKE ?
             ORDER BY m.sentAt DESC
             LIMIT ?",
        )?;

        let pattern = format!("%{}%", query);
        let rows = stmt
            .query_map(rusqlite::params![pattern, limit as i64], |row| {
                Ok(LocalMessage {
                    log_id: row.get(0)?,
                    chat_id: row.get(1)?,
                    author_id: row.get(2).unwrap_or(0),
                    sender_name: row.get(3).unwrap_or_default(),
                    message: row.get(4).unwrap_or_default(),
                    message_type: row.get(5).unwrap_or(0),
                    sent_at: row.get(6).unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn schema(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Find the memo chat (나와의 채팅) ID. Type 0 with activeMembersCount = 1.
    pub fn find_memo_chat_id(&self) -> Result<Option<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT chatId FROM NTChatRoom WHERE type = 0 AND activeMembersCount = 1 LIMIT 1",
        )?;
        let result = stmt.query_row([], |row| row.get::<_, i64>(0)).ok();
        Ok(result)
    }
}

#[derive(Debug, Serialize)]
pub struct LocalDbStatus {
    pub uuid_available: bool,
    pub user_id_available: bool,
    pub container_exists: bool,
    pub db_file_found: bool,
    pub db_path: Option<String>,
    pub decryptable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_sha256_produces_expected_length() {
        let result = pbkdf2_sha256(b"password", b"salt", 1, 32);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn pbkdf2_sha256_128_bytes() {
        let result = pbkdf2_sha256(b"test", b"salt", 1, 128);
        assert_eq!(result.len(), 128);
    }

    #[test]
    fn hashed_device_uuid_produces_base64() {
        let result = hashed_device_uuid("TEST-UUID");
        assert!(!result.is_empty());
        // SHA1 (20) + SHA256 (32) = 52 bytes → base64 ≈ 72 chars
        assert!(result.len() > 50);
    }

    #[test]
    fn longest_common_suffix_works() {
        let strings = vec!["abc123".to_string(), "def123".to_string()];
        assert_eq!(longest_common_suffix(&strings), Some("123".to_string()));
    }

    #[test]
    fn longest_common_suffix_none_when_empty() {
        let strings: Vec<String> = vec![];
        assert_eq!(longest_common_suffix(&strings), None);
    }

    #[test]
    fn longest_common_suffix_no_match() {
        let strings = vec!["abc".to_string(), "def".to_string()];
        assert_eq!(longest_common_suffix(&strings), None);
    }

    #[test]
    fn unique_user_id_accepts_identical_suffixes() {
        let s = vec!["199453377".to_string(), "199453377".to_string()];
        assert_eq!(unique_user_id(&s), Some(199453377));
    }

    #[test]
    fn unique_user_id_rejects_shared_tail() {
        // Different userIds that share a trailing run must NOT collapse to "3377".
        let s = vec!["199453377".to_string(), "23377".to_string()];
        assert_eq!(unique_user_id(&s), None);
    }

    #[test]
    fn unique_user_id_none_when_empty() {
        let s: Vec<String> = vec![];
        assert_eq!(unique_user_id(&s), None);
    }

    #[test]
    fn sha512_recovery_finds_small_preimage() {
        use sha2::Digest;
        let hash = hex::encode(sha2::Sha512::digest(b"12345"));
        assert_eq!(recover_user_id_from_sha512(&hash), Some(12345));
    }

    #[test]
    fn sha512_recovery_rejects_malformed_hash() {
        assert_eq!(recover_user_id_from_sha512("not-a-hash"), None);
    }

    #[test]
    fn saved_user_id_must_match_active_account_hash() {
        let user_id = 199453377;
        let hash = hex::encode(sha2::Sha512::digest(user_id.to_string().as_bytes()));
        let mut dict = plist::Dictionary::new();
        dict.insert(
            format!("DESIGNATEDFRIENDSREVISION:{hash}"),
            plist::Value::Integer(1.into()),
        );

        assert!(dict_has_active_user_id(&dict, user_id));
        assert!(!dict_has_active_user_id(&dict, user_id + 1));
    }

    #[test]
    fn saved_user_id_rejects_inactive_account_hash() {
        let user_id = 199453377;
        let hash = hex::encode(sha2::Sha512::digest(user_id.to_string().as_bytes()));
        let mut dict = plist::Dictionary::new();
        dict.insert(
            format!("DESIGNATEDFRIENDSREVISION:{hash}"),
            plist::Value::Integer(0.into()),
        );

        assert!(!dict_has_active_user_id(&dict, user_id));
    }
}
