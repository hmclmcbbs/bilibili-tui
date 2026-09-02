//! Bangumi detail page showing season info and episode list

use super::{Component, Theme, shortcut_footer};
use crate::api::bangumi::{BangumiEpisode, SeasonResult};
use crate::api::client::ApiClient;
use crate::application::AppAction;
use crate::storage::{Keybindings, VideoQuality};
use ratatui::{
    crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind},
    prelude::*,
    widgets::*,
};
use std::time::Instant;

pub struct BangumiDetailPage {
    pub season_id: i64,
    pub season: Option<SeasonResult>,
    pub loading: bool,
    pub error_message: Option<String>,
    pub episode_scroll: usize,
    pub selected_episode: usize,
    target_episode_id: Option<i64>,
    pub auto_play_pending: bool,
    /// Current user's follow state for this season (None = unknown / not logged in).
    pub followed: Option<bool>,
    /// Short message shown after a follow/unfollow operation.
    pub follow_msg: Option<String>,
    // Flat list of all episodes for navigation
    flat_episodes: Vec<FlatEpisode>,
    last_click_time: Option<Instant>,
    last_click_index: Option<usize>,
    /// Multi-select set for batch downloads. Stores episode ids.
    pub download_selection: std::collections::HashSet<i64>,
    /// Download resolution override. `None` = follow the global setting.
    pub download_quality: Option<VideoQuality>,
    /// Whether the download-quality picker popup is open.
    pub download_quality_picker: bool,
    download_quality_index: usize,
}

#[derive(Clone)]
struct FlatEpisode {
    section_title: String,
    _ep_index: usize,
    episode: BangumiEpisode,
}

impl BangumiDetailPage {
    pub fn new(season_id: i64) -> Self {
        Self {
            season_id,
            season: None,
            loading: true,
            error_message: None,
            episode_scroll: 0,
            selected_episode: 0,
            target_episode_id: None,
            auto_play_pending: false,
            followed: None,
            follow_msg: None,
            flat_episodes: Vec::new(),
            last_click_time: None,
            last_click_index: None,
            download_selection: std::collections::HashSet::new(),
            download_quality: None,
            download_quality_picker: false,
            download_quality_index: 0,
        }
    }

    pub fn new_for_episode(season_id: i64, ep_id: i64, auto_play: bool) -> Self {
        let mut page = Self::new(season_id);
        page.target_episode_id = Some(ep_id);
        page.auto_play_pending = auto_play;
        page
    }

    fn download_quality_label(&self) -> String {
        match self.download_quality {
            Some(q) => format!("下载:{}", q.label()),
            None => "下载:跟随".to_string(),
        }
    }

    fn quality_options() -> [Option<VideoQuality>; 8] {
        use crate::storage::VideoQuality as VQ;
        [
            None,
            Some(VQ::Best),
            Some(VQ::Q4k),
            Some(VQ::Q1080pHigh),
            Some(VQ::Q1080p),
            Some(VQ::Q720p),
            Some(VQ::Q480p),
            Some(VQ::Q360p),
        ]
    }

    fn quality_option_label(index: usize) -> String {
        match Self::quality_options().get(index).copied().flatten() {
            Some(q) => q.label().to_string(),
            None => "跟随全局".to_string(),
        }
    }

    fn current_quality_index(&self) -> usize {
        Self::quality_options()
            .iter()
            .position(|opt| *opt == self.download_quality)
            .unwrap_or(0)
    }

    pub fn set_season(&mut self, season: SeasonResult) {
        self.flat_episodes.clear();
        for section in season.all_sections() {
            for (ep_idx, ep) in section.episodes.iter().enumerate() {
                self.flat_episodes.push(FlatEpisode {
                    section_title: section.title.clone(),
                    _ep_index: ep_idx,
                    episode: ep.clone(),
                });
            }
        }
        self.season = Some(season);
        self.loading = false;
        self.error_message = None;
        self.followed = self
            .season
            .as_ref()
            .and_then(|s| s.user_status.as_ref())
            .filter(|u| u.login == 1)
            .map(|u| u.follow == 1);
        let target_index = self.target_episode_id.and_then(|ep_id| {
            self.flat_episodes
                .iter()
                .position(|episode| episode.episode.id == ep_id)
        });
        self.selected_episode = target_index.unwrap_or(0);
        if self.target_episode_id.is_some() && target_index.is_none() {
            self.auto_play_pending = false;
            self.error_message = Some("未找到历史记录对应的番剧剧集".to_string());
        }
        self.episode_scroll = 0;
        self.update_scroll();
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.loading = false;
    }

