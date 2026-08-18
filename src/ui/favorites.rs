use super::{Component, Theme, VideoCard, VideoCardGrid, shortcut_footer};
use crate::api::favorite::{
    CollectedFolder, FavoriteFolder, FavoriteResourceData, FavoriteSource, SeasonArchivesData,
    WatchLaterData,
};
use crate::application::AppAction;
use crate::storage::Keybindings;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind},
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::time::Instant;

pub struct FavoritesPage {
    pub mid: i64,
    pub created: Vec<FavoriteFolder>,
    pub collected: Vec<CollectedFolder>,
    pub selected_source: usize,
    pub focus_sources: bool,
    pub active_source: FavoriteSource,
    pub videos: VideoCardGrid,
    /// All loaded cards regardless of filter; rebuilt into `videos` when the
    /// local filter changes.
    all_cards: Vec<VideoCard>,
    filter: Option<String>,
    filter_input_mode: bool,
    filter_input: String,
    pub page: i32,
    pub total: i64,
    pub loading: bool,
    pub loading_more: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub message_set_at: Option<Instant>,
    pub input_mode: Option<InputMode>,
    pub input_text: String,
    /// After creating a folder, store its media_id here so the next
    /// folder-list refresh auto-navigates to it.
    pub pending_navigate_media_id: Option<i64>,
    last_click_time: Option<Instant>,
    last_click_index: Option<usize>,
}

pub enum InputMode {
    CreateFolder,
    RenameFolder,
}

impl FavoritesPage {
    pub fn new(mid: i64) -> Self {
        Self {
            mid,
            created: Vec::new(),
            collected: Vec::new(),
            selected_source: 0,
            focus_sources: true,
            active_source: FavoriteSource::WatchLater,
            videos: VideoCardGrid::new_list(),
            all_cards: Vec::new(),
            filter: None,
            filter_input_mode: false,
            filter_input: String::new(),
            page: 1,
            total: 0,
            loading: true,
            loading_more: false,
            error: None,
            message: None,
            message_set_at: None,
            input_mode: None,
            input_text: String::new(),
            pending_navigate_media_id: None,
            last_click_time: None,
            last_click_index: None,
        }
    }

    pub fn sources(&self) -> Vec<FavoriteSource> {
        let mut sources = vec![FavoriteSource::WatchLater];
        sources.extend(self.created.iter().map(|folder| FavoriteSource::Created {
            media_id: folder.id,
            title: folder.title.clone(),
        }));
        sources.extend(
            self.collected
                .iter()
                .map(|folder| FavoriteSource::Collected {
                    season_id: folder.id,
                    mid: folder.mid,
                    title: folder.title.clone(),
                }),
        );
        sources
    }

    pub fn apply_initial(
        &mut self,
        watch_later: WatchLaterData,
        created: Vec<FavoriteFolder>,
        collected: Vec<CollectedFolder>,
    ) {
        self.created = created;
        self.collected = collected;
        self.active_source = FavoriteSource::WatchLater;
        self.selected_source = 0;
        self.videos.clear();
        self.all_cards.clear();
        self.filter = None;
        self.filter_input.clear();
        self.page = 1;
        self.total = watch_later.count;
        self.append_watch_later(watch_later);
        self.loading = false;
        self.error = None;
    }

    /// Reload only the folder lists (created + collected) without resetting
    /// the active source or clearing videos.  Used after creating / deleting
    /// a folder so the sidebar refreshes but the user stays on the current
    /// source.
    /// If a `media_id` is stored here, the next folder-list refresh will
    /// auto-navigate to it.  Set by `CreateFavoriteFolder`, consumed by
    /// `apply_folder_list_refresh`.
    pub fn pending_navigate_media_id(&self) -> Option<i64> {
        self.pending_navigate_media_id
    }

