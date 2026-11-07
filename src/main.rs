mod auth;
mod auth_flow;
mod ax_send;
mod commands;
mod config;
mod credentials;
mod error;
mod export;
mod local_db;
mod loco;
mod loco_helpers;
mod media;
mod message_db;
mod model;
mod receive_inbox;
mod rest;
mod safe_send;
mod state;
mod util;

use std::io;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use chrono::TimeZone;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use crate::auth_flow::{set_auth_policy, AuthPolicy};
use crate::commands::read::ReadCommandOptions;
use crate::commands::watch::{WatchOptions, WebhookFormat};
use crate::config::load_config;
use crate::util::{format_outgoing_message, NO_COLOR, VERSION};

#[derive(Parser, Debug)]
#[command(name = "openkakao-cli")]
#[command(about = "OpenKakao Rust CLI", long_about = None)]
#[command(version = VERSION)]
struct Cli {
    #[arg(long, global = true, help = "Output as JSON")]
    json: bool,
    #[arg(long, global = true, help = "Disable colored output")]
    no_color: bool,
    #[arg(
        long,
        global = true,
        help = "Explicitly acknowledge unattended or non-interactive operation"
    )]
    unattended: bool,
    #[arg(
        long,
        global = true,
        help = "Allow non-interactive send operations when combined with --unattended"
    )]
    allow_non_interactive_send: bool,
    #[arg(
        long,
        global = true,
        help = "Allow watch read receipts, hooks, and webhooks when combined with --unattended"
    )]
    allow_watch_side_effects: bool,
    #[arg(
        long,
        global = true,
        alias = "no-signature",
        help = "Send the text exactly as given: no '⌞ₛₑₙₜ ᵥᵢₐ ₒₚₑₙₖₐₖₐₒ' line ([send] signature) and no invisible watermark"
    )]
    no_prefix: bool,
    #[arg(
        long,
        global = true,
        help = "Print [DONE] to stdout after command completes successfully"
    )]
    completion_promise: bool,
    #[arg(
        long,
        global = true,
        value_name = "SECS",
        help = "AX: keyboard/mouse idle seconds required before a --fallback-keys route raises KakaoTalk (default 2, 0 disables)"
    )]
    wait_idle: Option<u64>,
    #[arg(
        long,
        global = true,
        help = "AX: open a chat with no open window from the chat list's context menu (focus-free; marks it read)"
    )]
    open_if_needed: bool,
    #[arg(
        long,
        global = true,
        value_name = "N",
        help = "AX: with --open-if-needed, open the Nth (top to bottom) of several chats with the exact same name"
    )]
    pick: Option<usize>,
    #[arg(
        long,
        global = true,
        help = "AX: clear a draft left in the chat's composer before a text send (the draft is printed first)"
    )]
    discard_draft: bool,
    #[arg(
        long,
        global = true,
        help = "AX: use the older raise + keys routes (Return, clipboard ⌘V, row double-click) for builds without the AX buttons"
    )]
    fallback_keys: bool,
    #[arg(
        long,
        global = true,
        help = "AX: send, reply, edit or delete in a chat outside [safety] allowed_send_chats without device-owner authentication; needs [safety] allow_skip_allowlist = true (exact names only; same-name chats still refuse)"
    )]
    dangerously_skip_allowlist: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Verify token validity
    Auth,
    /// Show persisted auth recovery state and cooldowns
    AuthStatus,
    /// Extract credentials from KakaoTalk cache (or log in with --manual)
    Login {
        #[arg(long)]
        save: bool,
        /// Log in with email + password instead of reading the KakaoTalk cache.
        /// Required on recent KakaoTalk builds that no longer cache the token.
        #[arg(long)]
        manual: bool,
        /// Email or phone number for --manual login (prompted if omitted)
        #[arg(long)]
        email: Option<String>,
        /// Password for --manual login (prompted, hidden, if omitted)
        #[arg(long)]
        password: Option<String>,
        /// Override the app version string used in --manual login headers
        #[arg(long)]
        app_version: Option<String>,
    },
    /// Show own profile
    Me,
    /// List friends
    Friends {
        #[arg(short = 'f', long)]
        favorites: bool,
        #[arg(long)]
        hidden: bool,
        #[arg(short = 's', long)]
        search: Option<String>,
        #[arg(
            long,
            help = "Build a local friend graph from LOCO GETMEM across known chats"
        )]
        local: bool,
        #[arg(
            long,
            help = "When used with --local, only include users seen in this chat"
        )]
        chat_id: Option<i64>,
        #[arg(long, help = "When used with --local, only include this user")]
        user_id: Option<i64>,
    },
    /// List chat rooms
    Chats {
        #[arg(short = 'a', long = "all")]
        show_all: bool,
        #[arg(short = 'u', long)]
        unread: bool,
        #[arg(long, help = "Search chat rooms by title")]
        search: Option<String>,
        #[arg(long = "type", help = "Filter by type: dm, group, memo, open")]
        chat_type: Option<String>,
        #[arg(long, help = "Force REST chat list path instead of LOCO")]
        rest: bool,
    },
    /// Read messages from a chat room
    Read {
        chat_id: i64,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: usize,
        #[arg(long, help = "Before this logId (backward pagination)")]
        before: Option<i64>,
        #[arg(long, help = "Resume from cursor (logId from previous run)")]
        cursor: Option<i64>,
        #[arg(long, help = "Filter messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
        #[arg(long, help = "Fetch all available messages")]
        all: bool,
        #[arg(
            long,
            default_value_t = 100,
            help = "Delay between LOCO batches in ms (ignored for --rest)"
        )]
        delay_ms: u64,
        #[arg(long, help = "Allow LOCO full-history reads on open chats")]
        force: bool,
        #[arg(long, help = "Force REST read path instead of LOCO")]
        rest: bool,
    },
    /// List members of a chat room
    Members {
        chat_id: i64,
        #[arg(long, help = "Force REST member list path instead of LOCO")]
        rest: bool,
        #[arg(long, help = "Show richer LOCO member profile fields")]
        full: bool,
    },
    /// Get detailed information about a chat room
    Chatinfo { chat_id: i64 },
    /// Show account settings
    Settings,
    /// Get link preview (OG tags) for a URL
    Scrap { url: String },
    /// Show a friend's profile
    Profile {
        user_id: i64,
        #[arg(long, help = "Use chat-scoped LOCO member profile for this chat")]
        #[arg(conflicts_with = "local")]
        chat_id: Option<i64>,
        #[arg(
            long,
            help = "Resolve from the local LOCO friend graph built from known chats"
        )]
        local: bool,
    },
    /// Add a friend to favorites
    Favorite { user_id: i64 },
    /// Remove a friend from favorites
    Unfavorite { user_id: i64 },
    /// Hide a friend
    Hide { user_id: i64 },
    /// Unhide a friend
    Unhide { user_id: i64 },
    /// List profile cards (multi-profile)
    Profiles,
    /// Show notification alarm keywords
    Keywords,
    /// Show unread chat summary
    Unread,
    /// Export chat messages
    Export {
        chat_id: i64,
        #[arg(long, default_value = "txt", help = "Output format: json, csv, txt")]
        format: String,
        #[arg(short = 'o', long, help = "Output file (default: stdout)")]
        output: Option<String>,
    },
    /// Search messages in a chat room
    Search { chat_id: i64, query: String },
    /// Show chat statistics (message counts, activity, top participants)
    Stats {
        chat_id: i64,
        #[arg(
            long,
            help = "Number of recent messages to analyze (default: all available)"
        )]
        limit: Option<usize>,
        #[arg(long, help = "Only count messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
    },
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Attempt to renew OAuth token using cached refresh_token
    Renew,
    /// Re-login via login.json to obtain LOCO access_token
    Relogin {
        /// Generate fresh X-VC values instead of using cached one
        #[arg(long)]
        fresh_xvc: bool,
        /// Supply current password (cached password may be expired)
        #[arg(long)]
        password: Option<String>,
        /// Override email/phone from Cache.db
        #[arg(long)]
        email: Option<String>,
    },
    #[command(hide = true)]
    /// Test LOCO protocol connection (legacy command)
    LocoTest,
    /// Send a message via LOCO protocol
    Send {
        chat_id: i64,
        message: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Send a message to memo chat (나와의 채팅) via LOCO protocol
    SendMe {
        message: String,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Watch real-time messages via LOCO protocol
    Watch {
        #[arg(long, help = "Filter by chat ID")]
        chat_id: Option<i64>,
        #[arg(long, help = "Show raw BSON body")]
        raw: bool,
        #[arg(long, help = "Send read receipts (NOTIREAD) for incoming messages")]
        read_receipt: bool,
        #[arg(
            long,
            default_value_t = 10,
            help = "Max reconnect attempts (0 = no reconnect)"
        )]
        max_reconnect: u32,
        #[arg(
            long,
            default_value_t = 2,
            help = "Initial reconnect backoff delay in seconds (doubles each attempt)"
        )]
        reconnect_delay: u64,
        #[arg(
            long,
            default_value_t = 60,
            help = "Maximum reconnect backoff delay in seconds"
        )]
        reconnect_max_delay: u64,
        #[arg(long, help = "Auto-download media attachments")]
        download_media: bool,
        #[arg(
            long,
            default_value = "downloads",
            help = "Directory for downloaded media"
        )]
        download_dir: String,
        #[arg(long, help = "Run a local shell command for matched events")]
        hook_cmd: Option<String>,
        #[arg(long, help = "POST matched events to a webhook URL")]
        webhook_url: Option<String>,
        #[arg(
            long = "webhook-header",
            help = "Additional webhook header in 'Name: Value' format"
        )]
        webhook_header: Vec<String>,
        #[arg(
            long = "webhook-signing-secret",
            help = "Sign webhook payloads with HMAC-SHA256 and emit X-OpenKakao-Timestamp / X-OpenKakao-Signature"
        )]
        webhook_signing_secret: Option<String>,
        #[arg(long = "hook-chat-id", help = "Only trigger hooks for these chat IDs")]
        hook_chat_id: Vec<i64>,
        #[arg(
            long = "hook-keyword",
            help = "Only trigger hooks when message text contains keyword"
        )]
        hook_keyword: Vec<String>,
        #[arg(
            long = "hook-type",
            help = "Only trigger hooks for these message type codes"
        )]
        hook_type: Vec<i32>,
        #[arg(
            long = "webhook-format",
            help = "Webhook payload format: raw (default), slack, discord"
        )]
        webhook_format: Option<String>,
        #[arg(long, help = "Stop watch when a hook command fails")]
        hook_fail_fast: bool,
        #[arg(long, help = "Resume from last saved watch state (last-seen log IDs)")]
        resume: bool,
        #[arg(
            long,
            help = "Capture unknown push packets as JSON for protocol reverse engineering"
        )]
        capture: bool,
    },
    /// Send a photo via LOCO protocol (alias for send-file)
    SendPhoto {
        chat_id: i64,
        /// Path to image file (JPEG/PNG/GIF)
        file: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Send a file (photo/video/document) via LOCO protocol
    SendFile {
        chat_id: i64,
        /// Path to file
        file: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Delete a message via LOCO protocol
    Delete {
        chat_id: i64,
        log_id: i64,
        #[arg(long, help = "Allow deleting in open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Mark messages as read up to a specific message via LOCO protocol
    MarkRead { chat_id: i64, log_id: i64 },
    /// Add a reaction to a message via LOCO ACTION
    React {
        chat_id: i64,
        log_id: i64,
        /// Reaction type (1 = like)
        #[arg(short = 't', long, default_value = "1")]
        reaction_type: i32,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Edit a message via LOCO REWRITE (may return -203 on macOS)
    Edit {
        chat_id: i64,
        log_id: i64,
        message: String,
        #[arg(long, help = "Allow editing in open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Download media attachment from a specific message
    Download {
        chat_id: i64,
        log_id: i64,
        #[arg(short = 'o', long, help = "Output directory (default: downloads)")]
        output_dir: Option<String>,
    },
    /// Sync messages to local SQLite cache for offline search
    Cache {
        chat_id: i64,
        #[arg(long, help = "Max messages to sync (default: all)")]
        limit: Option<usize>,
    },
    /// Search locally cached messages
    CacheSearch {
        query: String,
        #[arg(long, help = "Limit search to this chat")]
        chat_id: Option<i64>,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: usize,
    },
    /// Show local cache statistics
    CacheStats,
    #[command(hide = true)]
    /// List chat rooms via LOCO protocol (legacy command)
    LocoChats {
        #[arg(short = 'a', long = "all")]
        show_all: bool,
    },
    #[command(hide = true)]
    /// Read messages via LOCO protocol (legacy command)
    LocoRead {
        chat_id: i64,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: i32,
        #[arg(long, help = "Resume from this logId (cursor from previous run)")]
        cursor: Option<i64>,
        #[arg(long, help = "Filter messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
        #[arg(long, help = "Fetch all available messages")]
        all: bool,
        #[arg(
            long,
            default_value_t = 100,
            help = "Delay between batches in ms (rate limit)"
        )]
        delay_ms: u64,
        #[arg(long, help = "Allow operations on open chats (higher ban risk)")]
        force: bool,
    },
    #[command(hide = true)]
    /// List members of a chat room via LOCO protocol (legacy command)
    LocoMembers { chat_id: i64 },
    #[command(hide = true)]
    /// Get chat room info via LOCO protocol (legacy command)
    LocoChatinfo { chat_id: i64 },
    /// List blocked/hidden-style members via LOCO protocol
    LocoBlocked,
    /// Probe a LOCO method and print the raw response
    Probe {
        method: String,
        #[arg(long, help = "JSON object body to send with the probe")]
        body: Option<String>,
        #[arg(
            long,
            help = "Wait for push packets instead of direct response (extends timeout to 10s)"
        )]
        capture_pushes: bool,
    },
    #[command(hide = true)]
    /// Inspect cached friend/profile hints for LOCO reverse engineering
    ProfileHints {
        #[arg(
            long,
            help = "Include a local KakaoTalk app-state file snapshot for before/after diffing"
        )]
        app_state: bool,
        #[arg(
            long,
            help = "Compare the current app-state snapshot against a previous profile-hints JSON file"
        )]
        app_state_diff: Option<String>,
        #[arg(
            long,
            help = "Also build a local LOCO friend graph and correlate cache hints"
        )]
        local_graph: bool,
        #[arg(long, help = "Generate SYNCMAINPF body candidates for this user")]
        user_id: Option<i64>,
        #[arg(
            long,
            help = "Probe generated SYNCMAINPF candidates in one LOCO session"
        )]
        probe_syncmainpf: bool,
        #[arg(
            long,
            help = "Probe generated UPLINKPROF candidates in one LOCO session"
        )]
        probe_uplinkprof: bool,
    },
    #[command(hide = true)]
    /// Probe an arbitrary LOCO method and print the raw response (legacy command)
    LocoProbe {
        method: String,
        #[arg(long, help = "JSON object body to send with the probe")]
        body: Option<String>,
    },
    /// Watch Cache.db for fresh tokens (poll every N seconds)
    WatchCache {
        #[arg(long, default_value_t = 10)]
        interval: u64,
    },
    /// List chats from local KakaoTalk database (no server contact, safe)
    LocalChats {
        #[arg(short = 'n', long, default_value_t = 50)]
        limit: usize,
    },
    /// Read messages from local KakaoTalk database (no server contact, safe)
    LocalRead {
        chat_id: i64,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: usize,
        #[arg(long, help = "Filter messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
    },
    /// Search messages in local KakaoTalk database (no server contact, safe)
    LocalSearch {
        query: String,
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
    },
    /// Show local KakaoTalk database schema
    LocalSchema,
    /// Send a message via AX automation (no server contact, drives KakaoTalk's UI directly)
    LocalSend {
        chat_name: String,
        message: String,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Send one file (any type, e.g. .docx/.pdf/.zip) via AX automation
    /// through the official KakaoTalk app
    #[command(
        long_about = "Send one file (any type) via AX automation through the official KakaoTalk app, \
                      with no server login and without raising KakaoTalk.\n\nThe file must exist, be a \
                      non-empty regular file (symlinks are resolved) with a UTF-8 path, and fit \
                      [ax] max_file_mb (default 300 MiB). The open chat window's 파일전송 button, the \
                      open panel's sidebar folder and file rows, 열기 and the preview's 1개 전송 are \
                      all driven by Accessibility. A chat outside allowed_send_chats is flagged and \
                      needs macOS device-owner authentication before the send; an allowlisted one \
                      does not. A file outside the top level of Desktop/Documents/Downloads \
                      is staged as a same-name hard link (or copy) in ~/Downloads and removed after \
                      a verified send. Delivery is verified by a new outgoing row showing the file \
                      name or [파일]/[사진]. Requires allow_ax_send (not for --dry-run). \
                      --fallback-keys uses the older clipboard + ⌘V route, \
                      which raises KakaoTalk briefly after --wait-idle and overwrites the clipboard."
    )]
    LocalSendFile {
        /// Exact title of the open chat window (and allowlisted display name)
        chat_name: String,
        /// Path to the file to send
        file: String,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Reply to one message in an open chat window, focus-free (experimental)
    #[command(
        long_about = "Reply to one message bubble in an open chat window via AX, without raising \
                      KakaoTalk (experimental; verified on KakaoTalk 26.9.0). The target is the newest \
                      text bubble (--to-last) or the one bubble whose whole text equals --match \
                      (two matches refuse). Reply mode must be confirmed (composer and banner) before \
                      any text is inserted. --dry-run reads the chat and shows the target only. Same \
                      allow_ax_send and allowlist/authentication rules as local-send."
    )]
    LocalReply {
        /// Exact title of the open chat window (and allowlisted display name)
        chat_name: String,
        /// Reply text
        message: String,
        /// Reply to the newest text bubble
        #[arg(
            long,
            conflicts_with = "match_text",
            required_unless_present = "match_text"
        )]
        to_last: bool,
        /// Reply to the one bubble whose whole text is exactly this
        #[arg(long = "match", value_name = "TEXT")]
        match_text: Option<String>,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Show the targeted bubble without acting")]
        dry_run: bool,
    },
    /// List the chat list's rows (name, unread count, members, time,
    /// preview), read-only; --unread keeps chats with unread messages
    AxChats {
        #[arg(long)]
        unread: bool,
    },
    /// Mark one chat read with KakaoTalk's own 읽음 처리, without opening it
    /// (found through KakaoTalk's chat search by exact name; --pick for
    /// duplicates)
    LocalMarkRead {
        /// Exact chat names; each is marked separately
        #[arg(required = true)]
        chat_names: Vec<String>,
    },
    /// Leave chats with KakaoTalk's 채팅방 나가기, without opening them (found
    /// through the chat search by exact name; --pick for duplicates). Cannot
    /// be undone: needs device-owner authentication; back up with ax-export first
    LocalLeaveChat {
        /// Exact chat names; each is left separately
        #[arg(required = true)]
        chat_names: Vec<String>,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Change the name this account shows for a chat (채팅방 설정 → 편집),
    /// focus-free; needs device-owner authentication
    LocalRenameChat {
        chat_name: String,
        /// The new name (1 to 50 characters)
        new_name: String,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// React to one message bubble with one of KakaoTalk's reactions
    /// (사랑, 엄지척, 체크, 웃음, 놀라움, 슬픔, 축하해, 박수, ...), focus-free
    /// through its menu's 리액션 (experimental; pressing the same one again
    /// can take it back)
    LocalReact {
        chat_name: String,
        /// The reaction's name as KakaoTalk labels it, e.g. 사랑 or 엄지척
        #[arg(long)]
        reaction: String,
        /// The newest text bubble
        #[arg(long, conflicts_with = "match_text")]
        to_last: bool,
        /// The bubble's whole text, exactly
        #[arg(long = "match", required_unless_present = "to_last")]
        match_text: Option<String>,
        #[arg(short = 'y', long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Edit one of your own messages in an open chat window, focus-free (experimental)
    #[command(
        long_about = "Edit one of your own message bubbles via AX, without raising KakaoTalk \
                      (experimental; verified on KakaoTalk 26.9.0). --match must equal the bubble's \
                      whole text (two matches refuse). Edit mode must load exactly that text before it \
                      is replaced, and success requires the bubble to show the new text with KakaoTalk's \
                      수정됨 mark. The new text is used as given (no prefix). --dry-run shows the target only."
    )]
    LocalEdit {
        /// Exact title of the open chat window (and allowlisted display name)
        chat_name: String,
        /// The bubble's current whole text
        #[arg(long = "match", value_name = "OLD_TEXT")]
        old: String,
        /// The new text
        new: String,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Show the targeted bubble without acting")]
        dry_run: bool,
    },
    /// Delete one message bubble via AX, through KakaoTalk's own delete menu
    #[command(
        long_about = "Delete one message bubble via AX, without raising KakaoTalk (experimental; \
                      measured on KakaoTalk 26.8). --match must equal the bubble's whole text (two \
                      matches refuse). Choose --for-everyone (모든 대화 상대에게서 삭제: your own \
                      messages, within 24 hours of sending, never in the memo chat) or --for-me \
                      (나에게서만 삭제: this Mac only). KakaoTalk decides what it offers: a scope its \
                      menu does not show fails with delete_unavailable, and nothing is deleted. \
                      Success requires the bubble to be gone or replaced by KakaoTalk's notice. \
                      --dry-run shows the target only."
    )]
    LocalDelete {
        /// Exact title of the open chat window (and allowlisted display name)
        chat_name: String,
        /// The bubble's whole text
        #[arg(long = "match", value_name = "TEXT")]
        text: String,
        /// Delete it for everyone in the chat (모든 대화 상대에게서 삭제)
        #[arg(long, conflicts_with = "for_me", required_unless_present = "for_me")]
        for_everyone: bool,
        /// Delete it from this Mac only (나에게서만 삭제)
        #[arg(long)]
        for_me: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Show the targeted bubble without acting")]
        dry_run: bool,
    },
    /// Send one photo via AX automation through the official KakaoTalk app
    LocalSendPhoto {
        chat_name: String,
        /// Path to a JPEG, PNG, GIF, WebP, or HEIF image
        file: String,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long, help = "Preview the action without executing")]
        dry_run: bool,
    },
    /// Queue and manually approve a crash-safe AX send
    SafeSend {
        #[command(subcommand)]
        action: SafeSendAction,
    },
    /// Sign a signed-out KakaoTalk back in from the login keychain (AX, no server login)
    #[command(
        long_about = "Sign a signed-out KakaoTalk (for example after a login on another device) back \
                      in by Accessibility. The password is read from the login keychain item \
                      `openkakao.kakaotalk`, which you save once with `security add-generic-password \
                      -s openkakao.kakaotalk -a <account> -w`; it never appears in argv or output. \
                      The KakaoTalk process must carry Kakao's code signature. A logout notice or an \
                      abandoned QR page is dismissed only when recognized. A rejected password is \
                      reported (login_rejected) and never retried. A device check or unknown dialog \
                      is left to you (login_needs_human)."
    )]
    AxLogin {
        /// KakaoTalk login id (usually the account email); defaults to [ax] login_account
        #[arg(long)]
        account: Option<String>,
    },
    /// Read recent messages via AX automation (no server contact, no local
    /// DB access — scrapes the open KakaoTalk chat window directly)
    AxRead {
        chat_name: String,
        #[arg(short = 'n', long, default_value_t = 20)]
        count: usize,
        /// Read the whole text of messages KakaoTalk folds behind 전체보기
        /// (opens and closes each one's dialog, focus-free)
        #[arg(long)]
        expand: bool,
    },
    /// Back up a chat's whole history through KakaoTalk's own 텍스트 파일로
    /// 저장 (CSV: Date, User, Message), focus-free
    AxExport {
        chat_name: String,
        /// Move the export here (a file or folder; never overwrites)
        #[arg(long)]
        output: Option<std::path::PathBuf>,
    },
    /// Search a chat's whole history (exports it through KakaoTalk first, or
    /// searches an earlier export with --from)
    AxSearch {
        chat_name: String,
        query: String,
        /// Show at most this many matches, the newest ones
        #[arg(short = 'n', long, default_value_t = 50)]
        limit: usize,
        /// Search this ax-export CSV instead of exporting again
        #[arg(long)]
        from: Option<std::path::PathBuf>,
        /// Keep the fresh export instead of removing it after the search
        #[arg(long)]
        keep: bool,
    },
    /// Find friends through KakaoTalk's own friends-tab search (name and
    /// status message), read-only; --birthdays lists 생일인 친구 instead
    AxFriends {
        #[arg(required_unless_present = "birthdays")]
        query: Option<String>,
        /// List the friends KakaoTalk shows under 생일인 친구 (birthdays
        /// around today, with its day label such as "오늘 11월 8일")
        #[arg(long, conflicts_with = "query")]
        birthdays: bool,
    },
    /// Per-chat writing-style samples for agents: your own messages in one
    /// chat (from KakaoTalk's export), kept only on this Mac
    Style {
        chat_name: String,
        /// Export the chat and keep your newest 200 messages there
        #[arg(long, conflicts_with_all = ["forget", "decline"])]
        learn: bool,
        /// Delete this chat's samples
        #[arg(long, conflicts_with = "decline")]
        forget: bool,
        /// Remember that you do not want this chat's tone learned
        #[arg(long)]
        decline: bool,
        /// Your name in this chat's export, if no message of yours is visible
        #[arg(long)]
        me: Option<String>,
    },
    /// Show a friend's 친구 정보 (private memo, own name), or set the memo and
    /// the name this account shows for them; a rename needs device-owner
    /// authentication
    AxFriend {
        name: String,
        /// Set the private memo (only you see it; at most 200 characters; "" clears it)
        #[arg(long)]
        memo: Option<String>,
        /// Set the name you see for this friend (at most 20 characters; "" restores theirs)
        #[arg(long)]
        rename: Option<String>,
        /// Open (or start) the 1:1 chat with this friend through 채팅하기
        #[arg(long, conflicts_with_all = ["memo", "rename"])]
        open_chat: bool,
        /// Block this channel (its ads stop; 설정 > 친구 > 차단친구 관리 undoes it).
        /// Channels only: blocking a person is refused
        #[arg(long, conflicts_with_all = ["memo", "rename", "open_chat"])]
        block: bool,
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Watch for incoming KakaoTalk messages via AX (no server contact,
    /// background) and fire hooks/webhooks on unread-count increases
    AxWatch {
        #[arg(long, default_value_t = 3)]
        interval: u64,
        #[arg(long)]
        hook_cmd: Option<String>,
        #[arg(long)]
        webhook_url: Option<String>,
        #[arg(long = "webhook-header")]
        webhook_header: Vec<String>,
        #[arg(long)]
        webhook_signing_secret: Option<String>,
        #[arg(long, default_value = "raw")]
        webhook_format: String,
        #[arg(long = "hook-chat")]
        hook_chat: Vec<String>,
        #[arg(long = "hook-keyword")]
        hook_keyword: Vec<String>,
        #[arg(long)]
        hook_fail_fast: bool,
    },
    /// Watch the macOS Notification Center DB for KakaoTalk messages
    /// (login-free, works with the KakaoTalk window closed)
    NotifWatch {
        #[arg(long, default_value_t = 3)]
        interval: u64,
        #[arg(
            long,
            help = "Emit KakaoTalk messages already retained in Notification Center on startup"
        )]
        replay_existing: bool,
        #[arg(
            long,
            help = "Persist before delivery, retry after restart, and reconcile retained notifications"
        )]
        durable: bool,
        #[arg(long)]
        hook_cmd: Option<String>,
        #[arg(long)]
        webhook_url: Option<String>,
        #[arg(long = "webhook-header")]
        webhook_header: Vec<String>,
        #[arg(long)]
        webhook_signing_secret: Option<String>,
        #[arg(long, default_value = "raw")]
        webhook_format: String,
        #[arg(long = "hook-chat")]
        hook_chat: Vec<String>,
        #[arg(long = "hook-keyword")]
        hook_keyword: Vec<String>,
        #[arg(long)]
        hook_fail_fast: bool,
    },
    /// First-run setup: AX readiness checks, send allowlist, agent skill
    #[command(
        long_about = "First-run setup for the login-free AX path. Shows read-only readiness checks \
                      (Accessibility, sign-in, main window on this Space, chat-list search), writes \
                      ~/.config/openkakao/config.toml (chats are only allowlisted from names you type \
                      or pass with --allow-chat; allowlisting one also sets allow_ax_send = true), and \
                      installs the openkakao-cli agent skill (npx skills add, or a direct SKILL.md \
                      download when npx is missing). It offers to launch KakaoTalk in the background \
                      and to assign it to All Desktops (the Dock restarts once; non-interactive runs \
                      do this only with --all-spaces). Safe to re-run."
    )]
    Init {
        /// Non-interactive: accept defaults (no skill unless --skill is passed; KakaoTalk is not launched)
        #[arg(long, short = 'y')]
        yes: bool,
        /// Where to install the agent skill
        #[arg(long, value_enum)]
        skill: Option<commands::init::SkillTarget>,
        /// Exact chat display name local-send may send to (repeatable)
        #[arg(long = "allow-chat", value_name = "CHAT_NAME")]
        allow_chat: Vec<String>,
        /// Assign KakaoTalk to All Desktops without asking (restarts the Dock once)
        #[arg(long)]
        all_spaces: bool,
    },
    /// Run diagnostic checks on KakaoTalk installation and connectivity
    Doctor {
        /// Also test LOCO booking connectivity (makes network request)
        #[arg(long)]
        loco: bool,
        /// Also inspect this open chat window (read-only): composer, 전송 button, file button
        #[arg(long, value_name = "CHAT_NAME")]
        chat: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SafeSendAction {
    /// Persist a send proposal without sending anything
    Propose {
        chat_name: String,
        message: String,
        #[arg(long, requires = "reply_log_id")]
        reply_chat_id: Option<i64>,
        #[arg(long, requires = "reply_chat_id")]
        reply_log_id: Option<i64>,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// List proposals and uncertain send attempts requiring attention
    List,
    /// Review and send from an interactive terminal; global unattended flags are rejected
    Approve { intent_id: String },
    /// Permanently cancel a proposed or uncertain intent
    Cancel {
        intent_id: String,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
    },
}

fn require_loco_write(config: &config::OpenKakaoConfig) -> Result<()> {
    if !config.safety.allow_loco_write {
        anyhow::bail!(
            "LOCO write operations are disabled by default to protect your account.\n\
             These operations (send, delete, edit, react) use the LOCO protocol which\n\
             may result in account suspension or deletion by Kakao.\n\n\
             To enable, add to ~/.config/openkakao/config.toml:\n\n\
             [safety]\n\
             allow_loco_write = true\n\n\
             Consider using local-read / local-chats / local-search for safe read-only access."
        );
    }
    Ok(())
}

fn require_ax_send(config: &config::OpenKakaoConfig) -> Result<()> {
    if !config.safety.allow_ax_send {
        return Err(error::coded(
            error::ErrorCode::AxSendDisabled,
            "AX-automation send is disabled by default.\n\
             local-send / local-send-file / local-send-photo drive the real KakaoTalk\n\
             window (insert your message or file, then send it on your behalf) —\n\
             treat them with the same care as `send`.\n\n\
             To enable, add to ~/.config/openkakao/config.toml:\n\n\
             [safety]\n\
             allow_ax_send = true\n\n\
             KakaoTalk must be running and already logged in; no server/LOCO contact is made.",
        ));
    }
    Ok(())
}

/// AX writes match chats by display name, not a chat id (the local DB that
/// could cross-check is unreadable on current KakaoTalk builds). An exact
/// `allowed_send_chats` entry is the human's standing approval; any other
/// target is flagged here and must pass device-owner authentication in the
/// AX transport (`ax_send::require_send_approval`).
fn warn_if_not_allowlisted(config: &config::OpenKakaoConfig, chat_name: &str) {
    if ax_send::is_allowlisted(&config.safety.allowed_send_chats, chat_name) {
        return;
    }
    if ax_send::skips_allowlist() {
        eprintln!(
            "⚠ \"{}\" is NOT in [safety] allowed_send_chats: --dangerously-skip-allowlist acts \
             without device-owner authentication.",
            util::escape_terminal_text(chat_name)
        );
    } else {
        eprintln!(
            "⚠ \"{}\" is NOT in [safety] allowed_send_chats: macOS device-owner authentication \
             (Touch ID or login password) is required. Add the exact name there to send without it.",
            util::escape_terminal_text(chat_name)
        );
    }
}

/// Whether this command can benefit from the warning about broken server-login
/// flows. Purely local commands should stay quiet: the warning is unrelated to
/// their operation and is especially noisy for long-running watch processes.
fn should_warn_about_server_login(command: &Commands) -> bool {
    !matches!(
        command,
        Commands::AuthStatus
            | Commands::Completions { .. }
            | Commands::CacheSearch { .. }
            | Commands::CacheStats
            | Commands::LocalChats { .. }
            | Commands::LocalRead { .. }
            | Commands::LocalSearch { .. }
            | Commands::LocalSchema
            | Commands::LocalSend { .. }
            | Commands::LocalSendFile { .. }
            | Commands::LocalReply { .. }
            | Commands::LocalEdit { .. }
            | Commands::LocalDelete { .. }
            | Commands::LocalReact { .. }
            | Commands::LocalRenameChat { .. }
            | Commands::LocalMarkRead { .. }
            | Commands::LocalLeaveChat { .. }
            | Commands::AxChats { .. }
            | Commands::LocalSendPhoto { .. }
            | Commands::SafeSend { .. }
            | Commands::AxLogin { .. }
            | Commands::AxRead { .. }
            | Commands::AxExport { .. }
            | Commands::AxFriends { .. }
            | Commands::Style { .. }
            | Commands::AxFriend { .. }
            | Commands::AxSearch { .. }
            | Commands::AxWatch { .. }
            | Commands::NotifWatch { .. }
            | Commands::Init { .. }
            | Commands::Doctor { .. }
    )
}

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    if let Err(error) = run(cli) {
        // Agents read stdout: with --json, failures are a JSON object there
        // too, with a stable `code` and a `fix` hint. Humans get stderr.
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&error::error_json(&error))
                    .unwrap_or_else(|_| "{}".to_string())
            );
        }
        eprintln!("Error: {error:?}");
        if let Some(code) = error::error_code(&error) {
            eprintln!("Fix ({}): {}", code.as_str(), code.fix());
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let config = load_config()?;
    if cli.dangerously_skip_allowlist && !config.safety.allow_skip_allowlist {
        return Err(error::coded(
            error::ErrorCode::ChatNotAllowed,
            "--dangerously-skip-allowlist is disabled: nothing was done.\n\
             It acts in chats outside allowed_send_chats without device-owner\n\
             authentication, so only the person at this Mac turns it on, in\n\
             ~/.config/openkakao/config.toml:\n\n\
             [safety]\n\
             allow_skip_allowlist = true",
        ));
    }
    openkakao_cli::human_auth::set_device_owner_auth_grace(
        config.safety.device_owner_auth_grace_secs,
    );
    ax_send::set_policy(ax_send::AxPolicy {
        wait_idle: std::time::Duration::from_secs(
            cli.wait_idle
                .or(config.ax.wait_idle_secs)
                .unwrap_or(ax_send::DEFAULT_WAIT_IDLE.as_secs()),
        ),
        open_if_needed: cli.open_if_needed || config.ax.open_if_needed,
        fallback_keys: cli.fallback_keys,
        pick: cli.pick,
        discard_draft: cli.discard_draft,
        skip_allowlist: cli.dangerously_skip_allowlist,
    });
    ax_send::set_allowed_send_chats(config.safety.allowed_send_chats.clone());
    set_auth_policy(AuthPolicy::from_config(&config.auth));
    let json = cli.json;
    let unattended = cli.unattended || config.mode.unattended;
    let allow_non_interactive_send =
        cli.allow_non_interactive_send || config.send.allow_non_interactive;
    let allow_watch_side_effects = cli.allow_watch_side_effects || config.watch.allow_side_effects;
    let min_unattended_send_interval_secs = config
        .safety
        .min_unattended_send_interval_secs
        .unwrap_or(10);
    let min_hook_interval_secs = config.safety.min_hook_interval_secs.unwrap_or(2);
    let min_webhook_interval_secs = config.safety.min_webhook_interval_secs.unwrap_or(2);
    let hook_timeout_secs = config.safety.hook_timeout_secs.unwrap_or(20);
    let webhook_timeout_secs = config.safety.webhook_timeout_secs.unwrap_or(10);
    let signature = (!cli.no_prefix && config.send.default_prefix != Some(false)).then(|| {
        config
            .send
            .signature
            .clone()
            .unwrap_or_else(|| util::DEFAULT_SIGNATURE.to_string())
    });
    let signature = signature.as_deref();
    // --no-prefix sends the text exactly as given: no signature, no watermark.
    let watermark = !cli.no_prefix && config.send.watermark != Some(false);

    // Respect NO_COLOR env var (https://no-color.org/) and --no-color flag
    if cli.no_color || std::env::var("NO_COLOR").is_ok() || json {
        NO_COLOR.store(true, Ordering::Relaxed);
    }

    // Server-login warning — printed to stderr so it never corrupts JSON on
    // stdout. Local-only commands do not show an unrelated auth warning.
    if should_warn_about_server_login(&cli.command)
        && std::env::var_os("OPENKAKAO_CLI_NO_DEPRECATION").is_none()
    {
        eprintln!("⚠️  Server login (login --save / login --manual) is broken on recent");
        eprintln!("   KakaoTalk macOS builds. Do NOT repeatedly retry login on an unregistered");
        eprintln!("   device — it can get your account's sub-device login blocked. Prefer");
        eprintln!(
            "   'local-send'/'ax-read'/'local-chats' — no server contact needed. See README."
        );
    }

    match cli.command {
        Commands::Auth => commands::auth::cmd_auth(json)?,
        Commands::AuthStatus => commands::auth::cmd_auth_status(json)?,
        Commands::Login {
            save,
            manual,
            email,
            password,
            app_version,
        } => {
            if manual {
                commands::auth::cmd_login_manual(save, email, password, app_version)?
            } else {
                commands::auth::cmd_login(save)?
            }
        }
        Commands::Me => commands::rest::cmd_me(json)?,
        Commands::Friends {
            favorites,
            hidden,
            search,
            local,
            chat_id,
            user_id,
        } => commands::rest::cmd_friends(favorites, hidden, search, local, chat_id, user_id, json)?,
        Commands::Chats {
            show_all,
            unread,
            search,
            chat_type,
            rest,
        } => commands::chats::cmd_chats(show_all, unread, search, chat_type, rest, json)?,
        Commands::Read {
            chat_id,
            count,
            before,
            cursor,
            since,
            all,
            delay_ms,
            force,
            rest,
        } => commands::read::cmd_read(
            chat_id,
            ReadCommandOptions {
                count,
                cursor: cursor.or(before),
                since,
                all,
                delay_ms,
                force,
                rest,
                json,
            },
        )?,
        Commands::Members {
            chat_id,
            rest,
            full,
        } => commands::members::cmd_members(chat_id, rest, full, json)?,
        Commands::Chatinfo { chat_id } => commands::rest::cmd_chatinfo(chat_id, json)?,
        Commands::Settings => commands::rest::cmd_settings(json)?,
        Commands::Scrap { url } => commands::rest::cmd_scrap(&url, json)?,
        Commands::Profile {
            user_id,
            chat_id,
            local,
        } => commands::profile::cmd_profile(user_id, chat_id, local, json)?,
        Commands::Favorite { user_id } => commands::rest::cmd_favorite(user_id, json)?,
        Commands::Unfavorite { user_id } => commands::rest::cmd_unfavorite(user_id, json)?,
        Commands::Hide { user_id } => commands::rest::cmd_hide(user_id, json)?,
        Commands::Unhide { user_id } => commands::rest::cmd_unhide(user_id, json)?,
        Commands::Profiles => commands::rest::cmd_profiles(json)?,
        Commands::Keywords => commands::rest::cmd_keywords(json)?,
        Commands::Unread => commands::rest::cmd_unread(json)?,
        Commands::Export {
            chat_id,
            format,
            output,
        } => commands::rest::cmd_export(chat_id, &format, output.as_deref(), json)?,
        Commands::Search { chat_id, query } => commands::rest::cmd_search(chat_id, &query, json)?,
        Commands::Stats {
            chat_id,
            limit,
            since,
        } => commands::analytics::cmd_stats(chat_id, limit, since.as_deref(), json)?,
        Commands::Completions { shell } => {
            generate(
                shell,
                &mut Cli::command(),
                "openkakao-cli",
                &mut io::stdout(),
            );
        }
        Commands::Renew => commands::auth::cmd_renew(json)?,
        Commands::Relogin {
            fresh_xvc,
            password,
            email,
        } => commands::auth::cmd_relogin(json, fresh_xvc, password, email)?,
        Commands::LocoTest => {
            eprintln!("[deprecated] 'loco-test' is now hidden. Prefer 'doctor --loco'.");
            commands::auth::cmd_loco_test()?
        }
        Commands::Send {
            chat_id,
            message,
            force,
            yes,
            dry_run,
        } => {
            let msg = format_outgoing_message(&message, signature, watermark);
            if dry_run {
                eprintln!(
                    "[dry-run] Would send to chat {}: \"{}\"",
                    chat_id,
                    util::truncate(&msg, 80)
                );
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true,
                        "action": "send",
                        "chat_id": chat_id,
                        "message": msg,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_send(commands::send::SendOptions {
                    chat_id,
                    message: msg,
                    force,
                    skip_confirm: yes,
                    unattended,
                    allow_non_interactive: allow_non_interactive_send,
                    min_interval_secs: min_unattended_send_interval_secs,
                    json,
                })?
            }
        }
        Commands::SendMe {
            message,
            yes,
            dry_run,
        } => {
            let reader = local_db::LocalDbReader::open()
                .context("Failed to open local DB to find memo chat")?;
            let memo_id = reader
                .find_memo_chat_id()?
                .context("Could not find memo chat (나와의 채팅) in local database")?;
            let msg = format_outgoing_message(&message, signature, watermark);
            if dry_run {
                eprintln!(
                    "[dry-run] Would send to memo chat {}: \"{}\"",
                    memo_id,
                    util::truncate(&msg, 80)
                );
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true,
                        "action": "send_me",
                        "chat_id": memo_id,
                        "message": msg,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_send(commands::send::SendOptions {
                    chat_id: memo_id,
                    message: msg,
                    force: false,
                    skip_confirm: yes,
                    unattended,
                    allow_non_interactive: allow_non_interactive_send,
                    min_interval_secs: min_unattended_send_interval_secs,
                    json,
                })?
            }
        }
        Commands::SendPhoto {
            chat_id,
            file,
            force,
            yes,
            dry_run,
        } => {
            if dry_run {
                eprintln!("[dry-run] Would send photo '{}' to chat {}", file, chat_id);
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true, "action": "send_photo", "chat_id": chat_id, "file": file,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_send_file(commands::send::SendFileOptions {
                    chat_id,
                    file_path: file,
                    force,
                    skip_confirm: yes,
                    unattended,
                    allow_non_interactive: allow_non_interactive_send,
                    min_interval_secs: min_unattended_send_interval_secs,
                    json,
                })?
            }
        }
        Commands::SendFile {
            chat_id,
            file,
            force,
            yes,
            dry_run,
        } => {
            if dry_run {
                eprintln!("[dry-run] Would send file '{}' to chat {}", file, chat_id);
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true, "action": "send_file", "chat_id": chat_id, "file": file,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_send_file(commands::send::SendFileOptions {
                    chat_id,
                    file_path: file,
                    force,
                    skip_confirm: yes,
                    unattended,
                    allow_non_interactive: allow_non_interactive_send,
                    min_interval_secs: min_unattended_send_interval_secs,
                    json,
                })?
            }
        }
        Commands::Delete {
            chat_id,
            log_id,
            force,
            yes,
            dry_run,
        } => {
            if dry_run {
                eprintln!(
                    "[dry-run] Would delete message {} from chat {}",
                    log_id, chat_id
                );
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true, "action": "delete", "chat_id": chat_id, "log_id": log_id,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_delete(commands::send::DeleteOptions {
                    chat_id,
                    log_id,
                    force,
                    skip_confirm: yes,
                    unattended,
                    allow_non_interactive: allow_non_interactive_send,
                    min_interval_secs: min_unattended_send_interval_secs,
                    json,
                })?
            }
        }
        Commands::MarkRead { chat_id, log_id } => {
            commands::send::cmd_mark_read(commands::send::MarkReadOptions {
                chat_id,
                log_id,
                json,
            })?
        }
        Commands::React {
            chat_id,
            log_id,
            reaction_type,
            dry_run,
        } => {
            if dry_run {
                eprintln!(
                    "[dry-run] Would react (type={}) to message {} in chat {}",
                    reaction_type, log_id, chat_id
                );
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true, "action": "react", "chat_id": chat_id, "log_id": log_id, "reaction_type": reaction_type,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_react(commands::send::ReactOptions {
                    chat_id,
                    log_id,
                    reaction_type,
                    json,
                })?
            }
        }
        Commands::Edit {
            chat_id,
            log_id,
            message,
            force,
            yes,
            dry_run,
        } => {
            let msg = format_outgoing_message(&message, signature, watermark);
            if dry_run {
                eprintln!(
                    "[dry-run] Would edit message {} in chat {}: \"{}\"",
                    log_id,
                    chat_id,
                    util::truncate(&msg, 80)
                );
                if json {
                    util::output_json(&serde_json::json!({
                        "dry_run": true, "action": "edit", "chat_id": chat_id, "log_id": log_id, "message": msg,
                    }))?;
                }
            } else {
                require_loco_write(&config)?;
                commands::send::cmd_edit(commands::send::EditOptions {
                    chat_id,
                    log_id,
                    message: msg,
                    force,
                    skip_confirm: yes,
                    unattended,
                    allow_non_interactive: allow_non_interactive_send,
                    min_interval_secs: min_unattended_send_interval_secs,
                    json,
                })?
            }
        }
        Commands::Watch {
            chat_id,
            raw,
            read_receipt,
            max_reconnect,
            reconnect_delay,
            reconnect_max_delay,
            download_media,
            download_dir,
            hook_cmd,
            webhook_url,
            webhook_header,
            webhook_signing_secret,
            webhook_format,
            hook_chat_id,
            hook_keyword,
            hook_type,
            hook_fail_fast,
            resume,
            capture,
        } => commands::watch::cmd_watch(WatchOptions {
            unattended,
            allow_side_effects: allow_watch_side_effects,
            filter_chat_id: chat_id,
            raw,
            read_receipt,
            max_reconnect: config.watch.default_max_reconnect.unwrap_or(max_reconnect),
            reconnect_delay_secs: reconnect_delay,
            reconnect_max_delay_secs: reconnect_max_delay,
            download_media,
            download_dir,
            hook_cmd,
            webhook_url,
            webhook_headers: webhook_header,
            webhook_signing_secret,
            hook_chat_ids: hook_chat_id,
            hook_keywords: hook_keyword,
            hook_types: hook_type,
            hook_fail_fast,
            min_hook_interval_secs,
            min_webhook_interval_secs,
            hook_timeout_secs,
            webhook_timeout_secs,
            allow_insecure_webhooks: config.safety.allow_insecure_webhooks,
            webhook_format: WebhookFormat::from_str_opt(webhook_format.as_deref())?,
            resume,
            json,
            capture,
        })?,
        Commands::Download {
            chat_id,
            log_id,
            output_dir,
        } => commands::download::cmd_download(chat_id, log_id, output_dir.as_deref(), json)?,
        Commands::Cache { chat_id, limit } => commands::analytics::cmd_cache(chat_id, limit, json)?,
        Commands::CacheSearch {
            query,
            chat_id,
            count,
        } => commands::analytics::cmd_cache_search(&query, chat_id, count, json)?,
        Commands::CacheStats => commands::analytics::cmd_cache_stats(json)?,
        Commands::LocoChats { show_all } => {
            eprintln!("[deprecated] 'loco-chats' is now hidden. Prefer 'chats' (LOCO by default).");
            commands::chats::cmd_loco_chats(show_all, false, None, None, json)?
        }
        Commands::LocoRead {
            chat_id,
            count,
            cursor,
            since,
            all,
            delay_ms,
            force,
        } => {
            eprintln!("[deprecated] 'loco-read' is now hidden. Prefer 'read' (LOCO by default).");
            commands::read::cmd_loco_read(
                chat_id,
                &commands::read::ReadCommandOptions {
                    count: count as usize,
                    cursor,
                    since,
                    all,
                    delay_ms,
                    force,
                    rest: false,
                    json,
                },
            )?
        }
        Commands::LocoMembers { chat_id } => {
            eprintln!(
                "[deprecated] 'loco-members' is now hidden. Prefer 'members' (LOCO by default)."
            );
            commands::members::cmd_loco_members(chat_id, false, json)?
        }
        Commands::LocoChatinfo { chat_id } => {
            eprintln!("[deprecated] 'loco-chatinfo' is now hidden. Prefer 'chatinfo'.");
            commands::probe::cmd_loco_chatinfo(chat_id, json)?
        }
        Commands::LocoBlocked => commands::members::cmd_loco_blocked(json)?,
        Commands::Probe {
            method,
            body,
            capture_pushes,
        } => commands::probe::cmd_loco_probe(&method, body.as_deref(), json, capture_pushes)?,
        Commands::ProfileHints {
            app_state,
            app_state_diff,
            local_graph,
            user_id,
            probe_syncmainpf,
            probe_uplinkprof,
        } => commands::profile::cmd_profile_hints(
            app_state,
            app_state_diff,
            local_graph,
            user_id,
            probe_syncmainpf,
            probe_uplinkprof,
            json,
        )?,
        Commands::LocoProbe { method, body } => {
            eprintln!("[deprecated] 'loco-probe' is now hidden. Prefer 'probe'.");
            commands::probe::cmd_loco_probe(&method, body.as_deref(), json, false)?
        }
        Commands::LocalChats { limit } => {
            let reader = local_db::LocalDbReader::open()?;
            let chats = reader.list_chats(limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&chats)?);
            } else {
                if chats.is_empty() {
                    println!("No chats found in local database.");
                } else {
                    let chat_type_label = |t: i32| -> &'static str {
                        match t {
                            0 => "DM",
                            1 => "Group",
                            _ => "Other",
                        }
                    };
                    for c in &chats {
                        let ts = chrono::Local
                            .timestamp_opt(c.last_updated_at, 0)
                            .single()
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default();
                        let unread = if c.unread_count > 0 {
                            format!(" [{}]", c.unread_count)
                        } else {
                            String::new()
                        };
                        println!(
                            "  {} | {} | {} ({}){} | {}",
                            c.chat_id,
                            chat_type_label(c.chat_type),
                            c.chat_name,
                            c.active_members_count,
                            unread,
                            ts,
                        );
                    }
                    println!("\n{} chats (local DB, no server contact)", chats.len());
                }
            }
        }
        Commands::LocalRead {
            chat_id,
            count,
            since,
        } => {
            let since_ts = util::parse_since_date(since.as_deref())?;
            let reader = local_db::LocalDbReader::open()?;
            let mut messages = reader.read_messages(chat_id, count, since_ts)?;
            messages.reverse(); // chronological order
            if json {
                println!("{}", serde_json::to_string_pretty(&messages)?);
            } else {
                if messages.is_empty() {
                    println!("No messages found in local database for chat {}.", chat_id);
                } else {
                    for m in &messages {
                        let ts = chrono::Local
                            .timestamp_opt(m.sent_at, 0)
                            .single()
                            .map(|dt| dt.format("%H:%M:%S").to_string())
                            .unwrap_or_default();
                        let sender = if m.sender_name.is_empty() {
                            format!("{}", m.author_id)
                        } else {
                            m.sender_name.clone()
                        };
                        let type_tag = util::message_type_label(m.message_type);
                        if m.message_type == 1 {
                            println!("  [{}] {}: {}", ts, sender, m.message);
                        } else {
                            println!("  [{}] {}: [{}] {}", ts, sender, type_tag, m.message);
                        }
                    }
                    println!(
                        "\n{} messages (local DB, no server contact)",
                        messages.len()
                    );
                }
            }
        }
        Commands::LocalSearch { query, count } => {
            let reader = local_db::LocalDbReader::open()?;
            let results = reader.search_messages(&query, count)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else {
                if results.is_empty() {
                    println!("No messages matching '{}' in local database.", query);
                } else {
                    for m in &results {
                        let ts = chrono::Local
                            .timestamp_opt(m.sent_at, 0)
                            .single()
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default();
                        let sender = if m.sender_name.is_empty() {
                            format!("{}", m.author_id)
                        } else {
                            m.sender_name.clone()
                        };
                        println!(
                            "  [{}] chat={} {}: {}",
                            ts,
                            m.chat_id,
                            sender,
                            util::truncate(&m.message, 80)
                        );
                    }
                    println!("\n{} results (local DB, no server contact)", results.len());
                }
            }
        }
        Commands::LocalSchema => {
            let reader = local_db::LocalDbReader::open()?;
            let tables = reader.schema()?;
            if json {
                let items: Vec<serde_json::Value> = tables
                    .iter()
                    .map(|(name, sql)| serde_json::json!({"name": name, "sql": sql}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                for (name, sql) in &tables {
                    println!("-- {}", name);
                    println!("{}\n", sql);
                }
            }
        }
        Commands::LocalSend {
            chat_name,
            message,
            yes,
            dry_run,
        } => {
            let msg = format_outgoing_message(&message, signature, watermark);
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            commands::local_send::cmd_local_send(commands::local_send::LocalSendOptions {
                chat_name,
                message: msg,
                skip_confirm: yes,
                dry_run,
                json,
            })?
        }
        Commands::LocalSendFile {
            chat_name,
            file,
            yes,
            dry_run,
        } => {
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            commands::local_send_file::cmd_local_send_file(
                commands::local_send_file::LocalSendFileOptions {
                    chat_name,
                    file,
                    max_file_mb: config.ax.max_file_mb.unwrap_or(config::DEFAULT_MAX_FILE_MB),
                    skip_confirm: yes,
                    dry_run,
                    json,
                },
            )?
        }
        Commands::LocalReply {
            chat_name,
            message,
            to_last,
            match_text,
            yes,
            dry_run,
        } => {
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            let target = match match_text {
                Some(text) if !to_last => ax_send::BubbleTarget::Text(text),
                _ => ax_send::BubbleTarget::Last,
            };
            commands::local_actions::cmd_local_reply(
                commands::local_actions::BubbleActionOptions {
                    chat_name,
                    target,
                    text: format_outgoing_message(&message, signature, watermark),
                    skip_confirm: yes,
                    dry_run,
                    json,
                },
            )?
        }
        Commands::LocalEdit {
            chat_name,
            old,
            new,
            yes,
            dry_run,
        } => {
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            commands::local_actions::cmd_local_edit(commands::local_actions::BubbleActionOptions {
                chat_name,
                target: ax_send::BubbleTarget::Text(old),
                text: new,
                skip_confirm: yes,
                dry_run,
                json,
            })?
        }
        Commands::AxChats { unread } => {
            let rows: Vec<_> = ax_send::scrape_chat_list()?
                .into_iter()
                .filter(|row| !unread || row.unread > 0)
                .collect();
            if json {
                let items: Vec<_> = rows
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "name": r.name,
                            "unread": r.unread,
                            "members": r.members,
                            "time": r.timestamp,
                            "preview": r.preview,
                        })
                    })
                    .collect();
                util::output_json(&serde_json::json!({"chats": items}))?;
            } else {
                for r in &rows {
                    println!(
                        "{} · {} unread · {} · {}",
                        util::escape_terminal_text(&r.name),
                        r.unread,
                        util::escape_terminal_text(&r.timestamp),
                        util::escape_terminal_text(&util::truncate(&r.preview, 50))
                    );
                }
                println!("\n{} chats", rows.len());
            }
        }
        Commands::LocalMarkRead { chat_names } => {
            require_ax_send(&config)?;
            let mut results = Vec::new();
            let mut failed = 0;
            for name in &chat_names {
                match ax_send::mark_read_via_ax(name) {
                    Ok(cleared) => {
                        if !json {
                            println!(
                                "{}: {cleared} marked read",
                                util::escape_terminal_text(name)
                            );
                        }
                        results.push(serde_json::json!({"chat_name": name, "status": "verified", "cleared": cleared}));
                    }
                    Err(error) => {
                        failed += 1;
                        let code = error::error_code(&error).map(|c| c.as_str());
                        if !json {
                            eprintln!("{}: {error:#}", util::escape_terminal_text(name));
                        }
                        results.push(serde_json::json!({"chat_name": name, "status": "failed", "code": code, "message": format!("{error:#}")}));
                    }
                }
            }
            if json {
                util::output_json(&serde_json::json!({"results": results}))?;
            }
            if failed > 0 {
                anyhow::bail!(
                    "{failed} of {} chats were not marked read",
                    chat_names.len()
                );
            }
        }
        Commands::LocalLeaveChat { chat_names, yes } => {
            require_ax_send(&config)?;
            eprintln!("Leave these KakaoTalk chats (their messages are removed from this account; this cannot be undone):");
            for name in &chat_names {
                eprintln!("  {}", util::escape_terminal_text(name));
            }
            if !yes {
                eprint!("Continue?\n[y/N] ");
                if !util::confirm()? {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            let mut results = Vec::new();
            let mut failed = 0;
            for name in &chat_names {
                match ax_send::leave_chat_via_ax(name) {
                    Ok(()) => {
                        if !json {
                            println!("{}: left", util::escape_terminal_text(name));
                        }
                        results.push(serde_json::json!({"chat_name": name, "status": "verified"}));
                    }
                    Err(error) => {
                        failed += 1;
                        let code = error::error_code(&error).map(|c| c.as_str());
                        if !json {
                            eprintln!("{}: {error:#}", util::escape_terminal_text(name));
                        }
                        results.push(serde_json::json!({"chat_name": name, "status": "failed", "code": code, "message": format!("{error:#}")}));
                    }
                }
            }
            if json {
                util::output_json(&serde_json::json!({"results": results}))?;
            }
            if failed > 0 {
                anyhow::bail!("{failed} of {} chats were not left", chat_names.len());
            }
        }
        Commands::LocalRenameChat {
            chat_name,
            new_name,
            yes,
        } => {
            require_ax_send(&config)?;
            eprintln!(
                "Rename KakaoTalk chat '{}' to '{}' (only you see this name)",
                util::escape_terminal_text(&chat_name),
                util::escape_terminal_text(&new_name)
            );
            if !yes {
                eprint!("Continue?\n[y/N] ");
                if !util::confirm()? {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            ax_send::rename_chat_via_ax(&chat_name, &new_name)?;
            if json {
                util::output_json(&serde_json::json!({
                    "chat_name": chat_name,
                    "new_name": new_name,
                    "status": "verified",
                }))?;
            } else {
                println!("Renamed to '{}'.", util::escape_terminal_text(&new_name));
            }
        }
        Commands::LocalReact {
            chat_name,
            reaction,
            to_last,
            match_text,
            yes,
            dry_run,
        } => {
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            let target = match match_text {
                Some(text) if !to_last => ax_send::BubbleTarget::Text(text),
                _ => ax_send::BubbleTarget::Last,
            };
            commands::local_actions::cmd_local_react(
                commands::local_actions::BubbleActionOptions {
                    chat_name,
                    target,
                    text: reaction,
                    skip_confirm: yes,
                    dry_run,
                    json,
                },
            )?
        }
        Commands::LocalDelete {
            chat_name,
            text,
            for_everyone,
            for_me: _,
            yes,
            dry_run,
        } => {
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            let scope = if for_everyone {
                ax_send::DeleteScope::Everyone
            } else {
                ax_send::DeleteScope::Me
            };
            commands::local_actions::cmd_local_delete(&chat_name, &text, scope, yes, dry_run, json)?
        }
        Commands::LocalSendPhoto {
            chat_name,
            file,
            yes,
            dry_run,
        } => {
            if !dry_run {
                require_ax_send(&config)?;
                warn_if_not_allowlisted(&config, &chat_name);
            }
            commands::local_send_photo::cmd_local_send_photo(
                commands::local_send_photo::LocalSendPhotoOptions {
                    chat_name,
                    file,
                    skip_confirm: yes,
                    dry_run,
                    json,
                },
            )?
        }
        Commands::SafeSend { action } => match action {
            SafeSendAction::Propose {
                chat_name,
                message,
                reply_chat_id,
                reply_log_id,
                idempotency_key,
            } => {
                let message = format_outgoing_message(&message, signature, watermark);
                commands::safe_send::cmd_propose(commands::safe_send::ProposeOptions {
                    chat_name,
                    message,
                    reply_to: reply_chat_id.zip(reply_log_id),
                    idempotency_key,
                    json,
                })?
            }
            SafeSendAction::List => commands::safe_send::cmd_list(json)?,
            SafeSendAction::Approve { intent_id } => {
                commands::safe_send::cmd_approve(commands::safe_send::ApproveOptions {
                    intent_id,
                    allow_ax_send: config.safety.allow_ax_send,
                    allowed_chats: config.safety.allowed_send_chats.clone(),
                    unattended,
                    json,
                })?
            }
            SafeSendAction::Cancel { intent_id, yes } => {
                commands::safe_send::cmd_cancel(&intent_id, yes, json)?
            }
        },
        Commands::AxLogin { account } => commands::ax_login::cmd_ax_login(
            account.or_else(|| config.ax.login_account.clone()),
            json,
        )?,
        Commands::AxFriends { query, .. } => {
            commands::ax_friends::cmd_ax_friends(query.as_deref(), json)?
        }
        Commands::Style {
            chat_name,
            learn,
            forget,
            decline,
            me,
        } => commands::style::cmd_style(commands::style::StyleOptions {
            chat_name: &chat_name,
            learn,
            forget,
            decline,
            me: me.as_deref(),
            json,
        })?,
        Commands::AxFriend {
            name,
            block: true,
            yes,
            ..
        } => {
            require_ax_send(&config)?;
            eprintln!(
                "Block KakaoTalk channel '{}' (its ads stop; 설정 > 친구 > 차단친구 관리 undoes it)",
                util::escape_terminal_text(&name)
            );
            if !yes {
                eprint!("Continue?\n[y/N] ");
                if !util::confirm()? {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            ax_send::block_channel_via_ax(&name)?;
            if json {
                util::output_json(&serde_json::json!({"name": name, "status": "blocked"}))?;
            } else {
                println!(
                    "Blocked the channel '{}' (설정 > 친구 > 차단친구 관리 undoes it).",
                    util::escape_terminal_text(&name)
                );
            }
        }
        Commands::AxFriend {
            name,
            open_chat: true,
            ..
        } => {
            ax_send::open_friend_chat_via_ax(&name)?;
            if json {
                util::output_json(&serde_json::json!({"chat_name": name, "status": "opened"}))?;
            } else {
                println!(
                    "Opened the chat with '{}'.",
                    util::escape_terminal_text(&name)
                );
            }
        }
        Commands::AxFriend {
            name,
            memo,
            rename,
            yes,
            ..
        } => commands::ax_friends::cmd_ax_friend(commands::ax_friends::AxFriendOptions {
            name: &name,
            memo: memo.as_deref(),
            rename: rename.as_deref(),
            skip_confirm: yes,
            json,
        })?,
        Commands::AxExport { chat_name, output } => {
            commands::ax_export::cmd_ax_export(&chat_name, output.as_deref(), json)?
        }
        Commands::AxSearch {
            chat_name,
            query,
            limit,
            from,
            keep,
        } => commands::ax_export::cmd_ax_search(commands::ax_export::AxSearchOptions {
            chat_name: &chat_name,
            query: &query,
            limit,
            from: from.as_deref(),
            keep,
            json,
        })?,
        Commands::AxRead {
            chat_name,
            count,
            expand,
        } => commands::ax_read::cmd_ax_read(commands::ax_read::AxReadOptions {
            chat_name,
            count,
            expand,
            json,
        })?,
        Commands::AxWatch {
            interval,
            hook_cmd,
            webhook_url,
            webhook_header,
            webhook_signing_secret,
            webhook_format,
            hook_chat,
            hook_keyword,
            hook_fail_fast,
        } => commands::ax_watch::cmd_ax_watch(commands::ax_watch::AxWatchOptions {
            interval_secs: interval,
            hook_cmd,
            webhook_url,
            webhook_headers: webhook_header,
            webhook_signing_secret,
            webhook_format: commands::watch::WebhookFormat::from_str_opt(Some(&webhook_format))?,
            hook_chats: hook_chat,
            hook_keywords: hook_keyword,
            fail_fast: hook_fail_fast,
            allow_insecure_webhooks: config.safety.allow_insecure_webhooks,
            min_hook_interval_secs,
            min_webhook_interval_secs,
            hook_timeout_secs,
            webhook_timeout_secs,
            json,
            unattended,
            allow_side_effects: allow_watch_side_effects,
        })?,
        Commands::NotifWatch {
            interval,
            replay_existing,
            durable,
            hook_cmd,
            webhook_url,
            webhook_header,
            webhook_signing_secret,
            webhook_format,
            hook_chat,
            hook_keyword,
            hook_fail_fast,
        } => commands::notif_watch::cmd_notif_watch(commands::notif_watch::NotifWatchOptions {
            interval_secs: interval,
            replay_existing,
            durable,
            hook_cmd,
            webhook_url,
            webhook_headers: webhook_header,
            webhook_signing_secret,
            webhook_format: commands::watch::WebhookFormat::from_str_opt(Some(&webhook_format))?,
            hook_chats: hook_chat,
            hook_keywords: hook_keyword,
            fail_fast: hook_fail_fast,
            allow_insecure_webhooks: config.safety.allow_insecure_webhooks,
            min_hook_interval_secs,
            min_webhook_interval_secs,
            hook_timeout_secs,
            webhook_timeout_secs,
            json,
            unattended,
            allow_side_effects: allow_watch_side_effects,
        })?,
        Commands::WatchCache { interval } => commands::auth::cmd_watch_cache(interval)?,
        Commands::Init {
            yes,
            skill,
            allow_chat,
            all_spaces,
        } => commands::init::cmd_init(commands::init::InitOptions {
            yes,
            skill,
            allow_chats: allow_chat,
            all_spaces,
            json,
        })?,
        Commands::Doctor { loco, chat } => {
            commands::doctor::cmd_doctor(json, loco, chat.as_deref(), &config)?
        }
    }

    if cli.completion_promise {
        println!("[DONE]");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::members::LocoMemberProfile;
    use crate::commands::profile::{
        build_syncmainpf_candidate, collect_hint_chat_ids, local_graph_hint_summary,
        parse_profile_cache_hint, LocalFriendGraphChatMeta, LocalFriendGraphEntry,
        LocalFriendGraphSnapshot, ProfileCacheHint,
    };
    use crate::commands::watch::{
        build_webhook_signature, parse_webhook_header, validate_webhook_url, watch_hook_matches,
        WatchHookConfig, WatchMessageEvent,
    };
    use crate::loco_helpers::should_retry_loco_probe_error;
    use crate::util::{require_permission, validate_outbound_message};

    #[test]
    fn outgoing_messages_end_with_the_signature_line() {
        assert_eq!(
            format_outgoing_message("hello", Some(util::DEFAULT_SIGNATURE), false),
            "hello\n\n⌞ₛₑₙₜ ᵥᵢₐ ₒₚₑₙₖₐₖₐₒ"
        );
        assert_eq!(
            format_outgoing_message("hello", Some("— 정훈 비서"), false),
            "hello\n\n— 정훈 비서"
        );
    }

    #[test]
    fn outgoing_messages_can_drop_the_signature() {
        assert_eq!(format_outgoing_message("hello", None, false), "hello");
        assert_eq!(format_outgoing_message("hello", Some(" "), false), "hello");
        // The invisible marker ends every message, once.
        let marked = format_outgoing_message("hello", None, true);
        assert_eq!(marked, format!("hello{}", util::WATERMARK));
        assert_eq!(format_outgoing_message(&marked, None, true), marked);
    }

    #[test]
    fn send_accepts_global_and_local_flags_after_subcommand() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "--unattended",
            "--allow-non-interactive-send",
            "send",
            "123",
            "hello",
            "--no-prefix",
            "-y",
        ])
        .expect("send should accept global and local flags");

        assert!(cli.no_prefix);
        assert!(cli.unattended);
        assert!(cli.allow_non_interactive_send);
        match cli.command {
            Commands::Send {
                chat_id,
                message,
                yes,
                dry_run,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(message, "hello");
                assert!(yes);
                assert!(!dry_run);
            }
            other => panic!("expected send command, got {other:?}"),
        }
    }

    #[test]
    fn unattended_flag_is_available_globally() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "--unattended",
            "--allow-watch-side-effects",
            "watch",
            "--hook-cmd",
            "cat",
        ])
        .expect("global unattended flag should parse");

        assert!(cli.unattended);
        assert!(cli.allow_watch_side_effects);
    }

    #[test]
    fn permission_gate_rejects_missing_opt_in() {
        let err = require_permission(false, "non-interactive send", "set the flags").unwrap_err();
        assert!(
            err.to_string().contains("set the flags"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn watch_hook_filters_match_expected_events() {
        let config = WatchHookConfig {
            command: Some("cat".to_string()),
            webhook_url: None,
            webhook_headers: Vec::new(),
            webhook_signing_secret: None,
            webhook_format: WebhookFormat::Raw,
            chat_ids: vec![42],
            chat_names: vec![],
            keywords: vec!["urgent".to_string()],
            message_types: vec![1],
            fail_fast: false,
            min_hook_interval_secs: 2,
            min_webhook_interval_secs: 2,
            hook_timeout_secs: 20,
            webhook_timeout_secs: 10,
        };
        let event = WatchMessageEvent {
            event_type: "message",
            received_at: "2026-03-08T00:00:00Z".to_string(),
            method: "MSG".to_string(),
            chat_id: 42,
            chat_name: "test".to_string(),
            log_id: 7,
            author_id: 9,
            author_nickname: "alice".to_string(),
            message_type: 1,
            message: "urgent: ping me".to_string(),
            attachment: String::new(),
            unread: 0,
        };

        assert!(watch_hook_matches(&config, &event));

        let wrong_chat = WatchMessageEvent {
            chat_id: 99,
            ..event.clone()
        };
        assert!(!watch_hook_matches(&config, &wrong_chat));

        let wrong_keyword = WatchMessageEvent {
            message: "casual update".to_string(),
            ..event.clone()
        };
        assert!(!watch_hook_matches(&config, &wrong_keyword));
    }

    #[test]
    fn watch_accepts_hook_flags() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "--unattended",
            "--allow-watch-side-effects",
            "watch",
            "--hook-cmd",
            "cat >/tmp/openkakao-hook.json",
            "--webhook-url",
            "https://example.com/openkakao",
            "--webhook-header",
            "Authorization: Bearer token",
            "--webhook-signing-secret",
            "super-secret",
            "--hook-chat-id",
            "123",
            "--hook-keyword",
            "urgent",
            "--hook-type",
            "1",
            "--hook-fail-fast",
        ])
        .expect("watch should accept hook flags");

        assert!(cli.unattended);
        assert!(cli.allow_watch_side_effects);
        match cli.command {
            Commands::Watch {
                hook_cmd,
                webhook_url,
                webhook_header,
                webhook_signing_secret,
                hook_chat_id,
                hook_keyword,
                hook_type,
                hook_fail_fast,
                ..
            } => {
                assert_eq!(hook_cmd.as_deref(), Some("cat >/tmp/openkakao-hook.json"));
                assert_eq!(
                    webhook_url.as_deref(),
                    Some("https://example.com/openkakao")
                );
                assert_eq!(
                    webhook_header,
                    vec!["Authorization: Bearer token".to_string()]
                );
                assert_eq!(webhook_signing_secret.as_deref(), Some("super-secret"));
                assert_eq!(hook_chat_id, vec![123]);
                assert_eq!(hook_keyword, vec!["urgent".to_string()]);
                assert_eq!(hook_type, vec![1]);
                assert!(hook_fail_fast);
            }
            other => panic!("expected watch command, got {other:?}"),
        }
    }

    #[test]
    fn read_accepts_transport_flags() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "read",
            "123",
            "--rest",
            "--delay-ms",
            "250",
            "--force",
        ])
        .expect("read should accept transport flags");

        match cli.command {
            Commands::Read {
                chat_id,
                rest,
                delay_ms,
                force,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert!(rest);
                assert_eq!(delay_ms, 250);
                assert!(force);
            }
            other => panic!("expected read command, got {other:?}"),
        }
    }

    #[test]
    fn chats_accepts_rest_flag() {
        let cli = Cli::try_parse_from(["openkakao-cli", "chats", "--rest", "--unread"])
            .expect("chats should accept --rest");

        match cli.command {
            Commands::Chats { rest, unread, .. } => {
                assert!(rest);
                assert!(unread);
            }
            other => panic!("expected chats command, got {other:?}"),
        }
    }

    #[test]
    fn members_accepts_rest_flag() {
        let cli = Cli::try_parse_from(["openkakao-cli", "members", "123", "--rest", "--full"])
            .expect("members should accept --rest and --full");

        match cli.command {
            Commands::Members {
                chat_id,
                rest,
                full,
            } => {
                assert_eq!(chat_id, 123);
                assert!(rest);
                assert!(full);
            }
            other => panic!("expected members command, got {other:?}"),
        }
    }

    #[test]
    fn profile_accepts_chat_id_flag() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "profile",
            "100000002",
            "--chat-id",
            "900000000000001",
        ])
        .expect("profile should accept --chat-id");

        match cli.command {
            Commands::Profile {
                user_id,
                chat_id,
                local,
            } => {
                assert_eq!(user_id, 100000002);
                assert_eq!(chat_id, Some(900000000000001));
                assert!(!local);
            }
            other => panic!("expected profile command, got {other:?}"),
        }
    }

    #[test]
    fn friends_accepts_local_flag() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "friends",
            "--local",
            "-s",
            "Alice",
            "--chat-id",
            "900000000000003",
            "--user-id",
            "100000003",
        ])
        .expect("friends should accept --local");

        match cli.command {
            Commands::Friends {
                local,
                search,
                favorites,
                hidden,
                chat_id,
                user_id,
            } => {
                assert!(local);
                assert_eq!(search.as_deref(), Some("Alice"));
                assert!(!favorites);
                assert!(!hidden);
                assert_eq!(chat_id, Some(900000000000003));
                assert_eq!(user_id, Some(100000003));
            }
            other => panic!("expected friends command, got {other:?}"),
        }
    }

    #[test]
    fn profile_accepts_local_flag() {
        let cli = Cli::try_parse_from(["openkakao-cli", "profile", "100000002", "--local"])
            .expect("profile should accept --local");

        match cli.command {
            Commands::Profile {
                user_id,
                chat_id,
                local,
            } => {
                assert_eq!(user_id, 100000002);
                assert_eq!(chat_id, None);
                assert!(local);
            }
            other => panic!("expected profile command, got {other:?}"),
        }
    }

    #[test]
    fn chatinfo_command_is_available() {
        let cli = Cli::try_parse_from(["openkakao-cli", "chatinfo", "123"])
            .expect("chatinfo should be available");

        match cli.command {
            Commands::Chatinfo { chat_id } => assert_eq!(chat_id, 123),
            other => panic!("expected chatinfo command, got {other:?}"),
        }
    }

    #[test]
    fn probe_command_is_available() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "probe",
            "BLSYNC",
            "--body",
            "{\"r\":0,\"pr\":0}",
        ])
        .expect("probe should be available");

        match cli.command {
            Commands::Probe { method, body, .. } => {
                assert_eq!(method, "BLSYNC");
                assert_eq!(body.as_deref(), Some("{\"r\":0,\"pr\":0}"));
            }
            other => panic!("expected probe command, got {other:?}"),
        }
    }

    #[test]
    fn profile_hints_command_is_available() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "profile-hints",
            "--local-graph",
            "--user-id",
            "100000003",
            "--probe-syncmainpf",
            "--probe-uplinkprof",
        ])
        .expect("profile-hints should be available");

        match cli.command {
            Commands::ProfileHints {
                app_state,
                app_state_diff,
                local_graph,
                user_id,
                probe_syncmainpf,
                probe_uplinkprof,
            } => {
                assert!(!app_state);
                assert!(app_state_diff.is_none());
                assert!(local_graph);
                assert_eq!(user_id, Some(100000003));
                assert!(probe_syncmainpf);
                assert!(probe_uplinkprof);
            }
            other => panic!("expected profile-hints command, got {other:?}"),
        }
    }

    #[test]
    fn profile_hints_accepts_app_state_diff() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "profile-hints",
            "--app-state",
            "--app-state-diff",
            "/tmp/profile-hints-before.json",
        ])
        .expect("profile-hints should accept --app-state-diff");

        match cli.command {
            Commands::ProfileHints {
                app_state,
                app_state_diff,
                ..
            } => {
                assert!(app_state);
                assert_eq!(
                    app_state_diff.as_deref(),
                    Some("/tmp/profile-hints-before.json")
                );
            }
            other => panic!("expected profile-hints command, got {other:?}"),
        }
    }

    #[test]
    fn probe_retry_helper_covers_common_socket_failures() {
        assert!(should_retry_loco_probe_error(&anyhow::anyhow!("early eof")));
        assert!(should_retry_loco_probe_error(&anyhow::anyhow!(
            "Connection reset by peer (os error 54)"
        )));
        assert!(should_retry_loco_probe_error(&anyhow::anyhow!(
            "broken pipe"
        )));
        assert!(!should_retry_loco_probe_error(&anyhow::anyhow!(
            "status=-203"
        )));
    }

    #[test]
    fn parse_friend_profile_cache_hint_extracts_ids_and_access_permit() {
        let hint = parse_profile_cache_hint(
            136,
            "https://katalk.kakao.com/mac/profile3/friend.json?accessPermit=example-access-permit-token&chatId=900000000000002&id=100000002",
            true,
        );

        assert_eq!(hint.kind, "friend");
        assert_eq!(hint.user_ids, vec![100000002]);
        assert_eq!(hint.chat_id, Some(900000000000002));
        assert_eq!(
            hint.access_permit.as_deref(),
            Some("example-access-permit-token")
        );
        assert!(hint.data_on_fs);
    }

    #[test]
    fn parse_friends_profile_cache_hint_extracts_ids_list() {
        let hint = parse_profile_cache_hint(
            88,
            "https://katalk.kakao.com/mac/profile3/friends.json?category=action&ids=%5B100000004%2C100000005%2C100000006%5D",
            false,
        );

        assert_eq!(hint.kind, "friends");
        assert_eq!(hint.user_ids, vec![100000004, 100000005, 100000006]);
        assert_eq!(hint.category.as_deref(), Some("action"));
        assert_eq!(hint.chat_id, None);
        assert_eq!(hint.access_permit, None);
    }

    #[test]
    fn collect_hint_chat_ids_prefers_user_specific_chat_hints() {
        let hints = vec![
            ProfileCacheHint {
                entry_id: 1,
                kind: "friend".into(),
                request_key: String::new(),
                user_ids: vec![100000002],
                chat_id: Some(900000000000002),
                access_permit: Some("permit-a".into()),
                category: None,
                data_on_fs: true,
            },
            ProfileCacheHint {
                entry_id: 2,
                kind: "friend".into(),
                request_key: String::new(),
                user_ids: vec![100000002],
                chat_id: Some(900000000000002),
                access_permit: Some("permit-b".into()),
                category: None,
                data_on_fs: true,
            },
            ProfileCacheHint {
                entry_id: 3,
                kind: "friend".into(),
                request_key: String::new(),
                user_ids: vec![100000003],
                chat_id: Some(900000000000003),
                access_permit: Some("permit-c".into()),
                category: None,
                data_on_fs: true,
            },
        ];

        assert_eq!(
            collect_hint_chat_ids(&hints, 100000002),
            vec![900000000000002]
        );
        assert_eq!(
            collect_hint_chat_ids(&hints, 100000003),
            vec![900000000000003]
        );
        assert!(collect_hint_chat_ids(&hints, 999).is_empty());
    }

    #[test]
    fn parse_loco_member_profile_from_getmem_doc() {
        let doc = bson::doc! {
            "userId": 100000002_i64,
            "accountId": 200000001_i64,
            "nickName": "Alice",
            "countryIso": "kr",
            "statusMessage": "hello",
            "profileImageUrl": "https://example.com/p.jpg",
            "fullProfileImageUrl": "https://example.com/p-full.jpg",
            "originalProfileImageUrl": "https://example.com/p-original.jpg",
            "accessPermit": "permit-token",
            "suspicion": "",
            "suspended": false,
            "memorial": false,
            "type": 0_i32,
            "ut": 100_i64,
        };

        let profile = LocoMemberProfile::from_getmem_doc(&doc);
        assert_eq!(
            profile,
            LocoMemberProfile {
                user_id: 100000002,
                account_id: 200000001,
                nickname: "Alice".into(),
                country_iso: "kr".into(),
                status_message: "hello".into(),
                profile_image_url: "https://example.com/p.jpg".into(),
                full_profile_image_url: "https://example.com/p-full.jpg".into(),
                original_profile_image_url: "https://example.com/p-original.jpg".into(),
                access_permit: "permit-token".into(),
                suspicion: String::new(),
                suspended: false,
                memorial: false,
                member_type: 0,
                ut: 100,
            }
        );
        assert_eq!(profile.as_chat_member().display_name(), "Alice");
    }

    #[test]
    fn local_graph_summary_carries_getmem_tokens() {
        let snapshot = LocalFriendGraphSnapshot {
            user_count: 1,
            chat_count: 1,
            failed_chat_ids: Vec::new(),
            chat_meta: vec![LocalFriendGraphChatMeta {
                chat_id: 900000000000002,
                title: "Example".into(),
                getmem_token: Some(777),
                member_count: 2,
            }],
            entries: vec![LocalFriendGraphEntry {
                user_id: 100000002,
                account_id: 200000001,
                nickname: "Alice".into(),
                country_iso: "KR".into(),
                status_message: String::new(),
                profile_image_url: String::new(),
                full_profile_image_url: String::new(),
                original_profile_image_url: String::new(),
                access_permits: vec!["permit-token".into()],
                suspicion: String::new(),
                suspended: false,
                memorial: false,
                member_type: 0,
                chat_ids: vec![900000000000002],
                chat_titles: vec!["Example".into()],
                is_self: false,
                hidden_like: false,
                hidden_block_type: None,
            }],
        };
        let hints = vec![ProfileCacheHint {
            entry_id: 1,
            kind: "friend".into(),
            request_key: String::new(),
            user_ids: vec![100000002],
            chat_id: Some(900000000000002),
            access_permit: Some("permit-token".into()),
            category: None,
            data_on_fs: true,
        }];

        let summary = local_graph_hint_summary(&snapshot, &hints);
        assert_eq!(summary.candidate_matches.len(), 1);
        assert_eq!(
            summary.candidate_matches[0].candidate_getmem_tokens,
            vec![777]
        );
    }

    #[test]
    fn syncmainpf_candidates_include_getmem_token_fields() {
        let snapshot = LocalFriendGraphSnapshot {
            user_count: 1,
            chat_count: 1,
            failed_chat_ids: Vec::new(),
            chat_meta: vec![LocalFriendGraphChatMeta {
                chat_id: 900000000000002,
                title: "Example".into(),
                getmem_token: Some(777),
                member_count: 2,
            }],
            entries: vec![LocalFriendGraphEntry {
                user_id: 100000002,
                account_id: 200000001,
                nickname: "Alice".into(),
                country_iso: "KR".into(),
                status_message: String::new(),
                profile_image_url: String::new(),
                full_profile_image_url: String::new(),
                original_profile_image_url: String::new(),
                access_permits: vec!["permit-token".into()],
                suspicion: String::new(),
                suspended: false,
                memorial: false,
                member_type: 0,
                chat_ids: vec![900000000000002],
                chat_titles: vec!["Example".into()],
                is_self: false,
                hidden_like: false,
                hidden_block_type: None,
            }],
        };

        let candidate = build_syncmainpf_candidate(&snapshot, &[], 100000002)
            .expect("candidate should be built");

        assert_eq!(candidate.getmem_tokens, vec![777]);
        assert!(candidate
            .bodies
            .iter()
            .any(|body| body.get("token").and_then(|v| v.as_i64()) == Some(777)));
        assert!(candidate
            .bodies
            .iter()
            .any(|body| body.get("profileToken").and_then(|v| v.as_i64()) == Some(777)));
        assert!(candidate
            .uplinkprof_bodies
            .iter()
            .any(|body| body.get("token").and_then(|v| v.as_i64()) == Some(777)));
    }

    #[test]
    fn legacy_loco_read_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-cli", "loco-read", "123", "--all"])
            .expect("legacy loco-read should remain available");

        match cli.command {
            Commands::LocoRead { chat_id, all, .. } => {
                assert_eq!(chat_id, 123);
                assert!(all);
            }
            other => panic!("expected loco-read command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_chats_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-cli", "loco-chats", "--all"])
            .expect("legacy loco-chats should remain available");

        match cli.command {
            Commands::LocoChats { show_all } => {
                assert!(show_all);
            }
            other => panic!("expected loco-chats command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_members_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-cli", "loco-members", "123"])
            .expect("legacy loco-members should remain available");

        match cli.command {
            Commands::LocoMembers { chat_id } => assert_eq!(chat_id, 123),
            other => panic!("expected loco-members command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_chatinfo_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-cli", "loco-chatinfo", "123"])
            .expect("legacy loco-chatinfo should remain available");

        match cli.command {
            Commands::LocoChatinfo { chat_id } => assert_eq!(chat_id, 123),
            other => panic!("expected loco-chatinfo command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_probe_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-cli", "loco-probe", "BLSYNC"])
            .expect("legacy loco-probe should remain available");

        match cli.command {
            Commands::LocoProbe { method, body } => {
                assert_eq!(method, "BLSYNC");
                assert!(body.is_none());
            }
            other => panic!("expected loco-probe command, got {other:?}"),
        }
    }

    #[test]
    fn webhook_header_requires_name_and_value() {
        assert_eq!(
            parse_webhook_header("Authorization: Bearer test").unwrap(),
            ("Authorization".to_string(), "Bearer test".to_string())
        );
        assert!(parse_webhook_header("MissingSeparator").is_err());
        assert!(parse_webhook_header("Header: ").is_err());
    }

    #[test]
    fn webhook_signature_is_stable_for_known_input() {
        let signature = build_webhook_signature("secret", "1700000000", br#"{"ok":true}"#).unwrap();
        assert_eq!(
            signature,
            "sha256=c1afc7c2df3db0690d7d75954610ed1a1d959ce96355ccb8c0a8bc09fd0cfc27"
        );
    }

    #[test]
    fn webhook_url_requires_https_or_loopback_http() {
        assert!(validate_webhook_url("https://example.com/hook", false).is_ok());
        assert!(validate_webhook_url("http://localhost:3000/hook", false).is_ok());
        assert!(validate_webhook_url("http://127.0.0.1:4000/hook", false).is_ok());
        assert!(validate_webhook_url("http://example.com/hook", false).is_err());
        assert!(validate_webhook_url("http://example.com/hook", true).is_ok());
    }

    #[test]
    fn outbound_message_must_not_be_blank() {
        assert!(validate_outbound_message("hello").is_ok());
        assert!(validate_outbound_message("   ").is_err());
    }

    #[test]
    fn stats_command_is_available() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "stats",
            "123",
            "--limit",
            "500",
            "--since",
            "2025-01-01",
        ])
        .expect("stats should accept limit and since");

        match cli.command {
            Commands::Stats {
                chat_id,
                limit,
                since,
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(limit, Some(500));
                assert_eq!(since.as_deref(), Some("2025-01-01"));
            }
            other => panic!("expected stats command, got {other:?}"),
        }
    }

    #[test]
    fn openkakao_error_loco_status_display() {
        use crate::error::OpenKakaoError;
        let err = OpenKakaoError::loco("SYNCMSG", -300);
        assert!(err.to_string().contains("SYNCMSG"));
        assert!(err.to_string().contains("-300"));
        assert!(err.is_retryable());
    }

    #[test]
    fn openkakao_error_token_expired_from_950() {
        use crate::error::OpenKakaoError;
        let err = OpenKakaoError::loco("LOGINLIST", -950);
        assert!(matches!(err, OpenKakaoError::TokenExpired));
        assert!(err.is_retryable());
    }

    #[test]
    fn openkakao_error_non_retryable_status() {
        use crate::error::OpenKakaoError;
        let err = OpenKakaoError::loco("WRITE", -203);
        assert!(!err.is_retryable());
    }

    #[test]
    fn check_loco_status_passes_on_zero() {
        use crate::loco_helpers::check_loco_status;
        let packet = crate::loco::packet::LocoPacket {
            packet_id: 1,
            status_code: 0,
            method: "TEST".into(),
            body_type: 0,
            body: bson::doc! { "status": 0_i32 },
        };
        assert!(check_loco_status("TEST", &packet).is_ok());
    }

    #[test]
    fn check_loco_status_fails_on_nonzero() {
        use crate::loco_helpers::check_loco_status;
        let packet = crate::loco::packet::LocoPacket {
            packet_id: 1,
            status_code: 0,
            method: "SYNCMSG".into(),
            body_type: 0,
            body: bson::doc! { "status": -300_i32 },
        };
        let err = check_loco_status("SYNCMSG", &packet).unwrap_err();
        assert!(err.to_string().contains("SYNCMSG"));
        assert!(err.to_string().contains("-300"));
    }

    #[test]
    fn watch_capture_flag_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "watch", "--capture"])
            .expect("watch should accept --capture");

        match cli.command {
            Commands::Watch { capture, .. } => {
                assert!(capture);
            }
            other => panic!("expected watch command, got {other:?}"),
        }
    }

    #[test]
    fn probe_capture_pushes_flag_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "probe", "PING", "--capture-pushes"])
            .expect("probe should accept --capture-pushes");

        match cli.command {
            Commands::Probe {
                method,
                capture_pushes,
                ..
            } => {
                assert_eq!(method, "PING");
                assert!(capture_pushes);
            }
            other => panic!("expected probe command, got {other:?}"),
        }
    }

    #[test]
    fn delete_command_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "delete", "123", "456", "--force", "-y"])
            .expect("delete should parse");
        match cli.command {
            Commands::Delete {
                chat_id,
                log_id,
                force,
                yes,
                dry_run,
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(log_id, 456);
                assert!(force);
                assert!(yes);
                assert!(!dry_run);
            }
            other => panic!("expected delete, got {other:?}"),
        }
    }

    #[test]
    fn mark_read_command_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "mark-read", "123", "456"])
            .expect("mark-read should parse");
        match cli.command {
            Commands::MarkRead { chat_id, log_id } => {
                assert_eq!(chat_id, 123);
                assert_eq!(log_id, 456);
            }
            other => panic!("expected mark-read, got {other:?}"),
        }
    }

    #[test]
    fn send_me_command_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "send-me", "test message"])
            .expect("send-me should parse");
        match cli.command {
            Commands::SendMe { message, .. } => {
                assert_eq!(message, "test message");
            }
            other => panic!("expected send-me, got {other:?}"),
        }
    }

    #[test]
    fn send_accepts_dry_run_flag() {
        let cli = Cli::try_parse_from(["openkakao-cli", "send", "123", "hello", "--dry-run"])
            .expect("send --dry-run should parse");
        match cli.command {
            Commands::Send {
                chat_id, dry_run, ..
            } => {
                assert_eq!(chat_id, 123);
                assert!(dry_run);
            }
            other => panic!("expected send, got {other:?}"),
        }
    }

    #[test]
    fn delete_accepts_dry_run_flag() {
        let cli = Cli::try_parse_from(["openkakao-cli", "delete", "123", "456", "--dry-run"])
            .expect("delete --dry-run should parse");
        match cli.command {
            Commands::Delete {
                chat_id,
                log_id,
                dry_run,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(log_id, 456);
                assert!(dry_run);
            }
            other => panic!("expected delete, got {other:?}"),
        }
    }

    #[test]
    fn edit_accepts_dry_run_flag() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "edit",
            "123",
            "456",
            "new text",
            "--dry-run",
        ])
        .expect("edit --dry-run should parse");
        match cli.command {
            Commands::Edit {
                chat_id,
                log_id,
                message,
                dry_run,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(log_id, 456);
                assert_eq!(message, "new text");
                assert!(dry_run);
            }
            other => panic!("expected edit, got {other:?}"),
        }
    }

    #[test]
    fn react_accepts_dry_run_flag() {
        let cli = Cli::try_parse_from(["openkakao-cli", "react", "123", "456", "--dry-run"])
            .expect("react --dry-run should parse");
        match cli.command {
            Commands::React {
                chat_id,
                log_id,
                dry_run,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(log_id, 456);
                assert!(dry_run);
            }
            other => panic!("expected react, got {other:?}"),
        }
    }

    #[test]
    fn local_chats_command_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "local-chats", "-n", "10"])
            .expect("local-chats should parse");
        match cli.command {
            Commands::LocalChats { limit } => assert_eq!(limit, 10),
            other => panic!("expected local-chats, got {other:?}"),
        }
    }

    #[test]
    fn local_read_command_parses() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "local-read",
            "123",
            "-n",
            "50",
            "--since",
            "2025-01-01",
        ])
        .expect("local-read should parse");
        match cli.command {
            Commands::LocalRead {
                chat_id,
                count,
                since,
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(count, 50);
                assert_eq!(since.as_deref(), Some("2025-01-01"));
            }
            other => panic!("expected local-read, got {other:?}"),
        }
    }

    #[test]
    fn local_search_command_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "local-search", "hello", "-n", "10"])
            .expect("local-search should parse");
        match cli.command {
            Commands::LocalSearch { query, count } => {
                assert_eq!(query, "hello");
                assert_eq!(count, 10);
            }
            other => panic!("expected local-search, got {other:?}"),
        }
    }

    #[test]
    fn local_schema_command_parses() {
        Cli::try_parse_from(["openkakao-cli", "local-schema"]).expect("local-schema should parse");
    }

    #[test]
    fn local_send_command_parses() {
        let cli = Cli::try_parse_from(["openkakao-cli", "local-send", "나와의 채팅", "hi", "-y"])
            .expect("local-send should parse");
        match cli.command {
            Commands::LocalSend {
                chat_name,
                message,
                yes,
                dry_run,
            } => {
                assert_eq!(chat_name, "나와의 채팅");
                assert_eq!(message, "hi");
                assert!(yes);
                assert!(!dry_run);
            }
            other => panic!("expected local-send, got {other:?}"),
        }
    }

    #[test]
    fn local_reply_and_edit_commands_parse() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "local-reply",
            "나와의 채팅",
            "답장",
            "--match",
            "원문",
            "--dry-run",
        ])
        .expect("local-reply --match should parse");
        match cli.command {
            Commands::LocalReply {
                to_last,
                match_text,
                dry_run,
                ..
            } => {
                assert!(!to_last);
                assert_eq!(match_text.as_deref(), Some("원문"));
                assert!(dry_run);
            }
            other => panic!("expected local-reply, got {other:?}"),
        }
        // One of --to-last / --match is required, and they conflict.
        assert!(Cli::try_parse_from(["openkakao-cli", "local-reply", "c", "m"]).is_err());
        assert!(Cli::try_parse_from([
            "openkakao-cli",
            "local-reply",
            "c",
            "m",
            "--to-last",
            "--match",
            "x"
        ])
        .is_err());
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "local-edit",
            "c",
            "--match",
            "old",
            "new",
            "-y",
        ])
        .expect("local-edit should parse");
        match cli.command {
            Commands::LocalEdit { old, new, yes, .. } => {
                assert_eq!((old.as_str(), new.as_str()), ("old", "new"));
                assert!(yes);
            }
            other => panic!("expected local-edit, got {other:?}"),
        }
    }

    #[test]
    fn local_send_photo_command_parses() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "local-send-photo",
            "Family",
            "/tmp/photo.jpg",
            "--dry-run",
        ])
        .expect("local-send-photo should parse");
        match cli.command {
            Commands::LocalSendPhoto {
                chat_name,
                file,
                yes,
                dry_run,
            } => {
                assert_eq!(chat_name, "Family");
                assert_eq!(file, "/tmp/photo.jpg");
                assert!(!yes);
                assert!(dry_run);
            }
            other => panic!("expected local-send-photo, got {other:?}"),
        }
    }

    #[test]
    fn safe_send_approve_command_parses_without_a_noninteractive_bypass() {
        let cli = Cli::try_parse_from(["openkakao-cli", "safe-send", "approve", "intent-123"])
            .expect("safe-send approve should parse");
        match cli.command {
            Commands::SafeSend {
                action: SafeSendAction::Approve { intent_id },
            } => assert_eq!(intent_id, "intent-123"),
            other => panic!("expected safe-send approve, got {other:?}"),
        }
    }

    #[test]
    fn safe_send_cancel_command_parses() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "safe-send",
            "cancel",
            "intent-123",
            "--yes",
        ])
        .expect("safe-send cancel should parse");
        match cli.command {
            Commands::SafeSend {
                action: SafeSendAction::Cancel { intent_id, yes },
            } => {
                assert_eq!(intent_id, "intent-123");
                assert!(yes);
            }
            other => panic!("expected safe-send cancel, got {other:?}"),
        }
    }

    #[test]
    fn ax_watch_command_parses() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "ax-watch",
            "--interval",
            "5",
            "--hook-keyword",
            "긴급",
            "--hook-chat",
            "정훈",
        ])
        .expect("ax-watch should parse");
        match cli.command {
            Commands::AxWatch {
                interval,
                hook_keyword,
                hook_chat,
                ..
            } => {
                assert_eq!(interval, 5);
                assert_eq!(hook_keyword, vec!["긴급".to_string()]);
                assert_eq!(hook_chat, vec!["정훈".to_string()]);
            }
            other => panic!("expected ax-watch, got {other:?}"),
        }
    }

    #[test]
    fn notif_watch_command_parses() {
        let cli = Cli::try_parse_from([
            "openkakao-cli",
            "notif-watch",
            "--interval",
            "5",
            "--replay-existing",
            "--durable",
            "--hook-keyword",
            "긴급",
            "--hook-chat",
            "정훈",
        ])
        .expect("notif-watch should parse");
        match cli.command {
            Commands::NotifWatch {
                interval,
                replay_existing,
                durable,
                hook_keyword,
                hook_chat,
                ..
            } => {
                assert_eq!(interval, 5);
                assert!(replay_existing);
                assert!(durable);
                assert_eq!(hook_keyword, vec!["긴급".to_string()]);
                assert_eq!(hook_chat, vec!["정훈".to_string()]);
            }
            other => panic!("expected notif-watch, got {other:?}"),
        }
    }

    #[test]
    fn local_only_commands_skip_server_login_warning() {
        for args in [
            vec!["openkakao-cli", "notif-watch"],
            vec!["openkakao-cli", "ax-watch"],
            vec!["openkakao-cli", "ax-read", "room"],
            vec!["openkakao-cli", "local-chats"],
            vec!["openkakao-cli", "cache-stats"],
            vec!["openkakao-cli", "doctor"],
        ] {
            let cli = Cli::try_parse_from(args).expect("local-only command should parse");
            assert!(!should_warn_about_server_login(&cli.command));
        }
    }

    #[test]
    fn server_command_keeps_server_login_warning() {
        let cli =
            Cli::try_parse_from(["openkakao-cli", "chats"]).expect("server command should parse");
        assert!(should_warn_about_server_login(&cli.command));
    }

    #[test]
    fn loco_write_disabled_by_default() {
        let config = crate::config::OpenKakaoConfig::default();
        assert!(!config.safety.allow_loco_write);
        assert!(require_loco_write(&config).is_err());
    }

    #[test]
    fn ax_send_disabled_by_default() {
        let config = crate::config::OpenKakaoConfig::default();
        assert!(!config.safety.allow_ax_send);
        assert!(require_ax_send(&config).is_err());
    }

    #[test]
    fn ax_send_enabled_when_configured() {
        let mut config = crate::config::OpenKakaoConfig::default();
        config.safety.allow_ax_send = true;
        assert!(require_ax_send(&config).is_ok());
    }

    #[test]
    fn loco_write_enabled_when_configured() {
        let mut config = crate::config::OpenKakaoConfig::default();
        config.safety.allow_loco_write = true;
        assert!(require_loco_write(&config).is_ok());
    }
}
