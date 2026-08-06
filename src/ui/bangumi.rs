//! Bangumi page with rank list

use super::video_card::{VideoCard, VideoCardGrid};
use super::{Component, Theme, shortcut_footer};
use crate::api::bangumi::SeasonRankItem;
use crate::api::client::ApiClient;
use crate::application::AppAction;
use crate::storage::Keybindings;
use ratatui::{
    crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind},
    prelude::*,
    widgets::*,
};
use std::time::Instant;

pub struct BangumiPage {
    pub index_grid: VideoCardGrid,
    pub loading: bool,
    pub error_message: Option<String>,
    /// Parallel array storing season_id for each index card
    pub index_season_ids: Vec<i64>,
    /// Follow list mode (我的追番)
    pub follow_mode: bool,
    pub follow_grid: VideoCardGrid,
    pub follow_season_ids: Vec<i64>,
    pub follow_loading: bool,
    pub follow_error: Option<String>,
    pub follow_loaded: bool,
    // Search mode (番剧搜索)
    pub search_input_mode: bool,
    pub search_input: String,
    pub search_query: String,
    pub search_grid: VideoCardGrid,
    pub search_season_ids: Vec<i64>,
    pub search_loading: bool,
    pub search_error: Option<String>,
    pub search_has_results: bool,
    pub search_history: Vec<String>,
    pub history_selected: Option<usize>,
    // Double-click detection
    last_click_time: Option<Instant>,
    last_click_index: Option<usize>,
}

impl BangumiPage {
    pub fn new() -> Self {
        Self {
            index_grid: VideoCardGrid::new(),
            loading: true,
            error_message: None,
            index_season_ids: Vec::new(),
            follow_mode: false,
            follow_grid: VideoCardGrid::new(),
            follow_season_ids: Vec::new(),
            follow_loading: false,
            follow_error: None,
            follow_loaded: false,
            search_input_mode: false,
            search_input: String::new(),
            search_query: String::new(),
            search_grid: VideoCardGrid::new(),
            search_season_ids: Vec::new(),
            search_loading: false,
            search_error: None,
            search_has_results: false,
            search_history: crate::storage::load_search_history(),
            history_selected: None,
            last_click_time: None,
            last_click_index: None,
        }
    }

    /// Reload persisted search history for the bangumi search picker.
    pub fn reload_search_history(&mut self) {
        self.search_history = crate::storage::load_search_history();
        if self.search_history.is_empty() {
            self.history_selected = None;
        } else if self.history_selected.is_none() {
            self.history_selected = Some(0);
        }
    }

    /// Whether we are currently showing search results
    pub fn is_searching(&self) -> bool {
        !self.search_query.is_empty() || self.search_input_mode
    }

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn start_search(&mut self, keyword: &str) {
        let kw = keyword.trim().to_string();
        self.search_query = kw.clone();
        self.search_input.clear();
        self.search_input_mode = false;
        self.search_loading = true;
        self.search_error = None;
        self.search_has_results = false;
        self.search_grid.clear();
        self.search_season_ids.clear();
    }

    pub fn set_search_items(&mut self, items: Vec<crate::api::search::SearchBangumiItem>) {
        self.search_grid.clear();
        self.search_season_ids.clear();
        for item in items {
            let Some(season_id) = item.season_id else { continue };
            self.search_season_ids.push(season_id);
            let card = VideoCard::new(
                None,
                None,
                item.display_title(),
                item.display_subtitle(),
                item.score_text(),
                item.badge_text().unwrap_or_default(),
                item.cover_url(),
            );
            self.search_grid.add_card(card);
        }
        self.search_loading = false;
        self.search_error = None;
        self.search_has_results = true;
    }

    pub fn set_search_error(&mut self, msg: String) {
        self.search_loading = false;
        self.search_error = Some(msg);
    }

    pub fn set_follow_items(&mut self, items: Vec<crate::api::space::SeriesInfo>) {
        self.follow_grid.clear();
        self.follow_season_ids.clear();
        for item in items {
            let Some(meta) = &item.meta else { continue };
            let Some(season_id) = meta.season_id else { continue };
            let title = meta
                .title
                .clone()
                .or_else(|| meta.name.clone())
                .unwrap_or_else(|| "未知番剧".to_string());
            self.follow_season_ids.push(season_id);
            let card = VideoCard::new(
                None,
                None,
                title,
                meta.description.clone().unwrap_or_default(),
                item.total.map(|t| format!("{}话", t)).unwrap_or_default(),
                String::new(),
                meta.cover.clone(),
            );
            self.follow_grid.add_card(card);
        }
        self.follow_loading = false;
        self.follow_error = None;
        self.follow_loaded = true;
    }

