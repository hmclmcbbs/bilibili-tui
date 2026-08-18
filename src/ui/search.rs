//! Search page with video card grid display

use super::video_card::{VideoCard, VideoCardGrid};
use super::{Component, Theme, shortcut_footer};
use crate::api::client::ApiClient;
use crate::api::search::{HotwordItem, SearchVideoItem};
use crate::application::AppAction;
use crate::storage::Keybindings;
use ratatui::{
    crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind},
    prelude::*,
    widgets::*,
};
use std::time::Instant;

/// Search result type toggle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Video,
    User,
}

/// Which picker panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerFocus {
    History,
    Hotwords,
}

pub struct SearchPage {
    pub query: String,
    pub mode: SearchMode,
    pub grid: VideoCardGrid,
    pub user_grid: VideoCardGrid,
    pub loading: bool,
    pub user_loading: bool,
    pub error_message: Option<String>,
    pub user_error: Option<String>,
    pub input_mode: bool,
    pub hotwords: Vec<HotwordItem>,
    pub hotword_error: Option<String>,
    pub hotword_loading: bool,
    pub show_hot_list: bool,
    picker_focus: PickerFocus,
    history_selected: Option<usize>,
    hot_selected: Option<usize>,
    pub history: Vec<String>,
    pub page: i32,
    pub total_results: i32,
    pub loading_more: bool,
    pub user_page: i32,
    pub user_total: i32,
    pub user_loading_more: bool,
    last_click_time: Option<Instant>,
    last_click_index: Option<usize>,
}

impl SearchPage {
    pub fn new() -> Self {
        let mut user_grid = VideoCardGrid::new_list();
        user_grid.card_height = 8;
        let history = crate::storage::load_search_history();
        Self {
            query: String::new(),
            mode: SearchMode::Video,
            grid: VideoCardGrid::new_list(),
            user_grid,
            loading: false,
            user_loading: false,
            error_message: None,
            user_error: None,
            input_mode: true,
            hotwords: Vec::new(),
            hotword_error: None,
            hotword_loading: false,
            show_hot_list: true,
            picker_focus: if history.is_empty() {
                PickerFocus::Hotwords
            } else {
                PickerFocus::History
            },
            history_selected: if history.is_empty() { None } else { Some(0) },
            hot_selected: None,
            history,
            page: 1,
            total_results: 0,
            loading_more: false,
            user_page: 1,
            user_total: 0,
            user_loading_more: false,
            last_click_time: None,
            last_click_index: None,
        }
    }

    /// Reload persisted search history (called after a search is executed so
    /// the newest keyword appears at the top of the picker list).
    pub fn reload_history(&mut self) {
        self.history = crate::storage::load_search_history();
        match self.picker_focus {
            PickerFocus::History => {
                if self.history.is_empty() {
                    self.history_selected = None;
                    if !self.hotwords.is_empty() {
                        self.hot_selected = Some(0);
                        self.picker_focus = PickerFocus::Hotwords;
                    }
                } else if self.history_selected.is_none() {
                    self.history_selected = Some(0);
                }
            }
            PickerFocus::Hotwords => {
                if self.hotwords.is_empty() && !self.history.is_empty() {
                    self.picker_focus = PickerFocus::History;
                    self.history_selected = Some(0);
                }
            }
        }
    }

    fn history_len(&self) -> usize {
        self.history.len()
    }

    fn hotword_len(&self) -> usize {
        self.hotwords.len()
    }

