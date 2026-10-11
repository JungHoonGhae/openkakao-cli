use std::sync::Arc;

use anyhow::{anyhow, Result};
use bson::{doc, Document};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

use crate::model::KakaoCredentials;

use super::crypto::LocoEncryptor;
use super::packet::{LocoPacket, PacketBuilder, HEADER_SIZE};

/// Maximum allowed frame/body size to prevent memory exhaustion from untrusted input.
const MAX_FRAME_SIZE: usize = 100 * 1024 * 1024;

const BOOKING_HOST: &str = "booking-loco.kakao.com";
const BOOKING_PORT: u16 = 443;
const DEFAULT_LOCO_PORT: u16 = 5223;

fn booking_request_body(credentials: &KakaoCredentials) -> Document {
    doc! {
        "MCCMNC": "99999",
        "os": "mac",
        // Retained as a Mac client-profile field. The Android request does not
        // establish that it is safe to remove from the desktop profile.
        "model": "",
        "userId": credentials.user_id,
    }
}

fn checkin_request_body(credentials: &KakaoCredentials) -> Document {
    doc! {
        "userId": credentials.user_id,
        "os": "mac",
        "ntype": 0_i32,
        "appVer": &credentials.app_version,
        "MCCMNC": "99999",
        "lang": "ko",
        // These are retained desktop/sub-device profile fields. Their absence
        // from Android is not evidence that the Mac LOCO profile should omit them.
        "countryISO": "KR",
        "useSub": true,
    }
}

fn loginlist_request_body(
    credentials: &KakaoCredentials,
    sync_chat_ids: &[(i64, i64)],
) -> Document {
    let chat_ids = sync_chat_ids
        .iter()
        .map(|(chat_id, _)| bson::Bson::Int64(*chat_id))
        .collect::<Vec<_>>();
    let max_ids = sync_chat_ids
        .iter()
        .map(|(_, max_id)| bson::Bson::Int64(*max_id))
        .collect::<Vec<_>>();

    doc! {
        "appVer": &credentials.app_version,
        "prtVer": "1",
        "os": "mac",
        "lang": "ko",
        "duuid": &credentials.device_uuid,
        "oauthToken": &credentials.oauth_token,
        // Retained as a Mac client-profile field; current Android omits it.
        "dtype": 2_i32,
        "ntype": 0_i32,
        "MCCMNC": "99999",
        "revision": 0_i32,
        "chatIds": bson::Bson::Array(chat_ids),
        "maxIds": bson::Bson::Array(max_ids),
        "lastTokenId": 0_i64,
        "lbk": 0_i32,
        "rp": bson::Bson::Binary(bson::Binary {
            subtype: bson::spec::BinarySubtype::Generic,
            bytes: vec![0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00],
        }),
        "bg": false,
    }
}

enum LocoStream {
    Tls(Box<TlsStream<TcpStream>>),
    Legacy {
        stream: TcpStream,
        encryptor: Box<LocoEncryptor>,
    },
}

