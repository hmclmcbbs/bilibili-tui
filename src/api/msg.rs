//! Message notification center API
//!
//! API endpoints (require login cookie):
//! - unread:  GET /x/msgfeed/unread
//! - replies: GET /x/msgfeed/reply?type=1
//! - at:      GET /x/msgfeed/at
//! - likes:   GET /x/msgfeed/like
//! - system:  POST /x/msgfeed/notice (GET /x/msgfeed/sys is retired)
//! - sessions: GET api.vc.bilibili.com/session_svr/v1/session_svr/get_sessions (WBI)
//! - chat:     GET api.vc.bilibili.com/svr_sync/v1/svr_sync/fetch_session_msgs (WBI)
//! - send:     POST customerservice.bilibili.com/x/custom/msg_svr/v1/send_msg (WBI)

use serde::Deserialize;

/// Unified notification item for all feed types
#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub id: i64,
    /// 1=reply 2=at 3=like 6=system
    pub notif_type: i32,
    pub user_name: Option<String>,
    pub user_mid: Option<i64>,
    /// Message text (reply content / at content / like action text)
    pub message: Option<String>,
    /// Content title (video/season title or system title)
    pub title: Option<String>,
    /// Related video bvid (if any)
    pub bvid: Option<String>,
    /// Related object id (aid for video comments, oid for dynamics)
    pub oid: Option<i64>,
    /// Unix timestamp
    pub ctime: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawTalkerInfo {
    #[serde(default)]
    pub uid: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pic_url: Option<String>,
}

/// A private-message conversation (会话) with one user.
#[derive(Debug, Clone)]
pub struct ChatSession {
    pub talker_id: i64,
    pub uname: String,
    pub face: Option<String>,
    pub unread_count: i32,
    pub last_msg: Option<ChatMessage>,
}

/// A single private message.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub sender_uid: i64,
    pub receiver_id: i64,
    pub msg_type: i32,
    pub content: String,
    pub timestamp: i64,
    /// Related video bvid (video-share messages), if any.
    pub bvid: Option<String>,
    /// Jump URL from the raw content (video/live/opus share), if any.
    pub jump_url: Option<String>,
    /// Video-share metadata (msg_type 11), when present.
    pub video_title: Option<String>,
    pub video_cover: Option<String>,
    pub video_view: Option<i64>,
    pub video_danmaku: Option<i64>,
    pub video_desc: Option<String>,
}

impl ChatMessage {
    /// Short type badge shown before the text (e.g. `[视频]`).
    pub fn type_badge(&self) -> &'static str {
        match self.msg_type {
            1 => "",
            2 => "[图片] ",
            3 => "[回复] ",
            4 => "[转发] ",
            5 => "[撤回] ",
            6 => "[分享] ",
            7 => "[礼物] ",
            9 => "[直播] ",
            10 => "[通知] ",
            11 => "[视频] ",
            12 => "[表情] ",
            13 => "[音乐] ",
            14 => "[链接] ",
            15 => "[商品] ",
            20 => "[动态] ",
            21 => "[专栏] ",
            22 => "[直播] ",
            23 => "[用户] ",
            24 => "[小说] ",
            _ => "[消息] ",
        }
    }
}