    fn move_history(&mut self, delta: i32) {
        let len = self.history_len();
        if len == 0 {
            self.history_selected = None;
            return;
        }
        let current = self.history_selected.unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len as i32) as usize;
        self.history_selected = Some(next);
    }

    fn move_hotwords(&mut self, delta: i32) {
        let len = self.hotword_len();
        if len == 0 {
            self.hot_selected = None;
            return;
        }
        let current = self.hot_selected.unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len as i32) as usize;
        self.hot_selected = Some(next);
    }

    fn move_focus(&mut self, delta: i32) {
        match self.picker_focus {
            PickerFocus::History => self.move_history(delta),
            PickerFocus::Hotwords => self.move_hotwords(delta),
        }
    }

    /// 清空搜索历史：删除持久化文件，刷新内存列表，并把焦点切到热搜栏。
    fn clear_history(&mut self) {
        crate::storage::clear_search_history();
        self.history.clear();
        self.history_selected = None;
        if self.picker_focus == PickerFocus::History {
            if !self.hotwords.is_empty() {
                self.picker_focus = PickerFocus::Hotwords;
                self.hot_selected = Some(0);
            }
        }
    }

    /// j/k 导航：历史栏在最底部按 j 切到热搜栏，热搜栏在最顶部按 k 切回历史栏。
    fn picker_nav(&mut self, delta: i32) {
        match self.picker_focus {
            PickerFocus::History => {
                if delta > 0 {
                    let len = self.history_len();
                    let cur = self.history_selected.unwrap_or(0);
                    if len > 0 && cur + 1 >= len {
                        // 到底部再按 j/↓：切到热搜栏（即使热搜还在加载也切，让用户看到焦点移动）
                        self.picker_focus = PickerFocus::Hotwords;
                        self.hot_selected = if self.hotwords.is_empty() { None } else { Some(0) };
                        return;
                    }
                    self.move_history(1);
                } else {
                    self.move_history(-1);
                }
            }
            PickerFocus::Hotwords => {
                if delta < 0 {
                    let cur = self.hot_selected.unwrap_or(0);
                    if cur == 0 {
                        if !self.history.is_empty() {
                            self.picker_focus = PickerFocus::History;
                            self.history_selected = Some(self.history_len().saturating_sub(1));
                        }
                        return;
                    }
                    self.move_hotwords(-1);
                } else {
                    self.move_hotwords(1);
                }
            }
        }
    }

    /// 按左方向键（h）时是否应该回到侧边栏：
    /// - 输入模式（input_mode）时不拦截，h 作为字符输入
    /// - 热词列表状态：h 没有其他用途，直接回侧边栏
    /// - 结果列表：已处于最左列时回侧边栏，否则交给网格左移
    pub fn wants_left_to_sidebar(&self) -> bool {
        // 输入模式但还没有任何字符时（正在浏览搜索历史/热搜下拉栏），
        // h 键应该回到侧边栏而不是当作字符输入。
        // 输入模式：有字符时 h 永远作为字符；无字符时仅当下拉栏有可选项才回侧边栏，
        // 否则 h 作为普通字符输入（方便输入含 h 的搜索词）。
        if self.input_mode {
            if !self.query.is_empty() {
                return false;
            }
            if self.show_hot_list && self.picker_has_items() {
                return true;
            }
            return false;
        }
        if self.show_hot_list {
            // 下拉栏可见时 h/← 一律回侧边栏；历史/热搜栏之间的切换用 j/k 在底部/顶部完成
            return true;
        }
        match self.mode {
            SearchMode::Video => self.grid.selected_index % self.grid.columns == 0,
            SearchMode::User => self.user_grid.selected_index % self.user_grid.columns == 0,
        }
    }

    /// 下拉栏当前是否有可导航的项（历史或热搜）。
    fn picker_has_items(&self) -> bool {
        if self.picker_focus == PickerFocus::History {
            !self.history.is_empty()
        } else {
            !self.hotwords.is_empty()
        }
    }
    pub fn set_results(&mut self, results: Vec<SearchVideoItem>, total: i32) {
        self.grid.clear();
        for item in results {
            let card = VideoCard::new(
                item.bvid.clone(),
                item.aid,
                item.display_title(),
                item.author_name().to_string(),
                item.format_play(),
                item.duration.clone().unwrap_or_default(),
                item.cover_url(),
            )
            .with_uploader_mid(item.mid);
            self.grid.add_card(card);
        }
        self.total_results = total;
        self.loading = false;
        self.input_mode = false;
        self.show_hot_list = false;
        self.error_message = None;
    }

    pub fn append_results(&mut self, results: Vec<SearchVideoItem>) {
        for item in results {
            let card = VideoCard::new(
                item.bvid.clone(),
                item.aid,
                item.display_title(),
                item.author_name().to_string(),
                item.format_play(),
                item.duration.clone().unwrap_or_default(),
                item.cover_url(),
            )
            .with_uploader_mid(item.mid);
            self.grid.add_card(card);
        }
        self.loading_more = false;
    }

    pub fn set_user_results(&mut self, results: Vec<crate::api::search::SearchUserItem>, total: i32) {
        self.user_grid.clear();
        for item in results {
            let card = VideoCard::user(
                item.mid,
                item.display_name(),
                item.format_fans(),
                item.format_videos(),
                item.sign_text(),
                item.face_url(),
            );
            self.user_grid.add_card(card);
        }
        self.user_total = total;
        self.user_loading = false;
        self.input_mode = false;
        self.show_hot_list = false;
        self.user_error = None;
    }

    pub fn append_user_results(&mut self, results: Vec<crate::api::search::SearchUserItem>) {
        for item in results {
            let card = VideoCard::user(
                item.mid,
                item.display_name(),
                item.format_fans(),
                item.format_videos(),
                item.sign_text(),
                item.face_url(),
            );
            self.user_grid.add_card(card);
        }
        self.user_loading_more = false;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.loading = false;
        self.loading_more = false;
        self.show_hot_list = false;
    }

    pub fn set_user_error(&mut self, msg: String) {
        self.user_error = Some(msg);
        self.user_loading = false;
        self.user_loading_more = false;
        self.show_hot_list = false;
    }

    pub fn start_hotword_loading(&mut self) {
        self.hotword_loading = true;
        self.hotword_error = None;
        self.hot_selected = None;
    }

    pub fn set_hotwords(&mut self, hotwords: Vec<HotwordItem>) {
        self.hotwords = hotwords;
        self.hotword_loading = false;
        self.hotword_error = None;
        self.hot_selected = if self.hotwords.is_empty() {
            None
        } else {
            Some(0)
        };
        // 历史为空时默认焦点落到热搜栏
        if self.history.is_empty() && !self.hotwords.is_empty() {
            self.picker_focus = PickerFocus::Hotwords;
        }
    }

    pub fn set_hotword_error(&mut self, msg: String) {
        self.hotword_error = Some(msg);
        self.hotword_loading = false;
    }

    pub async fn load_more(&mut self, api_client: &ApiClient) {
        if self.loading_more || self.query.is_empty() || self.show_hot_list {
            return;
        }

        if self.mode == SearchMode::User {
            return;
        }

        // Check if we have more results
        if self.grid.cards.len() >= self.total_results as usize {
            return;
        }

        self.loading_more = true;
        self.page += 1;

        match api_client.search_videos(&self.query, self.page).await {
            Ok(data) => {
                let results = data.result.unwrap_or_default();
                if results.is_empty() {
                    self.page -= 1;
                }
                self.append_results(results);
            }
            Err(_) => {
                self.page -= 1;
                self.loading_more = false;
            }
        }
    }

    pub async fn load_more_users(&mut self, api_client: &ApiClient) {
        if self.user_loading_more || self.query.is_empty() || self.show_hot_list {
            return;
        }

        if self.user_grid.cards.len() >= self.user_total as usize {
            return;
        }

        self.user_loading_more = true;
        self.user_page += 1;

        match api_client.search_users(&self.query, self.user_page).await {
            Ok(data) => {
                let results = data.result.unwrap_or_default();
                if results.is_empty() {
                    self.user_page -= 1;
                }
                self.append_user_results(results);
            }
            Err(_) => {
                self.user_page -= 1;
                self.user_loading_more = false;
            }
        }
    }

    pub fn poll_cover_results(&mut self) {
        self.grid.poll_cover_results();
        self.user_grid.poll_cover_results();
    }

    pub fn start_cover_downloads(&mut self) {
        self.grid.start_cover_downloads();
        self.user_grid.start_cover_downloads();
    }

    fn search_selected_picker(&mut self) -> Option<AppAction> {
        let keyword = match self.picker_focus {
            PickerFocus::History => {
                let idx = self.history_selected?;
                self.history.get(idx).cloned()?
            }
            PickerFocus::Hotwords => {
                let idx = self.hot_selected?;
                self.hotwords.get(idx).and_then(|item| item.keyword_text())?
            }
        };
        self.query = keyword.clone();
        self.loading = true;
        self.page = 1;
        self.show_hot_list = false;
        Some(AppAction::Search(keyword))
    }

    fn handle_user_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        match event.kind {
            MouseEventKind::ScrollDown => {
                if self.user_grid.move_down()
                    && self.user_grid.is_near_bottom(3)
                    && !self.user_loading_more
                {
                    return Some(AppAction::LoadMoreSearchUsers);
                }
                None
            }
            MouseEventKind::ScrollUp => {
                self.user_grid.move_up();
                None
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(10),
                        Constraint::Length(2),
                    ])
                    .split(area);

                let header_height = 2u16;
                let grid_area = Rect {
                    y: chunks[1].y + header_height,
                    height: chunks[1].height.saturating_sub(header_height),
                    x: chunks[1].x,
                    width: chunks[1].width,
                };

                if !grid_area.contains(ratatui::layout::Position::new(event.column, event.row)) {
                    return None;
                }

                let relative_y = event.row - grid_area.y;
                let click_row = (relative_y / self.user_grid.card_height) as usize;
                let actual_row = self.user_grid.scroll_row + click_row;

                let card_width = grid_area.width / self.user_grid.columns as u16;
                let click_col = (event.column.saturating_sub(grid_area.x) / card_width) as usize;

                let click_idx = actual_row * self.user_grid.columns + click_col;

                if click_idx < self.user_grid.cards.len() {
                    let now = Instant::now();
                    let is_double_click = self.last_click_index == Some(click_idx)
                        && self
                            .last_click_time
                            .is_some_and(|t| now.duration_since(t).as_millis() < 500);

                    if is_double_click {
                        self.last_click_time = None;
                        self.last_click_index = None;
                        if let Some(mid) = self
                            .user_grid
                            .cards
                            .get(click_idx)
                            .and_then(|card| card.uploader_mid)
                        {
                            return Some(AppAction::OpenUpPage(mid));
                        }
                    } else {
                        self.user_grid.selected_index = click_idx;
                        self.user_grid.update_scroll(self.user_grid.cached_visible_rows);
                        self.last_click_time = Some(now);
                        self.last_click_index = Some(click_idx);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn draw_hot_list(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // 两个独立栏：搜索历史（上）、热搜（下），各自带边框
        let hist_h = if self.history.is_empty() {
            0
        } else {
            (self.history.len() as u16 + 2).min(8)
        };
        let hot_h = if self.hotword_loading || !self.hotword_error.is_none() || !self.hotwords.is_empty()
        {
            (self.hotwords.len() as u16 + 2).min(8).max(3)
        } else {
            0
        };

        if hist_h == 0 && hot_h == 0 {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_subtle))
                .title(Span::styled(
                    " 搜索历史 / 热搜 ",
                    Style::default().fg(theme.bilibili_pink),
                ));
            let empty = Paragraph::new("暂无热搜数据")
                .style(Style::default().fg(theme.fg_secondary))
                .alignment(Alignment::Center)
                .block(block);
            frame.render_widget(empty, area);
            return;
        }

        let (hist_area, hot_area) = if hist_h > 0 && hot_h > 0 {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(hist_h), Constraint::Length(hot_h)])
                .split(area);
            (layout[0], layout[1])
        } else if hist_h > 0 {
            (area, Rect::default())
        } else {
            (Rect::default(), area)
        };

        if hist_h > 0 {
            self.draw_history_picker(frame, hist_area, theme);
        }
        if hot_h > 0 {
            self.draw_hotword_picker(frame, hot_area, theme);
        }
    }

    fn draw_history_picker(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let focused = self.picker_focus == PickerFocus::History;
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if focused {
                theme.bilibili_pink
            } else {
                theme.border_subtle
            }))
            .title(Span::styled(
                if focused {
                    " 搜索历史 (x 清空) "
                } else {
                    " 搜索历史 "
                },
                Style::default().fg(if focused {
                    theme.bilibili_pink
                } else {
                    theme.fg_muted
                }),
            ));

        let mut items: Vec<ListItem> = Vec::new();
        for keyword in &self.history {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("历史 ", Style::default().fg(theme.fg_muted)),
                Span::styled(keyword.as_str(), Style::default().fg(theme.fg_primary)),
            ])));
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().fg(theme.bilibili_pink).bg(theme.bg_highlight))
            .highlight_symbol("▶ ");
        let mut state = ListState::default().with_selected(self.history_selected);
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_hotword_picker(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let focused = self.picker_focus == PickerFocus::Hotwords;
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if focused {
                theme.bilibili_pink
            } else {
                theme.border_subtle
            }))
            .title(Span::styled(
                " 热搜 ",
                Style::default().fg(if focused {
                    theme.bilibili_pink
                } else {
                    theme.fg_muted
                }),
            ));

        if self.hotword_loading {
            let loading = Paragraph::new("⏳ 正在获取热搜...")
                .style(Style::default().fg(theme.fg_secondary))
                .alignment(Alignment::Center)
                .block(block);
            frame.render_widget(loading, area);
            return;
        }

        if let Some(err) = &self.hotword_error {
            let error_widget = Paragraph::new(format!("❌ {}", err))
                .style(Style::default().fg(theme.error))
                .alignment(Alignment::Center)
                .block(block);
            frame.render_widget(error_widget, area);
            return;
        }

        let mut items: Vec<ListItem> = Vec::new();
        for (idx, item) in self.hotwords.iter().enumerate() {
            let mut spans = vec![
                Span::styled(
                    format!("{:>2}. ", idx + 1),
                    Style::default().fg(theme.fg_muted),
                ),
                Span::styled(item.display_text(), Style::default().fg(theme.fg_primary)),
            ];

            if let Some(badge) = item.badge() {
                spans.push(Span::styled(
                    format!(" [{}]", badge),
                    Style::default().fg(theme.bilibili_pink),
                ));
            }

            items.push(ListItem::new(Line::from(spans)));
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().fg(theme.bilibili_pink).bg(theme.bg_highlight))
            .highlight_symbol("▶ ");
        let mut state = ListState::default().with_selected(self.hot_selected);
        frame.render_stateful_widget(list, area, &mut state);
    }
}

