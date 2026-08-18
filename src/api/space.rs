//! UP space profile and submission API models.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceInfo {
    pub mid: i64,
    pub name: String,
    pub face: Option<String>,
    pub sign: Option<String>,
    pub level: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationStat {
    pub mid: i64,
    pub following: Option<i64>,
    pub follower: Option<i64>,
    /// Whether the current user follows this UP (B站 field name: attation)
    #[serde(default)]
    pub attation: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceVideoOrder {
    Latest,
    Popular,
}

impl SpaceVideoOrder {
    pub fn api_value(self) -> &'static str {
        match self {
            Self::Latest => "pubdate",
            Self::Popular => "click",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoData {
    pub list: SpaceVideoList,
    pub page: SpaceVideoPage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoList {
    #[serde(default)]
    pub vlist: Vec<SpaceVideoItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoPage {
    pub count: i64,
    pub pn: Option<i32>,
    pub ps: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoItem {
    pub aid: i64,
    pub bvid: String,
    pub title: String,
    pub pic: Option<String>,
    /// Length formatted as `mm:ss` or `h:mm:ss` (e.g. `02:43`). The space
    /// API has no numeric `duration` field; duration is usually absent.
    #[serde(default)]
    pub length: Option<String>,
    pub duration: Option<i64>,
    pub play: Option<i64>,
    pub video_review: Option<i64>,
    pub created: Option<i64>,
    pub mid: Option<i64>,
    pub author: Option<String>,
}

/// Parse a `mm:ss` / `h:mm:ss` length string into seconds.
pub fn parse_length_to_seconds(length: &str) -> Option<i64> {
    let parts: Vec<&str> = length.split(':').collect();
    let mut secs: i64 = 0;
    for part in parts {
        let n: i64 = part.trim().parse().ok()?;
        secs = secs.checked_mul(60)?.checked_add(n)?;
    }
    Some(secs)
}

// ── 合集（series / seasons）模型 ──

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesListData {
    pub items_lists: Option<SeriesItemsLists>,
    pub page: Option<SeriesListPage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesItemsLists {
    #[serde(default)]
    pub series_list: Vec<SeriesInfo>,
    #[serde(default)]
    pub seasons_list: Vec<SeriesInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesListPage {
    #[serde(rename = "page_num")]
    pub num: Option<i32>,
    #[serde(rename = "page_size")]
    pub size: Option<i32>,
    pub total: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesInfo {
    pub id: Option<i64>,
    pub meta: Option<SeriesMeta>,
    pub total: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesMeta {
    pub season_id: Option<i64>,
    #[serde(default)]
    pub series_id: Option<i64>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub total: Option<i64>,
    #[serde(default)]
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesArchivesData {
    #[serde(default)]
    pub aids: Vec<i64>,
    pub archives: Option<Vec<SeriesArchiveItem>>,
    pub meta: Option<SeriesMeta>,
    pub page: Option<SeriesArchivesPage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesArchivesPage {
    #[serde(rename = "page_num")]
    pub num: Option<i32>,
    #[serde(rename = "page_size")]
    pub size: Option<i32>,
    pub total: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesArchiveItem {
    pub aid: i64,
    pub bvid: String,
    pub title: String,
    #[serde(rename = "pic")]
    pub cover: Option<String>,
    pub duration: Option<i64>,
    pub stat: Option<SeriesArchiveStat>,
    pub mid: Option<i64>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesArchiveStat {
    pub view: Option<i64>,
    pub danmaku: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::SpaceVideoOrder;

    #[test]
    fn space_sort_matches_web_query_values() {
        assert_eq!(SpaceVideoOrder::Latest.api_value(), "pubdate");
        assert_eq!(SpaceVideoOrder::Popular.api_value(), "click");
    }
}
