//! Dynamic feed page with video card grid display

use super::video_card::{VideoCard, VideoCardGrid};
use super::{Component, Theme, shortcut_footer};
use crate::api::client::ApiClient;
use crate::api::dynamic::DynamicItem;
use crate::application::AppAction;
use crate::storage::Keybindings;
use ratatui::{
    crossterm::event::{KeyCode, MouseButton, MouseEvent},
    prelude::*,
    widgets::*,
};
use std::collections::HashMap;
use std::time::Instant;

/// Dynamic feed tab types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicTab {
    /// All dynamics (视频+图文)
    All,
    /// Video dynamics only
    Videos,
    /// Image/Opus dynamics (图文动态)
    Images,
}

impl DynamicTab {
    pub fn label(&self) -> &str {
        match self {
            DynamicTab::All => "全部",
            DynamicTab::Videos => "视频",
            DynamicTab::Images => "图文",
        }
    }

    pub fn all_tabs() -> [DynamicTab; 3] {
        [DynamicTab::All, DynamicTab::Videos, DynamicTab::Images]
    }

    /// Get the API feed type parameter for this tab
    pub fn get_feed_type(&self) -> Option<&str> {
        match self {
            DynamicTab::All => None, // No type filter = all types
            DynamicTab::Videos => Some("video"),
            DynamicTab::Images => Some("draw"), // draw type includes both draw and opus
        }
    }
}

pub struct DynamicPage {
    pub grid: VideoCardGrid,
    pub loading: bool,
    pub error_message: Option<String>,
    pub offset: Option<String>,
    pub has_more: bool,
    pub loading_more: bool,
    pub current_tab: DynamicTab,
    pub tab_offsets: HashMap<DynamicTab, Option<String>>,
    pub up_list: Vec<crate::api::dynamic::UpListItem>,
    pub selected_up_index: usize,
    pub focus_up_list: bool,
    pub loading_up_list: bool,
    pub up_list_scroll_offset: usize,
    pub dynamic_items: Vec<DynamicItem>,
    last_click_time: Option<Instant>,
    last_click_index: Option<usize>,
    input_mode: bool,
    input_is_search: bool,
    input_text: String,
    filter_text: Option<String>,
    message: Option<String>,
}

impl DynamicPage {
    pub fn new() -> Self {
        Self {
            grid: VideoCardGrid::new_list(),
            loading: true,
            error_message: None,
            offset: None,
            has_more: false,
            loading_more: false,
            current_tab: DynamicTab::All,
            tab_offsets: HashMap::new(),
            up_list: Vec::new(),
            selected_up_index: 0,
            focus_up_list: true,
            loading_up_list: false,
            up_list_scroll_offset: 0,
            dynamic_items: Vec::new(),
            last_click_time: None,
            last_click_index: None,
            input_mode: false,
            input_is_search: false,
            input_text: String::new(),
            filter_text: None,
            message: None,
        }
    }

    pub fn set_message(&mut self, msg: String) {
        self.message = Some(msg);
    }

    /// Build a VideoCard for a dynamic item, if it matches the current tab.
    fn build_card_for_item(&self, item: &DynamicItem) -> Option<VideoCard> {
        let should_include = match self.current_tab {
            DynamicTab::All => item.is_video() || item.is_draw() || item.is_opus(),
            DynamicTab::Videos => item.is_video(),
            DynamicTab::Images => item.is_draw() || item.is_opus(),
        };
        if !should_include {
            return None;
        }

        // Handle video dynamics
        if item.is_video() {
            if let Some(bvid) = item.video_bvid() {
                return Some(VideoCard::new(
                    Some(bvid.to_string()),
                    None,
                    item.video_title().unwrap_or("无标题").to_string(),
                    item.author_name().to_string(),
                    format!("▶ {}", item.video_play()),
                    item.video_duration().to_string(),
                    item.video_cover().map(|s| s.to_string()),
                ));
            }
        }
        // Handle image dynamics (带图动态)
        else if item.is_draw() {
            let images = item.draw_images();
            let image_url = images.first().map(|s| s.to_string());
            let desc = item.desc_text().unwrap_or("图片动态");
            let image_count = if images.len() > 1 {
                format!(" [{}P]", images.len())
            } else {
                String::new()
            };
            return Some(VideoCard::new(
                None, // No bvid for images
                None,
                format!("{}{}", desc, image_count),
                item.author_name().to_string(),
                "📷 图片动态".to_string(),
                "".to_string(),
                image_url,
            ));
        }
        // Handle text/opus dynamics (图文动态)
        else if item.is_opus() {
            let text = item.opus_text().unwrap_or("图文动态");
            let images = item.opus_images();
            let image_url = images.first().map(|s| s.to_string());
            let image_count = if !images.is_empty() {
                format!(" [{}P]", images.len())
            } else {
                String::new()
            };
            return Some(VideoCard::new(
                None,
                None,
                format!("{}{}", text, image_count),
                item.author_name().to_string(),
                "📝 图文".to_string(),
                "".to_string(),
                image_url,
            ));
        }
        None
    }

