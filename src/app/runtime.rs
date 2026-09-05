use crate::app::App;
use crate::presentation::tui::{Component, Page};
use crate::presentation::tui::NavItem;
use crossterm::event::MouseEventKind;
use ratatui::layout::Position;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent},
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::io;

impl App {
    /// Main run loop
    pub async fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // Initialize the first page
        self.init_current_page().await;

        // Load the logged-in user profile for the sidebar (no-op when logged out).
        self.refresh_current_user().await;

        // Store the last content area for mouse handling
        let mut last_content_area = Rect::default();

        // Scroll accumulator for high-resolution mouse wheel throttling
        // Many modern mice generate multiple scroll events per physical "click"
        const SCROLL_THRESHOLD: i32 = 15; // Accumulate 15 events before scrolling
        let mut scroll_accumulator: i32 = 0;

        while !self.should_quit {
            terminal.draw(|frame| {
                last_content_area = self.get_content_area(frame.area());
                self.draw(frame);
            })?;

            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // mpv runs as an external window with --input-terminal=no,
                        // so it never reads our stdin. The TUI can keep handling
                        // keys normally even while playback is active; draining
                        // keys here made the TUI feel frozen during playback.
                        self.handle_input(key.code, key.modifiers).await;
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            scroll_accumulator += 1;
                            if scroll_accumulator >= SCROLL_THRESHOLD {
                                scroll_accumulator = 0;
                                self.handle_mouse(mouse, last_content_area).await;
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            scroll_accumulator -= 1;
                            if scroll_accumulator <= -SCROLL_THRESHOLD {
                                scroll_accumulator = 0;
                                self.handle_mouse(mouse, last_content_area).await;
                            }
                        }
                        _ => {
                            // Other mouse events (clicks) are handled immediately
                            self.handle_mouse(mouse, last_content_area).await;
                        }
                    },
                    _ => {}
                }
            }

            // Handle background tasks (like QR code polling)
            self.tick().await;
        }
        Ok(())
    }

    /// Compute the sidebar width based on the logged-in user's name length.
    fn sidebar_width(&self) -> u16 {
        let name_w = self
            .current_user
            .as_ref()
            .map(|u| {
                u.uname
                    .chars()
                    .map(|c| if c.is_ascii() { 1 } else { 2 })
                    .sum::<usize>() as u16
            })
            .unwrap_or(0);
        // avatar(5) + gap(2) + padding(4) + name
        let user_w = 5 + 2 + name_w + 4;
        // exp bar needs "经验 " + 16 blocks + " 100%" ≈ 24; add padding
        user_w.max(26).min(30)
    }

    /// Get the content area excluding sidebar
    fn get_content_area(&self, area: Rect) -> Rect {
        // Login page, VideoDetail, DynamicDetail, and BangumiDetail use full area
        if matches!(
            self.current_page,
            Page::Login(_)
                | Page::VideoDetail(_)
                | Page::DynamicDetail(_)
                | Page::ArticleDetail(_)
                | Page::BangumiDetail(_)
                | Page::Up(_)
        ) {
            return area;
        }

        // Main layout with sidebar
        if self.show_sidebar {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(self.sidebar_width()), // Sidebar
                    Constraint::Min(40),    // Content
                ])
                .split(area)[1]
        } else {
            area
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Login page, VideoDetail, DynamicDetail, and BangumiDetail don't show sidebar
        if matches!(
            self.current_page,
            Page::Login(_)
                | Page::VideoDetail(_)
                | Page::DynamicDetail(_)
                | Page::ArticleDetail(_)
                | Page::BangumiDetail(_)
                | Page::Up(_)
        ) {
            match &mut self.current_page {
                Page::Login(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::VideoDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::DynamicDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::ArticleDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::BangumiDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::Up(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                _ => {}
            }
            self.draw_playback_error(frame, area);
            return;
        }

        // Main layout with sidebar
        let chunks = if self.show_sidebar {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(self.sidebar_width()), // Sidebar
                    Constraint::Min(40),    // Content
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(40)])
                .split(area)
        };

        if self.show_sidebar && chunks.len() > 1 {
            self.last_sidebar_area = chunks[0];
            let user = self
                .current_user
                .as_ref()
                .map(|u| (u, &mut self.user_avatar));
            self.sidebar
                .draw(frame, chunks[0], &self.theme, user);
            self.draw_page(frame, chunks[1]);
        } else {
            self.draw_page(frame, chunks[0]);
        }
        self.draw_playback_error(frame, area);
    }

    fn draw_playback_error(&self, frame: &mut Frame, area: Rect) {
        let Some(error) = self.playback.last_error.as_deref() else {
            return;
        };
        let popup = Rect {
            x: area.x,
            y: area.bottom().saturating_sub(3),
            width: area.width,
            height: 3.min(area.height),
        };
        let message = Paragraph::new(error)
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title(" 播放错误 "));
        frame.render_widget(message, popup);
    }

    fn draw_page(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.current_page {
            Page::Login(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Home(page) => {
                if let Some(notice) = self.pending_home_notice.take() {
                    page.set_footer_notice(notice);
                }
                page.draw(frame, area, &self.theme, &self.keybindings);
            }
            Page::Search(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Sections(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Dynamic(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::DynamicDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::ArticleDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::VideoDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::History(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Favorites(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Downloads(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Live(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::LiveDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Settings(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Bangumi(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::BangumiDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Notifications(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Up(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Mall(page) => page.draw(frame, area, &self.theme, &self.keybindings),
        }
    }

    async fn handle_input(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Playback errors remain visible until the user acknowledges them with
        // the next key press; the key still performs its normal action.
        self.playback.last_error = None;
        if self.sidebar_active {
            if self.handle_sidebar_key(key).await {
                return;
            }
            // A non-sidebar key leaves sidebar mode and falls through to the
            // active page normally.
            self.leave_sidebar_mode();
        } else if self.sidebar_visible_page()
            && (key == KeyCode::Tab || key == KeyCode::BackTab)
        {
            // Tab now focuses the sidebar instead of cycling straight to the
            // next page. From there j/k move the highlight and Enter opens.
            self.sidebar_active = true;
            return;
        }
        let keys = &self.keybindings;
        let action = match &mut self.current_page {
            Page::Login(page) => page.handle_input(key, keys),
            Page::Home(page) => page.handle_input(key, keys),
            Page::Search(page) => page.handle_input(key, keys),
            Page::Sections(page) => page.handle_input(key, keys),
            Page::Dynamic(page) => page.handle_input_with_modifiers(key, modifiers, keys),
            Page::DynamicDetail(page) => page.handle_input(key, keys),
            Page::ArticleDetail(page) => page.handle_input(key, keys),
            Page::VideoDetail(page) => page.handle_input(key, keys),
            Page::History(page) => page.handle_input_with_modifiers(key, modifiers, keys),
            Page::Favorites(page) => page.handle_input(key, keys),
            Page::Downloads(page) => page.handle_input(key, keys),
            Page::Live(page) => page.handle_input(key, keys),
            Page::LiveDetail(page) => page.handle_input(key, keys),
            Page::Settings(page) => page.handle_input(key, keys),
            Page::Bangumi(page) => page.handle_input(key, keys),
            Page::BangumiDetail(page) => page.handle_input(key, keys),
            Page::Up(page) => page.handle_input(key, keys),
            Page::Notifications(page) => page.handle_input(key, keys),
            Page::Mall(page) => page.handle_input_with_modifiers(key, modifiers, keys),
        };

        if let Some(action) = action {
            self.handle_action(action).await;
        }
    }

    /// Handle hjkl / arrows / Enter while the user is operating the sidebar
    /// after clicking it. Returns `true` when the key was consumed.
    async fn handle_sidebar_key(&mut self, key: KeyCode) -> bool {
        use crate::application::AppAction;
        use crate::presentation::tui::NavItem;
        match key {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Esc => {
                self.leave_sidebar_mode();
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                // Move the sidebar highlight only; Enter opens the page.
                self.sidebar.next_highlight();
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.sidebar.prev_highlight();
                true
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.sidebar.hot_expanded {
                    // Collapse the 热门 group, staying in sidebar mode.
                    self.sidebar.select(NavItem::HotGroup);
                }
                true
            }
            KeyCode::Char('l') | KeyCode::Right => {
                // Expand the collapsed 热门 group (highlight moves to the first
                // feed). Does not open the page yet.
                if !self.sidebar.hot_expanded && self.sidebar.selected == NavItem::HotGroup {
                    self.sidebar.select(NavItem::HotGroup);
                }
                true
            }
            KeyCode::Enter => {
                if self.sidebar.selected == NavItem::HotGroup && !self.sidebar.hot_expanded {
                    // The group header itself has no page; expand it instead.
                    self.sidebar.select(NavItem::HotGroup);
                } else {
                    self.handle_action(AppAction::NavSelect(self.sidebar.selected))
                        .await;
                    self.sidebar_active = false;
                }
                true
            }
            _ => false,
        }
    }

    /// Pages that render the global sidebar (as opposed to full-area detail
    /// pages which use Tab for their own in-page switching).
    fn sidebar_visible_page(&self) -> bool {
        !matches!(
            self.current_page,
            Page::Login(_)
                | Page::VideoDetail(_)
                | Page::DynamicDetail(_)
                | Page::ArticleDetail(_)
                | Page::BangumiDetail(_)
                | Page::Up(_)
        )
    }

    /// Exit sidebar mode and restore the sidebar highlight to the item that
    /// matches the currently shown page.
    fn leave_sidebar_mode(&mut self) {
        self.sidebar_active = false;
        let item = match &self.current_page {
            Page::Home(page) => match page.feed() {
                crate::api::recommend::HomeFeed::Recommended => NavItem::Home,
                crate::api::recommend::HomeFeed::Popular => NavItem::Popular,
                crate::api::recommend::HomeFeed::Weekly => NavItem::Weekly,
                crate::api::recommend::HomeFeed::Ranking => NavItem::Ranking,
                crate::api::recommend::HomeFeed::MustWatch => NavItem::MustWatch,
            },
            Page::Search(_) => NavItem::Search,
            Page::Sections(_) => NavItem::Sections,
            Page::Dynamic(_) => NavItem::Dynamic,
            Page::History(_) => NavItem::History,
            Page::Favorites(_) => NavItem::Favorites,
            Page::Downloads(_) => NavItem::Downloads,
            Page::Live(_) => NavItem::Live,
            Page::Settings(_) => NavItem::Settings,
            Page::Bangumi(_) => NavItem::Bangumi,
            Page::Notifications(_) => NavItem::Notifications,
            Page::Mall(_) => NavItem::Mall,
            // Full-area pages never show the sidebar, so nothing to sync.
            _ => return,
        };
        if item.is_hot_feed() {
            self.sidebar.hot_expanded = true;
        }
        self.sidebar.select(item);
    }

    async fn handle_mouse(&mut self, event: MouseEvent, area: Rect) {
        // Sidebar clicks: select the nav item under the cursor.
        if self.show_sidebar {
            let pos = Position::new(event.column, event.row);
            if self.last_sidebar_area.contains(pos) {
                let header_h: u16 = if self.current_user.is_some() { 9 } else { 4 };
                // block has only a right border, so inner.y == area.y
                let nav_start = self.last_sidebar_area.y + header_h + 1;
                let row = (event.row.saturating_sub(nav_start)) as usize;
                let nav_rows = self
                    .last_sidebar_area
                    .height
                    .saturating_sub(header_h + 2)
                    .max(1) as usize;
                let scroll = self.sidebar.scroll_for_selected(nav_rows);
                let items = self.sidebar.visible_items();
                let idx = scroll + row;
                if row < nav_rows && idx < items.len() {
                    let item = items[idx];
                    match event.kind {
                        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                            self.sidebar_active = true;
                            self.handle_action(crate::application::AppAction::NavSelect(item)).await; return;
                        }
                        // scrolling over the sidebar moves selection sequentially
                        MouseEventKind::ScrollDown => {
                            self.sidebar_active = true;
                            self.sidebar.next();
                            self.handle_action(crate::application::AppAction::NavSelect(
                                self.sidebar.selected,
                            ))
                            .await;
                            return;
                        }
                        MouseEventKind::ScrollUp => {
                            self.sidebar_active = true;
                            self.sidebar.prev();
                            self.handle_action(crate::application::AppAction::NavSelect(
                                self.sidebar.selected,
                            ))
                            .await;
                            return;
                        }
                        _ => return,
                    }
                }
                return;
            }
        }
        // Any mouse interaction with the content area leaves sidebar mode and
        // restores the highlight to the current page's sidebar item.
        self.leave_sidebar_mode();
        let action = match &mut self.current_page {
            Page::Login(page) => page.handle_mouse(event, area),
            Page::Home(page) => page.handle_mouse(event, area),
            Page::Search(page) => page.handle_mouse(event, area),
            Page::Sections(page) => page.handle_mouse(event, area),
            Page::Dynamic(page) => page.handle_mouse(event, area),
            Page::DynamicDetail(page) => page.handle_mouse(event, area),
            Page::ArticleDetail(page) => page.handle_mouse(event, area),
            Page::VideoDetail(page) => page.handle_mouse(event, area),
            Page::History(page) => page.handle_mouse(event, area),
            Page::Favorites(page) => page.handle_mouse(event, area),
            Page::Downloads(page) => page.handle_mouse(event, area),
            Page::Live(page) => page.handle_mouse(event, area),
            Page::LiveDetail(page) => page.handle_mouse(event, area),
            Page::Settings(page) => page.handle_mouse(event, area),
            Page::Bangumi(page) => page.handle_mouse(event, area),
            Page::BangumiDetail(page) => page.handle_mouse(event, area),
            Page::Up(page) => page.handle_mouse(event, area),
            Page::Notifications(page) => page.handle_mouse(event, area),
            Page::Mall(page) => page.handle_mouse(event, area),
        };

        if let Some(action) = action {
            self.handle_action(action).await;
        }
    }

    pub(super) async fn tick(&mut self) {
        self.drain_network_events();
        self.poll_user_avatar();
        self.poll_matugen_theme();
        if let Some((items, source, start_index, order)) = self.pending_playlist.take() {
            self.start_playlist(items, source, start_index, order).await;
        }
        while let Ok(event) = self.playback_event_rx.try_recv() {
            let accepted = self.playback.apply_event(&event);
            match event {
                crate::domain::playback::PlaybackEvent::Finished {
                    session_id,
                    bvid: Some(bvid),
                } => {
                    if accepted
                        && self.auto_return_after_playback.as_ref()
                            == Some(&(session_id, bvid.clone()))
                        && matches!(&self.current_page, Page::VideoDetail(page) if page.bvid == bvid)
                    {
                        self.auto_return_after_playback = None;
                        self.handle_action(crate::application::AppAction::BackToList)
                            .await;
                    } else if self
                        .auto_return_after_playback
                        .as_ref()
                        .is_some_and(|(id, _)| *id == session_id)
                    {
                        self.auto_return_after_playback = None;
                    }
                }
                crate::domain::playback::PlaybackEvent::Failed { session_id, .. }
                    if accepted
                        && self
                            .auto_return_after_playback
                            .as_ref()
                            .is_some_and(|(id, _)| *id == session_id) =>
                {
                    self.auto_return_after_playback = None;
                }
                crate::domain::playback::PlaybackEvent::Started {
                    session_id,
                    bvid,
                    cid,
                    success,
                    error,
                } if accepted => {
                    if !success {
                        self.playback.last_error = error;
                    }
                    // Refresh stream support info on the detail page once the
                    // player has started (mirrors the old inline behaviour).
                    if let (Some(bvid), Some(cid)) = (bvid, cid)
                        && matches!(&self.current_page, Page::VideoDetail(page) if page.bvid == bvid)
                    {
                        let req_id = self.next_request_id("video_detail");
                        self.send_network_command(
                            crate::application::network::NetworkCommand::ProbeVideoStreams {
                                req_id,
                                bvid,
                                cid,
                            },
                        );
                    }
                }
                _ => {}
            }
        }

        let auto_play = if self.config.auto_play {
            match &mut self.current_page {
                Page::VideoDetail(page)
                    if page.auto_play_pending && !page.loading && page.video_info.is_some() =>
                {
                    page.auto_play_pending = false;
                    Some((Some(page.bvid.clone()), page.play_action()))
                }
                Page::BangumiDetail(page)
                    if page.auto_play_pending && !page.loading && page.season.is_some() =>
                {
                    page.auto_play_pending = false;
                    page.play_action().map(|action| (None, action))
                }
                _ => None,
            }
        } else {
            // Auto-play disabled: clear the pending flag so it doesn't fire
            // once the user re-enables the setting mid-session.
            if let Page::VideoDetail(page) = &mut self.current_page {
                page.auto_play_pending = false;
            }
            if let Page::BangumiDetail(page) = &mut self.current_page {
                page.auto_play_pending = false;
            }
            None
        };
        if let Some((return_bvid, action)) = auto_play {
            self.handle_action(action).await;
            if let (Some(bvid), Some(session_id)) = (return_bvid, self.playback.session_id) {
                self.auto_return_after_playback = Some((session_id, bvid));
            }
        }
        match &mut self.current_page {
            Page::Login(page) => {
                let client = &self.api_client;
                if let Some(action) = page.tick(client).await {
                    self.handle_action(action).await;
                }
            }
            Page::Home(page) => {
                // Non-blocking: poll completed downloads and start new ones
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::Search(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::Sections(page) => {
                page.videos.poll_cover_results();
                page.videos.start_cover_downloads();
            }
            Page::Dynamic(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::VideoDetail(page) => {
                page.tick();
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::ArticleDetail(page) => {
                page.poll_image_results();
                page.start_image_downloads();
            }
            Page::History(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::Favorites(page) => {
                page.tick();
                page.videos.poll_cover_results();
                page.videos.start_cover_downloads();
            }
            Page::Bangumi(page) => {
                page.index_grid.poll_cover_results();
                page.index_grid.start_cover_downloads();
                page.follow_grid.poll_cover_results();
                page.follow_grid.start_cover_downloads();
                page.search_grid.poll_cover_results();
                page.search_grid.start_cover_downloads();
            }
            Page::Up(page) => {
                page.tick();
                page.videos.poll_cover_results();
                page.videos.start_cover_downloads();
                page.favorite_videos.poll_cover_results();
                page.favorite_videos.start_cover_downloads();
                page.series_videos.poll_cover_results();
                page.series_videos.start_cover_downloads();
                page.series_cards.poll_cover_results();
                page.series_cards.start_cover_downloads();
                page.article_cards.poll_cover_results();
                page.article_cards.start_cover_downloads();
            }
            Page::Notifications(page) => {
                page.poll_avatar_results();
                page.poll_cover_results();
            }
            _ => {}
        }
    }
}