    pub fn set_follow_error(&mut self, msg: String) {
        self.follow_error = Some(msg);
        self.follow_loading = false;
    }

    pub fn enter_follow_mode(&mut self) {
        self.follow_mode = true;
        if !self.follow_loaded {
            self.follow_loading = true;
        }
    }

    pub fn exit_follow_mode(&mut self) {
        self.follow_mode = false;
    }

    pub fn set_index_items(&mut self, items: Vec<SeasonRankItem>) {
        self.index_grid.clear();
        self.index_season_ids.clear();
        for item in items {
            self.index_season_ids.push(item.season_id);
            let card = VideoCard::new(
                None,
                None,
                item.display_title(),
                item.display_subtitle(),
                item.score_text(),
                item.badge_text().unwrap_or("").to_string(),
                Some(item.cover_url()),
            );
            self.index_grid.add_card(card);
        }
        self.loading = false;
        self.error_message = None;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.loading = false;
    }

    pub async fn load_index(&mut self, api_client: &ApiClient) {
        self.loading = true;
        self.error_message = None;
        match api_client.get_bangumi_rank().await {
            Ok(items) => self.set_index_items(items),
            Err(e) => self.set_error(format!("加载番剧排行失败: {}", e)),
        }
    }

    /// Get selected index action
    fn selected_index_action(&self) -> Option<AppAction> {
        let idx = self.index_grid.selected_index;
        self.index_season_ids
            .get(idx)
            .copied()
            .map(AppAction::OpenBangumiDetail)
    }

    /// Get selected follow-list action
    fn selected_follow_action(&self) -> Option<AppAction> {
        let idx = self.follow_grid.selected_index;
        self.follow_season_ids
            .get(idx)
            .copied()
            .map(AppAction::OpenBangumiDetail)
    }

    /// Get selected search-result action
    fn selected_search_action(&self) -> Option<AppAction> {
        let idx = self.search_grid.selected_index;
        self.search_season_ids
            .get(idx)
            .copied()
            .map(AppAction::OpenBangumiDetail)
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let mut help_line = shortcut_footer(
            theme,
            [
                (
                    format!(
                        "{}/{}",
                        keys.get_arrow_keys_display(),
                        keys.get_nav_keys_display()
                    ),
                    "导航".into(),
                    theme.fg_accent,
                ),
                (
                    format!("{}/{}", keys.page_up, keys.page_down),
                    "翻页".into(),
                    theme.fg_accent,
                ),
                (keys.confirm.clone(), "详情".into(), theme.success),
                (keys.refresh.clone(), "刷新".into(), theme.info),
            ],
        );
        if !self.follow_mode {
            help_line.push_span(Span::styled(
                "   / 搜索",
                Style::default().fg(theme.info),
            ));
        }
        let help = Paragraph::new(help_line).alignment(Alignment::Center);
        frame.render_widget(help, area);
    }
}

