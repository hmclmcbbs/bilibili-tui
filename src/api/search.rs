//! Search API types and functions

use serde::Deserialize;

/// Search result for video type
#[derive(Debug, Deserialize)]
pub struct SearchData {
    pub result: Option<Vec<SearchVideoItem>>,
    #[serde(rename = "numResults")]
    pub num_results: Option<i32>,
    pub page: Option<i32>,
    pub pagesize: Option<i32>,
}

/// Individual video search result
#[derive(Debug, Clone, Deserialize)]
pub struct SearchVideoItem {
    pub aid: Option<i64>,
    pub bvid: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub pic: Option<String>,
    pub play: Option<i64>,
    pub duration: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "danmaku")]
    pub danmaku: Option<i64>,
    pub mid: Option<i64>,
}

impl SearchVideoItem {
    pub fn display_title(&self) -> String {
        // Remove HTML tags like <em class="keyword">
        self.title
            .as_deref()
            .unwrap_or("无标题")
            .replace("<em class=\"keyword\">", "")
            .replace("</em>", "")
    }

    pub fn author_name(&self) -> &str {
        self.author.as_deref().unwrap_or("未知")
    }

    pub fn format_play(&self) -> String {
        match self.play {
            Some(n) if n >= 10000 => format!("{:.1}万", n as f64 / 10000.0),
            Some(n) => format!("{}", n),
            None => "-".to_string(),
        }
    }

    pub fn cover_url(&self) -> Option<String> {
        self.pic.as_ref().map(|url| {
            if url.starts_with("//") {
                format!("https:{}", url)
            } else {
                url.clone()
            }
        })
    }
}

/// Search result for a user (UP主)
#[derive(Debug, Clone, Deserialize)]
pub struct SearchUserItem {
    pub mid: Option<i64>,
    pub uname: Option<String>,
    #[serde(rename = "usign")]
    pub sign: Option<String>,
    pub fans: Option<i64>,
    pub videos: Option<i64>,
    #[serde(rename = "upic")]
    pub face: Option<String>,
    pub level: Option<i64>,
}

impl SearchUserItem {
    /// Display name with HTML tags stripped
    pub fn display_name(&self) -> String {
        self.uname
            .as_deref()
            .unwrap_or("未知UP主")
            .replace("<em class=\"keyword\">", "")
            .replace("</em>", "")
    }

    pub fn sign_text(&self) -> String {
        self.sign
            .as_deref()
            .unwrap_or("这个人很懒，什么都没写")
            .replace("<em class=\"keyword\">", "")
            .replace("</em>", "")
    }

    pub fn format_fans(&self) -> String {
        match self.fans {
            Some(n) if n >= 10000 => format!("{:.1}万", n as f64 / 10000.0),
            Some(n) => format!("{}", n),
            None => "-".to_string(),
        }
    }

    pub fn format_videos(&self) -> String {
        match self.videos {
            Some(n) => format!("{}", n),
            None => "-".to_string(),
        }
    }

    pub fn face_url(&self) -> Option<String> {
        self.face.as_ref().map(|url| {
            if url.starts_with("//") {
                format!("https:{}", url)
            } else {
                url.clone()
            }
        })
    }
}

/// Response for user type search
#[derive(Debug, Deserialize)]
pub struct SearchUserData {
    pub result: Option<Vec<SearchUserItem>>,
    #[serde(rename = "numResults")]
    pub num_results: Option<i32>,
    pub page: Option<i32>,
    pub pagesize: Option<i32>,
}

/// Search result for a bangumi/season (media_bangumi)
#[derive(Debug, Clone, Deserialize)]
pub struct SearchBangumiItem {
    #[serde(rename = "season_id")]
    pub season_id: Option<i64>,
    pub title: Option<String>,
    pub cover: Option<String>,
    /// Can be a plain string ("9.7") or an object ({"user_score": {...}})
    pub score: Option<serde_json::Value>,
    /// Bilibili returns `areas`/`styles` as an array in most responses but as
    /// a plain string in some (e.g. "澳大利亚"). Keep as Value so either shape
    /// deserializes without failing the whole search request.
    #[serde(default)]
    pub areas: Option<serde_json::Value>,
    #[serde(default)]
    pub styles: Option<serde_json::Value>,
    pub desc: Option<String>,
    pub pubdate: Option<i64>,
    #[serde(rename = "media_type")]
    pub media_type: Option<i64>,
}