    pub fn apply_folder_list_refresh(
        &mut self,
        created: Vec<FavoriteFolder>,
        collected: Vec<CollectedFolder>,
    ) -> Option<i64> {
        self.created = created;
        self.collected = collected;
        self.loading = false;
        // Check if we need to auto-navigate to a newly created folder.
        if let Some(media_id) = self.pending_navigate_media_id.take() {
            if self.created.iter().any(|f| f.id == media_id) {
                self.active_source = FavoriteSource::Created {
                    media_id,
                    title: self
                        .created
                        .iter()
                        .find(|f| f.id == media_id)
                        .map(|f| f.title.clone())
                        .unwrap_or_default(),
                };
                self.selected_source = 1; // first created folder
                return Some(media_id);
            }
        }
        // Keep active_source unchanged – user stays on whatever they were
        // viewing.  If the active source was a created folder that no longer
        // exists (e.g. just deleted), fall back to WatchLater.
        if let FavoriteSource::Created { media_id, .. } = &self.active_source {
            if !self.created.iter().any(|f| f.id == *media_id) {
                self.active_source = FavoriteSource::WatchLater;
                self.selected_source = 0;
                self.videos.clear();
            }
        }
        None
    }

    pub fn begin_source_load(&mut self, source: FavoriteSource) {
        self.active_source = source;
        self.page = 1;
        self.total = 0;
        self.videos.clear();
        self.all_cards.clear();
        self.filter = None;
        self.filter_input.clear();
        self.loading = true;
        self.error = None;
    }

    /// Set a feedback message that auto-clears after 3 seconds.
    pub fn set_message(&mut self, msg: String) {
        self.message = Some(msg);
        self.message_set_at = Some(Instant::now());
    }

    /// Auto-clear the feedback message after 3 seconds.
    pub fn tick(&mut self) {
        if let Some(set_at) = self.message_set_at {
            if set_at.elapsed().as_secs() >= 3 {
                self.message = None;
                self.message_set_at = None;
            }
        }
    }

    pub fn apply_watch_later(&mut self, page: i32, data: WatchLaterData) {
        if page == 1 {
            self.videos.clear();
            self.all_cards.clear();
        }
        self.page = page;
        self.total = data.count;
        self.append_watch_later(data);
        self.apply_filter();
        self.finish_load();
    }

    fn append_watch_later(&mut self, data: WatchLaterData) {
        for item in data.list {
            let author = item
                .owner
                .as_ref()
                .map(|owner| owner.name.clone())
                .unwrap_or_else(|| "未知UP".to_string());
            let uploader_mid = item.owner.as_ref().map(|owner| owner.mid);
            let views = item
                .stat
                .and_then(|stat| stat.view)
                .map(format_count)
                .unwrap_or_else(|| "-".to_string());
            let card = VideoCard::new(
                Some(item.bvid),
                Some(item.aid),
                item.title,
                author,
                views,
                format_duration(item.duration.unwrap_or_default()),
                item.pic,
            )
            .with_uploader_mid(uploader_mid);
            self.videos.add_card(card.clone());
            self.all_cards.push(card);
        }
    }

    pub fn apply_created(&mut self, page: i32, data: FavoriteResourceData) {
        if page == 1 {
            self.videos.clear();
            self.all_cards.clear();
        }
        self.page = page;
        self.total = data
            .info
            .as_ref()
            .and_then(|info| info.media_count)
            .unwrap_or_default() as i64;
        for item in data.medias {
            let Some(bvid) = item.bvid else { continue };
            let author = item
                .upper
                .as_ref()
                .map(|upper| upper.name.clone())
                .unwrap_or_else(|| "未知UP".to_string());
            let uploader_mid = item.upper.as_ref().map(|upper| upper.mid);
            let views = item
                .cnt_info
                .and_then(|count| count.play)
                .map(format_count)
                .unwrap_or_else(|| "-".to_string());
            let card = VideoCard::new(
                Some(bvid),
                Some(item.id),
                item.title,
                author,
                views,
                format_duration(item.duration.unwrap_or_default()),
                item.cover,
            )
            .with_uploader_mid(uploader_mid);
            self.videos.add_card(card.clone());
            self.all_cards.push(card);
        }
        self.apply_filter();
        self.finish_load();
    }

    pub fn apply_collected(&mut self, page: i32, data: SeasonArchivesData) {
        if page == 1 {
            self.videos.clear();
            self.all_cards.clear();
        }
        self.page = data.page.page_num;
        self.total = data.page.total;
        let author = match &self.active_source {
            FavoriteSource::Collected { title, .. } => title.clone(),
            _ => "合集".to_string(),
        };
        for item in data.archives {
            let views = item
                .stat
                .and_then(|stat| stat.view)
                .map(format_count)
                .unwrap_or_else(|| "-".to_string());
            let card = VideoCard::new(
                Some(item.bvid),
                Some(item.aid),
                item.title,
                author.clone(),
                views,
                format_duration(item.duration.unwrap_or_default()),
                item.pic,
            );
            self.videos.add_card(card.clone());
            self.all_cards.push(card);
        }
        self.apply_filter();
        self.finish_load();
    }