impl LocoStream {
    async fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        match self {
            LocoStream::Tls(s) => {
                s.write_all(data).await?;
                s.flush().await?;
            }
            LocoStream::Legacy { stream, .. } => {
                stream.write_all(data).await?;
                stream.flush().await?;
            }
        }
        Ok(())
    }

    async fn send_packet(&mut self, packet: &LocoPacket) -> Result<()> {
        let raw = packet.encode();
        match self {
            LocoStream::Tls(_) => self.send_raw(&raw).await,
            LocoStream::Legacy { encryptor, stream } => {
                let encrypted = encryptor.encrypt(&raw);
                stream.write_all(&encrypted).await?;
                stream.flush().await?;
                Ok(())
            }
        }
    }

    async fn recv_packet(&mut self) -> Result<LocoPacket> {
        match self {
            LocoStream::Tls(s) => {
                let mut header = vec![0u8; HEADER_SIZE];
                s.read_exact(&mut header).await?;
                let (_, _, _, _, body_length) = LocoPacket::decode_header(&header)?;
                let body_len = body_length as usize;
                if body_len > MAX_FRAME_SIZE {
                    return Err(anyhow!("Body size {} exceeds limit", body_len));
                }
                let mut body = vec![0u8; body_len];
                s.read_exact(&mut body).await?;
                let mut full = header;
                full.extend_from_slice(&body);
                LocoPacket::decode(&full)
            }
            LocoStream::Legacy {
                stream, encryptor, ..
            } => {
                // Read first encrypted frame
                let mut size_buf = [0u8; 4];
                stream.read_exact(&mut size_buf).await?;
                let size = ReadBytesExt::read_u32::<LittleEndian>(&mut Cursor::new(&size_buf[..]))?
                    as usize;
                if size > MAX_FRAME_SIZE {
                    return Err(anyhow!("Frame size {} exceeds limit", size));
                }
                let mut frame = vec![0u8; size];
                stream.read_exact(&mut frame).await?;
                let mut decrypted = encryptor.decrypt(&frame)?;

                // Parse header to determine total packet size
                if decrypted.len() >= HEADER_SIZE {
                    let (_, _, _, _, body_length) = LocoPacket::decode_header(&decrypted)?;
                    let total_needed = HEADER_SIZE + body_length as usize;
                    if total_needed > MAX_FRAME_SIZE + HEADER_SIZE {
                        return Err(anyhow!("Total packet size {} exceeds limit", total_needed));
                    }

                    // Read additional frames if the first frame doesn't contain the full packet
                    while decrypted.len() < total_needed {
                        let fragment_result = timeout(Duration::from_secs(30), async {
                            let mut size_buf2 = [0u8; 4];
                            stream.read_exact(&mut size_buf2).await?;
                            let size2 = ReadBytesExt::read_u32::<LittleEndian>(&mut Cursor::new(
                                &size_buf2[..],
                            ))? as usize;
                            if size2 > MAX_FRAME_SIZE {
                                return Err(anyhow!("Frame size {} exceeds limit", size2));
                            }
                            let mut frame2 = vec![0u8; size2];
                            stream.read_exact(&mut frame2).await?;
                            Ok::<Vec<u8>, anyhow::Error>(frame2)
                        })
                        .await;

                        match fragment_result {
                            Ok(Ok(frame2)) => {
                                let decrypted2 = encryptor.decrypt(&frame2)?;
                                decrypted.extend_from_slice(&decrypted2);
                            }
                            Ok(Err(e)) => return Err(e),
                            Err(_) => return Err(anyhow!("Frame reassembly timed out after 30s")),
                        }
                    }
                }

                LocoPacket::decode(&decrypted)
            }
        }
    }
}

async fn tls_connect(host: &str, port: u16) -> Result<TlsStream<TcpStream>> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(root_store)
    .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));
    let server_name = host.to_string().try_into()?;
    let tcp = TcpStream::connect((host, port)).await?;
    let tls = connector.connect(server_name, tcp).await?;
    Ok(tls)
}