impl SearchBangumiItem {
    /// Strip HTML em tags from title
    pub fn display_title(&self) -> String {
        self.title
            .as_deref()
            .unwrap_or("无标题")
            .replace("<em class=\"keyword\">", "")
            .replace("</em>", "")
    }

    pub fn display_subtitle(&self) -> String {
        let mut parts = Vec::new();
        self.push_name_list(&mut parts, self.areas.as_ref());
        self.push_name_list(&mut parts, self.styles.as_ref());
        if parts.is_empty() {
            "-".to_string()
        } else {
            parts.join(" · ")
        }
    }

    /// Append names from a Value that may be a string ("澳大利亚") or an
    /// array of objects ([{"name":"日本"}]).
    fn push_name_list(&self, parts: &mut Vec<String>, value: Option<&serde_json::Value>) {
        let Some(value) = value else { return };
        match value {
            serde_json::Value::String(s) if !s.is_empty() => parts.push(s.clone()),
            serde_json::Value::Array(arr) => {
                let names: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                    .collect();
                if !names.is_empty() {
                    parts.push(names.join("/"));
                }
            }
            _ => {}
        }
    }

    pub fn score_text(&self) -> String {
        match &self.score {
            Some(serde_json::Value::String(s)) if !s.is_empty() => format!("{}分", s),
            Some(serde_json::Value::Object(obj)) => {
                if let Some(us) = obj.get("user_score") {
                    if let Some(score) = us.get("score") {
                        if let Some(s) = score.as_str() {
                            return format!("{}分", s);
                        }
                    }
                }
                String::new()
            }
            _ => String::new(),
        }
    }

    pub fn badge_text(&self) -> Option<String> {
        match &self.score {
            Some(serde_json::Value::String(s)) if !s.is_empty() => Some(format!("评分 {}", s)),
            Some(serde_json::Value::Object(obj)) => {
                if let Some(us) = obj.get("user_score") {
                    if let Some(score) = us.get("score") {
                        if let Some(s) = score.as_str() {
                            return Some(format!("评分 {}", s));
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn cover_url(&self) -> Option<String> {
        self.cover.as_ref().map(|url| {
            if url.starts_with("//") {
                format!("https:{}", url)
            } else {
                url.clone()
            }
        })
    }

    pub fn description(&self) -> String {
        self.desc.clone().unwrap_or_default()
    }
}

/// Hot search item from web endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct HotwordItem {
    pub keyword: Option<String>,
    pub show_name: Option<String>,
    pub icon: Option<String>,
    pub pos: Option<i32>,
    pub word_type: Option<i32>,
}

impl HotwordItem {
    /// Display name prefers show_name over keyword
    pub fn display_text(&self) -> String {
        self.show_name
            .as_ref()
            .or(self.keyword.as_ref())
            .cloned()
            .unwrap_or_else(|| "-".to_string())
    }

    /// Keyword to trigger search
    pub fn keyword_text(&self) -> Option<String> {
        self.keyword
            .clone()
            .or_else(|| self.show_name.clone())
            .filter(|s| !s.is_empty())
    }

    /// Optional badge based on word_type
    pub fn badge(&self) -> Option<&'static str> {
        match self.word_type.unwrap_or_default() {
            4 => Some("新"),
            5 => Some("热"),
            7 => Some("直播"),
            9 => Some("梗"),
            11 => Some("话题"),
            12 => Some("独家"),
            _ => None,
        }
    }
}

/// Response for hot search list (web)
#[derive(Debug, Deserialize)]
pub struct HotwordResponse {
    pub code: Option<i32>,
    pub message: Option<String>,
    pub list: Option<Vec<HotwordItem>>, // Top 10 hot words
}
