use super::{Component, Theme, VideoCard, VideoCardGrid, shortcut_footer};
use crate::api::{
    favorite::{FavoriteFolder, FavoriteOrder, FavoriteResourceData},
    space::{
        RelationStat, SeriesArchivesData, SeriesInfo, SpaceInfo, SpaceVideoData, SpaceVideoOrder,
        parse_length_to_seconds,
    },
};
use crate::application::AppAction;
use crate::domain::playback::PlayOrder;
use crate::storage::Keybindings;
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpTab {
    Videos,
    Favorites,
    Collections,
}

pub struct UpPage {
    pub mid: i64,
    pub profile: Option<SpaceInfo>,
    pub relation: Option<RelationStat>,
    pub tab: UpTab,
    pub video_order: SpaceVideoOrder,
    pub play_order: PlayOrder,
    pub videos: VideoCardGrid,
    pub video_page: i32,
    pub video_total: i64,
    pub folders: Vec<FavoriteFolder>,
    pub folder_selected: usize,
    pub active_folder: Option<i64>,
    pub pending_folder: Option<i64>,
    pub favorite_videos: VideoCardGrid,
    pub favorite_page: i32,
    pub favorite_order: FavoriteOrder,
    pub favorite_has_more: bool,
    // ── 合集状态 ──
    pub series_list: Vec<SeriesInfo>,
    pub series_cards: VideoCardGrid,
    pub series_selected: usize,
    pub series_list_loaded: bool,
    pub pending_series: Option<i64>,
    pub active_series: Option<i64>,
    pub series_videos: VideoCardGrid,
    pub series_page: i32,
    pub series_has_more: bool,
    pub loading: bool,
    pub loading_more: bool,
    pub error: Option<String>,
    /// Whether the current user follows this uploader. None = unknown.
    pub is_followed: Option<bool>,
    /// One-line follow feedback message.
    pub follow_msg: Option<String>,
    /// When the follow feedback was set, for auto-clear.
    pub follow_msg_set_at: Option<std::time::Instant>,
}

impl UpPage {
    pub fn new(mid: i64) -> Self {
        Self {
            mid,
            profile: None,
            relation: None,
            tab: UpTab::Videos,
            video_order: SpaceVideoOrder::Latest,
            play_order: PlayOrder::Forward,
            videos: VideoCardGrid::new(),
            video_page: 1,
            video_total: 0,
            folders: Vec::new(),
            folder_selected: 0,
            active_folder: None,
            pending_folder: None,
            favorite_videos: VideoCardGrid::new(),
            favorite_page: 1,
            favorite_order: FavoriteOrder::RecentlyFavorited,
            favorite_has_more: false,
            series_list: Vec::new(),
            series_cards: VideoCardGrid::new(),
            series_selected: 0,
            series_list_loaded: false,
            pending_series: None,
            active_series: None,
            series_videos: VideoCardGrid::new(),
            series_page: 1,
            series_has_more: false,
            loading: true,
            loading_more: false,
            error: None,
            is_followed: None,
            follow_msg: None,
            follow_msg_set_at: None,
        }
    }

    pub fn new_favorites(mid: i64) -> Self {
        let mut page = Self::new(mid);
        page.tab = UpTab::Favorites;
        page
    }

    pub fn apply_initial(
        &mut self,
        profile: SpaceInfo,
        relation: Option<RelationStat>,
        is_followed: Option<bool>,
        videos: SpaceVideoData,
        folders: Vec<FavoriteFolder>,
    ) {
        self.profile = Some(profile);
        self.relation = relation;
        self.is_followed = is_followed;
        self.folders = folders;
        self.videos.clear();
        self.video_page = 1;
        self.video_total = videos.page.count;
        self.append_space_videos(videos);
        self.loading = false;
        self.error = None;
    }

    pub fn apply_more_videos(&mut self, page: i32, videos: SpaceVideoData) {
        self.video_page = page;
        self.video_total = videos.page.count;
        self.append_space_videos(videos);
        self.loading_more = false;
    }

    fn append_space_videos(&mut self, videos: SpaceVideoData) {
        for video in videos.list.vlist {
            let duration = video
                .length
                .as_deref()
                .and_then(parse_length_to_seconds)
                .or(video.duration)
                .map(format_duration)
                .unwrap_or_else(|| "--:--".to_string());
            let views = format_count(video.play.unwrap_or_default());
            let card = VideoCard::new(
                Some(video.bvid),
                Some(video.aid),
                video.title,
                video
                    .author
                    .or_else(|| self.profile.as_ref().map(|p| p.name.clone()))
                    .unwrap_or_else(|| "未知UP".to_string()),
                views,
                duration,
                video.pic,
            )
            .with_uploader_mid(Some(video.mid.unwrap_or(self.mid)));
            self.videos.add_card(card);
        }
    }