/// Execute a one-shot LOCO request: connect, send one packet, read one response, close.
async fn loco_oneshot(
    host: &str,
    port: u16,
    packet: &LocoPacket,
    use_tls: bool,
) -> Result<LocoPacket> {
    if use_tls {
        let mut tls = tls_connect(host, port).await?;
        tls.write_all(&packet.encode()).await?;
        tls.flush().await?;

        let mut header = vec![0u8; HEADER_SIZE];
        tls.read_exact(&mut header).await?;
        let (_, _, _, _, body_length) = LocoPacket::decode_header(&header)?;
        let body_len = body_length as usize;
        if body_len > MAX_FRAME_SIZE {
            anyhow::bail!(
                "TLS body size {} exceeds limit {}",
                body_len,
                MAX_FRAME_SIZE
            );
        }
        let mut body = vec![0u8; body_len];
        tls.read_exact(&mut body).await?;

        let mut full = header;
        full.extend_from_slice(&body);
        tls.shutdown().await.ok();
        LocoPacket::decode(&full)
    } else {
        let mut tcp = TcpStream::connect((host, port)).await?;
        let enc = LocoEncryptor::new();

        // Send handshake
        let handshake = enc.build_handshake_packet()?;
        tcp.write_all(&handshake).await?;
        tcp.flush().await?;

        // Send encrypted packet
        let encrypted = enc.encrypt(&packet.encode());
        tcp.write_all(&encrypted).await?;
        tcp.flush().await?;

        // Read response
        let mut size_buf = [0u8; 4];
        tcp.read_exact(&mut size_buf).await?;
        let size =
            ReadBytesExt::read_u32::<LittleEndian>(&mut Cursor::new(&size_buf[..]))? as usize;
        if size > MAX_FRAME_SIZE {
            anyhow::bail!(
                "Legacy frame size {} exceeds limit {}",
                size,
                MAX_FRAME_SIZE
            );
        }
        let mut frame = vec![0u8; size];
        tcp.read_exact(&mut frame).await?;

        let decrypted = enc.decrypt(&frame)?;
        tcp.shutdown().await.ok();
        LocoPacket::decode(&decrypted)
    }
}

pub struct LocoClient {
    pub credentials: KakaoCredentials,
    packet_builder: PacketBuilder,
    stream: Option<LocoStream>,
    /// Optional (chatId, maxId) pairs to include in LOGINLIST for message sync.
    /// When set, the server returns chatLog data for these chats.
    pub sync_chat_ids: Vec<(i64, i64)>,
    is_dirty: bool,
}

#[derive(Debug, Default)]
pub struct ProbeCommandResult {
    pub response: Option<LocoPacket>,
    pub pushes: Vec<LocoPacket>,
}

impl LocoClient {
    pub fn new(credentials: KakaoCredentials) -> Self {
        Self {
            credentials,
            packet_builder: PacketBuilder::new(),
            stream: None,
            sync_chat_ids: Vec::new(),
            is_dirty: false,
        }
    }

    /// Phase 1: Booking — get configuration and checkin server info.
    pub async fn booking(&self) -> Result<Document> {
        let builder = PacketBuilder::new();
        let pkt = builder.build("GETCONF", booking_request_body(&self.credentials));
        eprintln!(
            "[booking] Connecting to {}:{}...",
            BOOKING_HOST, BOOKING_PORT
        );
        let response = loco_oneshot(BOOKING_HOST, BOOKING_PORT, &pkt, true).await?;
        let status = response.status();
        eprintln!("[booking] Got config (status={})", status);
        Ok(response.body)
    }

    /// Phase 2: Checkin — get assigned LOCO chat server.
    pub async fn checkin(&self, checkin_host: &str, checkin_port: u16) -> Result<(Document, bool)> {
        let builder = PacketBuilder::new();
        let pkt = builder.build("CHECKIN", checkin_request_body(&self.credentials));

        // Try TLS first on 443, then checkin_port TLS, then legacy, then fallback 995
        let mut attempts: Vec<(bool, u16)> =
            vec![(true, 443), (true, checkin_port), (false, checkin_port)];
        if checkin_port != 995 {
            attempts.push((false, 995));
        }

        for (use_tls, port) in &attempts {
            eprintln!(
                "[checkin] Trying {}:{} (TLS={})...",
                checkin_host, port, use_tls
            );
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                loco_oneshot(checkin_host, *port, &pkt, *use_tls),
            )
            .await
            {
                Ok(Ok(response)) => {
                    if let Ok(host) = response.body.get_str("host") {
                        let loco_port = response
                            .body
                            .get_i32("port")
                            .ok()
                            .filter(|&p| p > 0 && p <= 65535)
                            .map(|p| p as u16)
                            .unwrap_or(DEFAULT_LOCO_PORT);
                        eprintln!(
                            "[checkin] Server: {}:{} (TLS={}, port={})",
                            host, loco_port, use_tls, port
                        );
                        return Ok((response.body, *use_tls));
                    }
                    eprintln!("[checkin] No host in response, trying next...");
                }
                Ok(Err(e)) => {
                    eprintln!("[checkin] TLS={} port={} failed: {}", use_tls, port, e);
                }
                Err(_) => {
                    eprintln!("[checkin] TLS={} port={} timed out", use_tls, port);
                }
            }
        }