    /// Rebuild the grid from stored items, applying tab + filter.
    pub fn apply_filter(&mut self) {
        self.grid.clear();
        for item in &self.dynamic_items {
            if let Some(filter) = &self.filter_text {
                if !filter.trim().is_empty() {
                    let haystack = [
                        item.video_title().unwrap_or("").to_string(),
                        item.author_name().to_string(),
                        item.desc_text().unwrap_or("").to_string(),
                        item.opus_text().unwrap_or("").to_string(),
                    ]
                    .join(" ");
                    if !haystack.to_lowercase().contains(&filter.to_lowercase()) {
                        continue;
                    }
                }
            }
            if let Some(card) = self.build_card_for_item(item) {
                self.grid.add_card(card);
            }
        }
    }

    pub fn set_filter(&mut self, filter: Option<String>) {
        let filter = filter.map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
        self.filter_text = filter;
        self.apply_filter();
    }

    pub fn filter_text(&self) -> Option<&str> {
        self.filter_text.as_deref()
    }

    pub fn set_up_list(&mut self, up_list: Vec<crate::api::dynamic::UpListItem>) {
        self.up_list = up_list;
        self.loading_up_list = false;
    }

    pub fn select_up(&mut self, index: usize) {
        if index <= self.up_list.len() {
            self.selected_up_index = index;
            self.update_up_scroll();
            self.grid.clear();
            self.loading = true;
        }
    }

    /// Update scroll offset to keep selected UP visible
    fn update_up_scroll(&mut self) {
        const VISIBLE_UPS: usize = 10;
        // selected_up_index 0 is "全部", so actual UP indices start from 1
        // up_list_scroll_offset is the first UP index (1-based) to show after "全部"
        if self.selected_up_index == 0 {
            // "全部" is always visible, scroll to beginning
            self.up_list_scroll_offset = 0;
        } else {
            // Ensure selected UP is within visible range
            let effective_idx = self.selected_up_index; // 1-based index into up_list
            if effective_idx <= self.up_list_scroll_offset {
                // Selected is before visible range, scroll left
                self.up_list_scroll_offset = effective_idx.saturating_sub(1);
            } else if effective_idx > self.up_list_scroll_offset + VISIBLE_UPS {
                // Selected is after visible range, scroll right
                self.up_list_scroll_offset = effective_idx.saturating_sub(VISIBLE_UPS);
            }
        }
    }

    pub fn get_selected_up_mid(&self) -> Option<i64> {
        if self.selected_up_index == 0 {
            None
        } else {
            self.up_list.get(self.selected_up_index - 1).map(|u| u.mid)
        }
    }

    pub fn switch_tab(&mut self, tab: DynamicTab) {
        if self.current_tab != tab {
            self.current_tab = tab;
            self.offset = self.tab_offsets.get(&tab).cloned().flatten();
            self.grid.clear();
            self.loading = true;
            self.error_message = None;
        }
    }