    pub fn apply_favorite_resources(
        &mut self,
        media_id: i64,
        page: i32,
        resources: FavoriteResourceData,
    ) {
        if page == 1 || self.active_folder != Some(media_id) {
            self.favorite_videos.clear();
        }
        self.active_folder = Some(media_id);
        self.pending_folder = None;
        self.favorite_page = page;
        self.favorite_has_more = resources.has_more.unwrap_or(false);
        for media in resources.medias {
            let Some(bvid) = media.bvid else { continue };
            let upper_name = media
                .upper
                .as_ref()
                .map(|upper| upper.name.clone())
                .unwrap_or_else(|| "未知UP".to_string());
            let upper_mid = media.upper.as_ref().map(|upper| upper.mid);
            let views = media
                .cnt_info
                .as_ref()
                .and_then(|count| count.play)
                .map(format_count)
                .unwrap_or_else(|| "-".to_string());
            let card = VideoCard::new(
                Some(bvid),
                Some(media.id),
                media.title,
                upper_name,
                views,
                format_duration(media.duration.unwrap_or_default()),
                media.cover,
            )
            .with_uploader_mid(upper_mid);
            self.favorite_videos.add_card(card);
        }
        self.loading = false;
        self.loading_more = false;
        self.error = None;
    }

    pub fn apply_series_list(&mut self, data: crate::api::space::SeriesListData) {
        let mut all: Vec<SeriesInfo> = Vec::new();
        if let Some(lists) = data.items_lists {
            all.extend(lists.series_list);
            all.extend(lists.seasons_list);
        }
        self.series_cards.clear();
        for series in &all {
            let name = series
                .meta
                .as_ref()
                .and_then(|m| m.name.as_deref().or(m.title.as_deref()))
                .unwrap_or("未命名合集");
            let count = series
                .meta
                .as_ref()
                .and_then(|m| m.total)
                .unwrap_or(0);
            let pic = series.meta.as_ref().and_then(|m| m.cover.clone());
            let card = VideoCard::new(
                None,
                None,
                name.to_string(),
                format!("{}个视频", count),
                String::new(),
                String::new(),
                pic,
            );
            self.series_cards.add_card(card);
        }
        self.series_list = all;
        self.series_list_loaded = true;
        self.loading = false;
        self.error = None;
    }