impl ChatMessage {
    pub fn format_time(&self) -> String {
        if self.timestamp <= 0 {
            return String::new();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let diff = now - self.timestamp;
        if diff < 60 {
            "刚刚".to_string()
        } else if diff < 3600 {
            format!("{}分钟前", diff / 60)
        } else if diff < 86400 {
            format!("{}小时前", diff / 3600)
        } else if diff < 2592000 {
            format!("{}天前", diff / 86400)
        } else {
            format!("{}月前", diff / 2592000)
        }
    }
}

/// Session list wrapper: GET session_svr/get_sessions
#[derive(Debug, Deserialize)]
pub struct SessionListData {
    #[serde(default)]
    pub session_list: Vec<serde_json::Value>,
    #[serde(default)]
    pub has_more: Option<i32>,
}

/// Chat detail wrapper: GET svr_sync/fetch_session_msgs
#[derive(Debug, Deserialize)]
pub struct ChatDetailData {
    #[serde(default)]
    pub messages: Vec<serde_json::Value>,
    #[serde(default)]
    pub has_more: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct RawSession {
    #[serde(default)]
    pub talker_id: Option<i64>,
    /// customerservice endpoint nests the user info here.
    #[serde(default)]
    pub talker_info: Option<RawTalkerInfo>,
    #[serde(default)]
    pub unread_count: Option<i32>,
    #[serde(default)]
    pub last_msg: Option<serde_json::Value>,
    #[serde(default)]
    pub account_info: Option<RawAccountInfo>,
}

#[derive(Debug, Deserialize)]
struct RawAccountInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub face: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawChatMsg {
    #[serde(default)]
    pub sender_uid: Option<i64>,
    #[serde(default)]
    pub receiver_id: Option<i64>,
    #[serde(default)]
    pub msg_type: Option<i32>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

/// Parse a raw chat message value. The `content` field is a JSON string
/// like `{"content":"hello"}` (text), `{"title":"...","bvid":"..."}`
/// (video share) or `{"title":"...","text":"..."}` (system notice).
/// We extract human-readable text according to `msg_type`; the raw string
/// is kept as a fallback so nothing ever shows up as raw JSON.
pub fn parse_chat_message(value: &serde_json::Value) -> Option<ChatMessage> {
    let raw: RawChatMsg = serde_json::from_value(value.clone()).ok()?;
    let msg_type = raw.msg_type.unwrap_or(1);
    let raw_content = raw.content.unwrap_or_default();
    let mut text = parse_message_content(msg_type, &raw_content);
    // Final guard: no matter which msg_type branch leaked raw JSON, never
    // show it in the UI. Pull out a readable field or show a placeholder.
    if text.starts_with('{') || text.starts_with('[') {
        text = readable_or_placeholder(&text);
    }
    let (bvid, jump_url) = parse_message_links(msg_type, &raw_content);
    let video_meta = parse_video_meta(msg_type, &raw_content);
    Some(ChatMessage {
        sender_uid: raw.sender_uid.unwrap_or(0),
        receiver_id: raw.receiver_id.unwrap_or(0),
        msg_type,
        content: text,
        timestamp: raw.timestamp.unwrap_or(0),
        bvid,
        jump_url,
        video_title: video_meta.0,
        video_cover: video_meta.1,
        video_view: video_meta.2,
        video_danmaku: video_meta.3,
        video_desc: video_meta.4,
    })
}

/// Extract video-share metadata (title/cover/view/danmaku/desc) from the
/// `content` JSON string of a video-share message (msg_type 11) or a
/// reserve-release notification (msg_type 10 with a video jump_uri).
fn parse_video_meta(
    msg_type: i32,
    raw: &str,
) -> (Option<String>, Option<String>, Option<i64>, Option<i64>, Option<String>) {
    if msg_type != 11 && msg_type != 10 {
        return (None, None, None, None, None);
    }
    let obj: Option<serde_json::Value> = serde_json::from_str(raw).ok();
    let Some(obj) = obj else {
        return (None, None, None, None, None);
    };
    if msg_type == 10 {
        // Reserve-release notice: only treat as a video card when it links
        // to a video and carries a cover. Other system notices (login,
        // delivery, game beta...) stay plain text.
        let jump_uri = obj
            .get("jump_uri")
            .or_else(|| obj.get("jump_url"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !jump_uri.contains("/video/") {
            return (None, None, None, None, None);
        }
        let title = obj
            .get("modules")
            .and_then(|m| m.as_array())
            .and_then(|a| a.first())
            .and_then(|modu| modu.get("detail"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                obj.get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
        let cover = obj
            .get("biz_content")
            .and_then(|b| b.get("cover"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                obj.get("cover")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
        // UP name from the second module ("UP主") if present.
        let up = obj
            .get("modules")
            .and_then(|m| m.as_array())
            .and_then(|a| a.iter().find(|modu| {
                modu.get("title").and_then(|t| t.as_str()).unwrap_or("") == "UP主"
            }))
            .and_then(|modu| modu.get("detail"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let desc = up.map(|u| format!("UP主: {}", u));
        return (title, cover, None, None, desc);
    }
    let title = obj.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
    let cover = obj.get("cover").and_then(|v| v.as_str()).map(|s| s.to_string());
    let view = obj.get("view").and_then(|v| v.as_i64());
    let danmaku = obj.get("danmaku").and_then(|v| v.as_i64());
    let desc = obj.get("desc").and_then(|v| v.as_str()).map(|s| s.to_string());
    (title, cover, view, danmaku, desc)
}

impl ChatMessage {
    /// True when this message is a video share with a playable BV id.
    /// Reserve-release notices (msg_type 10) linking to a video with a
    /// cover are treated as video cards too.
    pub fn is_video_share(&self) -> bool {
        if self.bvid.is_none() {
            return false;
        }
        match self.msg_type {
            11 => true,
            10 => self.video_cover.is_some(),
            _ => false,
        }
    }
}

/// Compact display text used in the session list for the last message.
/// Video shares are prefixed so they don't look like the other side just
/// said the video title as plain text.
pub fn session_last_text(msg: &ChatMessage) -> String {
    if msg.is_video_share() {
        let title = msg.video_title.as_deref().unwrap_or("视频");
        format!("[视频] {}", title)
    } else {
        let c = msg.content.trim();
        // Defensive: if the content still looks like raw JSON (e.g. parsed
        // by an older binary or an unknown message shape), pull out the
        // human-readable field instead of showing `{"title":...,"text":...}`.
        if c.starts_with('{') || c.starts_with('[') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(c) {
                let readable = if c.starts_with('[') {
                    // Bilibili session list returns array-shaped system notices for
                    // conversations where the other side has not replied/followed yet,
                    // e.g. [{"Text":"对方主动回复或关注你前，最多发送1条信息",...}].
                    v.as_array()
                        .and_then(|a| a.first())
                        .and_then(|first| {
                            first
                                .get("Text")
                                .or_else(|| first.get("text"))
                                .or_else(|| first.get("content"))
                                .or_else(|| first.get("title"))
                        })
                        .and_then(|x| x.as_str())
                        .filter(|s| !s.is_empty())
                } else {
                    v.get("content")
                        .and_then(|x| x.as_str())
                        .filter(|s| !s.is_empty())
                        .or_else(|| v.get("text").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
                        .or_else(|| v.get("title").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
                };
                if let Some(s) = readable {
                    return s.to_string();
                }
            }
            // Malformed JSON: try the lenient extractor before giving up.
            if let Some(s) = extract_readable_fallback(c) {
                return s;
            }
            // Last resort: never leak raw JSON into the UI.
            return "(系统提示)".to_string();
        }
        msg.content.clone()
    }
}

/// Extract a BV id and a jump URL from a message content JSON string.
fn parse_message_links(msg_type: i32, raw: &str) -> (Option<String>, Option<String>) {
    let obj: Option<serde_json::Value> = serde_json::from_str(raw).ok();
    let Some(obj) = obj else {
        return (None, None);
    };
    let jump = obj
        .get("jump_uri")
        .or_else(|| obj.get("jump_url"))
        .or_else(|| obj.get("uri"))
        .or_else(|| obj.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let bvid = obj
        .get("bvid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            jump
                .as_deref()
                .and_then(|u| extract_bvid_from_uri(u))
                .map(|s| s.to_string())
        })
        .or_else(|| {
            // some video shares put the BV in the title field
            if msg_type == 11 {
                obj.get("title")
                    .and_then(|v| v.as_str())
                    .and_then(|t| extract_bvid_from_uri(t))
                    .map(|s| s.to_string())
            } else {
                None
            }
        });
    (bvid, jump)
}

/// Best-effort extractor for malformed message content JSON. Bilibili
/// sometimes returns hint messages whose `content` string is not strictly
/// valid JSON (e.g. a stray quote: `"信息""`), so serde fails and we would
/// otherwise show the raw JSON. This scans for `"Key":"value"` pairs and
/// returns the first readable value.
fn extract_readable_fallback(raw: &str) -> Option<String> {
    for key in [
        "\"Text\"",
        "\"text\"",
        "\"content\"",
        "\"title\"",
        "\"msg_text\"",
        "\"name\"",
    ] {
        let Some(idx) = raw.find(key) else { continue };
        let rest = &raw[idx + key.len()..];
        let Some(colon) = rest.find(':') else { continue };
        let after = rest[colon + 1..].trim_start();
        let Some(start) = after.strip_prefix('"') else { continue };
        let bytes = start.as_bytes();
        let mut end = 0;
        while end < bytes.len() {
            if bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\') {
                break;
            }
            end += 1;
        }
        if end > 0 {
            let mut v = start[..end].to_string();
            // Strip stray leading/trailing quotes from malformed values.
            if v.len() > 1 && v.starts_with('"') && v.ends_with('"') {
                v = v[1..v.len() - 1].to_string();
            }
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Extract human-readable text from the `content` JSON string according to
/// the message type. Falls back to the raw string when nothing matches.
fn parse_message_content(msg_type: i32, raw: &str) -> String {
    let obj: Option<serde_json::Value> = serde_json::from_str(raw).ok();
    if obj.is_none() {
        // Malformed JSON (Bilibili hint messages): pull out any readable value
        // instead of showing the raw JSON.
        if let Some(s) = extract_readable_fallback(raw) {
            return s;
        }
    }
    // Some system-shaped last messages arrive as an array, e.g.
    // [{"Text":"对方主动回复或关注你前，最多发送1条信息","color_day":"#9499A0",...}].
    // Pull out the readable field before dispatching on msg_type.
    if let Some(serde_json::Value::Array(items)) = &obj {
        if let Some(first) = items.first() {
            let readable = first
                .get("Text")
                .or_else(|| first.get("text"))
                .or_else(|| first.get("content"))
                .or_else(|| first.get("title"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if let Some(s) = readable {
                return s;
            }
        }
        return readable_or_placeholder(raw);
    } else if let Some(serde_json::Value::Object(map)) = &obj {
        // Same hint sometimes arrives as a single object with a
        // capitalised "Text" key: {"Text":"...","color_day":"#..."}.
        let readable = map
            .get("Text")
            .or_else(|| map.get("text"))
            .or_else(|| map.get("content"))
            .or_else(|| map.get("title"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        if let Some(s) = readable {
            return s;
        }
    }
    match msg_type {
        // plain text
        1 => obj
            .as_ref()
            .and_then(|v| v.get("content").and_then(|c| c.as_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| raw.to_string()),
        // image / sticker
        2 | 12 => {
            let text = obj
                .as_ref()
                .and_then(|v| v.get("text").and_then(|t| t.as_str()))
                .filter(|t| !t.is_empty())
                .map(|s| s.to_string());
            match text {
                Some(t) => t,
                None => "[图片]".to_string(),
            }
        }
        // reply / quote: show the quoted text if present
        3 => {
            let reply = obj
                .as_ref()
                .and_then(|v| v.get("reply_content").and_then(|c| c.as_str()));
            let mine = obj
                .as_ref()
                .and_then(|v| v.get("content").and_then(|c| c.as_str()));
            match (reply, mine) {
                (Some(r), Some(m)) if !m.is_empty() && r != m => {
                    format!("[回复] {} ｜ 回复内容: {}", m, r)
                }
                (_, Some(m)) if !m.is_empty() => format!("[回复] {}", m),
                (Some(r), _) => format!("[回复] {}", r),
                _ => raw.to_string(),
            }
        }
        // forward / share: use title or text
        4 | 6 => {
            let title = obj
                .as_ref()
                .and_then(|v| v.get("title").and_then(|t| t.as_str()));
            let text = obj
                .as_ref()
                .and_then(|v| v.get("text").and_then(|t| t.as_str()));
            match (title, text) {
                (Some(t), Some(x)) if !x.is_empty() && t != x => format!("[分享] {}: {}", t, x),
                (Some(t), _) if !t.is_empty() => format!("[分享] {}", t),
                (_, Some(x)) if !x.is_empty() => format!("[分享] {}", x),
                _ => raw.to_string(),
            }
        }
        // gift (直播礼物)
        7 => {
            let content = obj
                .as_ref()
                .and_then(|v| v.get("content").and_then(|c| c.as_str()));
            match content {
                Some(c) if !c.is_empty() => format!("[礼物] {}", c),
                _ => "[礼物]".to_string(),
            }
        }
        // live share
        9 | 22 => {
            let title = obj
                .as_ref()
                .and_then(|v| v.get("title").and_then(|t| t.as_str()));
            match title {
                Some(t) if !t.is_empty() => format!("[直播] {}", t),
                _ => "[直播]".to_string(),
            }
        }
        // system notice (登录通知、风控提醒等)
        10 => {
            let title = obj
                .as_ref()
                .and_then(|v| v.get("title").and_then(|t| t.as_str()));
            let text = obj
                .as_ref()
                .and_then(|v| v.get("text").and_then(|t| t.as_str()));
            match (title, text) {
                (Some(t), Some(x)) if !x.is_empty() => format!("[通知] {}: {}", t, x),
                (Some(t), _) if !t.is_empty() => format!("[通知] {}", t),
                _ => raw.to_string(),
            }
        }
        // video share (up主投喂/分享的视频卡片)
        11 => {
            let title = obj
                .as_ref()
                .and_then(|v| v.get("title").and_then(|t| t.as_str()));
            match title {
                Some(t) if !t.is_empty() => format!("{}", t),
                _ => raw.to_string(),
            }
        }
        // music / web page / goods / novel / user share: use title or name
        13 | 14 | 15 | 24 | 23 => {
            let title = obj
                .as_ref()
                .and_then(|v| v.get("title").and_then(|t| t.as_str()))
                .or_else(|| {
                    obj.as_ref()
                        .and_then(|v| v.get("name").and_then(|n| n.as_str()))
                });
            match title {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => raw.to_string(),
            }
        }
        // opus / dynamic share
        20 | 21 => {
            let title = obj
                .as_ref()
                .and_then(|v| v.get("title").and_then(|t| t.as_str()))
                .or_else(|| {
                    obj.as_ref()
                        .and_then(|v| v.get("text").and_then(|t| t.as_str()))
                });
            match title {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => raw.to_string(),
            }
        }
        // recalled
        5 => "[对方撤回了一条消息]".to_string(),
        _ => obj
            .as_ref()
            .and_then(|v| v.get("content").and_then(|c| c.as_str()))
            .or_else(|| {
                obj.as_ref()
                    .and_then(|v| v.get("text").and_then(|t| t.as_str()))
            })
            .or_else(|| {
                obj.as_ref()
                    .and_then(|v| v.get("Text").and_then(|t| t.as_str()))
            })
            .or_else(|| {
                obj.as_ref()
                    .and_then(|v| v.get("title").and_then(|t| t.as_str()))
            })
            .map(|s| s.to_string())
            .unwrap_or_else(|| readable_or_placeholder(raw)),
    }
}

/// Last-resort for parsed-but-unreadable content: never leak raw JSON into
/// the UI. Try the lenient extractor, then fall back to a placeholder.
fn readable_or_placeholder(raw: &str) -> String {
    if let Some(s) = extract_readable_fallback(raw) {
        return s;
    }
    "(系统提示)".to_string()
}

/// Parse a raw session value.
pub fn parse_session(value: &serde_json::Value) -> Option<ChatSession> {
    let raw: RawSession = serde_json::from_value(value.clone()).ok()?;
    let talker_id = raw.talker_id.or_else(|| raw.talker_info.as_ref().and_then(|t| t.uid))?;
    let last_msg = raw.last_msg.as_ref().and_then(parse_chat_message);
    let uname = raw
        .account_info
        .as_ref()
        .and_then(|a| a.name.clone())
        .filter(|n| !n.is_empty())
        .or_else(|| {
            raw.talker_info
                .as_ref()
                .and_then(|t| t.name.clone())
                .filter(|n| !n.is_empty())
        })
        .unwrap_or_else(|| format!("用户{}", talker_id));
    let face = raw
        .account_info
        .and_then(|a| a.face)
        .or_else(|| raw.talker_info.as_ref().and_then(|t| t.pic_url.clone()));
    Some(ChatSession {
        talker_id,
        uname,
        face,
        unread_count: raw.unread_count.unwrap_or(0),
        last_msg,
    })
}

impl NotificationItem {
    pub fn format_time(&self) -> String {
        if let Some(ctime) = self.ctime {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let diff = now - ctime;
            if diff < 60 {
                "刚刚".to_string()
            } else if diff < 3600 {
                format!("{}分钟前", diff / 60)
            } else if diff < 86400 {
                format!("{}小时前", diff / 3600)
            } else if diff < 2592000 {
                format!("{}天前", diff / 86400)
            } else {
                format!("{}月前", diff / 2592000)
            }
        } else {
            String::new()
        }
    }
}

/// Feed data wrapper
#[derive(Debug, Deserialize)]
pub struct FeedData {
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
    #[serde(default)]
    pub page: Option<FeedPage>,
}

/// Like-feed wrapper: the like API returns `data.latest` (newest) and
/// `data.total` (full history) instead of a flat `items` array.
#[derive(Debug, Deserialize)]
pub struct LikeFeedData {
    #[serde(default)]
    pub latest: Option<LikeFeedSection>,
    #[serde(default)]
    pub total: Option<LikeFeedSection>,
}

#[derive(Debug, Deserialize)]
pub struct LikeFeedSection {
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
}

/// System notification wrapper: returned by
/// `message.bilibili.com/x/sys-msg/query_unified_notify`.
#[derive(Debug, Deserialize)]
pub struct SystemNotifyData {
    #[serde(default)]
    pub system_notify_list: Vec<serde_json::Value>,
}

/// Raw system notification item shape.
#[derive(Debug, Deserialize)]
struct RawSystemNotify {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    /// JSON string; the actual text lives under the `web` key.
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub source: Option<RawSystemSource>,
    #[serde(default)]
    pub time_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSystemSource {
    #[serde(default)]
    pub name: Option<String>,
}

/// Parse a raw system-notify value into a unified notification item.
pub fn parse_system_notify(value: &serde_json::Value) -> Option<NotificationItem> {
    let entry: RawSystemNotify = serde_json::from_value(value.clone()).ok()?;
    let message = entry
        .content
        .as_deref()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("web").and_then(|w| w.as_str()).map(|s| s.to_string()))
        .or(entry.content.clone());
    let ctime = entry
        .time_at
        .as_deref()
        .and_then(|t| {
            // "2026-03-16 19:00:00" -> unix seconds (Beijing time assumed local)
            let s = t.trim();
            let mut parts = s.split(['-', ' ', ':']).filter_map(|p| p.parse::<i64>().ok());
            let (y, mo, d, h, mi, sec) = (
                parts.next()?,
                parts.next()?,
                parts.next()?,
                parts.next().unwrap_or(0),
                parts.next().unwrap_or(0),
                parts.next().unwrap_or(0),
            );
            Some(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec - 8 * 3600)
        });
    Some(NotificationItem {
        id: entry.id.unwrap_or(0),
        notif_type: 6,
        user_name: entry
            .source
            .and_then(|s| s.name)
            .filter(|n| !n.is_empty())
            .or_else(|| Some("系统通知".to_string())),
        user_mid: None,
        message,
        title: entry.title,
        bvid: None,
        oid: None,
        ctime,
    })
}

/// Days from civil date (Howard Hinnant's algorithm), proleptic Gregorian.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[derive(Debug, Deserialize)]
pub struct FeedPage {
    #[serde(default)]
    pub next: Option<i32>,
}

/// Unread count response
#[derive(Debug, Deserialize)]
pub struct UnreadData {
    #[serde(default)]
    pub at: i32,
    #[serde(default)]
    pub chat: i32,
    #[serde(default)]
    pub like: i32,
    #[serde(default)]
    pub reply: i32,
    #[serde(default)]
    pub sys_msg: i32,
}

// ─── Raw item shapes from the API ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawUser {
    #[serde(default)]
    pub mid: Option<i64>,
    #[serde(default)]
    pub uname: Option<String>,
    /// Actual field name returned by the msgfeed API.
    #[serde(default)]
    pub nickname: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawContent {
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawReply {
    #[serde(default)]
    pub rpid: Option<i64>,
    #[serde(default)]
    pub oid: Option<i64>,
    #[serde(default)]
    pub oid_type: Option<i64>,
    #[serde(default)]
    pub ctime: Option<i64>,
    #[serde(default)]
    pub content: Option<RawContent>,
}

#[derive(Debug, Deserialize)]
struct RawItem {
    #[serde(default)]
    pub bvid: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
    /// @-mention message text lives here for the at feed.
    #[serde(default)]
    pub source_content: Option<String>,
    /// Like feed carries the video link here; we extract the bvid.
    #[serde(default)]
    pub uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLike {
    #[serde(default)]
    pub like_time: Option<i64>,
    #[serde(default)]
    pub r#type: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub r#type: Option<i64>,
    #[serde(default)]
    pub user: Option<RawUser>,
    /// Like feed wraps users in an array (can be multiple).
    #[serde(default)]
    pub users: Vec<RawUser>,
    #[serde(default)]
    pub reply: Option<RawReply>,
    #[serde(default)]
    pub item: Option<RawItem>,
    #[serde(default)]
    pub like: Option<RawLike>,
    #[serde(default)]
    pub at_time: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub time: Option<i64>,
}

/// Parse a raw feed value into a unified notification item.
/// `feed_type` is the source tab: 1=reply 2=at 3=like 6=system.
pub fn parse_feed_item(value: &serde_json::Value, feed_type: i32) -> Option<NotificationItem> {
    let entry: RawEntry = serde_json::from_value(value.clone()).ok()?;
    let id = entry.id.unwrap_or(0);
    // Prefer `user.nickname` (actual API field), fall back to `user.uname`,
    // then to the first entry of `users` (like feed).
    let user = entry.user.as_ref().or_else(|| entry.users.first());
    let user_name = user
        .and_then(|u| u.nickname.clone().or_else(|| u.uname.clone()));
    let user_mid = user.and_then(|u| u.mid);
    let message = entry
        .reply
        .as_ref()
        .and_then(|r| r.content.as_ref())
        .and_then(|c| c.message.clone())
        .or_else(|| entry.item.as_ref().and_then(|i| i.source_content.clone()))
        .or(entry.content.clone());
    let title = entry
        .item
        .as_ref()
        .and_then(|i| i.title.clone())
        .or(entry.title.clone());
    let bvid = entry
        .item
        .as_ref()
        .and_then(|i| i.bvid.clone())
        .or_else(|| {
            entry
                .item
                .as_ref()
                .and_then(|i| i.uri.clone())
                .and_then(|uri| extract_bvid_from_uri(&uri))
        });
    let oid = entry.reply.as_ref().and_then(|r| r.oid);
    let ctime = entry
        .reply
        .as_ref()
        .and_then(|r| r.ctime)
        .or(entry.at_time)
        .or(entry.like.as_ref().and_then(|l| l.like_time))
        .or(entry.time);

    Some(NotificationItem {
        id,
        notif_type: feed_type,
        user_name,
        user_mid,
        message,
        title,
        bvid,
        oid,
        ctime,
    })
}

/// Pull a BV id out of a bilibili uri like
/// `https://www.bilibili.com/video/BV12T4y1u7my?dm_progress=...`.
fn extract_bvid_from_uri(uri: &str) -> Option<String> {
    let idx = uri.find("/video/")?;
    let rest = &uri[idx + "/video/".len()..];
    // BV ids only contain alphanumerics; stop at the first non-alnum
    // character (query separator, trailing slash...). If the URI ends
    // right after the id there is no separator and the whole rest is the id.
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    let bvid = &rest[..end];
    if bvid.starts_with("BV") && bvid.len() >= 10 {
        Some(bvid.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_share_metadata() {
        let raw = r#"{"title":"谎言成真？我要开始演出了","times":1064,"cover":"http://i2.hdslb.com/bfs/archive/x.jpg","rid":117036147609930,"type_":8,"desc":"","bvid":"BV1ssuw6TE3t","view":674306,"danmaku":1706,"pub_date":1785837600,"attach_msg":null}"#;
        let msg = parse_chat_message(&serde_json::json!({
            "sender_uid": 654552,
            "receiver_id": 646812016,
            "msg_type": 11,
            "content": raw,
            "timestamp": 1785837676,
        }))
        .expect("message parses");
        assert!(msg.is_video_share());
        assert_eq!(msg.bvid.as_deref(), Some("BV1ssuw6TE3t"));
        assert_eq!(msg.video_title.as_deref(), Some("谎言成真？我要开始演出了"));
        assert_eq!(msg.video_view, Some(674306));
        assert_eq!(msg.video_danmaku, Some(1706));
        assert_eq!(session_last_text(&msg), "[视频] 谎言成真？我要开始演出了");
    }

    #[test]
    fn plain_text_message_has_no_video_meta() {
        let msg = parse_chat_message(&serde_json::json!({
            "sender_uid": 654552,
            "receiver_id": 646812016,
            "msg_type": 1,
            "content": r#"{"content":"hello"}"#,
            "timestamp": 1785837676,
        }))
        .expect("message parses");
        assert!(!msg.is_video_share());
        assert_eq!(msg.content, "hello");
        assert_eq!(session_last_text(&msg), "hello");
    }

    #[test]
    fn reserve_release_notice_is_video_card() {
        // msg_type 10 "视频上线提醒" linking to a video with biz_content.cover.
        let raw = r#"{"title":"视频上线提醒","text":"你预约的视频已上线，快来看看吧~","jump_text":"观看视频","jump_uri":"https://www.bilibili.com/video/BV1akMq6mEos","modules":[{"title":"视频主题","detail":"Bili_Board术力口周榜第113期2026年8月5日第31周"},{"title":"UP主","detail":"Bili-Board_Atel"}],"jump_text_2":"","jump_uri_2":"","jump_text_3":"","jump_uri_3":"","notifier":null,"jump_uri_config":{"all_uri":"https://www.bilibili.com/video/BV1akMq6mEos","harmony_uri":"https://www.bilibili.com/video/BV1akMq6mEos","text":"观看视频"},"jump_uri_2_config":{"harmony_uri":"","text":""},"jump_uri_3_config":{"harmony_uri":"","text":""},"biz_content":{"cover":"http://i2.hdslb.com/bfs/archive/def2270954c6c9dc518150417252d9ac044fb1c9.jpg","backup_cover":"http://i2.hdslb.com/bfs/archive/def2270954c6c9dc518150417252d9ac044fb1c9.jpg","refresh_type":1,"biz_type":1,"biz_id1":"117038479579761","biz_id2":"","biz_status":0}}"#;
        let msg = parse_chat_message(&serde_json::json!({
            "sender_uid": 3546800279522160_i64,
            "receiver_id": 646812016_i64,
            "msg_type": 10,
            "content": raw,
            "timestamp": 1783679401,
        }))
        .expect("message parses");
        assert!(msg.is_video_share(), "reserve notice should be a video card");
        assert_eq!(msg.bvid.as_deref(), Some("BV1akMq6mEos"));
        assert_eq!(
            msg.video_title.as_deref(),
            Some("Bili_Board术力口周榜第113期2026年8月5日第31周")
        );
        assert!(msg.video_cover.is_some());
        assert_eq!(session_last_text(&msg), "[视频] Bili_Board术力口周榜第113期2026年8月5日第31周");
    }

    #[test]
    fn non_video_system_notice_stays_plain() {
        // Login notice: jump_uri points to account pages, not a video.
        let raw = r#"{"title":"登录操作通知","text":"你的账号在新设备或平台登录成功","jump_text":"查看详情","jump_uri":"https://account.bilibili.com/h5/account-h5/notice/notice-login?mid=646812016","modules":[{"title":"设备/平台","detail":"Chrome浏览器"}],"jump_text_2":"","jump_uri_2":"","jump_text_3":"","jump_uri_3":"","notifier":null,"jump_uri_config":{"harmony_uri":"","text":""},"jump_uri_2_config":{"harmony_uri":"","text":""},"jump_uri_3_config":{"harmony_uri":"","text":""}}"#;
        let msg = parse_chat_message(&serde_json::json!({
            "sender_uid": 12076317,
            "receiver_id": 646812016,
            "msg_type": 10,
            "content": raw,
            "timestamp": 1783679401,
        }))
        .expect("message parses");
        assert!(!msg.is_video_share(), "non-video notice stays plain text");
        assert!(msg.bvid.is_none());
    }

    #[test]
    fn malformed_array_hint_message_extracts_text() {
        // The exact shape the user reported: an array-shaped system hint whose
        // content string is not strictly valid JSON (stray quote after 信息).
        let raw = r##"[{"Text":"对方主动回复或关注你前，最多发送1条信息"",color_day":"#9499A0","color_nig":"#9499A0"}]"##;
        let msg = parse_chat_message(&serde_json::json!({
            "sender_uid": 0,
            "receiver_id": 646812016,
            "msg_type": 1,
            "content": raw,
            "timestamp": 1783679401,
        }))
        .expect("message parses");
        assert_eq!(msg.content, "对方主动回复或关注你前，最多发送1条信息");
        assert_eq!(session_last_text(&msg), "对方主动回复或关注你前，最多发送1条信息");
    }
}