    pub fn set_feed(&mut self, items: Vec<DynamicItem>, offset: Option<String>, has_more: bool) {
        self.grid.clear();
        self.dynamic_items.clear();

        // Process items based on current tab filter
        for item in items.into_iter() {
            self.dynamic_items.push(item.clone());
            if let Some(card) = self.build_card_for_item(&item) {
                self.grid.add_card(card);
            }
        }

        // Save offset for current tab
        self.tab_offsets.insert(self.current_tab, offset.clone());
        self.offset = offset;
        self.has_more = has_more;
        self.loading = false;
        self.apply_filter();
    }

    pub fn append_feed(&mut self, items: Vec<DynamicItem>, offset: Option<String>, has_more: bool) {
        // Process items based on current tab filter
        for item in items.into_iter() {
            self.dynamic_items.push(item.clone());
            if let Some(card) = self.build_card_for_item(&item) {
                self.grid.add_card(card);
            }
        }

        // Save offset for current tab
        self.tab_offsets.insert(self.current_tab, offset.clone());
        self.offset = offset;
        self.has_more = has_more;
        self.loading_more = false;
        self.apply_filter();
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.loading = false;
        self.loading_more = false;
    }

    pub async fn load_more(&mut self, api_client: &ApiClient) {
        if self.loading_more || !self.has_more {
            return;
        }

        self.loading_more = true;

        let feed_type = self.current_tab.get_feed_type();
        let host_mid = self.get_selected_up_mid();
        match api_client
            .get_dynamic_feed(self.offset.as_deref(), feed_type, host_mid)
            .await
        {
            Ok(data) => {
                let items = data.items.unwrap_or_default();
                let offset = data.offset;
                let has_more = data.has_more.unwrap_or(false);
                self.append_feed(items, offset, has_more);
            }
            Err(_) => {
                self.loading_more = false;
            }
        }
    }

    pub fn poll_cover_results(&mut self) {
        self.grid.poll_cover_results();
    }

    pub fn start_cover_downloads(&mut self) {
        self.grid.start_cover_downloads();
    }

    /// Get the currently selected dynamic item (if any)
    pub fn selected_dynamic_item(&self) -> Option<&DynamicItem> {
        let selected_index = self.grid.selected_index;
        self.dynamic_items.get(selected_index)
    }
}

impl Default for DynamicPage {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicPage {
    fn draw_up_list(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut items = vec![ListItem::new("全部")];
        items.extend(self.up_list.iter().map(|user| {
            let marker = if user.has_update { "● " } else { "  " };
            ListItem::new(format!("{marker}{}", user.uname))
        }));
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_subtle))
                    .title(" 关注的UP主 "),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(if self.focus_up_list {
                theme.bilibili_pink
            } else {
                theme.bilibili_cyan
            }));
        let mut state = ListState::default().with_selected(Some(self.selected_up_index));
        frame.render_stateful_widget(list, area, &mut state);
    }
}