impl Default for SearchPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for SearchPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(10),   // Results grid
                Constraint::Length(2), // Help
            ])
            .split(area);

        // Search input
        let input_style = if self.input_mode {
            Style::default().fg(theme.warning)
        } else {
            Style::default().fg(theme.fg_primary)
        };

        let mode_title = match self.mode {
            SearchMode::Video => " 🔍 搜索视频  [1]视频 [2]UP主 ",
            SearchMode::User => " 🔍 搜索UP主  [1]视频 [2]UP主 ",
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if self.input_mode {
                Style::default().fg(theme.bilibili_pink)
            } else {
                Style::default().fg(theme.border_subtle)
            })
            .title(Span::styled(
                mode_title,
                Style::default().fg(theme.bilibili_pink),
            ));

        let cursor_char = if self.input_mode { "▌" } else { "" };
        let input = Paragraph::new(format!("{}{}", self.query, cursor_char))
            .style(input_style)
            .block(input_block);
        frame.render_widget(input, chunks[0]);

        // Results
        if self.show_hot_list {
            // 搜索下拉栏：输入框正下方，固定小高度
            let hist_h = if self.history.is_empty() {
                0
            } else {
                (self.history.len() as u16 + 2).min(8)
            };
            let hot_h = if self.hotword_loading
                || !self.hotword_error.is_none()
                || !self.hotwords.is_empty()
            {
                (self.hotwords.len() as u16 + 2).min(8).max(3)
            } else {
                0
            };
            let dropdown_height = (hist_h + hot_h).max(3).min(16);
            let dropdown_area = Rect {
                y: chunks[0].y + chunks[0].height,
                height: dropdown_height,
                x: chunks[0].x,
                width: chunks[0].width,
            };
            self.draw_hot_list(frame, dropdown_area, theme);

            // 下拉栏下方：显示已有结果或提示
            let results_top = dropdown_area.y + dropdown_area.height;
            let results_area = Rect {
                y: results_top,
                height: chunks[1].height.saturating_sub(results_top.saturating_sub(chunks[1].y)),
                ..chunks[1]
            };
            if self.mode == SearchMode::User {
                if self.user_grid.cards.is_empty() {
                    let empty = Paragraph::new(if self.query.is_empty() {
                        "输入关键词开始搜索UP主"
                    } else {
                        "没有找到相关UP主"
                    })
                    .style(Style::default().fg(theme.fg_secondary))
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(theme.border_unfocused)),
                    );
                    frame.render_widget(empty, results_area);
                } else {
                    self.user_grid.render(frame, results_area, theme);
                }
            } else if self.grid.cards.is_empty() {
                let empty = Paragraph::new(if self.query.is_empty() {
                    "输入关键词开始搜索"
                } else {
                    "没有找到相关视频"
                })
                .style(Style::default().fg(theme.fg_secondary))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme.border_unfocused)),
                );
                frame.render_widget(empty, results_area);
            } else {
                self.grid.render(frame, results_area, theme);
            }
        } else if self.mode == SearchMode::User {
            if self.user_loading {
                let loading = Paragraph::new("⏳ 搜索UP主中...")
                    .style(Style::default().fg(theme.warning))
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(theme.border_unfocused))
                            .title(Span::styled(
                                format!(" 结果 ({}) ", self.user_total),
                                Style::default().fg(theme.fg_secondary),
                            )),
                    );
                frame.render_widget(loading, chunks[1]);
            } else if let Some(error) = &self.user_error {
                let error_widget = Paragraph::new(format!("❌ {}", error))
                    .style(Style::default().fg(theme.error))
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(theme.border_unfocused)),
                    );
                frame.render_widget(error_widget, chunks[1]);
            } else if self.user_grid.cards.is_empty() {
                let empty = Paragraph::new(if self.query.is_empty() {
                    "输入关键词开始搜索UP主"
                } else {
                    "没有找到相关UP主"
                })
                .style(Style::default().fg(theme.fg_secondary))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme.border_unfocused)),
                );
                frame.render_widget(empty, chunks[1]);
            } else {
                let header = Paragraph::new(Line::from(vec![
                    Span::styled(" 搜索结果 ", Style::default().fg(theme.bilibili_pink)),
                    Span::styled(
                        format!("({}/{})", self.user_grid.cards.len(), self.user_total),
                        Style::default().fg(theme.fg_muted),
                    ),
                    if self.user_loading_more {
                        Span::styled(" 加载中...", Style::default().fg(theme.warning))
                    } else {
                        Span::raw("")
                    },
                ]))
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme.border_subtle)),
                );

                let header_area = Rect {
                    height: 2,
                    ..chunks[1]
                };
                let grid_area = Rect {
                    y: chunks[1].y + 2,
                    height: chunks[1].height.saturating_sub(2),
                    ..chunks[1]
                };

                frame.render_widget(header, header_area);
                self.user_grid.render(frame, grid_area, theme);
            }
        } else if self.loading {
            let loading = Paragraph::new("⏳ 搜索中...")
                .style(Style::default().fg(theme.warning))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme.border_unfocused))
                        .title(Span::styled(
                            format!(" 结果 ({}) ", self.total_results),
                            Style::default().fg(theme.fg_secondary),
                        )),
                );
            frame.render_widget(loading, chunks[1]);
        } else if let Some(error) = &self.error_message {
            let error_widget = Paragraph::new(format!("❌ {}", error))
                .style(Style::default().fg(theme.error))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme.border_unfocused)),
                );
            frame.render_widget(error_widget, chunks[1]);
        } else if self.grid.cards.is_empty() {
            let empty = Paragraph::new(if self.query.is_empty() {
                "输入关键词开始搜索"
            } else {
                "没有找到相关视频"
            })
            .style(Style::default().fg(theme.fg_secondary))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_unfocused)),
            );
            frame.render_widget(empty, chunks[1]);
        } else {
            // Render with header
            let header = Paragraph::new(Line::from(vec![
                Span::styled(" 搜索结果 ", Style::default().fg(theme.bilibili_pink)),
                Span::styled(
                    format!("({}/{})", self.grid.cards.len(), self.total_results),
                    Style::default().fg(theme.fg_muted),
                ),
                if self.loading_more {
                    Span::styled(" 加载中...", Style::default().fg(theme.warning))
                } else {
                    Span::raw("")
                },
            ]))
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_subtle)),
            );

            let header_area = Rect {
                height: 2,
                ..chunks[1]
            };
            let grid_area = Rect {
                y: chunks[1].y + 2,
                height: chunks[1].height.saturating_sub(2),
                ..chunks[1]
            };

            frame.render_widget(header, header_area);
            self.grid.render(frame, grid_area, theme);
        }

        // Help
        let help = if self.input_mode {
            shortcut_footer(
                theme,
                [
                    (keys.confirm.clone(), "搜索".into(), theme.success),
                    (keys.back.clone(), "取消".into(), theme.info),
                    (keys.nav_next_page.clone(), "导航".into(), theme.fg_accent),
                ],
            )
        } else {
            shortcut_footer(
                theme,
                [
                    (
                        format!(
                            "{}/{}, {}/{}",
                            keys.get_arrow_keys_display(),
                            keys.get_nav_keys_display(),
                            keys.page_up,
                            keys.page_down
                        ),
                        "导航/翻页".into(),
                        theme.fg_accent,
                    ),
                    (keys.confirm.clone(), "详情".into(), theme.success),
                    (keys.search_focus.clone(), "搜索".into(), theme.info),
                    ("1/2".into(), "视频/UP".into(), theme.fg_accent),
                    ("x".into(), "清历史".into(), theme.fg_accent),
                    (keys.nav_next_page.clone(), "切换".into(), theme.info),
                ],
            )
        };
        let help = Paragraph::new(help).alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    fn handle_input(
        &mut self,
        key: KeyCode,
        keys: &crate::storage::Keybindings,
    ) -> Option<AppAction> {
        // Tab switching: 1 = video, 2 = user
        if key == KeyCode::Char('1') && self.mode != SearchMode::Video {
            self.mode = SearchMode::Video;
            self.show_hot_list = false;
            if !self.query.trim().is_empty() && !self.input_mode {
                self.loading = true;
                self.page = 1;
                return Some(AppAction::Search(self.query.clone()));
            }
            return Some(AppAction::None);
        }
        if key == KeyCode::Char('2') && self.mode != SearchMode::User {
            self.mode = SearchMode::User;
            self.show_hot_list = false;
            if !self.query.trim().is_empty() && !self.input_mode {
                self.user_loading = true;
                self.user_page = 1;
                return Some(AppAction::SearchUsers(self.query.clone()));
            }
            return Some(AppAction::None);
        }

        if self.input_mode {
            match key {
                KeyCode::Char(c) => {
                    // 下拉栏可见且还没输入任何字符时，j/k 作为导航键（仅当下拉栏有可选项）
                    if self.show_hot_list && self.query.is_empty() && self.picker_has_items() {
                        if c == 'j' {
                            self.picker_nav(1);
                            return Some(AppAction::None);
                        }
                        if c == 'k' {
                            self.picker_nav(-1);
                            return Some(AppAction::None);
                        }
                        if c == 'x' || c == 'X' {
                            if self.picker_focus == PickerFocus::History && !self.history.is_empty() {
                                self.clear_history();
                                return Some(AppAction::None);
                            }
                        }
                    }
                    self.query.push(c);
                    self.show_hot_list = true;
                    match self.picker_focus {
                        PickerFocus::History => {
                            if self.history_selected.is_none() && !self.history.is_empty() {
                                self.history_selected = Some(0);
                            }
                        }
                        PickerFocus::Hotwords => {
                            if self.hot_selected.is_none() && !self.hotwords.is_empty() {
                                self.hot_selected = Some(0);
                            }
                        }
                    }
                    Some(AppAction::None)
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.show_hot_list = true;
                    match self.picker_focus {
                        PickerFocus::History => {
                            if self.history_selected.is_none() && !self.history.is_empty() {
                                self.history_selected = Some(0);
                            }
                        }
                        PickerFocus::Hotwords => {
                            if self.hot_selected.is_none() && !self.hotwords.is_empty() {
                                self.hot_selected = Some(0);
                            }
                        }
                    }
                    Some(AppAction::None)
                }
                KeyCode::Up => {
                    if self.show_hot_list {
                        self.picker_nav(-1);
                    }
                    Some(AppAction::None)
                }
                KeyCode::Down => {
                    if self.show_hot_list {
                        self.picker_nav(1);
                    }
                    Some(AppAction::None)
                }
                KeyCode::Left => {
                    // 下拉栏可见时回侧边栏由外层处理；否则作为字符输入
                    self.query.push('h');
                    self.show_hot_list = true;
                    return Some(AppAction::None);
                }
                KeyCode::Right => {
                    // 不再用于切换栏，作为字符输入
                    self.query.push('l');
                    self.show_hot_list = true;
                    return Some(AppAction::None);
                }
                KeyCode::Enter => {
                    if !self.query.trim().is_empty() {
                        self.show_hot_list = false;
                        match self.mode {
                            SearchMode::Video => {
                                self.loading = true;
                                self.page = 1;
                                Some(AppAction::Search(self.query.clone()))
                            }
                            SearchMode::User => {
                                self.user_loading = true;
                                self.user_page = 1;
                                Some(AppAction::SearchUsers(self.query.clone()))
                            }
                        }
                    } else if self.show_hot_list {
                        self.search_selected_picker()
                    } else {
                        Some(AppAction::None)
                    }
                }
                KeyCode::Delete => {
                    if self.show_hot_list
                        && self.query.is_empty()
                        && self.picker_focus == PickerFocus::History
                        && !self.history.is_empty()
                    {
                        self.clear_history();
                        return Some(AppAction::None);
                    }
                    Some(AppAction::None)
                }
                KeyCode::Esc => {
                    self.input_mode = false;
                    Some(AppAction::None)
                }
                _ if keys.matches_nav_next(key) => Some(AppAction::NavNext),
                _ if keys.matches_nav_prev(key) => Some(AppAction::NavPrev),
                _ => Some(AppAction::None),
            }
        } else if self.show_hot_list {
            if keys.matches_up(key) || key == KeyCode::Char('k') {
                self.picker_nav(-1);
                return Some(AppAction::None);
            }
            if keys.matches_down(key) || key == KeyCode::Char('j') {
                self.picker_nav(1);
                return Some(AppAction::None);
            }
            if keys.matches_confirm(key) {
                return self.search_selected_picker();
            }
            if key == KeyCode::Char('x')
                || key == KeyCode::Char('X')
                || key == KeyCode::Delete
            {
                if self.picker_focus == PickerFocus::History && !self.history.is_empty() {
                    self.clear_history();
                    return Some(AppAction::None);
                }
            }
            if keys.matches_search_focus(key) {
                self.input_mode = true;
                self.show_hot_list = true;
                return Some(AppAction::None);
            }
            if keys.matches_nav_next(key) {
                return Some(AppAction::NavNext);
            }
            if keys.matches_nav_prev(key) {
                return Some(AppAction::NavPrev);
            }
            if keys.matches_quit(key) {
                return Some(AppAction::Quit);
            }
            Some(AppAction::None)
        } else {
            match self.mode {
                SearchMode::User => {
                    if keys.matches_page_down(key) {
                        self.user_grid.move_page_down();
                        if self.user_grid.is_near_bottom(self.user_grid.cached_visible_rows)
                            && !self.user_loading_more
                        {
                            return Some(AppAction::LoadMoreSearchUsers);
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_page_up(key) {
                        self.user_grid.move_page_up();
                        return Some(AppAction::None);
                    }
                    if keys.matches_down(key) {
                        self.user_grid.move_down();
                        if self.user_grid.is_near_bottom(3) && !self.user_loading_more {
                            return Some(AppAction::LoadMoreSearchUsers);
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_up(key) {
                        self.user_grid.move_up();
                        return Some(AppAction::None);
                    }
                    if keys.matches_right(key) {
                        self.user_grid.move_right();
                        return Some(AppAction::None);
                    }
                    if keys.matches_left(key) {
                        self.user_grid.move_left();
                        return Some(AppAction::None);
                    }
                    if keys.matches_confirm(key) {
                        if let Some(mid) = self
                            .user_grid
                            .selected_card()
                            .and_then(|card| card.uploader_mid)
                        {
                            return Some(AppAction::OpenUpPage(mid));
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_search_focus(key) {
                        self.input_mode = true;
                        self.show_hot_list = true;
                        if self.hot_selected.is_none() && !self.hotwords.is_empty() {
                            self.hot_selected = Some(0);
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_nav_next(key) {
                        return Some(AppAction::NavNext);
                    }
                    if keys.matches_nav_prev(key) {
                        return Some(AppAction::NavPrev);
                    }
                    if keys.matches_quit(key) {
                        return Some(AppAction::Quit);
                    }
                    Some(AppAction::None)
                }
                SearchMode::Video => {
                    if keys.matches_page_down(key) {
                        self.grid.move_page_down();
                        if self.grid.is_near_bottom(self.grid.cached_visible_rows)
                            && !self.loading_more
                        {
                            return Some(AppAction::LoadMoreSearch);
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_page_up(key) {
                        self.grid.move_page_up();
                        return Some(AppAction::None);
                    }
                    if keys.matches_down(key) {
                        self.grid.move_down();
                        // Check for pagination
                        if self.grid.is_near_bottom(3) && !self.loading_more {
                            return Some(AppAction::LoadMoreSearch);
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_up(key) {
                        self.grid.move_up();
                        return Some(AppAction::None);
                    }
                    if keys.matches_right(key) {
                        self.grid.move_right();
                        return Some(AppAction::None);
                    }
                    if keys.matches_left(key) {
                        self.grid.move_left();
                        return Some(AppAction::None);
                    }
                    if key == KeyCode::Char('u')
                        && let Some(mid) =
                            self.grid.selected_card().and_then(|card| card.uploader_mid)
                    {
                        return Some(AppAction::OpenUpPage(mid));
                    }
                    if keys.matches_confirm(key) {
                        if let Some(card) = self.grid.selected_card()
                            && let (Some(bvid), Some(aid)) = (&card.bvid, card.aid)
                        {
                            return Some(AppAction::OpenVideoDetail(bvid.clone(), aid));
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_search_focus(key) {
                        self.input_mode = true;
                        self.show_hot_list = true;
                        if self.hot_selected.is_none() && !self.hotwords.is_empty() {
                            self.hot_selected = Some(0);
                        }
                        return Some(AppAction::None);
                    }
                    if keys.matches_nav_next(key) {
                        return Some(AppAction::NavNext);
                    }
                    if keys.matches_nav_prev(key) {
                        return Some(AppAction::NavPrev);
                    }
                    if keys.matches_quit(key) {
                        return Some(AppAction::Quit);
                    }
                    Some(AppAction::None)
                }
            }
        }
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        // Don't handle mouse in input mode
        if self.input_mode {
            return None;
        }

        // Handle hot list mouse interactions
        if self.show_hot_list {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(2),
                ])
                .split(area);

            let hist_h = if self.history.is_empty() {
                0
            } else {
                (self.history.len() as u16 + 2).min(8)
            };
            let hot_h = if self.hotword_loading
                || !self.hotword_error.is_none()
                || !self.hotwords.is_empty()
            {
                (self.hotwords.len() as u16 + 2).min(8).max(3)
            } else {
                0
            };
            let dropdown_height = (hist_h + hot_h).max(3).min(16);
            let dropdown_area = Rect {
                y: chunks[0].y + chunks[0].height,
                height: dropdown_height,
                x: chunks[0].x,
                width: chunks[0].width,
            };

            if !dropdown_area.contains(ratatui::layout::Position::new(event.column, event.row)) {
                return None;
            }

            return match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    // 历史栏区域
                    let hist_area = Rect {
                        y: dropdown_area.y,
                        height: hist_h.min(dropdown_area.height),
                        ..dropdown_area
                    };
                    let hot_area = Rect {
                        y: dropdown_area.y + hist_area.height,
                        height: dropdown_area.height.saturating_sub(hist_area.height),
                        ..dropdown_area
                    };
                    if hist_h > 0
                        && hist_area.contains(ratatui::layout::Position::new(
                            event.column,
                            event.row,
                        ))
                    {
                        let idx = event.row.saturating_sub(hist_area.y + 1) as usize;
                        if idx < self.history.len() {
                            self.picker_focus = PickerFocus::History;
                            self.history_selected = Some(idx);
                            return self.search_selected_picker();
                        }
                        return None;
                    }
                    if hot_h > 0
                        && hot_area.contains(ratatui::layout::Position::new(
                            event.column,
                            event.row,
                        ))
                    {
                        let idx = event.row.saturating_sub(hot_area.y + 1) as usize;
                        if idx < self.hotwords.len() {
                            self.picker_focus = PickerFocus::Hotwords;
                            self.hot_selected = Some(idx);
                            return self.search_selected_picker();
                        }
                    }
                    None
                }
                MouseEventKind::ScrollDown => {
                    self.move_focus(1);
                    Some(AppAction::None)
                }
                MouseEventKind::ScrollUp => {
                    self.move_focus(-1);
                    Some(AppAction::None)
                }
                _ => None,
            };
        }

        if self.show_hot_list {
            return None;
        }

        if self.mode == SearchMode::User {
            return self.handle_user_mouse(event, area);
        }

        match event.kind {
            MouseEventKind::ScrollDown => {
                if self.grid.move_down() {
                    // Only check pagination if actually moved
                    if self.grid.is_near_bottom(3) && !self.loading_more {
                        return Some(AppAction::LoadMoreSearch);
                    }
                }
                None
            }
            MouseEventKind::ScrollUp => {
                self.grid.move_up();
                None
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(10),
                        Constraint::Length(2),
                    ])
                    .split(area);

                let header_height = 2u16;
                let grid_area = Rect {
                    y: chunks[1].y + header_height,
                    height: chunks[1].height.saturating_sub(header_height),
                    x: chunks[1].x,
                    width: chunks[1].width,
                };

                if !grid_area.contains(ratatui::layout::Position::new(event.column, event.row)) {
                    return None;
                }

                let relative_y = event.row - grid_area.y;
                let click_row = (relative_y / self.grid.card_height) as usize;
                let actual_row = self.grid.scroll_row + click_row;

                let card_width = grid_area.width / self.grid.columns as u16;
                let click_col = (event.column.saturating_sub(grid_area.x) / card_width) as usize;

                let click_idx = actual_row * self.grid.columns + click_col;

                if click_idx < self.grid.cards.len() {
                    let now = Instant::now();
                    let is_double_click = self.last_click_index == Some(click_idx)
                        && self
                            .last_click_time
                            .is_some_and(|t| now.duration_since(t).as_millis() < 500);

                    if is_double_click {
                        self.last_click_time = None;
                        self.last_click_index = None;
                        if let Some(card) = self.grid.cards.get(click_idx)
                            && let (Some(bvid), Some(aid)) = (&card.bvid, card.aid)
                        {
                            return Some(AppAction::OpenVideoDetail(bvid.clone(), aid));
                        }
                    } else {
                        self.grid.selected_index = click_idx;
                        self.grid.update_scroll(self.grid.cached_visible_rows);
                        self.last_click_time = Some(now);
                        self.last_click_index = Some(click_idx);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