    /// Apply the follow state resolved by the network layer (from the real
    /// follow list when the season view reported login=0).
    pub fn apply_follow_status(&mut self, followed: bool) {
        self.followed = Some(followed);
    }

    pub async fn load_data(&mut self, api_client: &ApiClient) {
        self.loading = true;
        self.error_message = None;
        match api_client.get_bangumi_season(self.season_id).await {
            Ok(season) => self.set_season(season),
            Err(e) => self.set_error(format!("加载番剧详情失败: {}", e)),
        }
    }

    fn selected_action(&self) -> Option<AppAction> {
        self.flat_episodes
            .get(self.selected_episode)
            .map(|fe| AppAction::PlayBangumiEpisode {
                ep_id: fe.episode.id,
                season_id: self.season_id,
                title: fe.episode.display_title(),
            })
    }

    pub fn play_action(&self) -> Option<AppAction> {
        self.selected_action()
    }

    fn move_down(&mut self) {
        if self.selected_episode + 1 < self.flat_episodes.len() {
            self.selected_episode += 1;
            self.update_scroll();
        }
    }

    fn move_up(&mut self) {
        if self.selected_episode > 0 {
            self.selected_episode -= 1;
            self.update_scroll();
        }
    }

    fn update_scroll(&mut self) {
        let visible = 12usize;
        if self.selected_episode < self.episode_scroll {
            self.episode_scroll = self.selected_episode;
        } else if self.selected_episode >= self.episode_scroll + visible {
            self.episode_scroll = self.selected_episode.saturating_sub(visible - 1);
        }
    }

