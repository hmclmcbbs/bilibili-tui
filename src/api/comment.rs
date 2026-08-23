//! Comment API types and functions

use serde::Deserialize;

/// Comment list response
#[derive(Debug, Deserialize)]
pub struct CommentData {
    pub page: Option<CommentPage>,
    pub replies: Option<Vec<CommentItem>>,
    pub hots: Option<Vec<CommentItem>>,
}

#[derive(Debug, Deserialize)]
pub struct CommentPage {
    pub num: Option<i32>,
    pub size: Option<i32>,
    pub count: Option<i32>,
    pub acount: Option<i32>,
}

/// Individual comment item
#[derive(Debug, Clone, Deserialize)]
pub struct CommentItem {
    pub rpid: i64,
    pub oid: i64,
    pub mid: i64,
    pub parent: i64,
    pub count: Option<i32>,
    pub rcount: Option<i32>,
    pub floor: Option<i32>,
    pub ctime: Option<i64>,
    pub like: Option<i32>,
    pub member: Option<CommentMember>,
    pub content: Option<CommentContent>,
    pub replies: Option<Vec<CommentItem>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommentMember {
    pub mid: Option<String>,
    pub uname: Option<String>,
    pub avatar: Option<String>,
    pub level_info: Option<LevelInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelInfo {
    pub current_level: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommentContent {
    pub message: Option<String>,
    /// Inline pictures attached to the comment (B站: content.pictures[].img_src).
    #[serde(default)]
    pub pictures: Option<Vec<CommentPicture>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommentPicture {
    #[serde(default)]
    pub img_src: Option<String>,
    #[serde(default)]
    pub img_width: Option<i64>,
    #[serde(default)]
    pub img_height: Option<i64>,
}

impl CommentItem {
    pub fn author_name(&self) -> &str {
        self.member
            .as_ref()
            .and_then(|m| m.uname.as_deref())
            .unwrap_or("匿名")
    }

    pub fn message(&self) -> &str {
        self.content
            .as_ref()
            .and_then(|c| c.message.as_deref())
            .unwrap_or("")
    }

    /// URLs of pictures attached to the comment, if any.
    pub fn pictures(&self) -> Vec<String> {
        self.content
            .as_ref()
            .and_then(|c| c.pictures.as_ref())
            .map(|ps| {
                ps.iter()
                    .filter_map(|p| p.img_src.clone())
                    .filter(|u| !u.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn format_like(&self) -> String {
        match self.like {
            Some(n) if n >= 10000 => format!("{:.1}万", n as f64 / 10000.0),
            Some(n) => format!("{}", n),
            None => "-".to_string(),
        }
    }

    pub fn format_time(&self) -> String {
        if let Some(ctime) = self.ctime {
            // Convert timestamp to relative time
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
            "".to_string()
        }
    }

    pub fn reply_count(&self) -> i32 {
        self.rcount.unwrap_or(0)
    }
}

/// Comment type enum for different content types
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum CommentType {
    /// 视频 (Video)
    Video = 1,
    /// 话题 (Topic)
    Topic = 6,
    /// 活动 (Activity)
    Activity = 10,
    /// 相簿/图片动态 (Photo Album)
    Album = 11,
    /// 专栏 (Article)
    Article = 12,
    /// 音频 (Audio)
    Audio = 14,
    /// 动态（纯文字 & 分享）
    Dynamic = 17,
    /// 合辑 (Playlist)
    Playlist = 19,
    /// 课程 (Course)
    Course = 33,
}

#[allow(dead_code)]
impl CommentType {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }
}

/// Response for adding a comment
#[derive(Debug, Deserialize)]
pub struct AddCommentResponse {
    pub success_action: Option<i32>,
    pub success_toast: Option<String>,
    pub need_captcha: Option<bool>,
    pub rpid: Option<i64>,
    pub rpid_str: Option<String>,
    pub root: Option<i64>,
    pub parent: Option<i64>,
    pub reply: Option<CommentItem>,
}
