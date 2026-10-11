use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_TYPE,
};
use serde_json::json;
use serde_json::Value;

use sha2::{Digest, Sha512};

use crate::error::OpenKakaoError;
use crate::model::{
    json_i64, json_string, ChatMember, ChatMessage, ChatRoom, Friend, KakaoCredentials, MyProfile,
};

const BASE_URL: &str = "https://katalk.kakao.com";
const PILSNER_URL: &str = "https://talk-pilsner.kakao.com";

pub struct KakaoRestClient {
    creds: KakaoCredentials,
    client: Client,
}

impl KakaoRestClient {
    pub fn new(creds: KakaoCredentials) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { creds, client })
    }

    pub fn verify_token(&self) -> Result<bool> {
        let r = self.request_raw(
            "POST",
            &format!("{BASE_URL}/mac/account/more_settings.json"),
            Some("since=0&locale_country=KR"),
        )?;
        Ok(json_i64(&r, "status") == 0)
    }

    pub fn get_my_profile(&self) -> Result<MyProfile> {
        let profile = self.request(
            "POST",
            &format!("{BASE_URL}/mac/profile3/me.json"),
            Some("since=0"),
        )?;
        let settings = self.request(
            "POST",
            &format!("{BASE_URL}/mac/account/more_settings.json"),
            Some("since=0&locale_country=KR"),
        )?;

        let p = profile.get("profile").cloned().unwrap_or(Value::Null);

        Ok(MyProfile {
            nickname: json_string(&p, "nickname"),
            status_message: json_string(&p, "statusMessage"),
            account_id: json_i64(&settings, "accountId"),
            email: json_string(&settings, "emailAddress"),
            user_id: {
                let id = json_i64(&p, "userId");
                if id == 0 {
                    self.creds.user_id
                } else {
                    id
                }
            },
            profile_image_url: json_string(&p, "fullProfileImageUrl"),
        })
    }

    pub fn get_friend_profile(&self, user_id: i64) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/profile3/friend.json"),
            Some(&format!("id={user_id}")),
        )
    }

    pub fn get_profiles(&self) -> Result<Value> {
        self.request("GET", &format!("{BASE_URL}/mac/profile/list.json"), None)
    }

    pub fn get_friends(&self) -> Result<Vec<Friend>> {
        let r = self.request(
            "POST",
            &format!("{BASE_URL}/mac/friends/update.json"),
            Some("since=0"),
        )?;

        let mut out = Vec::new();
        if let Some(arr) = r.get("friends").and_then(Value::as_array) {
            for item in arr {
                out.push(Friend::from_json(item));
            }
        } else if let Some(arr) = r.get("added").and_then(Value::as_array) {
            for item in arr {
                out.push(Friend::from_json(item));
            }
        }

        Ok(out)
    }

    pub fn add_favorite(&self, user_id: i64) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/friends/add_favorite.json"),
            Some(&format!("id={user_id}")),
        )
    }

    pub fn remove_favorite(&self, user_id: i64) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/friends/remove_favorite.json"),
            Some(&format!("id={user_id}")),
        )
    }

    pub fn hide_friend(&self, user_id: i64) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/friends/hide.json"),
            Some(&format!("id={user_id}")),
        )
    }

    pub fn unhide_friend(&self, user_id: i64) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/friends/unhide.json"),
            Some(&format!("id={user_id}")),
        )
    }

    pub fn get_alarm_keywords(&self) -> Result<Value> {
        self.request(
            "GET",
            &format!("{BASE_URL}/mac/alarm_keywords/list.json"),
            None,
        )
    }

    pub fn get_chats(&self, cursor: Option<i64>) -> Result<(Vec<ChatRoom>, Option<i64>)> {
        let url = if let Some(c) = cursor {
            format!("{PILSNER_URL}/messaging/chats?cursor={c}")
        } else {
            format!("{PILSNER_URL}/messaging/chats")
        };

        let r = self.request("GET", &url, None)?;
        let mut rooms = Vec::new();

        if let Some(chats) = r.get("chats").and_then(Value::as_array) {
            for chat in chats {
                rooms.push(ChatRoom::from_json(chat));
            }
        }

        let next_cursor = if r.get("last").and_then(Value::as_bool).unwrap_or(false) {
            None
        } else {
            let n = json_i64(&r, "nextCursor");
            if n == 0 {
                None
            } else {
                Some(n)
            }
        };

        Ok((rooms, next_cursor))
    }

    pub fn get_all_chats(&self) -> Result<Vec<ChatRoom>> {
        let mut all = Vec::new();
        let mut cursor: Option<i64> = None;

        loop {
            let (rooms, next_cursor) = self.get_chats(cursor)?;
            all.extend(rooms);
            if next_cursor.is_none() {
                break;
            }
            cursor = next_cursor;
        }

        Ok(all)
    }

    pub fn get_chat_members(&self, chat_id: i64) -> Result<Vec<ChatMember>> {
        let r = self.request(
            "GET",
            &format!("{PILSNER_URL}/messaging/chats/{chat_id}/members"),
            None,
        )?;

        let mut members = Vec::new();
        if let Some(arr) = r.get("members").and_then(Value::as_array) {
            for member in arr {
                members.push(ChatMember::from_json(member));
            }
        }

        Ok(members)
    }

    /// Get one page of messages. Returns (messages, next_cursor).
    /// next_cursor=0 means no more pages.
    ///
    /// NOTE: `fromLogId` and `sinceMessageId` do NOT work for pagination.
    /// Only `?cursor=` works.
    pub fn get_messages(
        &self,
        chat_id: i64,
        cursor: Option<i64>,
    ) -> Result<(Vec<ChatMessage>, i64)> {
        let url = if let Some(c) = cursor {
            format!("{PILSNER_URL}/messaging/chats/{chat_id}/messages?cursor={c}")
        } else {
            format!("{PILSNER_URL}/messaging/chats/{chat_id}/messages")
        };

        let r = self.request("GET", &url, None)?;

        let mut messages = Vec::new();
        if let Some(arr) = r.get("chatLogs").and_then(Value::as_array) {
            for msg in arr {
                messages.push(ChatMessage::from_json(msg));
            }
        }

        let next_cursor = r.get("nextCursor").and_then(Value::as_i64).unwrap_or(0);
        Ok((messages, next_cursor))
    }

    /// Fetch all available messages using cursor pagination.
    ///
    /// The pilsner server only caches messages for chats recently opened
    /// in the KakaoTalk Mac app. Most chats will return empty results.
    pub fn get_all_messages(&self, chat_id: i64, max_pages: usize) -> Result<Vec<ChatMessage>> {
        let mut all = Vec::new();
        let mut cursor: Option<i64> = None;

        for _ in 0..max_pages {
            let (messages, next_cursor) = self.get_messages(chat_id, cursor)?;
            if messages.is_empty() {
                break;
            }
            all.extend(messages);
            if next_cursor == 0 {
                break;
            }
            cursor = Some(next_cursor);
        }

        all.sort_by_key(|m| m.log_id);
        all.dedup_by_key(|m| m.log_id);
        Ok(all)
    }

    /// Attempt to renew the OAuth token using a refresh_token (legacy endpoint).
    /// Returns the raw JSON response (may contain access_token, refresh_token, etc.)
    pub fn renew_token(&self, refresh_token: &str) -> Result<Value> {
        let encoded_token = urlencoding::encode(refresh_token);
        let body = format!("grant_type=refresh_token&refresh_token={encoded_token}");
        self.request_raw(
            "POST",
            &format!("{BASE_URL}/mac/account/renew_token.json"),
            Some(&body),
        )
    }

    /// Attempt to refresh the OAuth token using oauth2_token.json (node-kakao style).
    /// Sends both access_token and refresh_token as required by Kakao's OAuth.
    pub fn oauth2_token(&self, refresh_token: &str) -> Result<Value> {
        let access_token = urlencoding::encode(&self.creds.oauth_token);
        let refresh = urlencoding::encode(refresh_token);
        let body =
            format!("grant_type=refresh_token&access_token={access_token}&refresh_token={refresh}");
        self.request_raw(
            "POST",
            &format!("{BASE_URL}/mac/account/oauth2_token.json"),
            Some(&body),
        )
    }

    /// Call login.json with cached credentials and X-VC header.
    /// Returns the raw JSON response.
    pub fn login_direct(
        &self,
        email: &str,
        password: &str,
        device_uuid: &str,
        device_name: &str,
        x_vc: &str,
    ) -> Result<Value> {
        let user_agent = if self.creds.user_agent.is_empty() {
            format!("KT/{} Mc/26.1.0 ko", self.creds.app_version)
        } else {
            self.creds.user_agent.clone()
        };
        self.login_direct_with_ua(email, password, device_uuid, device_name, x_vc, &user_agent)
    }

    fn login_direct_with_ua(
        &self,
        email: &str,
        password: &str,
        device_uuid: &str,
        device_name: &str,
        x_vc: &str,
        user_agent: &str,
    ) -> Result<Value> {
        let encoded_name = urlencoding::encode(device_name);
        let encoded_uuid = urlencoding::encode(device_uuid);
        let encoded_password = urlencoding::encode(password);
        let encoded_email = urlencoding::encode(email);
        let body = format!(
            "device_name={encoded_name}&device_uuid={encoded_uuid}&email={encoded_email}&os_version=26.1.0&password={encoded_password}&permanent=1"
        );

        self.post_account_form("login.json", &body, x_vc, user_agent)
    }

    /// POST a urlencoded form to a `/mac/account/<endpoint>` route with the
    /// unauthenticated header set (A, User-Agent, optional X-VC) and parse the JSON
    /// reply. Shared by login, request_passcode, and register_device.
    fn post_account_form(
        &self,
        endpoint: &str,
        body: &str,
        x_vc: &str,
        user_agent: &str,
    ) -> Result<Value> {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ko"));

        let a_header = if self.creds.a_header.is_empty() {
            format!("mac/{}/ko", self.creds.app_version)
        } else {
            self.creds.a_header.clone()
        };
        headers.insert(
            "A",
            HeaderValue::from_str(&a_header).context("Invalid A header")?,
        );

        headers.insert(
            "User-Agent",
            HeaderValue::from_str(user_agent).context("Invalid User-Agent header")?,
        );

        if !x_vc.is_empty() {
            headers.insert(
                "X-VC",
                HeaderValue::from_str(x_vc).context("Invalid X-VC header")?,
            );
        }

        let response = self
            .client
            .post(format!("{BASE_URL}/mac/account/{endpoint}"))
            .headers(headers)
            .body(body.to_string())
            .send()
            .with_context(|| format!("{endpoint} request failed"))?;

        let text = response.text().context("Failed to read response")?;
        serde_json::from_str(&text).with_context(|| {
            format!(
                "Failed to parse {} response: {}",
                endpoint,
                &text[..200.min(text.len())]
            )
        })
    }

    /// POST a json to a `/mac/account/<endpoint>` route with the
    /// unauthenticated header set (A, User-Agent, optional X-VC) and parse the JSON
    fn post_account_json(
        &self,
        endpoint: &str,
        body: &str,
        x_vc: &str,
        user_agent: &str,
    ) -> Result<Value> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ko"));

        let a_header = if self.creds.a_header.is_empty() {
            format!("mac/{}/ko", self.creds.app_version)
        } else {
            self.creds.a_header.clone()
        };
        headers.insert(
            "A",
            HeaderValue::from_str(&a_header).context("Invalid A header")?,
        );

        headers.insert(
            "User-Agent",
            HeaderValue::from_str(user_agent).context("Invalid User-Agent header")?,
        );

        if !x_vc.is_empty() {
            headers.insert(
                "X-VC",
                HeaderValue::from_str(x_vc).context("Invalid X-VC header")?,
            );
        }

        let response = self
            .client
            .post(format!("{BASE_URL}/mac/account/{endpoint}"))
            .headers(headers)
            .body(body.to_string())
            .send()
            .with_context(|| format!("{endpoint} request failed"))?;

        let text = response.text().context("Failed to read response")?;
        serde_json::from_str(&text).with_context(|| {
            format!(
                "Failed to parse {} response: {}",
                endpoint,
                &text[..200.min(text.len())]
            )
        })
    }

    pub fn request_passcode(
        &self,
        email: &str,
        password: &str,
        device_uuid: &str,
        device_name: &str,
    ) -> Result<Value> {
        let user_agent = format!("KT/{} Mc/26.1.0 ko", self.creds.app_version);
        let xvc = Self::generate_xvc(&user_agent, email, device_uuid);
        let body = json!({
            "email": email,
            "password": password,
            "permanent": true,
            "device": {
                "name": device_name,
                "uuid": device_uuid,
                "model": device_name,
                "osVersion": "26.1.0"
            }
        })
        .to_string();
        self.post_account_json("passcodeLogin/generate", &body, &xvc, &user_agent)
    }

    pub fn register_device(&self, email: &str, password: &str, device_uuid: &str) -> Result<Value> {
        let user_agent = format!("KT/{} Mc/26.1.0 ko", self.creds.app_version);
        let xvc = Self::generate_xvc(&user_agent, email, device_uuid);
        let body = json!({
            "email": email,
            "password": password,
            "device": {
                "uuid": device_uuid
            }
        })
        .to_string();
        self.post_account_json("passcodeLogin/registerDevice", &body, &xvc, &user_agent)
    }

    pub fn get_settings(&self) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/account/more_settings.json"),
            Some("since=0&locale_country=KR"),
        )
    }

    pub fn get_scrap_preview(&self, url: &str) -> Result<Value> {
        let encoded = urlencoding::encode(url);
        let body = format!("url={encoded}");
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/scrap/preview.json"),
            Some(&body),
        )
    }

    /// Generate X-VC header for Mac KakaoTalk.
    /// Algorithm: SHA-512("YLLAS|{loginId}|{uuid}|GRAEB|{userAgent}")[0:16]
    pub fn generate_xvc(user_agent: &str, login_id: &str, device_uuid: &str) -> String {
        let input = format!("YLLAS|{login_id}|{device_uuid}|GRAEB|{user_agent}");
        let h = hex::encode(Sha512::digest(input.as_bytes()));
        h[..16].to_string()
    }

    /// Login using the Mac X-VC algorithm.
    /// Always uses the short User-Agent format ("KT/{ver} Mc/{os} ko") for both
    /// the X-VC hash and the request header, matching what the real app sends.
    pub fn login_with_xvc(
        &self,
        email: &str,
        password: &str,
        device_uuid: &str,
        device_name: &str,
    ) -> Result<Value> {
        let user_agent = format!("KT/{} Mc/26.1.0 ko", self.creds.app_version);

        let xvc = Self::generate_xvc(&user_agent, email, device_uuid);
        self.login_direct_with_ua(email, password, device_uuid, device_name, &xvc, &user_agent)
    }

    fn request(&self, method: &str, url: &str, body: Option<&str>) -> Result<Value> {
        let parsed = self.request_raw(method, url, body)?;
        if let Some(status) = parsed.get("status").and_then(Value::as_i64) {
            if status != 0 {
                let message = parsed
                    .get("message")
                    .or_else(|| parsed.get("msg"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                return Err(OpenKakaoError::RestApi { status, message }.into());
            }
        }
        Ok(parsed)
    }

    fn request_raw(&self, method: &str, url: &str, body: Option<&str>) -> Result<Value> {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ko"));

        // Use rest_token for pilsner endpoints, oauth_token for katalk endpoints
        let token = if url.starts_with(PILSNER_URL) {
            self.creds
                .rest_token
                .as_deref()
                .unwrap_or(&self.creds.oauth_token)
        } else {
            &self.creds.oauth_token
        };
        let auth = HeaderValue::from_str(token).context("Invalid Authorization header")?;
        headers.insert(AUTHORIZATION, auth);

        let a_header = if self.creds.a_header.is_empty() {
            format!("mac/{}/ko", self.creds.app_version)
        } else {
            self.creds.a_header.clone()
        };
        headers.insert(
            "A",
            HeaderValue::from_str(&a_header).context("Invalid A header")?,
        );

        let user_agent = if self.creds.user_agent.is_empty() {
            format!("KT/{} Mc/26.1.0 ko", self.creds.app_version)
        } else {
            self.creds.user_agent.clone()
        };
        headers.insert(
            "User-Agent",
            HeaderValue::from_str(&user_agent).context("Invalid User-Agent header")?,
        );

        let request = match method {
            "GET" => self.client.get(url).headers(headers),
            "POST" => self
                .client
                .post(url)
                .headers(headers)
                .body(body.unwrap_or_default().to_string()),
            _ => return Err(anyhow!("Unsupported HTTP method: {method}")),
        };

        let response = request
            .send()
            .with_context(|| format!("HTTP request failed: {method} {url}"))?;
        let http_status = response.status();
        let text = response
            .text()
            .context("Failed to read HTTP response body")?;

        // Detect pilsner UNAUTHENTICATED (HTTP 401/403 or JSON reason field)
        if !http_status.is_success() {
            // Try to parse for a reason field
            if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                if parsed.get("reason").and_then(Value::as_str) == Some("UNAUTHENTICATED") {
                    return Err(OpenKakaoError::RestApi {
                        status: -(http_status.as_u16() as i64),
                        message: "UNAUTHENTICATED: pilsner requires Cache.db bearer token".into(),
                    }
                    .into());
                }
            }
            return Err(anyhow!("HTTP {}: {}", http_status.as_u16(), text));
        }

        let parsed: Value = serde_json::from_str(&text).with_context(|| {
            format!(
                "Failed to parse JSON response (HTTP {http_status}): {}",
                text.chars().take(200).collect::<String>()
            )
        })?;

        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xvc_is_deterministic_sha512_prefix() {
        // X-VC = SHA512("YLLAS|{login_id}|{uuid}|GRAEB|{user_agent}")[..16]
        let ua = "KT/3.7.0 Mc/26.1.0 ko";
        let xvc = KakaoRestClient::generate_xvc(ua, "user@example.com", "ABCD-1234");
        assert_eq!(xvc.len(), 16);
        assert!(xvc.chars().all(|c| c.is_ascii_hexdigit()));
        // Stable across calls with the same inputs.
        let again = KakaoRestClient::generate_xvc(ua, "user@example.com", "ABCD-1234");
        assert_eq!(xvc, again);
        // Different device UUID changes the value.
        let other = KakaoRestClient::generate_xvc(ua, "user@example.com", "WXYZ-9999");
        assert_ne!(xvc, other);
    }
}