    pub fn apply_series_archives(&mut self, series_id: i64, page: i32, data: SeriesArchivesData) {
        if page == 1 || self.active_series != Some(series_id) {
            self.series_videos.clear();
        }
        self.active_series = Some(series_id);
        self.pending_series = None;
        self.series_page = page;
        let total: i64 = data
            .meta
            .as_ref()
            .and_then(|m| m.total)
            .or_else(|| data.page.as_ref().and_then(|p| p.total).map(|t| t as i64))
            .unwrap_or(0);
        self.series_has_more = self.series_videos.cards.len() < total as usize;
        if let Some(archives) = data.archives {
            for item in archives {
                let views = item
                    .stat
                    .as_ref()
                    .and_then(|s| s.view)
                    .map(format_count)
                    .unwrap_or_else(|| "-".to_string());
                let card = VideoCard::new(
                    Some(item.bvid),
                    Some(item.aid),
                    item.title,
                    item.author
                        .or_else(|| self.profile.as_ref().map(|p| p.name.clone()))
                        .unwrap_or_else(|| "未知UP".to_string()),
                    views,
                    format_duration(item.duration.unwrap_or_default()),
                    item.cover,
                )
                .with_uploader_mid(item.mid.or(Some(self.mid)));
                self.series_videos.add_card(card);
            }
        }
        self.loading = false;
        self.loading_more = false;
        self.error = None;
    }

    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.loading_more = false;
        self.pending_folder = None;
        self.pending_series = None;
        self.error = Some(error);
    }

    /// Auto-clear the follow feedback message after 3 seconds.
    pub fn tick(&mut self) {
        if let Some(set_at) = self.follow_msg_set_at {
            if set_at.elapsed().as_secs() >= 3 {
                self.follow_msg = None;
                self.follow_msg_set_at = None;
            }
        }
    }

    fn selected_grid(&mut self) -> &mut VideoCardGrid {
        match self.tab {
            UpTab::Videos => &mut self.videos,
            UpTab::Favorites => &mut self.favorite_videos,
            UpTab::Collections => &mut self.series_videos,
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Split header into left (UP info) and right (follow box)
        let header_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(area);

        let (name, sign) = self
            .profile
            .as_ref()
            .map(|p| (p.name.as_str(), p.sign.as_deref().unwrap_or("暂无签名")))
            .unwrap_or(("加载中…", ""));
        let stats = self
            .relation
            .as_ref()
            .map(|r| {
                format!(
                    "关注 {}  ·  粉丝 {}",
                    format_count(r.following.unwrap_or_default()),
                    format_count(r.follower.unwrap_or_default())
                )
            })
            .unwrap_or_default();
        let lines = vec![
            Line::from(Span::styled(
                name,
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(sign),
            Line::from(Span::styled(stats, Style::default().fg(theme.fg_muted))),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(" UP主空间 ")),
            header_chunks[0],
        );

        // Follow status box
        let follow_state = match self.is_followed {
            Some(true) => "已关注".to_string(),
            Some(false) => "未关注".to_string(),
            None => "关注状态未知".to_string(),
        };
        let state_style = match self.is_followed {
            Some(true) => theme.success,
            Some(false) => theme.warning,
            None => theme.fg_muted,
        };
        let mut follow_lines = vec![
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(if self.is_followed == Some(true) {
                        theme.success
                    } else {
                        theme.fg_muted
                    }),
                ),
                Span::styled(follow_state, Style::default().fg(state_style).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(Span::styled(
                "[f] 切换关注",
                Style::default().fg(theme.info),
            )),
        ];
        if let Some(msg) = &self.follow_msg {
            follow_lines.push(Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(theme.info),
            )));
        }
        let border_style = if self.is_followed == Some(true) {
            Style::default().fg(theme.success)
        } else {
            Style::default().fg(theme.border_subtle)
        };
        frame.render_widget(
            Paragraph::new(follow_lines)
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(border_style)
                        .title(" 关注 "),
                ),
            header_chunks[1],
        );
    }
}

