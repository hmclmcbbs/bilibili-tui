//! Bangumi (番剧/影视) API types

use serde::Deserialize;

// ─── Timeline ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TimelineDay {
    pub date: String,
    #[serde(rename = "date_ts")]
    pub date_ts: i64,
    #[serde(rename = "day_of_week")]
    pub day_of_week: i32,
    pub episodes: Vec<TimelineEpisode>,
    #[serde(rename = "is_today")]
    pub is_today: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimelineEpisode {
    pub cover: String,
    pub delay: i32,
    #[serde(rename = "ep_cover")]
    pub ep_cover: String,
    #[serde(rename = "episode_id")]
    pub episode_id: i64,
    pub follows: String,
    pub plays: String,
    #[serde(rename = "pub_index")]
    pub pub_index: String,
    #[serde(rename = "pub_time")]
    pub pub_time: String,
    #[serde(rename = "pub_ts")]
    pub pub_ts: i64,
    pub published: i32,
    #[serde(rename = "season_id")]
    pub season_id: i64,
    #[serde(rename = "square_cover")]
    pub square_cover: String,
    pub title: String,
}

impl TimelineEpisode {
    pub fn cover_url(&self) -> String {
        let url = if self.ep_cover.is_empty() {
            &self.cover
        } else {
            &self.ep_cover
        };
        if url.starts_with("//") {
            format!("https:{}", url)
        } else {
            url.clone()
        }
    }
}

// ─── Season Rank (replaces index) ───────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonRankItem {
    pub badge: String,
    pub cover: String,
    #[serde(rename = "new_ep")]
    pub new_ep: Option<NewEp>,
    #[serde(rename = "season_id")]
    pub season_id: i64,
    pub title: String,
    pub rating: Option<String>,
    pub stat: Option<RankStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewEp {
    #[serde(rename = "index_show")]
    pub index_show: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankStat {
    pub view: Option<i64>,
}

impl SeasonRankItem {
    pub fn cover_url(&self) -> String {
        if self.cover.starts_with("//") {
            format!("https:{}", self.cover)
        } else {
            self.cover.clone()
        }
    }

    pub fn display_title(&self) -> String {
        self.title.clone()
    }

    pub fn display_subtitle(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref ep) = self.new_ep
            && !ep.index_show.is_empty()
        {
            parts.push(ep.index_show.clone());
        }
        if let Some(ref stat) = self.stat
            && let Some(view) = stat.view
        {
            parts.push(format_views(view));
        }
        if parts.is_empty() {
            "-".to_string()
        } else {
            parts.join(" · ")
        }
    }

    pub fn badge_text(&self) -> Option<&str> {
        if self.badge.is_empty() {
            None
        } else {
            Some(&self.badge)
        }
    }

    pub fn score_text(&self) -> String {
        self.rating.clone().unwrap_or_default()
    }
}

fn format_views(view: i64) -> String {
    if view >= 100000000 {
        format!("{:.1}亿", view as f64 / 100000000.0)
    } else if view >= 10000 {
        format!("{:.1}万", view as f64 / 10000.0)
    } else {
        view.to_string()
    }
}

// ─── Season Detail ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SeasonResult {
    pub title: String,
    #[serde(rename = "season_id")]
    pub season_id: i64,
    pub cover: String,
    #[serde(rename = "square_cover")]
    pub square_cover: String,
    pub evaluate: Option<String>,
    pub link: Option<String>,
    pub episodes: Option<Vec<BangumiEpisode>>,
    pub section: Option<Vec<EpisodeSection>>,
    pub rating: Option<SeasonRating>,
    pub stat: Option<SeasonStat>,
    pub badge: Option<String>,
    #[serde(rename = "is_finish")]
    pub is_finish: Option<i32>,
    #[serde(rename = "index_show")]
    pub index_show: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SeasonRating {
    pub score: f32,
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct SeasonStat {
    pub views: Option<i64>,
    pub danmakus: Option<i64>,
    #[serde(rename = "favorite")]
    pub favorites: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpisodeSection {
    pub episodes: Vec<BangumiEpisode>,
    pub id: i64,
    pub title: String,
    #[serde(rename = "type")]
    pub section_type: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BangumiEpisode {
    pub aid: i64,
    #[serde(default)]
    pub badge: String,
    pub cid: i64,
    /// Episode duration in milliseconds (0 when the API omits it).
    #[serde(default)]
    pub duration: i64,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub from: String,
    pub id: i64,
    #[serde(rename = "is_premiere", default)]
    pub is_premiere: i32,
    #[serde(rename = "long_title", default)]
    pub long_title: String,
    #[serde(rename = "share_url", default)]
    pub share_url: String,
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub vid: String,
}

impl BangumiEpisode {
    pub fn cover_url(&self) -> String {
        if self.cover.starts_with("//") {
            format!("https:{}", self.cover)
        } else {
            self.cover.clone()
        }
    }

    pub fn display_title(&self) -> String {
        if self.long_title.is_empty() {
            format!("第{}集", self.title)
        } else {
            format!("{} {}", self.title, self.long_title)
        }
    }

    pub fn badge_text(&self) -> Option<&str> {
        if self.badge.is_empty() {
            None
        } else {
            Some(&self.badge)
        }
    }
}

impl SeasonResult {
    pub fn cover_url(&self) -> String {
        let url = if self.cover.is_empty() {
            self.square_cover.as_str()
        } else {
            &self.cover
        };
        if url.starts_with("//") {
            format!("https:{}", url)
        } else {
            url.to_string()
        }
    }

    pub fn all_sections(&self) -> Vec<EpisodeSection> {
        let mut sections = Vec::new();
        // Modern API puts main episodes directly in `result.episodes`
        if let Some(ref eps) = self.episodes
            && !eps.is_empty()
        {
            sections.push(EpisodeSection {
                episodes: eps.clone(),
                id: 0,
                title: "正片".to_string(),
                section_type: 0,
            });
        }
        // Legacy `main_section` is no longer present in new API
        if let Some(ref secs) = self.section {
            for s in secs {
                if !s.episodes.is_empty() {
                    sections.push(s.clone());
                }
            }
        }
        sections
    }

    pub fn total_episodes(&self) -> usize {
        self.all_sections().iter().map(|s| s.episodes.len()).sum()
    }
}