        Err(anyhow!("All checkin attempts failed"))
    }

    /// Phase 3: Connect to a LOCO server (persistent connection).
    pub async fn connect(&mut self, host: &str, port: u16, use_tls: bool) -> Result<()> {
        eprintln!(
            "[loco] Connecting to {}:{} (TLS={})...",
            host, port, use_tls
        );

        if use_tls {
            let tls = tls_connect(host, port).await?;
            self.stream = Some(LocoStream::Tls(Box::new(tls)));
        } else {
            let mut tcp = TcpStream::connect((host, port)).await?;
            let enc = LocoEncryptor::new();
            let handshake = enc.build_handshake_packet()?;
            if (std::env::var("OPENKAKAO_CLI_DEBUG").is_ok()
                || std::env::var("OPENKAKAO_RS_DEBUG").is_ok())
                && handshake.len() >= 12
            {
                let key_size =
                    u32::from_le_bytes([handshake[0], handshake[1], handshake[2], handshake[3]]);
                let key_type =
                    u32::from_le_bytes([handshake[4], handshake[5], handshake[6], handshake[7]]);
                let enc_type =
                    u32::from_le_bytes([handshake[8], handshake[9], handshake[10], handshake[11]]);
                eprintln!(
                    "[handshake] key_size={}, key_type={}, encrypt_type={}, total_len={}",
                    key_size,
                    key_type,
                    enc_type,
                    handshake.len()
                );
            }
            tcp.write_all(&handshake).await?;
            tcp.flush().await?;
            self.stream = Some(LocoStream::Legacy {
                stream: tcp,
                encryptor: Box::new(enc),
            });
        }

        self.is_dirty = false;
        eprintln!("[loco] Connected");
        Ok(())
    }

    /// Send a command and wait for the matching response (by packet_id).
    /// Skips any server push packets received before the response.
    pub async fn send_command(&mut self, method: &str, body: Document) -> Result<LocoPacket> {
        if self.is_dirty {
            eprintln!("[loco] Connection dirty, disconnecting for fresh reconnect");
            self.disconnect();
            return Err(anyhow!("Connection was dirty, disconnected for reconnect"));
        }

        let packet = self.packet_builder.build(method, body);
        let expected_id = packet.packet_id;
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;
        if let Err(e) = stream.send_packet(&packet).await {
            self.is_dirty = true;
            return Err(e);
        }

        // Read packets until we find the response matching our packet_id
        let mut skip_count = 0usize;
        loop {
            let response = match stream.recv_packet().await {
                Ok(r) => r,
                Err(e) => {
                    self.is_dirty = true;
                    return Err(e);
                }
            };
            if response.packet_id == expected_id {
                return Ok(response);
            }
            skip_count += 1;
            if skip_count >= 500 {
                self.is_dirty = true;
                return Err(anyhow!(
                    "Too many push packets skipped ({}), response not received for {}",
                    skip_count,
                    method
                ));
            }
            // Skip push packets (packet_id 0 or non-matching)
            eprintln!(
                "[loco] Skipping push: {} (id={})",
                response.method, response.packet_id
            );
        }
    }

    /// Probe-oriented command path that preserves interleaved push packets.
    /// Useful for reverse-engineering methods whose useful data arrives as pushes
    /// before, instead of, or without a direct matching response packet.
    pub async fn send_command_collect(
        &mut self,
        method: &str,
        body: Document,
        idle_timeout: Duration,
    ) -> Result<ProbeCommandResult> {
        let packet = self.packet_builder.build(method, body);
        let expected_id = packet.packet_id;
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;
        stream.send_packet(&packet).await?;

        let mut result = ProbeCommandResult::default();

        loop {
            match timeout(idle_timeout, stream.recv_packet()).await {
                Ok(Ok(packet)) => {
                    if packet.packet_id == expected_id {
                        result.response = Some(packet);
                        return Ok(result);
                    }
                    result.pushes.push(packet);
                }
                Ok(Err(err)) => {
                    if result.pushes.is_empty() {
                        return Err(err);
                    }
                    return Ok(result);
                }
                Err(_) => return Ok(result),
            }
        }
    }

    /// Send a raw packet without waiting for response (for PING keepalive).
    pub async fn send_packet(&mut self, method: &str, body: Document) -> Result<()> {
        let packet = self.packet_builder.build(method, body);
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;
        stream.send_packet(&packet).await
    }

    /// Receive a single packet from the stream.
    pub async fn recv_packet(&mut self) -> Result<LocoPacket> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;
        stream.recv_packet().await
    }

    /// Phase 3: Authenticate with LOGINLIST.
    /// When `sync_chat_ids` is set, includes those chatIds/maxIds so the server
    /// returns chatLog data with recent messages for those chats.
    pub async fn login(&mut self) -> Result<Document> {
        let login_body = loginlist_request_body(&self.credentials, &self.sync_chat_ids);

        if std::env::var("OPENKAKAO_CLI_DEBUG").is_ok()
            || std::env::var("OPENKAKAO_RS_DEBUG").is_ok()
        {
            eprintln!(
                "[login] LOGINLIST: appVer={}, os=mac, token_len={}",
                self.credentials.app_version,
                self.credentials.oauth_token.len(),
            );
        }

        let response = self.send_command("LOGINLIST", login_body).await?;

        let user_id = response
            .body
            .get_i64("userId")
            .or_else(|_| response.body.get_i32("userId").map(|v| v as i64))
            .unwrap_or(0);
        let status = response.status();

        eprintln!("[login] Status: {}, userId: {}", status, user_id);
        Ok(response.body)
    }

    /// Execute the full connection flow: booking -> checkin -> connect -> login.
    pub async fn full_connect(&mut self) -> Result<Document> {
        // Phase 1: Booking
        let config = self.booking().await?;

        // Extract checkin hosts from ticket.lsl
        let checkin_hosts: Vec<String> = config
            .get_document("ticket")
            .ok()
            .and_then(|t| t.get_array("lsl").ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if checkin_hosts.is_empty() {
            return Err(anyhow!("No checkin hosts in booking response"));
        }

        // Get ports from wifi config
        let ports: Vec<u16> = config
            .get_document("wifi")
            .ok()
            .and_then(|w| w.get_array("ports").ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_i32().map(|p| p as u16))
                    .collect()
            })
            .unwrap_or_default();

        let checkin_host = &checkin_hosts[0];
        let checkin_port = ports.first().copied().unwrap_or(DEFAULT_LOCO_PORT);

        // Phase 2: Checkin
        let (checkin_data, use_tls) = self.checkin(checkin_host, checkin_port).await?;

        let loco_host = checkin_data
            .get_str("host")
            .map_err(|_| anyhow!("No LOCO host from checkin"))?
            .to_string();
        let loco_port = checkin_data
            .get_i32("port")
            .map(|p| p as u16)
            .unwrap_or(DEFAULT_LOCO_PORT);

        // Phase 3: Connect and login
        self.connect(&loco_host, loco_port, use_tls).await?;
        let login_data = self.login().await?;

        Ok(login_data)
    }

    /// Disconnect from the LOCO server, dropping the stream.
    pub fn disconnect(&mut self) {
        self.stream = None;
        self.is_dirty = false;
    }

    /// Gracefully shut down the stream before disconnecting.
    pub async fn disconnect_graceful(&mut self) {
        if let Some(stream) = &mut self.stream {
            match stream {
                LocoStream::Tls(s) => {
                    if let Err(e) = tokio::io::AsyncWriteExt::shutdown(s.as_mut()).await {
                        eprintln!("[loco] TLS shutdown error (ignored): {}", e);
                    }
                }
                LocoStream::Legacy { stream: tcp, .. } => {
                    if let Err(e) = tokio::io::AsyncWriteExt::shutdown(tcp).await {
                        eprintln!("[loco] TCP shutdown error (ignored): {}", e);
                    }
                }
            }
        }
        self.stream = None;
        self.is_dirty = false;
    }

    /// Execute full_connect with exponential backoff retry.
    /// Retries on transient errors (connection refused, timeout, TLS errors).
    /// Does NOT retry on auth errors (-950, -999) as those need different fixes.
    pub async fn full_connect_with_retry(&mut self, max_retries: u32) -> Result<Document> {
        let mut attempt = 0;
        loop {
            match self.full_connect().await {
                Ok(data) => return Ok(data),
                Err(e) => {
                    let err_msg = e.to_string();
                    // Don't retry on auth/protocol errors
                    if err_msg.contains("-950")
                        || err_msg.contains("-999")
                        || err_msg.contains("-400")
                    {
                        return Err(e);
                    }

                    attempt += 1;
                    if attempt > max_retries {
                        return Err(anyhow!(
                            "Failed after {} attempts. Last error: {}",
                            max_retries,
                            e
                        ));
                    }

                    let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
                    eprintln!(
                        "[loco] Attempt {}/{} failed: {}. Retrying in {:?}...",
                        attempt, max_retries, e, delay
                    );
                    tokio::time::sleep(delay).await;
                    // Reset stream for fresh connection
                    self.disconnect();
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Upload a file to the LOCO media server.
///
/// Flow: connect to vhost:port → send POST packet → send raw bytes → wait for status.
pub async fn loco_upload(
    vhost: &str,
    port: u16,
    user_id: i64,
    key: &str,
    chat_id: i64,
    data: &[u8],
    msg_type: i32,
    width: i32,
    height: i32,
    app_version: &str,
) -> Result<()> {
    eprintln!("[upload] Connecting to {}:{}...", vhost, port);

    // Upload server uses legacy encrypted connection (same as main LOCO)
    let mut tcp = TcpStream::connect((vhost, port)).await?;
    let enc = LocoEncryptor::new();
    let handshake = enc.build_handshake_packet()?;
    tcp.write_all(&handshake).await?;
    tcp.flush().await?;

    let mut stream = LocoStream::Legacy {
        stream: tcp,
        encryptor: Box::new(enc),
    };

    // Build POST packet
    let builder = PacketBuilder::new();
    let post_body = doc! {
        "u": user_id,
        "k": key,
        "t": msg_type,
        "s": data.len() as i64,
        "c": chat_id,
        "mid": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
        "w": width,
        "h": height,
        "mm": "99999",
        "nt": 0_i32,
        "os": "mac",
        "av": app_version,
        "ex": "{}",
        "ns": false,
        "dt": 4_i32,
        "scp": 1_i32,
    };
    let post_pkt = builder.build("POST", post_body);
    stream.send_packet(&post_pkt).await?;

    // Read POST response
    let post_resp = stream.recv_packet().await?;
    let post_status = post_resp.status();
    if post_status != 0 {
        return Err(anyhow!(
            "POST failed (status={}): {:?}",
            post_status,
            post_resp.body
        ));
    }
    eprintln!("[upload] POST accepted, sending {} bytes...", data.len());

    // Send raw file data through the encrypted channel
    match &mut stream {
        LocoStream::Legacy {
            stream: tcp,
            encryptor,
        } => {
            let encrypted = encryptor.encrypt(data);
            tcp.write_all(&encrypted).await?;
            tcp.flush().await?;
        }
        LocoStream::Tls(s) => {
            s.write_all(data).await?;
            s.flush().await?;
        }
    }

    // Wait for upload completion response
    let upload_resp = stream.recv_packet().await?;
    let upload_status = upload_resp.status();
    if upload_status != 0 {
        return Err(anyhow!(
            "Upload failed (status={}): {:?}",
            upload_status,
            upload_resp.body
        ));
    }

    eprintln!("[upload] Complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> KakaoCredentials {
        KakaoCredentials::new(
            "test-oauth-token".to_string(),
            42_i64,
            "test-device-uuid".to_string(),
            "26.6.1".to_string(),
            "KT/26.6.1 Mac/26.0".to_string(),
            "mac/26.6.1/ko".to_string(),
        )
    }

    #[test]
    fn booking_body_adds_mac_confirmed_user_id_and_retains_compatibility_fields() {
        let body = booking_request_body(&credentials());

        assert_eq!(body.len(), 4);
        assert_eq!(body.get_str("MCCMNC"), Ok("99999"));
        assert_eq!(body.get_str("os"), Ok("mac"));
        assert_eq!(body.get_i64("userId"), Ok(42));
        assert_eq!(body.get_str("model"), Ok(""));
    }

    #[test]
    fn checkin_body_preserves_desktop_subdevice_profile() {
        let body = checkin_request_body(&credentials());

        assert_eq!(body.len(), 8);
        assert_eq!(body.get_i64("userId"), Ok(42));
        assert_eq!(body.get_str("os"), Ok("mac"));
        assert_eq!(body.get_i32("ntype"), Ok(0));
        assert_eq!(body.get_str("appVer"), Ok("26.6.1"));
        assert_eq!(body.get_str("MCCMNC"), Ok("99999"));
        assert_eq!(body.get_str("lang"), Ok("ko"));
        assert_eq!(body.get_str("countryISO"), Ok("KR"));
        assert_eq!(body.get_bool("useSub"), Ok(true));
    }

    #[test]
    fn loginlist_body_preserves_current_profile_and_sync_pairs() {
        let body = loginlist_request_body(&credentials(), &[(100, 900), (200, 0)]);

        assert_eq!(body.len(), 16);
        assert_eq!(body.get_str("appVer"), Ok("26.6.1"));
        assert_eq!(body.get_str("prtVer"), Ok("1"));
        assert_eq!(body.get_str("os"), Ok("mac"));
        assert_eq!(body.get_str("lang"), Ok("ko"));
        assert_eq!(body.get_str("duuid"), Ok("test-device-uuid"));
        assert_eq!(body.get_str("oauthToken"), Ok("test-oauth-token"));
        assert_eq!(body.get_i32("dtype"), Ok(2));
        assert_eq!(body.get_i32("ntype"), Ok(0));
        assert_eq!(body.get_str("MCCMNC"), Ok("99999"));
        assert_eq!(body.get_i32("revision"), Ok(0));
        assert_eq!(body.get_i64("lastTokenId"), Ok(0));
        assert_eq!(body.get_i32("lbk"), Ok(0));
        assert_eq!(body.get_bool("bg"), Ok(false));
        assert!(!body.contains_key("isSw"));
        assert_eq!(
            body.get_array("chatIds"),
            Ok(&vec![bson::Bson::Int64(100), bson::Bson::Int64(200)])
        );
        assert_eq!(
            body.get_array("maxIds"),
            Ok(&vec![bson::Bson::Int64(900), bson::Bson::Int64(0)])
        );

        let rp = body.get_binary_generic("rp").expect("rp binary field");
        assert_eq!(rp, &[0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn loginlist_body_uses_empty_sync_arrays_by_default() {
        let body = loginlist_request_body(&credentials(), &[]);

        assert_eq!(body.get_array("chatIds"), Ok(&vec![]));
        assert_eq!(body.get_array("maxIds"), Ok(&vec![]));
    }
}