    /// Rebuild the visible card grid from `all_cards` according to the active
    /// local filter.  Filtering happens on the already-loaded pages only.
    fn apply_filter(&mut self) {
        let Some(filter) = self.filter.as_deref() else {
            if self.videos.cards.len() != self.all_cards.len() {
                self.videos.cards = self.all_cards.clone();
                self.videos.selected_index = 0;
                self.videos.update_scroll(self.videos.cached_visible_rows);
            }
            return;
        };
        let filter = filter.trim().to_lowercase();
        self.videos.cards = if filter.is_empty() {
            self.all_cards.clone()
        } else {
            self.all_cards
                .iter()
                .filter(|card| card.title.to_lowercase().contains(&filter))
                .cloned()
                .collect()
        };
        self.videos.selected_index = 0;
        self.videos.update_scroll(self.videos.cached_visible_rows);
    }

    fn finish_load(&mut self) {
        self.loading = false;
        self.loading_more = false;
        self.error = None;
    }

    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.loading_more = false;
        self.error = Some(error);
    }

    fn source_label(source: &FavoriteSource) -> &str {
        match source {
            FavoriteSource::WatchLater => "稍后再看",
            FavoriteSource::Created { title, .. } | FavoriteSource::Collected { title, .. } => {
                title
            }
        }
    }
}