impl Component for UpPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(2),
            ])
            .split(area);
        self.draw_header(frame, chunks[0], theme);

        let sort = match self.video_order {
            SpaceVideoOrder::Latest => "最新发布",
            SpaceVideoOrder::Popular => "最多播放",
        };
        let favorite_sort = match self.favorite_order {
            FavoriteOrder::RecentlyFavorited => "最近收藏",
            FavoriteOrder::MostPlayed => "最多播放",
            FavoriteOrder::RecentlyPublished => "最近投稿",
        };
        let play_order = match self.play_order {
            PlayOrder::Forward => "顺序播放",
            PlayOrder::Reverse => "倒序播放",
            PlayOrder::Shuffle => "随机播放",
        };
        let tabs = Tabs::new(vec!["1 投稿", "2 收藏夹", "3 合集"])
            .select(match self.tab {
                UpTab::Videos => 0,
                UpTab::Favorites => 1,
                UpTab::Collections => 2,
            })
            .highlight_style(Style::default().fg(theme.bilibili_pink))
            .block(Block::default().borders(Borders::ALL).title(format!(
                " {} · {play_order} ",
                match self.tab {
                    UpTab::Videos => sort,
                    UpTab::Favorites => favorite_sort,
                    UpTab::Collections => "合集",
                }
            )));
        frame.render_widget(tabs, chunks[1]);

        if self.loading {
            frame.render_widget(Paragraph::new("正在加载…"), chunks[2]);
        } else if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(theme.error)),
                chunks[2],
            );
        } else if self.tab == UpTab::Videos {
            self.videos.render(frame, chunks[2], theme);
        } else if self.tab == UpTab::Favorites && self.active_folder.is_some() {
            self.favorite_videos.render(frame, chunks[2], theme);
        } else if self.tab == UpTab::Collections && self.active_series.is_some() {
            self.series_videos.render(frame, chunks[2], theme);
        } else if self.tab == UpTab::Favorites {
            // 收藏夹列表
            let items: Vec<ListItem> = self
                .folders
                .iter()
                .map(|folder| {
                    ListItem::new(format!(
                        "{}  ({}个视频)",
                        folder.title,
                        folder.media_count.unwrap_or_default()
                    ))
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(self.folder_selected));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_symbol("▶ ")
                    .highlight_style(Style::default().fg(theme.bilibili_cyan)),
                chunks[2],
                &mut state,
            );
        } else if self.tab == UpTab::Collections {
            // 合集列表（带封面）
            if self.series_list.is_empty() && self.series_list_loaded {
                frame.render_widget(Paragraph::new("该UP主没有创建合集"), chunks[2]);
            } else {
                self.series_cards.render(frame, chunks[2], theme);
            }
        }

        frame.render_widget(
            Paragraph::new(shortcut_footer(
                theme,
                [
                    ("1/2/3".into(), "投稿/收藏夹/合集".into(), theme.info),
                    (
                        format!("{}/{}", keys.page_up, keys.page_down),
                        "翻页".into(),
                        theme.fg_accent,
                    ),
                    ("o".into(), "最新/热门".into(), theme.info),
                    ("s".into(), "顺序/倒序/随机".into(), theme.info),
                    (keys.play.clone(), "连播".into(), theme.success),
                    (keys.confirm.clone(), "打开".into(), theme.success),
                    (keys.back.clone(), "返回".into(), theme.info),
                ],
            ))
            .alignment(Alignment::Center),
            chunks[3],
        );
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        if keys.matches_back(key) || keys.matches_quit(key) {
            // 收藏夹内 → 回收藏夹列表；合集内 → 回合集列表
            if self.tab == UpTab::Favorites
                && self.pending_folder.take().is_some()
            {
                self.loading = false;
                return Some(AppAction::None);
            }
            if self.tab == UpTab::Favorites
                && self.active_folder.take().is_some()
            {
                self.favorite_videos.clear();
                return Some(AppAction::None);
            }
            if self.tab == UpTab::Collections
                && self.pending_series.take().is_some()
            {
                self.loading = false;
                return Some(AppAction::None);
            }
            if self.tab == UpTab::Collections
                && self.active_series.take().is_some()
            {
                self.series_videos.clear();
                return Some(AppAction::None);
            }
            return Some(AppAction::BackToList);
        }
        match key {
            KeyCode::Char('1') => {
                self.tab = UpTab::Videos;
                return Some(AppAction::None);
            }
            KeyCode::Char('2') => {
                self.tab = UpTab::Favorites;
                return Some(AppAction::None);
            }
            KeyCode::Char('3') => {
                self.tab = UpTab::Collections;
                if !self.series_list_loaded {
                    self.loading = true;
                    return Some(AppAction::OpenSeriesFolder(0));
                }
                return Some(AppAction::None);
            }
            KeyCode::Char('o') if self.tab == UpTab::Videos => {
                self.video_order = if self.video_order == SpaceVideoOrder::Latest {
                    SpaceVideoOrder::Popular
                } else {
                    SpaceVideoOrder::Latest
                };
                return Some(AppAction::SwitchUpVideoOrder(self.video_order));
            }
            KeyCode::Char('o') if self.tab == UpTab::Favorites => {
                self.favorite_order = self.favorite_order.next();
                if self.active_folder.is_some() {
                    self.loading = true;
                    return Some(AppAction::SwitchFavoriteOrder(self.favorite_order));
                }
                return Some(AppAction::None);
            }
            KeyCode::Char('s') => {
                self.play_order = match self.play_order {
                    PlayOrder::Forward => PlayOrder::Reverse,
                    PlayOrder::Reverse => PlayOrder::Shuffle,
                    PlayOrder::Shuffle => PlayOrder::Forward,
                };
                return Some(AppAction::None);
            }
            KeyCode::Char('f') => {
                return Some(AppAction::ToggleFollow { mid: self.mid });
            }
            _ => {}
        }

        // ── 收藏夹列表选择 ──
        if self.tab == UpTab::Favorites && self.active_folder.is_none() {
            if keys.matches_down(key) && self.folder_selected + 1 < self.folders.len() {
                self.folder_selected += 1;
            } else if keys.matches_up(key) && self.folder_selected > 0 {
                self.folder_selected -= 1;
            } else if keys.matches_confirm(key)
                && let Some(folder) = self.folders.get(self.folder_selected)
            {
                self.loading = true;
                self.pending_folder = Some(folder.id);
                return Some(AppAction::OpenFavoriteFolder(folder.id));
            }
            return Some(AppAction::None);
        }

        // ── 合集列表选择 ──
        if self.tab == UpTab::Collections && self.active_series.is_none() {
            if keys.matches_down(key) {
                self.series_cards.move_down();
            } else if keys.matches_up(key) {
                self.series_cards.move_up();
            } else if keys.matches_right(key) {
                self.series_cards.move_right();
            } else if keys.matches_left(key) {
                self.series_cards.move_left();
            } else if keys.matches_confirm(key)
                && self.series_cards.selected_index < self.series_list.len()
            {
                if let Some(series) = self.series_list.get(self.series_cards.selected_index) {
                    if let Some(id) = series.meta.as_ref().and_then(|m| m.season_id) {
                        self.loading = true;
                        self.pending_series = Some(id);
                        return Some(AppAction::OpenSeriesFolder(id));
                    }
                }
            }
            return Some(AppAction::None);
        }

        let tab = self.tab;
        let loading_more = self.loading_more;
        let video_total = self.video_total;
        let favorite_has_more = self.favorite_has_more;
        let series_has_more = self.series_has_more;
        let grid = self.selected_grid();
        if keys.matches_play(key) {
            if tab == UpTab::Videos {
                return Some(AppAction::PlayUpAll {
                    mid: self.mid,
                    name: self
                        .profile
                        .as_ref()
                        .map(|profile| profile.name.clone())
                        .unwrap_or_else(|| "UP主投稿".to_string()),
                    video_order: self.video_order,
                    play_order: self.play_order,
                });
            } else if tab == UpTab::Favorites {
                let media_id = self.active_folder.unwrap_or_default();
                let title = self
                    .folders
                    .iter()
                    .find(|folder| folder.id == media_id)
                    .map(|folder| folder.title.clone())
                    .unwrap_or_else(|| "收藏夹".to_string());
                return Some(AppAction::PlayFavoriteAll {
                    media_id,
                    title,
                    favorite_order: self.favorite_order,
                    play_order: self.play_order,
                });
            }
            return Some(AppAction::None);
        }
        if keys.matches_page_down(key) {
            grid.move_page_down();
            if grid.is_near_bottom(grid.cached_visible_rows) && !loading_more {
                if tab == UpTab::Videos && grid.cards.len() < video_total as usize {
                    return Some(AppAction::LoadMoreUpVideos);
                }
                if tab == UpTab::Favorites && favorite_has_more {
                    return Some(AppAction::LoadMoreFavoriteResources);
                }
                if tab == UpTab::Collections && series_has_more {
                    return Some(AppAction::LoadMoreSeriesVideos);
                }
            }
            return Some(AppAction::None);
        }
        if keys.matches_page_up(key) {
            grid.move_page_up();
            return Some(AppAction::None);
        }
        if keys.matches_down(key) {
            grid.move_down();
            if grid.is_near_bottom(grid.cached_visible_rows) && !loading_more {
                if tab == UpTab::Videos && grid.cards.len() < video_total as usize {
                    return Some(AppAction::LoadMoreUpVideos);
                }
                if tab == UpTab::Favorites && favorite_has_more {
                    return Some(AppAction::LoadMoreFavoriteResources);
                }
                if tab == UpTab::Collections && series_has_more {
                    return Some(AppAction::LoadMoreSeriesVideos);
                }
            }
        } else if keys.matches_up(key) {
            grid.move_up();
        } else if keys.matches_right(key) {
            grid.move_right();
        } else if keys.matches_left(key) {
            grid.move_left();
        } else if keys.matches_confirm(key)
            && let Some(card) = grid.selected_card()
            && let (Some(bvid), Some(aid)) = (&card.bvid, card.aid)
        {
            return Some(AppAction::OpenVideoDetail(bvid.clone(), aid));
        }
        Some(AppAction::None)
    }
}

fn format_count(value: i64) -> String {
    if value >= 10_000 {
        format!("{:.1}万", value as f64 / 10_000.0)
    } else {
        value.to_string()
    }
}

fn format_duration(seconds: i64) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_order_cycles_through_shuffle() {
        let mut page = UpPage::new(1);
        let keys = Keybindings::default();
        page.handle_input(KeyCode::Char('s'), &keys);
        assert_eq!(page.play_order, PlayOrder::Reverse);
        page.handle_input(KeyCode::Char('s'), &keys);
        assert_eq!(page.play_order, PlayOrder::Shuffle);
        page.handle_input(KeyCode::Char('s'), &keys);
        assert_eq!(page.play_order, PlayOrder::Forward);
    }

    #[test]
    fn back_cancels_a_pending_folder_without_leaving_up_page() {
        let mut page = UpPage::new_favorites(1);
        page.pending_folder = Some(10);
        page.loading = true;
        let action = page.handle_input(KeyCode::Esc, &Keybindings::default());
        assert!(matches!(action, Some(AppAction::None)));
        assert_eq!(page.pending_folder, None);
        assert!(!page.loading);
    }
}