    fn render_info(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_subtle))
            .title(Span::styled(
                " 📺 番剧信息 ",
                Style::default().fg(theme.bilibili_pink),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(ref season) = self.season {
            let mut constraints = vec![
                Constraint::Length(1), // Title
                Constraint::Length(1), // Score / status
            ];
            if season.evaluate.is_some() {
                constraints.push(Constraint::Min(2));
            }
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            // Title
            let title = Paragraph::new(season.title.clone()).style(
                Style::default()
                    .fg(theme.fg_primary)
                    .add_modifier(Modifier::BOLD),
            );
            frame.render_widget(title, chunks[0]);

            // Score & status
            let mut stats_spans = Vec::new();
            if let Some(ref rating) = season.rating {
                stats_spans.push(Span::styled(
                    format!("⭐ {:.1}分", rating.score),
                    Style::default().fg(theme.warning),
                ));
                stats_spans.push(Span::styled(" · ", Style::default().fg(theme.fg_muted)));
            }
            if let Some(ref index_show) = season.index_show {
                stats_spans.push(Span::styled(
                    index_show,
                    Style::default().fg(theme.fg_secondary),
                ));
            }
            if let Some(is_finish) = season.is_finish {
                stats_spans.push(Span::styled(
                    if is_finish == 1 {
                        " · 已完结"
                    } else {
                        " · 连载中"
                    },
                    Style::default().fg(theme.fg_secondary),
                ));
            }
            if let Some(ref badge) = season.badge
                && !badge.is_empty()
            {
                stats_spans.push(Span::styled(
                    format!(" · {}", badge),
                    Style::default().fg(theme.bilibili_pink),
                ));
            }
            if let Some(followed) = self.followed {
                stats_spans.push(Span::styled(" │ ", Style::default().fg(theme.fg_muted)));
                let follow_text = if followed {
                    "[f] 已追番 ✓"
                } else {
                    "[f] 未追番"
                };
                let follow_color = if followed { theme.success } else { theme.fg_secondary };
                stats_spans.push(Span::styled(
                    follow_text,
                    Style::default().fg(follow_color).add_modifier(Modifier::BOLD),
                ));
                if let Some(msg) = &self.follow_msg {
                    stats_spans.push(Span::styled(
                        format!(" {}", msg),
                        Style::default().fg(theme.fg_secondary),
                    ));
                }
            }
            frame.render_widget(Paragraph::new(Line::from(stats_spans)), chunks[1]);

            // Description
            if let Some(ref desc) = season.evaluate
                && !desc.is_empty()
            {
                let desc_text = if desc.chars().count() > 200 {
                    desc.chars().take(197).collect::<String>() + "…"
                } else {
                    desc.clone()
                };
                frame.render_widget(
                    Paragraph::new(desc_text)
                        .style(Style::default().fg(theme.fg_muted))
                        .wrap(Wrap { trim: true }),
                    chunks[2],
                );
            }
        }
    }

    fn render_episodes(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.flat_episodes.is_empty() {
            let msg = Paragraph::new("暂无剧集信息")
                .style(Style::default().fg(theme.fg_muted))
                .alignment(Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_subtle))
            .title(Span::styled(
                " 📋 选集 ",
                Style::default().fg(theme.bilibili_pink),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut items: Vec<ListItem> = Vec::new();
        let mut last_section = String::new();

        for (global_idx, fe) in self.flat_episodes.iter().enumerate() {
            // Section header when section changes
            if fe.section_title != last_section {
                last_section = fe.section_title.clone();
                items.push(
                    ListItem::new(format!("▸ {}", last_section)).style(
                        Style::default()
                            .fg(theme.fg_secondary)
                            .add_modifier(Modifier::BOLD),
                    ),
                );
            }

            let is_selected = global_idx == self.selected_episode;
            let ep = &fe.episode;
            let badge = ep
                .badge_text()
                .map(|b| format!(" [{}]", b))
                .unwrap_or_default();
            let content = format!("  {}{}", ep.display_title(), badge);

            let style = if is_selected {
                Style::default()
                    .fg(theme.bilibili_pink)
                    .bg(theme.bg_highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg_primary)
            };

            items.push(ListItem::new(content).style(style));
        }

        let list = List::new(items)
            .style(Style::default().fg(theme.fg_primary))
            .scroll_padding(2);

        let mut state = ListState::default();
        // Adjust for section headers in scroll - we render headers inline so
        // the selected index maps directly to items including headers.
        // Wait, we need to account for section headers in the list.
        // Let me recalculate: each section header adds 1 item before its episodes.
        // Actually, I need to pre-compute the mapping.
        state.select(Some(self.list_index_for_episode(self.selected_episode)));

        frame.render_stateful_widget(list, inner, &mut state);
    }

    /// Map episode index to list index (accounting for section headers)
    fn list_index_for_episode(&self, ep_idx: usize) -> usize {
        let mut list_idx = 0;
        let mut last_section = String::new();
        for (current_ep, fe) in self.flat_episodes.iter().enumerate() {
            if fe.section_title != last_section {
                last_section = fe.section_title.clone();
                list_idx += 1;
            }
            if current_ep == ep_idx {
                return list_idx;
            }
            list_idx += 1;
        }
        list_idx
    }
}

impl Component for BangumiDetailPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        if self.loading {
            let spinner = Paragraph::new("加载中...")
                .style(Style::default().fg(theme.fg_muted))
                .alignment(Alignment::Center);
            frame.render_widget(spinner, area);
            return;
        }

        if let Some(ref err) = self.error_message {
            let error = Paragraph::new(err.as_str())
                .style(Style::default().fg(theme.error))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(error, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6), // Info
                Constraint::Min(6),    // Episodes
                Constraint::Length(2), // Help
            ])
            .split(area);

        self.render_info(frame, chunks[0], theme);
        self.render_episodes(frame, chunks[1], theme);

        // Download-quality picker popup (over the episodes area).
        if self.download_quality_picker {
            let popup_height = (Self::quality_options().len() as u16) + 2;
            let popup_width = 40u16.min(chunks[1].width.saturating_sub(4));
            let popup_area = Rect {
                x: chunks[1].x + (chunks[1].width.saturating_sub(popup_width)) / 2,
                y: chunks[1].y + (chunks[1].height.saturating_sub(popup_height)) / 2,
                width: popup_width,
                height: popup_height.min(chunks[1].height),
            };
            frame.render_widget(Clear, popup_area);
            let picker_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.fg_accent))
                .title(Span::styled(
                    " 下载分辨率 [↑↓ 选择, Enter 确认, Esc 取消] ",
                    Style::default()
                        .fg(theme.fg_accent)
                        .add_modifier(Modifier::BOLD),
                ));
            let items: Vec<ListItem> = (0..Self::quality_options().len())
                .map(|i| {
                    let label = Self::quality_option_label(i);
                    if i == self.download_quality_index {
                        ListItem::new(format!("▶ {label}"))
                    } else {
                        ListItem::new(format!("  {label}"))
                    }
                })
                .collect();
            let list = List::new(items)
                .block(picker_block)
                .highlight_style(Style::default().fg(theme.fg_accent));
            frame.render_widget(list, popup_area);
        }

        // Help
        let help_items: Vec<(String, String, Color)> = if self.download_quality_picker {
            vec![
                ("↑↓".into(), "选择".into(), theme.fg_accent),
                ("Enter".into(), "确认".into(), theme.success),
                ("Esc".into(), "取消".into(), theme.info),
            ]
        } else {
            vec![
                (
                    format!("{}/{}", keys.nav_up, keys.nav_down),
                    "滚动".into(),
                    theme.fg_accent,
                ),
                (
                    format!("{} / {}", keys.nav_next_page, keys.nav_prev_page),
                    "切侧边栏".into(),
                    theme.fg_accent,
                ),
                ("f".into(), "追番/取消".into(), theme.info),
                (
                    "x".into(),
                    self.download_quality_label(),
                    theme.fg_accent,
                ),
                (keys.confirm.clone(), "播放".into(), theme.success),
                (keys.back.clone(), "返回".into(), theme.info),
            ]
        };
        let help = Paragraph::new(shortcut_footer(theme, help_items)).alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);

        // 常驻下载进度条（从全局下载状态读取，覆盖底部最后一行）
        if let Some(status) = crate::infrastructure::download::current_status() {
            let progress_line = if status.total > 1 {
                format!(
                    "下载中 {}/{} · {} {}",
                    status.done, status.total, status.current_title, status.current_msg
                )
            } else {
                format!("下载中 · {} {}", status.current_title, status.current_msg)
            };
            let area = frame.area();
            let bottom = Rect {
                x: area.x,
                y: area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            let bar = Paragraph::new(progress_line)
                .style(Style::default().fg(theme.bilibili_pink))
                .alignment(Alignment::Left);
            frame.render_widget(bar, bottom);
        }
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        // Global keybindings
        if keys.matches_quit(key) {
            return Some(AppAction::Quit);
        }
        if keys.matches_back(key) {
            return Some(AppAction::BackToList);
        }
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }
        if keys.matches_next_theme(key) {
            return Some(AppAction::NextTheme);
        }
        if keys.matches_open_settings(key) {
            return Some(AppAction::SwitchToSettings);
        }

        if self.loading {
            return Some(AppAction::None);
        }

        // Download-quality picker mode (after pressing `x`).
        if self.download_quality_picker {
            match key {
                KeyCode::Esc | KeyCode::Char('x') => {
                    self.download_quality_picker = false;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.download_quality_index > 0 {
                        self.download_quality_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let last = Self::quality_options().len() - 1;
                    if self.download_quality_index < last {
                        self.download_quality_index += 1;
                    }
                }
                KeyCode::Enter => {
                    self.download_quality =
                        Self::quality_options().get(self.download_quality_index).copied().flatten();
                    self.download_quality_picker = false;
                }
                _ => {}
            }
            return Some(AppAction::None);
        }

        if key == KeyCode::Char('x') {
            self.download_quality_index = self.current_quality_index();
            self.download_quality_picker = true;
            return Some(AppAction::None);
        }

        if key == KeyCode::Char('f') {
            return Some(AppAction::ToggleBangumiFollow {
                season_id: self.season_id,
            });
        }
        if key == KeyCode::Char(' ') {
            // Toggle multi-select for batch download (keyed by episode id).
            if let Some(ep) = self.flat_episodes.get(self.selected_episode) {
                let id = ep.episode.id;
                if self.download_selection.contains(&id) {
                    self.download_selection.remove(&id);
                } else {
                    self.download_selection.insert(id);
                }
            }
            return Some(AppAction::None);
        }
        if key == KeyCode::Char('d') {
            // Download the currently focused episode.
            if let Some(ep) = self.flat_episodes.get(self.selected_episode) {
                let item = crate::application::DownloadItem {
                    kind: "bangumi".to_string(),
                    bvid: String::new(),
                    ep_id: ep.episode.id,
                    title: ep.episode.title.clone(),
                    aid: ep.episode.aid,
                    cid: ep.episode.cid,
                    pic_url: ep.episode.cover_url(),
                    duration_secs: ep.episode.duration / 1000,
                    quality: self.download_quality,
                };
                return Some(AppAction::DownloadMedia { items: vec![item] });
            }
            return Some(AppAction::None);
        }
        if key == KeyCode::Char('D') {
            // Download all selected episodes, or current if nothing selected.
            let items: Vec<crate::application::DownloadItem> = if self.download_selection.is_empty() {
                self.flat_episodes
                    .get(self.selected_episode)
                    .map(|ep| crate::application::DownloadItem {
                        kind: "bangumi".to_string(),
                        bvid: String::new(),
                        ep_id: ep.episode.id,
                        title: ep.episode.title.clone(),
                        aid: ep.episode.aid,
                        cid: ep.episode.cid,
                        pic_url: ep.episode.cover_url(),
                        duration_secs: ep.episode.duration / 1000,
                        quality: self.download_quality,
                    })
                    .into_iter()
                    .collect()
            } else {
                self.download_selection
                    .iter()
                    .filter_map(|id| {
                        self.flat_episodes
                            .iter()
                            .find(|e| e.episode.id == *id)
                            .map(|ep| crate::application::DownloadItem {
                                kind: "bangumi".to_string(),
                                bvid: String::new(),
                                ep_id: ep.episode.id,
                                title: ep.episode.title.clone(),
                                aid: ep.episode.aid,
                                cid: ep.episode.cid,
                                pic_url: ep.episode.cover_url(),
                                duration_secs: ep.episode.duration / 1000,
                                quality: self.download_quality,
                            })
                    })
                    .collect()
            };
            return Some(AppAction::DownloadMedia { items });
        }

        if keys.matches_down(key) {
            self.move_down();
            return Some(AppAction::None);
        }
        if keys.matches_up(key) {
            self.move_up();
            return Some(AppAction::None);
        }
        if keys.matches_play(key) || keys.matches_confirm(key) {
            return self.selected_action();
        }

        Some(AppAction::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_episode_is_preselected_for_auto_play() {
        let season: SeasonResult = serde_json::from_value(serde_json::json!({
            "title": "season",
            "season_id": 7,
            "cover": "",
            "square_cover": "",
            "evaluate": null,
            "link": null,
            "episodes": [
                {"aid": 1, "cid": 11, "id": 101, "title": "1"},
                {"aid": 2, "cid": 22, "id": 202, "title": "2"}
            ],
            "section": null,
            "rating": null,
            "stat": null,
            "badge": null,
            "is_finish": null,
            "index_show": null
        }))
        .unwrap();
        let mut page = BangumiDetailPage::new_for_episode(7, 202, true);
        page.set_season(season);

        assert_eq!(page.selected_episode, 1);
        assert!(page.auto_play_pending);
        assert!(matches!(
            page.play_action(),
            Some(AppAction::PlayBangumiEpisode { ep_id: 202, .. })
        ));
    }
}