impl Component for FavoritesPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(30)])
            .split(area);
        let sources = self.sources();
        let mut items = vec![
            ListItem::new("其他收藏").style(
                Style::default()
                    .fg(theme.fg_muted)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        items.push(ListItem::new("  稍后再看"));
        items.push(
            ListItem::new("我创建的收藏夹").style(
                Style::default()
                    .fg(theme.fg_muted)
                    .add_modifier(Modifier::BOLD),
            ),
        );
        items.extend(self.created.iter().map(|folder| {
            ListItem::new(format!(
                "  {}  ({})",
                folder.title,
                folder.media_count.unwrap_or_default()
            ))
        }));
        items.push(
            ListItem::new("我追的合集/收藏夹").style(
                Style::default()
                    .fg(theme.fg_muted)
                    .add_modifier(Modifier::BOLD),
            ),
        );
        items.extend(self.collected.iter().map(|folder| {
            ListItem::new(format!(
                "  {}  ({})",
                folder.title,
                folder.media_count.unwrap_or_default()
            ))
        }));
        let state_index = if self.selected_source == 0 {
            1
        } else if self.selected_source <= self.created.len() {
            self.selected_source + 2
        } else {
            self.selected_source + 3
        };
        let mut state = ListState::default().with_selected(Some(state_index));
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" 收藏 "))
                .highlight_symbol("▶ ")
                .highlight_style(if self.focus_sources {
                    Style::default().fg(theme.bilibili_pink)
                } else {
                    Style::default().fg(theme.bilibili_cyan)
                }),
            chunks[0],
            &mut state,
        );

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(2),
            ])
            .split(chunks[1]);
        let header_title = if self.filter_input_mode {
            format!("筛选收藏: {}_", self.filter_input)
        } else if let Some(mode) = &self.input_mode {
            match mode {
                InputMode::CreateFolder => format!("输入收藏夹名称: {}_", self.input_text),
                InputMode::RenameFolder => format!("重命名收藏夹: {}_", self.input_text),
            }
        } else if let Some(ref msg) = self.message {
            msg.clone()
        } else {
            format!(
                "{}  ·  {}/{}{}",
                Self::source_label(&self.active_source),
                self.videos.cards.len(),
                self.total,
                self.filter
                    .as_deref()
                    .filter(|f| !f.is_empty())
                    .map(|f| format!("  |  筛选: {}", f))
                    .unwrap_or_default()
            )
        };
        let header_style = if self.filter_input_mode || self.input_mode.is_some() {
            Style::default().fg(theme.warning)
        } else if self.message.is_some() {
            Style::default().fg(theme.success)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new(header_title).style(header_style)
                .block(Block::default().borders(Borders::ALL)),
            right[0],
        );
        if self.loading {
            frame.render_widget(Paragraph::new("正在加载…"), right[1]);
        } else if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(theme.error)),
                right[1],
            );
        } else if self.videos.cards.is_empty() && !self.all_cards.is_empty() {
            frame.render_widget(
                Paragraph::new("没有匹配的收藏，按 / 修改筛选")
                    .style(Style::default().fg(theme.fg_secondary)),
                right[1],
            );
        } else {
            self.videos.render(frame, right[1], theme);
        }
        frame.render_widget(
            Paragraph::new(shortcut_footer(
                theme,
                [
                    ("↑/↓".into(), "选择".into(), theme.fg_accent),
                    (
                        format!("{}/{}", keys.page_up, keys.page_down),
                        "翻页".into(),
                        theme.fg_accent,
                    ),
                    ("←/→".into(), "收藏/视频".into(), theme.fg_accent),
                    (keys.confirm.clone(), "打开".into(), theme.success),
                    ("n".into(), "新建收藏夹".into(), theme.info),
                    ("r".into(), "重命名".into(), theme.info),
                    ("x".into(), "删除收藏夹".into(), theme.error),
                    ("Del".into(), "移除视频".into(), theme.warning),
                    ("/".into(), "搜索".into(), theme.info),
                    (keys.nav_next_page.clone(), "下一页面".into(), theme.info),
                    (keys.nav_prev_page.clone(), "上一页面".into(), theme.info),
                ],
            ))
            .alignment(Alignment::Center),
            right[2],
        );
        let _ = sources;
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        // Local filter input mode (triggered by `/`)
        if self.filter_input_mode {
            match key {
                KeyCode::Esc => {
                    self.filter_input_mode = false;
                    self.filter_input.clear();
                    self.filter = None;
                    self.apply_filter();
                    return Some(AppAction::None);
                }
                KeyCode::Enter => {
                    self.filter_input_mode = false;
                    let kw = self.filter_input.trim().to_string();
                    self.filter = if kw.is_empty() { None } else { Some(kw.clone()) };
                    crate::storage::save_search_history(&kw);
                    self.apply_filter();
                    return Some(AppAction::None);
                }
                KeyCode::Backspace => {
                    self.filter_input.pop();
                    let kw = self.filter_input.trim().to_string();
                    self.filter = if kw.is_empty() { None } else { Some(kw) };
                    self.apply_filter();
                    return Some(AppAction::None);
                }
                KeyCode::Char(c) => {
                    self.filter_input.push(c);
                    let kw = self.filter_input.trim().to_string();
                    self.filter = if kw.is_empty() { None } else { Some(kw) };
                    self.apply_filter();
                    return Some(AppAction::None);
                }
                _ => return Some(AppAction::None),
            }
        }
        // Text input mode for creating folder name
        if self.input_mode.is_some() {
            match key {
                KeyCode::Esc => {
                    self.input_mode = None;
                    self.input_text.clear();
                    return Some(AppAction::None);
                }
                KeyCode::Enter => {
                    if let Some(InputMode::CreateFolder) = &self.input_mode {
                        let title = self.input_text.trim().to_string();
                        self.input_mode = None;
                        self.input_text.clear();
                        if !title.is_empty() {
                            return Some(AppAction::CreateFavoriteFolder {
                                title,
                                intro: String::new(),
                                privacy: 0,
                            });
                        }
                    } else if let Some(InputMode::RenameFolder) = &self.input_mode {
                        let title = self.input_text.trim().to_string();
                        self.input_mode = None;
                        self.input_text.clear();
                        if !title.is_empty() {
                            let sources = self.sources();
                            if let Some(source) = sources.get(self.selected_source) {
                                if let FavoriteSource::Created { media_id, .. } = source {
                                    return Some(AppAction::RenameFavoriteFolder {
                                        media_id: *media_id,
                                        title,
                                    });
                                }
                            }
                        }
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Char(c) => {
                    self.input_text.push(c);
                    return Some(AppAction::None);
                }
                KeyCode::Backspace => {
                    self.input_text.pop();
                    return Some(AppAction::None);
                }
                _ => return Some(AppAction::None),
            }
        }
        if keys.matches_quit(key) {
            return Some(AppAction::Quit);
        }
        if key == KeyCode::Char('/') {
            self.filter_input_mode = true;
            self.filter_input.clear();
            self.filter = None;
            self.apply_filter();
            return Some(AppAction::None);
        }
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }
        let sources = self.sources();
        // Pane switching has priority over loading and list navigation.
        if keys.matches_left(key) {
            self.focus_sources = true;
            if self.loading || self.loading_more {
                self.loading = false;
                self.loading_more = false;
                return Some(AppAction::CancelPendingLoads);
            }
            return Some(AppAction::None);
        }
        if keys.matches_right(key) {
            if self.focus_sources
                && let Some(source) = sources.get(self.selected_source)
                && *source != self.active_source
            {
                self.focus_sources = false;
                return Some(AppAction::SelectFavoriteSource(source.clone()));
            }
            self.focus_sources = false;
            return Some(AppAction::None);
        }
        if self.focus_sources {
            if keys.matches_down(key) && self.selected_source + 1 < sources.len() {
                self.selected_source += 1;
            } else if keys.matches_up(key) && self.selected_source > 0 {
                self.selected_source -= 1;
            } else if key == KeyCode::Char('n') {
                self.input_mode = Some(InputMode::CreateFolder);
                self.input_text.clear();
                return Some(AppAction::None);
            } else if key == KeyCode::Char('r') {
                // Rename the selected created folder
                if let Some(source) = sources.get(self.selected_source) {
                    if let FavoriteSource::Created { title, .. } = source {
                        self.input_mode = Some(InputMode::RenameFolder);
                        self.input_text = title.clone();
                        return Some(AppAction::None);
                    }
                }
                return Some(AppAction::None);
        } else if key == KeyCode::Char('x') {
            // Delete the selected created folder
            if let Some(source) = sources.get(self.selected_source) {
                if let FavoriteSource::Created { media_id, title } = source {
                    let title = title.clone();
                    let mid = self.mid;
                    self.set_message(format!("已删除收藏夹: {title}"));
                    return Some(AppAction::DeleteFavoriteFolder(*media_id));
                }
            }
            return Some(AppAction::None);
        } else if keys.matches_confirm(key)
                && let Some(source) = sources.get(self.selected_source)
                && *source != self.active_source
            {
                return Some(AppAction::SelectFavoriteSource(source.clone()));
            }
            return Some(AppAction::None);
        }

        if keys.matches_page_down(key) {
            self.videos.move_page_down();
            if self.videos.is_near_bottom(self.videos.cached_visible_rows)
                && !self.loading_more
                && self.videos.cards.len() < self.total as usize
            {
                self.loading_more = true;
                return Some(AppAction::LoadMoreFavorites);
            }
            return Some(AppAction::None);
        }
        if keys.matches_page_up(key) {
            self.videos.move_page_up();
            return Some(AppAction::None);
        }

        if keys.matches_down(key) {
            self.videos.move_down();
            if self.videos.is_near_bottom(self.videos.cached_visible_rows)
                && !self.loading_more
                && self.videos.cards.len() < self.total as usize
            {
                self.loading_more = true;
                return Some(AppAction::LoadMoreFavorites);
            }
        } else if keys.matches_up(key) {
            self.videos.move_up();
        } else if key == KeyCode::Delete {
            // Remove selected video from current folder / watch-later
            if let Some(card) = self.videos.selected_card()
                && let Some(aid) = card.aid
            {
                match &self.active_source {
                    FavoriteSource::Created { media_id, .. } => {
                        let media_id = *media_id;
                        return Some(AppAction::FavoriteVideoInFolder {
                            aid,
                            media_id,
                            add: false,
                        });
                    }
                    FavoriteSource::WatchLater => {
                        return Some(AppAction::RemoveFromWatchLater { aid });
                    }
                    _ => {}
                }
            }
        } else if keys.matches_confirm(key)
            && let Some(card) = self.videos.selected_card()
            && let (Some(bvid), Some(aid)) = (&card.bvid, card.aid)
        {
            return Some(AppAction::OpenVideoDetail(bvid.clone(), aid));
        }
        Some(AppAction::None)
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        let position = Position::new(event.column, event.row);
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(30)])
            .split(area);
        let video_parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(8),
                Constraint::Length(2),
            ])
            .split(panes[1]);
        match event.kind {
            MouseEventKind::ScrollDown if panes[1].contains(position) => {
                self.focus_sources = false;
                if self.videos.move_down()
                    && self.videos.is_near_bottom(self.videos.cached_visible_rows)
                    && !self.loading_more
                    && self.videos.cards.len() < self.total as usize
                {
                    self.loading_more = true;
                    return Some(AppAction::LoadMoreFavorites);
                }
            }
            MouseEventKind::ScrollUp if panes[1].contains(position) => {
                self.focus_sources = false;
                self.videos.move_up();
            }
            MouseEventKind::Down(MouseButton::Left) if panes[0].contains(position) => {
                self.focus_sources = true;
                let item_row = event.row.saturating_sub(panes[0].y + 1) as usize;
                let source_index = if item_row == 1 {
                    Some(0)
                } else if (3..3 + self.created.len()).contains(&item_row) {
                    Some(item_row - 2)
                } else {
                    let first_collected = 4 + self.created.len();
                    (item_row >= first_collected)
                        .then_some(1 + self.created.len() + item_row - first_collected)
                };
                if let Some(index) = source_index.filter(|index| *index < self.sources().len()) {
                    self.selected_source = index;
                    if let Some(source) = self.sources().get(index)
                        && *source != self.active_source
                    {
                        return Some(AppAction::SelectFavoriteSource(source.clone()));
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) if video_parts[1].contains(position) => {
                self.focus_sources = false;
                if self.videos.select_at(event.row, video_parts[1]) {
                    let index = self.videos.selected_index;
                    let now = Instant::now();
                    let double = self.last_click_index == Some(index)
                        && self
                            .last_click_time
                            .is_some_and(|time| now.duration_since(time).as_millis() < 500);
                    self.last_click_index = Some(index);
                    self.last_click_time = Some(now);
                    if double
                        && let Some(card) = self.videos.selected_card()
                        && let (Some(bvid), Some(aid)) = (&card.bvid, card.aid)
                    {
                        return Some(AppAction::OpenVideoDetail(bvid.clone(), aid));
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Middle) if video_parts[1].contains(position) => {
                if let Some(card) = self.videos.selected_card()
                    && let (Some(bvid), Some(aid)) = (&card.bvid, card.aid)
                {
                    return Some(AppAction::OpenVideoDetail(bvid.clone(), aid));
                }
            }
            _ => {}
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
    fn favorites_is_a_normal_sidebar_page() {
        let mut page = FavoritesPage::new(1);
        let keys = Keybindings::default();
        assert!(matches!(
            page.handle_input(KeyCode::Tab, &keys),
            Some(AppAction::NavNext)
        ));
        assert!(matches!(page.active_source, FavoriteSource::WatchLater));
    }

    #[test]
    fn source_list_starts_with_watch_later_then_created_and_collected() {
        let mut page = FavoritesPage::new(1);
        page.created.push(FavoriteFolder {
            id: 2,
            fid: None,
            mid: 1,
            title: "我的收藏".to_string(),
            media_count: Some(1),
            fav_state: None,
            attr: None,
        });
        page.collected.push(CollectedFolder {
            id: 3,
            mid: 4,
            title: "我追的合集".to_string(),
            media_count: Some(2),
            state: Some(0),
        });
        let sources = page.sources();
        assert!(matches!(sources[0], FavoriteSource::WatchLater));
        assert!(matches!(sources[1], FavoriteSource::Created { .. }));
        assert!(matches!(sources[2], FavoriteSource::Collected { .. }));
    }

    #[test]
    fn favorites_video_list_is_single_column() {
        let page = FavoritesPage::new(1);
        assert_eq!(page.videos.columns, 1);
        assert!(page.videos.list_layout);
    }

    #[test]
    fn left_cancels_loading_before_video_navigation() {
        let mut page = FavoritesPage::new(1);
        page.focus_sources = false;
        page.loading = true;
        let keys = Keybindings::default();
        assert!(matches!(
            page.handle_input(KeyCode::Left, &keys),
            Some(AppAction::CancelPendingLoads)
        ));
        assert!(page.focus_sources);
        assert!(!page.loading);
    }
}