impl Component for DynamicPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(30)])
            .split(area);
        self.draw_up_list(frame, panes[0], theme);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(10),
                Constraint::Length(if self.input_mode { 3 } else { 2 }),
            ])
            .split(panes[1]);
        let header_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(3)])
            .split(chunks[0]);
        let title = Paragraph::new(Line::from(vec![
            Span::styled(" 📺 ", Style::default()),
            Span::styled(
                "关注动态",
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD),
            ),
            if let Some(ref filter) = self.filter_text {
                Span::styled(
                    format!(" | 筛选: {filter}"),
                    Style::default().fg(theme.warning),
                )
            } else {
                Span::raw("")
            },
            if self.loading_more {
                Span::styled(" 加载中...", Style::default().fg(theme.warning))
            } else {
                Span::raw("")
            },
        ]))
        .block(Block::default().borders(Borders::TOP | Borders::LEFT | Borders::RIGHT));
        frame.render_widget(title, header_chunks[0]);

        let tabs = DynamicTab::all_tabs()
            .iter()
            .enumerate()
            .flat_map(|(index, tab)| {
                let prefix = (index > 0).then_some(Span::raw("  "));
                let style = if *tab == self.current_tab {
                    Style::default()
                        .fg(theme.fg_accent)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(theme.fg_secondary)
                };
                [
                    prefix,
                    Some(Span::styled(
                        format!("[{}] {}", index + 1, tab.label()),
                        style,
                    )),
                ]
                .into_iter()
                .flatten()
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(Line::from(tabs))
                .block(Block::default().borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT))
                .alignment(Alignment::Center),
            header_chunks[1],
        );

        if self.loading {
            frame.render_widget(
                Paragraph::new("⏳ 加载动态中...")
                    .style(Style::default().fg(theme.warning))
                    .alignment(Alignment::Center),
                chunks[1],
            );
        } else if let Some(error) = &self.error_message {
            frame.render_widget(
                Paragraph::new(format!("❌ {error}"))
                    .style(Style::default().fg(theme.error))
                    .alignment(Alignment::Center),
                chunks[1],
            );
        } else if self.grid.cards.is_empty() {
            let empty_msg = if self.filter_text.is_some() {
                "没有匹配的动态，按 / 修改筛选"
            } else {
                "暂无动态，请先登录并关注UP主"
            };
            frame.render_widget(
                Paragraph::new(empty_msg)
                    .style(Style::default().fg(theme.fg_secondary))
                    .alignment(Alignment::Center),
                chunks[1],
            );
        } else {
            self.grid.render(frame, chunks[1], theme);
        }

        // Bottom: input box (when active) or hints + feedback
        if self.input_mode {
            let input_title = if self.input_is_search {
                " 搜索动态 (Enter 筛选, Esc 取消) "
            } else {
                " 发布动态 (Enter 发布, Esc 取消) "
            };
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.bilibili_pink))
                .title(Span::styled(
                    input_title,
                    Style::default().fg(theme.fg_secondary),
                ));
            let input_inner = input_block.inner(chunks[2]);
            frame.render_widget(input_block, chunks[2]);
            frame.render_widget(
                Paragraph::new(if self.input_text.is_empty() {
                    " ".to_string()
                } else {
                    self.input_text.clone()
                }),
                input_inner,
            );
            let mut items: Vec<(String, String, Color)> = vec![
                ("↑/↓".into(), "选择动态".into(), theme.fg_accent),
                (
                    format!("{}/{}", keys.page_up, keys.page_down),
                    "翻页".into(),
                    theme.fg_accent,
                ),
                ("←/→".into(), "切换面板".into(), theme.fg_accent),
                ("/".into(), "搜索".into(), theme.bilibili_pink),
                ("w".into(), "发布动态".into(), theme.bilibili_pink),
                ("u".into(), "UP主页".into(), theme.fg_accent),
                (keys.tab_1.clone(), "切标签".into(), theme.info),
                (keys.confirm.clone(), "详情".into(), theme.success),
                (keys.refresh.clone(), "刷新".into(), theme.info),
            ];
            if let Some(ref filter) = self.filter_text {
                items.push(("".into(), format!(" | 筛选: {filter}"), theme.fg_secondary));
            }
            if let Some(ref msg) = self.message {
                items.push(("".into(), format!(" | {msg}"), theme.fg_accent));
            }
            frame.render_widget(
                Paragraph::new(shortcut_footer(theme, items)).alignment(Alignment::Center),
                chunks[2],
            );
        }
    }

    fn handle_input_with_modifiers(
        &mut self,
        key: KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        keys: &crate::storage::Keybindings,
    ) -> Option<AppAction> {
        let _ = modifiers;
        // Input mode: typing search term or dynamic content.
        if self.input_mode {
            match key {
                KeyCode::Esc => {
                    self.input_mode = false;
                    self.input_is_search = false;
                    self.input_text.clear();
                    return Some(AppAction::None);
                }
                KeyCode::Enter => {
                    let content = std::mem::take(&mut self.input_text);
                    self.input_mode = false;
                    let is_search = self.input_is_search;
                    self.input_is_search = false;
                    if content.trim().is_empty() {
                        return Some(AppAction::None);
                    }
                    if is_search {
                        self.set_filter(Some(content));
                        self.message = Some("已筛选动态".to_string());
                        return Some(AppAction::None);
                    }
                    return Some(AppAction::PostDynamic { content });
                }
                KeyCode::Backspace => {
                    self.input_text.pop();
                    return Some(AppAction::None);
                }
                KeyCode::Char(c) => {
                    self.input_text.push(c);
                    return Some(AppAction::None);
                }
                _ => return Some(AppAction::None),
            }
        } else if key == KeyCode::Esc {
            // 筛选已应用时按 Esc 取消筛选，回到完整列表
            if self.filter_text.is_some() {
                self.set_filter(None);
                self.message = Some("已取消筛选".to_string());
            }
            return Some(AppAction::None);
        } else if keys.matches_quit(key) {
            return Some(AppAction::Quit);
        }
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }

        if key == KeyCode::Char('/') {
            self.input_mode = true;
            self.input_is_search = true;
            self.input_text.clear();
            return Some(AppAction::None);
        }
        if key == KeyCode::Char('w') {
            self.input_mode = true;
            self.input_is_search = false;
            self.input_text.clear();
            return Some(AppAction::None);
        }

        if self.focus_up_list {
            if keys.matches_down(key) {
                if self.selected_up_index < self.up_list.len() {
                    return Some(AppAction::SelectUpMaster(self.selected_up_index + 1));
                }
            } else if keys.matches_up(key) {
                if self.selected_up_index > 0 {
                    return Some(AppAction::SelectUpMaster(self.selected_up_index - 1));
                }
            } else if keys.matches_right(key) || keys.matches_confirm(key) {
                self.focus_up_list = false;
            }
            return Some(AppAction::None);
        }

        if keys.matches_left(key) {
            self.focus_up_list = true;
            if self.loading || self.loading_more {
                self.loading = false;
                self.loading_more = false;
                return Some(AppAction::CancelPendingLoads);
            }
            return Some(AppAction::None);
        }

        if keys.matches_page_down(key) {
            self.grid.move_page_down();
            if self.grid.is_near_bottom(self.grid.cached_visible_rows)
                && !self.loading_more
                && self.has_more
            {
                return Some(AppAction::LoadMoreDynamic);
            }
            return Some(AppAction::None);
        }
        if keys.matches_page_up(key) {
            self.grid.move_page_up();
            return Some(AppAction::None);
        }

        if keys.matches_down(key) {
            self.grid.move_down();
            if self.grid.is_near_bottom(3) && !self.loading_more && self.has_more {
                return Some(AppAction::LoadMoreDynamic);
            }
            return Some(AppAction::None);
        }
        if keys.matches_up(key) {
            self.grid.move_up();
            return Some(AppAction::None);
        }

        // Direct tab access
        if keys.matches_tab_1(key) {
            return Some(AppAction::SwitchDynamicTab(DynamicTab::All));
        }
        if keys.matches_tab_2(key) {
            return Some(AppAction::SwitchDynamicTab(DynamicTab::Videos));
        }
        if keys.matches_tab_3(key) {
            return Some(AppAction::SwitchDynamicTab(DynamicTab::Images));
        }

        // Open selected card
        if key == KeyCode::Char('u')
            && let Some(mid) = self
                .selected_dynamic_item()
                .and_then(|item| item.author_mid())
        {
            return Some(AppAction::OpenUpPage(mid));
        }
        if keys.matches_confirm(key) {
            if let Some(card) = self.grid.selected_card() {
                // Video card - open video detail
                if let Some(ref bvid) = card.bvid {
                    return Some(AppAction::OpenVideoDetail(bvid.clone(), 0));
                }
                // Non-video card (draw/opus) - open dynamic detail
                else if let Some(item) = self.selected_dynamic_item()
                    && (item.is_draw() || item.is_opus())
                    && let Some(id) = &item.id_str
                {
                    return Some(AppAction::OpenDynamicDetail(id.clone()));
                }
            }
            return Some(AppAction::None);
        }

        // Refresh
        if keys.matches_refresh(key) {
            self.loading = true;
            self.grid.clear();
            return Some(AppAction::RefreshDynamic);
        }

        Some(AppAction::None)
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        use crossterm::event::MouseEventKind;
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(30)])
            .split(area);
        let position = ratatui::layout::Position::new(event.column, event.row);

        if panes[0].contains(position) {
            self.focus_up_list = true;
            match event.kind {
                MouseEventKind::ScrollDown if self.selected_up_index < self.up_list.len() => {
                    return Some(AppAction::SelectUpMaster(self.selected_up_index + 1));
                }
                MouseEventKind::ScrollUp if self.selected_up_index > 0 => {
                    return Some(AppAction::SelectUpMaster(self.selected_up_index - 1));
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let row = event.row.saturating_sub(panes[0].y + 1) as usize;
                    if row <= self.up_list.len() {
                        self.focus_up_list = true;
                        if row != self.selected_up_index {
                            return Some(AppAction::SelectUpMaster(row));
                        }
                    }
                }
                _ => {}
            }
            return Some(AppAction::None);
        }
        if !panes[1].contains(position) {
            return None;
        }
        self.focus_up_list = false;

        match event.kind {
            MouseEventKind::ScrollDown => {
                if self.grid.move_down()
                    && self.grid.is_near_bottom(3)
                    && !self.loading_more
                    && self.has_more
                {
                    return Some(AppAction::LoadMoreDynamic);
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
                        Constraint::Length(5),
                        Constraint::Min(10),
                        Constraint::Length(2),
                    ])
                    .split(panes[1]);

                let grid_area = chunks[1];

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
                        if let Some(card) = self.grid.cards.get(click_idx) {
                            if let Some(ref bvid) = card.bvid {
                                return Some(AppAction::OpenVideoDetail(bvid.clone(), 0));
                            } else if let Some(item) = self.dynamic_items.get(click_idx)
                                && (item.is_draw() || item.is_opus())
                                && let Some(id) = &item.id_str
                            {
                                return Some(AppAction::OpenDynamicDetail(id.clone()));
                            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_uses_sidebar_up_selection_and_single_column_cards() {
        let page = DynamicPage::new();
        assert!(page.focus_up_list);
        assert_eq!(page.grid.columns, 1);
        assert!(page.grid.list_layout);
    }

    #[test]
    fn dynamic_right_switches_to_the_card_pane() {
        let mut page = DynamicPage::new();
        let keys = Keybindings::default();
        assert!(matches!(
            page.handle_input_with_modifiers(
                KeyCode::Right,
                crossterm::event::KeyModifiers::NONE,
                &keys
            ),
            Some(AppAction::None)
        ));
        assert!(!page.focus_up_list);
    }

    #[test]
    fn dynamic_slash_enters_search_mode_and_enter_applies_filter() {
        let mut page = DynamicPage::new();
        let keys = Keybindings::default();

        // Press '/' -> enter search input mode
        let action = page.handle_input_with_modifiers(
            KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
            &keys,
        );
        assert!(matches!(action, Some(AppAction::None)));
        assert!(page.input_mode);
        assert!(page.input_is_search);

        // Type a keyword
        for ch in ['测', '试'] {
            page.handle_input_with_modifiers(
                KeyCode::Char(ch),
                crossterm::event::KeyModifiers::NONE,
                &keys,
            );
        }
        assert_eq!(page.input_text, "测试");

        // Press Enter -> apply filter
        let action = page.handle_input_with_modifiers(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
            &keys,
        );
        assert!(matches!(action, Some(AppAction::None)));
        assert!(!page.input_mode);
        assert_eq!(page.filter_text(), Some("测试"));
    }

    #[test]
    fn dynamic_w_enters_publish_mode() {
        let mut page = DynamicPage::new();
        let keys = Keybindings::default();

        let action = page.handle_input_with_modifiers(
            KeyCode::Char('w'),
            crossterm::event::KeyModifiers::NONE,
            &keys,
        );
        assert!(matches!(action, Some(AppAction::None)));
        assert!(page.input_mode);
        assert!(!page.input_is_search);
    }
}