impl Default for BangumiPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for BangumiPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(5),    // Content
                Constraint::Length(2), // Help
            ])
            .split(area);

        // Title
        let title_text = if self.follow_mode {
            "我的追番"
        } else if self.is_searching() {
            "番剧搜索"
        } else {
            "番剧排行"
        };
        let mut title_spans = vec![
            Span::styled("🎬 ", Style::default().fg(theme.bilibili_pink)),
            Span::styled(
                title_text,
                Style::default()
                    .fg(theme.fg_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  [Tab] 切换",
                Style::default().fg(theme.fg_secondary),
            ),
        ];
        if self.search_input_mode {
            title_spans.push(Span::styled(
                format!("  搜索: {}_", self.search_input),
                Style::default().fg(theme.success),
            ));
        } else if !self.search_query.is_empty() {
            title_spans.push(Span::styled(
                format!("  「{}」", self.search_query),
                Style::default().fg(theme.fg_secondary),
            ));
        }
        let title = Paragraph::new(Line::from(title_spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_subtle)),
            )
            .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        // Content
        let content_area = chunks[1];
        let mut render_area = content_area;

        // 搜索输入模式：标题栏下方弹出搜索历史下拉栏，剩余区域继续显示内容
        if self.search_input_mode {
            if self.search_input.is_empty() && !self.search_history.is_empty() {
                let hist_len = self.search_history.len() as u16;
                let dropdown_height = hist_len.min(8) + 2;
                let dropdown_area = Rect {
                    height: dropdown_height,
                    ..render_area
                };
                let items: Vec<ListItem> = self
                    .search_history
                    .iter()
                    .enumerate()
                    .map(|(idx, kw)| {
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                "历史 ",
                                Style::default().fg(theme.fg_muted),
                            ),
                            Span::styled(kw.as_str(), Style::default().fg(theme.fg_primary)),
                        ]))
                        .style(if Some(idx) == self.history_selected {
                            Style::default().fg(theme.bilibili_pink)
                        } else {
                            Style::default()
                        })
                    })
                    .collect();
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(theme.border_subtle))
                            .title(Span::styled(
                                " 搜索历史（↑↓ 选择，Enter 搜索，Esc 取消） ",
                                Style::default().fg(theme.bilibili_pink),
                            )),
                    )
                    .highlight_symbol("▶ ");
                let mut state = ListState::default().with_selected(self.history_selected);
                frame.render_stateful_widget(list, dropdown_area, &mut state);
                render_area = Rect {
                    y: render_area.y + dropdown_height,
                    height: render_area.height.saturating_sub(dropdown_height),
                    ..render_area
                };
            } else {
                let hint = Paragraph::new("输入番剧名后按回车搜索，Esc 取消")
                    .style(Style::default().fg(theme.fg_muted))
                    .alignment(Alignment::Center);
                let hint_area = Rect {
                    height: 1,
                    ..render_area
                };
                frame.render_widget(hint, hint_area);
                render_area = Rect {
                    y: render_area.y + 1,
                    height: render_area.height.saturating_sub(1),
                    ..render_area
                };
            }
        }

        if self.follow_mode {
            if self.follow_loading {
                let spinner = Paragraph::new("加载追番中...")
                    .style(Style::default().fg(theme.fg_muted))
                    .alignment(Alignment::Center);
                frame.render_widget(spinner, render_area);
            } else if let Some(ref err) = self.follow_error {
                let error = Paragraph::new(err.as_str())
                    .style(Style::default().fg(theme.error))
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true });
                frame.render_widget(error, render_area);
            } else if self.follow_grid.cards.is_empty() {
                let empty = Paragraph::new("还没有追番，去番剧详情页按 f 追番吧")
                    .style(Style::default().fg(theme.fg_muted))
                    .alignment(Alignment::Center);
                frame.render_widget(empty, render_area);
            } else {
                self.follow_grid.render(frame, render_area, theme);
            }
        } else if self.search_loading {
            let spinner = Paragraph::new("搜索中...")
                .style(Style::default().fg(theme.fg_muted))
                .alignment(Alignment::Center);
            frame.render_widget(spinner, render_area);
        } else if let Some(ref err) = self.search_error {
            let error = Paragraph::new(err.as_str())
                .style(Style::default().fg(theme.error))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(error, render_area);
        } else if self.search_has_results {
            if self.search_grid.cards.is_empty() {
                let empty = Paragraph::new("没有找到相关番剧")
                    .style(Style::default().fg(theme.fg_muted))
                    .alignment(Alignment::Center);
                frame.render_widget(empty, render_area);
            } else {
                self.search_grid.render(frame, render_area, theme);
            }
        } else if self.loading {
            let spinner = Paragraph::new("加载中...")
                .style(Style::default().fg(theme.fg_muted))
                .alignment(Alignment::Center);
            frame.render_widget(spinner, render_area);
        } else if let Some(ref err) = self.error_message {
            let error = Paragraph::new(err.as_str())
                .style(Style::default().fg(theme.error))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(error, render_area);
        } else {
            self.index_grid.render(frame, render_area, theme);
        }

        // Footer help
        self.render_footer(frame, chunks[2], theme, keys);
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        // Global keybindings
        if keys.matches_quit(key) {
            return Some(AppAction::Quit);
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
        if keys.matches_refresh(key) {
            return Some(AppAction::RefreshBangumi);
        }
        if key == KeyCode::Tab || key == KeyCode::BackTab {
            return Some(AppAction::SwitchBangumiTab(if self.follow_mode {
                crate::application::BangumiTab::Index
            } else {
                crate::application::BangumiTab::Follow
            }));
        }

        // Search input mode: all keys go to the input buffer
        if self.search_input_mode {
            match key {
                KeyCode::Esc => {
                    self.search_input_mode = false;
                    self.search_input.clear();
                    self.history_selected = None;
                }
                KeyCode::Enter => {
                    if self.search_input.is_empty() {
                        if let Some(idx) = self.history_selected
                            && let Some(kw) = self.search_history.get(idx)
                        {
                            self.search_input_mode = false;
                            self.search_input.clear();
                            self.history_selected = None;
                            return Some(AppAction::SearchBangumi {
                                keyword: kw.clone(),
                            });
                        }
                    } else {
                        let keyword = self.search_input.trim().to_string();
                        self.search_input_mode = false;
                        self.search_input.clear();
                        self.history_selected = None;
                        if !keyword.is_empty() {
                            return Some(AppAction::SearchBangumi { keyword });
                        }
                    }
                }
                KeyCode::Up => {
                    if self.search_input.is_empty() && !self.search_history.is_empty() {
                        let len = self.search_history.len();
                        let current = self.history_selected.unwrap_or(0);
                        self.history_selected = Some(if current == 0 { len - 1 } else { current - 1 });
                    }
                }
                KeyCode::Down => {
                    if self.search_input.is_empty() && !self.search_history.is_empty() {
                        let len = self.search_history.len();
                        let current = self.history_selected.unwrap_or(0);
                        self.history_selected = Some((current + 1) % len);
                    }
                }
                KeyCode::Backspace => {
                    self.search_input.pop();
                    self.history_selected = None;
                }
                KeyCode::Char(c) => {
                    self.search_input.push(c);
                    self.history_selected = None;
                }
                _ => {}
            }
            return Some(AppAction::None);
        }

        // "/" opens search input (not in follow mode)
        if !self.follow_mode && key == KeyCode::Char('/') {
            self.search_input_mode = true;
            self.search_input.clear();
            self.reload_search_history();
            return Some(AppAction::None);
        }

        if self.loading && !self.follow_mode && !self.is_searching() {
            return Some(AppAction::None);
        }
        if self.follow_loading && self.follow_mode {
            return Some(AppAction::None);
        }
        if self.search_loading && self.is_searching() {
            return Some(AppAction::None);
        }

        let grid = if self.follow_mode {
            &mut self.follow_grid
        } else if self.is_searching() {
            &mut self.search_grid
        } else {
            &mut self.index_grid
        };

        if keys.matches_page_down(key) {
            grid.move_page_down();
            return Some(AppAction::None);
        }
        if keys.matches_page_up(key) {
            grid.move_page_up();
            return Some(AppAction::None);
        }

        if keys.matches_down(key) {
            grid.move_down();
            return Some(AppAction::None);
        }
        if keys.matches_up(key) {
            grid.move_up();
            return Some(AppAction::None);
        }
        if keys.matches_left(key) {
            grid.move_left();
            return Some(AppAction::None);
        }
        if keys.matches_right(key) {
            grid.move_right();
            return Some(AppAction::None);
        }
        if keys.matches_play(key) || keys.matches_confirm(key) {
            return if self.follow_mode {
                self.selected_follow_action()
            } else if self.is_searching() {
                self.selected_search_action()
            } else {
                self.selected_index_action()
            };
        }

        Some(AppAction::None)
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        let MouseEvent {
            kind, row, column, ..
        } = event;

        let content_top = area.y + 3; // skip title
        if row < content_top {
            return None;
        }

        // Approximate grid click handling
        let rel_row = (row - content_top) as usize;
        let rel_col = column as usize;
        let columns = self.index_grid.columns;
        let card_height = self.index_grid.card_height as usize;
        let visible_row = rel_row / card_height;
        let approx_idx = visible_row * columns + rel_col / 20; // rough estimate

        if approx_idx < self.index_grid.cards.len() {
            self.index_grid.selected_index = approx_idx;

            if kind == MouseEventKind::Down(MouseButton::Left) {
                let now = Instant::now();
                let is_double = self
                    .last_click_time
                    .and_then(|t| now.duration_since(t).as_millis().le(&500).then_some(()))
                    .is_some()
                    && self.last_click_index == Some(approx_idx);

                self.last_click_time = Some(now);
                self.last_click_index = Some(approx_idx);

                if is_double {
                    return self.selected_index_action();
                }
            }
        }

        None
    }
}
