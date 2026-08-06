//! Message notification center page
//!
//! Four tabs: 私信 / @我的 / 点赞 / 系统. The chat tab lists private-message
//! conversations (/x/session/v2/sessions); Enter opens the chat detail view
//! with an input box at the bottom. The other tabs list notification items
//! fetched from /x/msgfeed.

use super::{Component, Theme, shortcut_footer};
use crate::api::client::ApiClient;
use crate::api::msg::{ChatMessage, ChatSession, NotificationItem};
use crate::application::AppAction;
use crate::storage::Keybindings;
use crate::api::msg::session_last_text;
use image::DynamicImage;
use ratatui::{
    crossterm::event::{KeyCode, KeyModifiers, MouseEvent},
    prelude::*,
    widgets::*,
};
use ratatui_image::{StatefulImage, picker::Picker, protocol::StatefulProtocol};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use unicode_width::UnicodeWidthStr;

/// Result of a background avatar download.
struct AvatarResult {
    index: usize,
    protocol: Option<StatefulProtocol>,
}

/// Result of a background video-cover download.
struct CoverResult {
    index: usize,
    protocol: Option<StatefulProtocol>,
}

/// Number of display lines `text` occupies when wrapped to `width` columns.
/// Uses unicode display widths (CJK = 2) so the estimate matches ratatui's
/// rendering closely enough for scroll math.
fn wrapped_line_count(text: &str, width: u16) -> usize {
    let width = width.max(1) as usize;
    let mut lines = 1usize;
    let mut cur = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            lines += 1;
            cur = 0;
            continue;
        }
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if cur + w > width && w <= width {
            lines += 1;
            cur = 0;
        }
        cur += w;
    }
    lines
}

/// Split `text` into display lines that fit `width` columns (CJK = 2 cols).
fn wrap_text(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
            continue;
        }
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if cur_w + w > width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        cur.push(ch);
        cur_w += w;
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifTab {
    Chat,
    At,
    Like,
    Sys,
}

impl NotifTab {
    pub fn label(&self) -> &'static str {
        match self {
            NotifTab::Chat => "私信",
            NotifTab::At => "@我",
            NotifTab::Like => "点赞",
            NotifTab::Sys => "系统",
        }
    }

    pub fn feed_type(&self) -> i32 {
        match self {
            NotifTab::Chat => 1,
            NotifTab::At => 2,
            NotifTab::Like => 3,
            NotifTab::Sys => 6,
        }
    }

    pub fn all() -> [NotifTab; 4] {
        [NotifTab::Chat, NotifTab::At, NotifTab::Like, NotifTab::Sys]
    }
}

/// One display row in the grouped message list.
#[derive(Debug, Clone)]
pub enum NotifRow {
    /// A per-user section header (not selectable).
    Header { name: String, count: usize },
    /// A single notification item, referencing index into `items`.
    Item { item_idx: usize },
}

pub struct NotificationsPage {
    pub tab: NotifTab,
    /// Whether we are inside the chat detail view (chat_view = true) or the
    /// session/notification list (false).
    pub chat_view: bool,
    /// Conversation list (chat tab).
    pub sessions: Vec<ChatSession>,
    pub session_selected: usize,
    pub session_scroll: usize,
    /// Current conversation.
    pub chat_talker: Option<ChatSession>,
    pub chat_messages: Vec<ChatMessage>,
    pub chat_input: String,
    /// Whether the chat input box is active (opened with `/`).
    pub chat_input_active: bool,
    pub chat_loading: bool,
    pub chat_sending: bool,
    /// Selected message index inside `chat_messages` (chat detail view).
    pub chat_selected: usize,
    pub chat_scroll: usize,
    pub items: Vec<NotificationItem>,
    /// Grouped rows: a user header followed by that user's items.
    pub rows: Vec<NotifRow>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub loading: bool,
    pub loading_more: bool,
    pub has_more: bool,
    pub message: Option<String>,
    /// unread counts (reply, at, like, sys)
    pub unread: (i32, i32, i32, i32),
    /// Avatar rendering: one protocol slot per session (index-aligned).
    picker: Arc<Picker>,
    avatar_tx: mpsc::Sender<AvatarResult>,
    avatar_rx: mpsc::Receiver<AvatarResult>,
    avatar_protocols: Vec<Option<StatefulProtocol>>,
    pending_avatars: HashSet<usize>,
    /// Video-cover rendering: one protocol slot per chat message (index-aligned).
    cover_tx: mpsc::Sender<CoverResult>,
    cover_rx: mpsc::Receiver<CoverResult>,
    cover_protocols: Vec<Option<StatefulProtocol>>,
    pending_covers: HashSet<usize>,
}

impl NotificationsPage {
    pub fn new() -> Self {
        let picker = Arc::new(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
        let (avatar_tx, avatar_rx) = mpsc::channel(64);
        let (cover_tx, cover_rx) = mpsc::channel(64);
        Self {
            tab: NotifTab::Chat,
            chat_view: false,
            sessions: Vec::new(),
            session_selected: 0,
            session_scroll: 0,
            chat_talker: None,
            chat_messages: Vec::new(),
            chat_input: String::new(),
            chat_input_active: false,
            chat_loading: false,
            chat_sending: false,
            chat_selected: 0,
            chat_scroll: 0,
            items: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            loading: false,
            loading_more: false,
            has_more: false,
            message: None,
            unread: (0, 0, 0, 0),
            picker,
            avatar_tx,
            avatar_rx,
            avatar_protocols: Vec::new(),
            pending_avatars: HashSet::new(),
            cover_tx,
            cover_rx,
            cover_protocols: Vec::new(),
            pending_covers: HashSet::new(),
        }
    }

    pub fn begin_loading(&mut self) {
        self.loading = true;
        self.message = None;
    }

    pub fn apply_items(&mut self, items: Vec<NotificationItem>, append: bool) {
        if append {
            self.items.extend(items);
        } else {
            self.items = items;
            self.selected = 0;
            self.scroll_offset = 0;
        }
        self.rebuild_rows();
        self.has_more = self.items.len() >= 20;
        self.loading = false;
        self.loading_more = false;
    }

    /// Apply the conversation list (chat tab).
    pub fn apply_sessions(&mut self, sessions: Vec<ChatSession>) {
        self.sessions = sessions;
        if self.session_selected >= self.sessions.len() {
            self.session_selected = self.sessions.len().saturating_sub(1);
        }
        self.avatar_protocols = (0..self.sessions.len()).map(|_| None).collect();
        self.pending_avatars.clear();
        self.start_avatar_downloads();
        self.loading = false;
        self.loading_more = false;
        self.message = None;
    }

    /// Show an error when the session list (or chat detail) fails to load,
    /// instead of leaving the spinner stuck forever.
    pub fn apply_sessions_error(&mut self, msg: String) {
        self.loading = false;
        self.loading_more = false;
        self.message = Some(msg);
    }

    /// Show an error when a notification feed fails to load.
    pub fn apply_items_error(&mut self, msg: String) {
        self.loading = false;
        self.loading_more = false;
        self.message = Some(msg);
    }

    /// Poll for completed avatar downloads (non-blocking, called every tick).
    pub fn poll_avatar_results(&mut self) {
        while let Ok(result) = self.avatar_rx.try_recv() {
            if result.index < self.avatar_protocols.len() {
                self.avatar_protocols[result.index] = result.protocol;
            }
            self.pending_avatars.remove(&result.index);
        }
    }

    /// Poll for completed cover downloads (non-blocking, called every tick).
    pub fn poll_cover_results(&mut self) {
        while let Ok(result) = self.cover_rx.try_recv() {
            if result.index < self.cover_protocols.len() {
                self.cover_protocols[result.index] = result.protocol;
            }
            self.pending_covers.remove(&result.index);
        }
    }

    /// Spawn background downloads for video-share covers without a protocol yet.
    fn start_cover_downloads(&mut self) {
        for (idx, msg) in self.chat_messages.iter().enumerate() {
            if self.cover_protocols[idx].is_some() || self.pending_covers.contains(&idx) {
                continue;
            }
            let Some(cover_url) = msg.video_cover.clone() else {
                continue;
            };
            self.pending_covers.insert(idx);
            let tx = self.cover_tx.clone();
            let picker = Arc::clone(&self.picker);
            tokio::spawn(async move {
                let protocol = download_cover(&cover_url, &picker).await;
                let _ = tx.send(CoverResult { index: idx, protocol }).await;
            });
        }
    }

    /// Spawn background downloads for all sessions without an avatar yet.
    fn start_avatar_downloads(&mut self) {
        for (idx, session) in self.sessions.iter().enumerate() {
            if self.avatar_protocols[idx].is_some() || self.pending_avatars.contains(&idx) {
                continue;
            }
            let Some(face_url) = session.face.clone() else {
                continue;
            };
            self.pending_avatars.insert(idx);
            let tx = self.avatar_tx.clone();
            let picker = Arc::clone(&self.picker);
            tokio::spawn(async move {
                let protocol = download_avatar(&face_url, &picker).await;
                let _ = tx.send(AvatarResult { index: idx, protocol }).await;
            });
        }
    }

    /// Begin loading a chat with the given talker.
    pub fn open_chat(&mut self, talker_id: i64) {
        if let Some(session) = self.sessions.iter().find(|s| s.talker_id == talker_id) {
            self.chat_talker = Some(session.clone());
        }
        self.chat_view = true;
        self.chat_messages.clear();
        self.cover_protocols.clear();
        self.pending_covers.clear();
        self.chat_input.clear();
        self.chat_loading = true;
        self.chat_sending = false;
        self.message = None;
    }

    /// Apply chat history.
    pub fn apply_chat_messages(&mut self, talker_id: i64, messages: Vec<ChatMessage>) {
        if self.chat_talker.as_ref().map(|s| s.talker_id) != Some(talker_id) {
            return;
        }
        self.chat_messages = messages;
        self.cover_protocols = (0..self.chat_messages.len()).map(|_| None).collect();
        self.pending_covers.clear();
        self.start_cover_downloads();
        self.chat_selected = self.chat_messages.len().saturating_sub(1);
        self.chat_scroll = self.chat_selected;
        self.chat_loading = false;
        self.message = None;
    }

    /// Handle a send result. On failure keep the input text so the user can
    /// retry; on success clear it.
    pub fn apply_chat_sent(&mut self, ok: bool, error: Option<String>) {
        self.chat_sending = false;
        if ok {
            self.chat_input.clear();
            self.message = Some("已发送".to_string());
        } else {
            self.message = Some(error.unwrap_or_else(|| "发送失败".to_string()));
        }
    }

    /// Rebuild the per-user grouped rows from `self.items`, preserving item order
    /// within each user section.
    fn rebuild_rows(&mut self) {
        self.rows.clear();
        // First-seen order of users: (mid, display name)
        let mut order: Vec<(i64, String)> = Vec::new();
        let mut seen: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for item in &self.items {
            let mid = item.user_mid.unwrap_or(0);
            let name = item
                .user_name
                .clone()
                .unwrap_or_else(|| "未知用户".to_string());
            if !seen.contains_key(&mid) {
                seen.insert(mid, order.len());
                order.push((mid, name));
            }
        }
        // Count items per user.
        let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for item in &self.items {
            let mid = item.user_mid.unwrap_or(0);
            *counts.entry(mid).or_insert(0) += 1;
        }
        for (mid, name) in order {
            let count = counts.get(&mid).copied().unwrap_or(0);
            self.rows.push(NotifRow::Header { name, count });
            for (idx, item) in self.items.iter().enumerate() {
                if item.user_mid.unwrap_or(0) == mid {
                    self.rows.push(NotifRow::Item { item_idx: idx });
                }
            }
        }
    }

    /// Index of the next selectable item row after `from_row`.
    fn next_item_row(&self, from_row: usize) -> Option<usize> {
        (from_row + 1..self.rows.len()).find(|&r| matches!(self.rows[r], NotifRow::Item { .. }))
    }

    /// Index of the previous selectable item row before `from_row`.
    fn prev_item_row(&self, from_row: usize) -> Option<usize> {
        (0..from_row).rev().find(|&r| matches!(self.rows[r], NotifRow::Item { .. }))
    }

    /// Current row index for `selected` item.
    fn selected_row(&self) -> Option<usize> {
        self.rows.iter().position(|r| {
            matches!(r, NotifRow::Item { item_idx } if *item_idx == self.selected)
        })
    }

    pub fn apply_unread(&mut self, reply: i32, at: i32, like: i32, sys: i32) {
        self.unread = (reply, at, like, sys);
    }

    pub fn set_message(&mut self, msg: String) {
        self.message = Some(msg);
        self.loading = false;
        self.loading_more = false;
    }

    fn visible_range(&self, height: u16) -> (usize, usize) {
        let page = (height as usize).saturating_sub(4).max(1);
        let start = self.scroll_offset;
        let end = (start + page).min(self.rows.len());
        (start, end)
    }

    /// Relative time string for a unix timestamp (or empty).
    fn rel_time(ct: i64) -> String {
        if ct <= 0 {
            return String::new();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let diff = now - ct;
        if diff < 60 {
            "刚刚".to_string()
        } else if diff < 3600 {
            format!("{}分钟前", diff / 60)
        } else if diff < 86400 {
            format!("{}小时前", diff / 3600)
        } else if diff < 2592000 {
            format!("{}天前", diff / 86400)
        } else {
            format!("{}月前", diff / 2592000)
        }
    }

    /// Display lines for one notification item, styled like a chat message:
    /// `用户名: 内容` wrapped, then the title (if any), then a relative-time
    /// line (if any).  Matches the ordinary-message layout in `draw_chat`.
    fn item_display_lines(&self, item_idx: usize, width: u16) -> Vec<String> {
        let Some(item) = self.items.get(item_idx) else {
            return vec!["(无内容)".to_string()];
        };
        let width = width.max(1);
        let name = item
            .user_name
            .clone()
            .unwrap_or_else(|| "未知用户".to_string());
        let mut lines = Vec::new();
        let msg = item.message.as_deref().unwrap_or("").trim();
        if !msg.is_empty() {
            lines.extend(wrap_text(&format!("{}: {}", name, msg), width));
        } else if let Some(t) = item.title.as_deref() {
            let t = t.trim();
            if !t.is_empty() {
                lines.extend(wrap_text(&format!("{}: {}", name, t), width));
            }
        } else {
            lines.push(format!("{}: (无内容)", name));
        }
        if let Some(t) = item.title.as_deref() {
            let t = t.trim();
            if !t.is_empty() {
                lines.extend(wrap_text(&format!("  {}", t), width));
            }
        }
        if let Some(ct) = item.ctime {
            let rel = Self::rel_time(ct);
            if !rel.is_empty() {
                lines.push(format!("    ({})", rel));
            }
        }
        lines
    }

    /// Build the display-line table for the grouped list. Each header is one
    /// line; each item is expanded to as many lines as its wrapped text takes.
    /// Returns (row_idx, display_line) pairs where display_line is the line
    /// number within that logical row (0-based).
    fn display_rows(&self, width: u16) -> Vec<(usize, usize)> {
        let wrap_width = width.saturating_sub(2).max(1);
        let mut out = Vec::new();
        for (ri, row) in self.rows.iter().enumerate() {
            match row {
                NotifRow::Header { .. } => out.push((ri, 0)),
                NotifRow::Item { item_idx } => {
                    let lines = self.item_display_lines(*item_idx, wrap_width).len().max(1);
                    for li in 0..lines {
                        out.push((ri, li));
                    }
                }
            }
        }
        out
    }

    /// Logical row index (into `rows`) for a display-line index.
    fn display_line_count(&self, width: u16) -> usize {
        self.display_rows(width).len()
    }
}

impl Default for NotificationsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationsPage {
    fn switch_tab(&mut self, tab: NotifTab) -> AppAction {
        if self.tab == tab {
            return AppAction::None;
        }
        self.tab = tab;
        self.chat_view = false;
        self.chat_talker = None;
        self.chat_messages.clear();
        self.chat_input.clear();
        self.sessions.clear();
        self.avatar_protocols.clear();
        self.pending_avatars.clear();
        self.items.clear();
        self.rows.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        AppAction::SwitchNotifTab(tab)
    }
}

impl Component for NotificationsPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        if self.chat_view {
            self.draw_chat(frame, area, theme);
            return;
        }
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tabs + unread
                Constraint::Length(1), // spacer
                Constraint::Min(5),    // List
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Tabs
        let mut tab_spans = Vec::new();
        for (i, tab) in NotifTab::all().iter().enumerate() {
            let unread = match tab {
                NotifTab::Chat => self.unread.0,
                NotifTab::At => self.unread.1,
                NotifTab::Like => self.unread.2,
                NotifTab::Sys => self.unread.3,
            };
            let is_active = *tab == self.tab;
            let label = if unread > 0 {
                format!(" {} ({}) ", tab.label(), unread)
            } else {
                format!(" {} ", tab.label())
            };
            let style = if is_active {
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(if unread > 0 {
                    theme.warning
                } else {
                    theme.fg_secondary
                })
            };
            if i > 0 {
                tab_spans.push(Span::styled(" │ ", Style::default().fg(theme.fg_secondary)));
            }
            tab_spans.push(Span::styled(label, style));
        }
        let tab_line = Line::from(tab_spans);
        let tabs = Paragraph::new(tab_line)
            .block(Block::default().borders(Borders::BOTTOM).border_style(
                Style::default().fg(theme.border_subtle),
            ));
        frame.render_widget(tabs, chunks[0]);

        // List
        if self.tab == NotifTab::Chat {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_subtle));
            let inner = block.inner(chunks[2]);
            frame.render_widget(block, chunks[2]);
            if self.loading && self.sessions.is_empty() {
                let msg = Paragraph::new(Line::from(Span::styled(
                    "  ⏳ 加载中...",
                    Style::default().fg(theme.fg_secondary),
                )));
                frame.render_widget(msg, inner);
            } else if self.sessions.is_empty() {
                let msg = Paragraph::new(Line::from(Span::styled(
                    "  (空) 暂无私信会话，说点什么吧",
                    Style::default().fg(theme.fg_secondary),
                )));
                frame.render_widget(msg, inner);
            }
            const ROW_H: u16 = 3;
            let max_rows = (inner.height / ROW_H).max(1);
            let start = self.session_scroll;
            let end = (start + max_rows as usize).min(self.sessions.len());
            for (row, i) in (start..end).enumerate() {
                let row_area = Rect::new(
                    inner.x,
                    inner.y + (row as u16) * ROW_H,
                    inner.width,
                    ROW_H,
                );
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(7), Constraint::Min(5)])
                    .split(row_area);
                let avatar_area = cols[0];
                let text_area = cols[1];
                let is_selected = i == self.session_selected;
                // Avatar (6x3 cells) or placeholder
                match self.avatar_protocols.get_mut(i).and_then(|p| p.as_mut()) {
                    Some(protocol) => {
                        let img = StatefulImage::new();
                        let img_area = Rect::new(avatar_area.x, avatar_area.y, 5, ROW_H);
                        frame.render_stateful_widget(img, img_area, protocol);
                    }
                    None => {
                        let ph = Paragraph::new(Line::from(Span::styled(
                            if self.pending_avatars.contains(&i) { "◌" } else { "○" },
                            Style::default().fg(theme.fg_secondary),
                        )));
                        frame.render_widget(ph, Rect::new(avatar_area.x, avatar_area.y, 5, 1));
                    }
                }
                let text_style = |s: Style| {
                    if is_selected {
                        s.bg(theme.selection_bg)
                    } else {
                        s
                    }
                };
                let session = &self.sessions[i];
                let unread = if session.unread_count > 0 {
                    format!(" ({})", session.unread_count)
                } else {
                    String::new()
                };
                let name_style = if is_selected {
                    Style::default()
                        .fg(theme.fg_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg_primary)
                };
                let last_text = session
                    .last_msg
                    .as_ref()
                    .map(session_last_text)
                    .unwrap_or_default();
                let time = session
                    .last_msg
                    .as_ref()
                    .map(|m| m.format_time())
                    .unwrap_or_default();
                // Row 0: name + unread on the left, time on the right.
                let time_width = UnicodeWidthStr::width(time.as_str()) as u16;
                let name_max = text_area.width.saturating_sub(time_width + 1).max(4);
                let mut name_spans = vec![
                    Span::styled(
                        if is_selected { "▶ " } else { "  " },
                        text_style(Style::default().fg(if is_selected {
                            theme.bilibili_pink
                        } else {
                            theme.fg_secondary
                        })),
                    ),
                    Span::styled(format!("{}{}", session.uname, unread), text_style(name_style)),
                ];
                if !time.is_empty() {
                    let used = UnicodeWidthStr::width(format!("{}{}", session.uname, unread).as_str()) as u16 + 2;
                    if used < name_max {
                        name_spans.push(Span::raw(" ".repeat((name_max - used) as usize)));
                    }
                    name_spans.push(Span::styled(
                        time,
                        text_style(Style::default().fg(theme.fg_secondary)),
                    ));
                }
                let name_para = Paragraph::new(Line::from(name_spans));
                frame.render_widget(name_para, Rect::new(text_area.x, text_area.y, text_area.width, 1));
                // Rows 1-2: last message, wrapped to two rows max.
                if !last_text.is_empty() {
                    let last_para = Paragraph::new(Line::from(Span::styled(
                        format!("  {}", last_text),
                        text_style(Style::default().fg(theme.fg_secondary)),
                    )))
                    .wrap(Wrap { trim: true });
                    frame.render_widget(
                        last_para,
                        Rect::new(text_area.x, text_area.y + 1, text_area.width, 2),
                    );
                }
            }
            let footer = shortcut_footer(
                theme,
                [
                    ("↑/↓".to_string(), "移动".to_string(), theme.info),
                    ("1-4".to_string(), "切换标签".to_string(), theme.info),
                    ("Tab".to_string(), "切换侧边栏".to_string(), theme.info),
                    ("Enter".to_string(), "打开聊天".to_string(), theme.success),
                    ("R".to_string(), "刷新".to_string(), theme.info),
                    (keys.back.to_string(), "返回".to_string(), theme.warning),
                ],
            );
            frame.render_widget(Paragraph::new(footer), chunks[3]);
            return;
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_subtle));
        let inner = block.inner(chunks[2]);
        frame.render_widget(block, chunks[2]);
        if self.loading && self.items.is_empty() {
            let msg = Paragraph::new(Line::from(Span::styled(
                "  ⏳ 加载中...",
                Style::default().fg(theme.fg_secondary),
            )));
            frame.render_widget(msg, inner);
        } else if self.items.is_empty() {
            let text = if let Some(err) = &self.message {
                format!("  {}", err)
            } else if self.tab == NotifTab::Sys {
                "  暂无系统消息".to_string()
            } else {
                "  (空) 暂无消息".to_string()
            };
            let style = if self.message.is_some() {
                Style::default().fg(theme.warning)
            } else {
                Style::default().fg(theme.fg_secondary)
            };
            let msg = Paragraph::new(Line::from(Span::styled(text, style)));
            frame.render_widget(msg, inner);
        } else {
            // Grouped rows: section header + notification items.
            let display = self.display_rows(inner.width);
            if display.is_empty() {
                let msg = Paragraph::new(Line::from(Span::styled(
                    "  (空) 暂无消息",
                    Style::default().fg(theme.fg_secondary),
                )));
                frame.render_widget(msg, inner);
                return;
            }
            // Keep the selected item visible: compute its first display line.
            let sel_disp = display
                .iter()
                .position(|(ri, _li)| {
                    matches!(self.rows[*ri], NotifRow::Item { item_idx } if item_idx == self.selected)
                })
                .unwrap_or(0);
            let height = inner.height as usize;
            if self.scroll_offset > sel_disp {
                self.scroll_offset = sel_disp;
            }
            let sel_lines = display
                .iter()
                .filter(|(ri, _li)| {
                    matches!(self.rows[*ri], NotifRow::Item { item_idx } if item_idx == self.selected)
                })
                .count();
            if sel_disp + sel_lines > self.scroll_offset + height {
                self.scroll_offset = sel_disp + sel_lines - height;
            }
            let start = self.scroll_offset.min(display.len());
            let max_rows = inner.height as usize;
            let end = (start + max_rows).min(display.len());
            for (i, row_idx) in (start..end).enumerate() {
                let row_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                let (logical_row, line_no) = display[row_idx];
                match &self.rows[logical_row] {
                    NotifRow::Header { name, count } => {
                        let line = Line::from(Span::styled(
                            format!("── {} ({}条) ──", name, count),
                            Style::default().fg(theme.border_subtle),
                        ));
                        frame.render_widget(Paragraph::new(line), row_area);
                    }
                    NotifRow::Item { item_idx } => {
                        let is_selected = *item_idx == self.selected;
                        let wrap_width = inner.width.saturating_sub(2).max(1);
                        let lines = self.item_display_lines(*item_idx, wrap_width);
                        let text = lines
                            .get(line_no)
                            .cloned()
                            .unwrap_or_default();
                        let base = if is_selected {
                            Style::default().bg(theme.selection_bg)
                        } else {
                            Style::default()
                        };
                        // First line: `用户名: 内容` with the name bold and
                        // colored, matching the chat-detail message style.
                        let line = if line_no == 0 {
                            match text.split_once(':') {
                                Some((head, rest)) => Line::from(vec![
                                    Span::styled(
                                        format!("  {}:", head),
                                        base.fg(theme.info).add_modifier(Modifier::BOLD),
                                    ),
                                    Span::styled(rest, base.fg(theme.fg_primary)),
                                ]),
                                None => Line::from(Span::styled(format!("  {}", text), base)),
                            }
                        } else {
                            Line::from(Span::styled(format!("  {}", text), base))
                        };
                        frame.render_widget(Paragraph::new(line), row_area);
                    }
                }
            }
        }

        // Footer
        let footer = shortcut_footer(
            theme,
            [
                ("↑/↓".to_string(), "移动".to_string(), theme.info),
                ("1-4".to_string(), "切换标签".to_string(), theme.info),
                ("Tab".to_string(), "切换侧边栏".to_string(), theme.info),
                ("Enter".to_string(), "打开".to_string(), theme.success),
                ("R".to_string(), "刷新".to_string(), theme.info),
                (keys.back.to_string(), "返回".to_string(), theme.warning),
            ],
        );
        frame.render_widget(Paragraph::new(footer), chunks[3]);
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        if self.chat_view {
            if keys.matches_back(key) || keys.matches_quit(key) {
                if self.chat_input_active {
                    // Esc in input mode: cancel input, keep the chat open.
                    self.chat_input.clear();
                    self.chat_input_active = false;
                    return Some(AppAction::None);
                }
                return Some(AppAction::BackToChatList);
            }
            match key {
                KeyCode::Char('/') => {
                    // Slash opens the input box.
                    self.chat_input.clear();
                    self.chat_input_active = true;
                    return Some(AppAction::None);
                }
                KeyCode::Enter => {
                    if self.chat_input_active {
                        let content = self.chat_input.trim().to_string();
                        let talker_id = self.chat_talker.as_ref().map(|s| s.talker_id);
                        if content.is_empty() {
                            self.chat_input_active = false;
                            return Some(AppAction::None);
                        }
                        if self.chat_sending {
                            return Some(AppAction::None);
                        }
                        if let Some(talker_id) = talker_id {
                            self.chat_sending = true;
                            self.message = None;
                            return Some(AppAction::SendChatMessage { talker_id, content });
                        }
                        return Some(AppAction::None);
                    }
                    // Enter without input: open the selected message if it is a
                    // video share.
                    if let Some(msg) = self.chat_messages.get(self.chat_selected) {
                        if let Some(bvid) = &msg.bvid {
                            return Some(AppAction::OpenChatVideo(bvid.clone()));
                        }
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.chat_input_active {
                        self.chat_input.push('j');
                        return Some(AppAction::None);
                    }
                    if self.chat_selected + 1 < self.chat_messages.len() {
                        self.chat_selected += 1;
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.chat_input_active {
                        self.chat_input.push('k');
                        return Some(AppAction::None);
                    }
                    if self.chat_selected > 0 {
                        self.chat_selected -= 1;
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Backspace => {
                    if self.chat_input_active {
                        self.chat_input.pop();
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    // Open the most recent video-share message in this chat.
                    if let Some(bvid) = self
                        .chat_messages
                        .iter()
                        .rev()
                        .find_map(|m| m.bvid.clone())
                    {
                        return Some(AppAction::OpenChatVideo(bvid));
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Char(c) => {
                    if self.chat_input_active {
                        self.chat_input.push(c);
                    }
                    return Some(AppAction::None);
                }
                _ => return Some(AppAction::None),
            }
        }
        if keys.matches_quit(key) || keys.matches_back(key) {
            return Some(AppAction::BackToList);
        }
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }
        match key {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                return Some(AppAction::RefreshNotifications);
            }
            KeyCode::Char('1') => return Some(self.switch_tab(NotifTab::Chat)),
            KeyCode::Char('2') => return Some(self.switch_tab(NotifTab::At)),
            KeyCode::Char('3') => return Some(self.switch_tab(NotifTab::Like)),
            KeyCode::Char('4') => return Some(self.switch_tab(NotifTab::Sys)),
            KeyCode::Left => {
                let tabs = NotifTab::all();
                let cur = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
                let next = if cur == 0 { tabs.len() - 1 } else { cur - 1 };
                return Some(self.switch_tab(tabs[next]));
            }
            KeyCode::Right => {
                let tabs = NotifTab::all();
                let cur = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
                let next = (cur + 1) % tabs.len();
                return Some(self.switch_tab(tabs[next]));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.tab == NotifTab::Chat {
                    if self.session_selected + 1 < self.sessions.len() {
                        self.session_selected += 1;
                        if self.session_selected >= self.session_scroll + 8 {
                            self.session_scroll += 1;
                        }
                    }
                    return Some(AppAction::None);
                }
                if let Some(cur) = self.selected_row() {
                    if let Some(next) = self.next_item_row(cur) {
                        if let NotifRow::Item { item_idx } = self.rows[next] {
                            self.selected = item_idx;
                            // Scroll offset is adjusted during draw to keep
                            // the selected item fully visible.
                        }
                    }
                }
                return Some(AppAction::None);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.tab == NotifTab::Chat {
                    if self.session_selected > 0 {
                        self.session_selected -= 1;
                        if self.session_selected < self.session_scroll {
                            self.session_scroll = self.session_selected;
                        }
                    }
                    return Some(AppAction::None);
                }
                if let Some(cur) = self.selected_row() {
                    if let Some(prev) = self.prev_item_row(cur) {
                        if let NotifRow::Item { item_idx } = self.rows[prev] {
                            self.selected = item_idx;
                            // Scroll offset is adjusted during draw.
                        }
                    }
                }
                return Some(AppAction::None);
            }
            KeyCode::PageDown => {
                if self.tab == NotifTab::Chat {
                    self.session_selected =
                        (self.session_selected + 10).min(self.sessions.len().saturating_sub(1));
                    return Some(AppAction::None);
                }
                self.selected = (self.selected + 10).min(self.items.len().saturating_sub(1));
                return Some(AppAction::None);
            }
            KeyCode::PageUp => {
                if self.tab == NotifTab::Chat {
                    self.session_selected = self.session_selected.saturating_sub(10);
                    return Some(AppAction::None);
                }
                self.selected = self.selected.saturating_sub(10);
                return Some(AppAction::None);
            }
            KeyCode::Enter => {
                if self.tab == NotifTab::Chat {
                    if let Some(session) = self.sessions.get(self.session_selected) {
                        return Some(AppAction::OpenChat(session.talker_id));
                    }
                    return Some(AppAction::None);
                }
                if let Some(item) = self.items.get(self.selected) {
                    if let Some(bvid) = &item.bvid {
                        return Some(AppAction::OpenVideoDetail(bvid.clone(), item.oid.unwrap_or(0)));
                    }
                }
                return Some(AppAction::None);
            }
            _ => {}
        }
        None
    }

    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        let _ = (event, area);
        None
    }
}

impl NotificationsPage {
    /// Draw the chat detail view: header + message list + input box.
    fn draw_chat(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(5),    // messages
                Constraint::Length(3), // input / hint (constant height)
            ])
            .split(area);

        let name = self
            .chat_talker
            .as_ref()
            .map(|s| s.uname.clone())
            .unwrap_or_else(|| "聊天".to_string());
        // Header: avatar (if downloaded) + title
        let header_area = chunks[0];
        let talker_id = self.chat_talker.as_ref().map(|s| s.talker_id).unwrap_or(0);
        let avatar_idx = self
            .sessions
            .iter()
            .position(|s| s.talker_id == talker_id);
        let header_block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border_subtle));
        let header_inner = header_block.inner(header_area);
        frame.render_widget(header_block, header_area);
        // left: 8x2 avatar; right: title text
        let header_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(9), Constraint::Min(10)])
            .split(header_inner);
        let av_area = header_cols[0];
        let text_area = header_cols[1];
        if let Some(idx) = avatar_idx {
            if let Some(Some(protocol)) = self.avatar_protocols.get_mut(idx).map(|p| p.as_mut()) {
                let img = StatefulImage::new();
                let img_area = Rect::new(av_area.x, av_area.y, 7, 2);
                frame.render_stateful_widget(img, img_area, protocol);
            }
        }
        let has_video = self
            .chat_messages
            .iter()
            .any(|m| m.bvid.is_some());
        let title = if self.chat_input_active {
            format!("── {} ──  [Esc 取消输入 | Enter 发送]", name)
        } else if has_video {
            format!("── {} ──  [Esc 返回 | j/k 选择 | Enter 打开视频 | / 输入]", name)
        } else {
            format!("── {} ──  [Esc 返回 | j/k 选择 | / 输入]", name)
        };
        let header = Paragraph::new(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme.info)
                .add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(header, text_area);

        // Messages area: manual rows so video shares can render as 2-line
        // cards and the selected message can be highlighted.
        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_subtle));
        let list_inner = list_block.inner(chunks[1]);
        frame.render_widget(list_block, chunks[1]);

        if self.chat_loading && self.chat_messages.is_empty() {
            let msg = Paragraph::new(Line::from(Span::styled(
                "  ⏳ 加载中...",
                Style::default().fg(theme.fg_secondary),
            )));
            frame.render_widget(msg, list_inner);
            self.draw_chat_bottom(frame, chunks[2], theme);
            return;
        }
        if self.chat_messages.is_empty() {
            let msg = Paragraph::new(Line::from(Span::styled(
                "  (空) 暂无消息，说点什么吧",
                Style::default().fg(theme.fg_secondary),
            )));
            frame.render_widget(msg, list_inner);
            self.draw_chat_bottom(frame, chunks[2], theme);
            return;
        }

        // Row layout: video shares take 7 rows (cover + info), matching the
        // list cards used in video-detail collections. Ordinary messages
        // wrap when their content is wider than the message area, so their
        // height is dynamic: wrapped content rows + one timestamp row (when
        // the message has a time).  The row table is rebuilt every frame so
        // a terminal resize re-wraps automatically.
        let wrap_width = list_inner.width.saturating_sub(2).max(1);
        let msg_rows = |msg: &ChatMessage| -> usize {
            if msg.is_video_share() {
                7
            } else {
                let time_rows = if msg.format_time().is_empty() { 0 } else { 1 };
                let sender = if msg.sender_uid != talker_id { "我" } else { name.as_str() };
                let badge = msg.type_badge();
                let content: String = msg.content.chars().take(120).collect();
                let full = format!("{}: {}{}", sender, badge, content);
                wrapped_line_count(&full, wrap_width).max(1) + time_rows
            }
        };
        let mut row_of: Vec<usize> = Vec::with_capacity(self.chat_messages.len());
        let mut row = 0usize;
        for msg in &self.chat_messages {
            row_of.push(row);
            row += msg_rows(msg);
        }
        let height = list_inner.height as usize;
        // Bottom boundary of the message area. Rows at or beyond this y are
        // outside the list box (they would paint over the border and the
        // "按 / 输入消息" hint bar), so they must never be drawn.
        let bottom = list_inner.y + height as u16;
        let sel_row = row_of.get(self.chat_selected).copied().unwrap_or(0);
        // The selected message must be fully visible, not just its first
        // row: a 7-row video card scrolled to the bottom used to be cut to
        // a fragment (cover clamped, text rows skipped) so it rendered
        // smaller than the other cards. Count the rows the selected message
        // occupies and scroll so its LAST row fits.
        let sel_rows = self
            .chat_messages
            .get(self.chat_selected)
            .map(msg_rows)
            .unwrap_or(1);
        if sel_row < self.chat_scroll {
            self.chat_scroll = sel_row;
        }
        if sel_row + sel_rows > self.chat_scroll + height {
            self.chat_scroll = sel_row + sel_rows - height;
        }
        // Clamp the scroll so the last message can be fully visible too.
        // Without this, the final message (often a video card) stays cut by
        // the bottom edge no matter how far down you scroll.
        if let Some(&last_row) = row_of.last() {
            let last_rows = self
                .chat_messages
                .last()
                .map(msg_rows)
                .unwrap_or(1);
            let max_scroll = (last_row + last_rows).saturating_sub(height);
            if self.chat_scroll > max_scroll {
                self.chat_scroll = max_scroll;
            }
        }

        let scroll_i = self.chat_scroll as i64;
        for (i, msg) in self.chat_messages.iter().enumerate() {
            let msg_row = row_of[i];
            let rows_used = msg_rows(msg);
            // Signed visibility: a negative visible_start means the card is
            // partially scrolled off the top. saturating_sub used to turn
            // that into 0, which re-drew the whole card on top of the rows
            // below it (overlap bug).
            let visible_start = msg_row as i64 - scroll_i;
            if visible_start + rows_used as i64 <= 0 {
                continue;
            }
            if visible_start >= height as i64 {
                break;
            }
            // Rows clipped off the top of this card by scrolling.
            let clipped_top = (-visible_start).max(0) as u16;
            let visible_row = visible_start.max(0) as u16;
            if visible_row >= height as u16 {
                continue;
            }
            let is_me = msg.sender_uid != talker_id;
            let is_selected = i == self.chat_selected;
            // Ordinary messages and video-card text keep their background
            // transparent unless selected, so the terminal background and
            // cover images show through (only the selected card gets a
            // highlight background).
            let s_style = |s: Style| {
                if is_selected {
                    s.bg(theme.selection_bg)
                } else {
                    s
                }
            };
            let card_style = |s: Style| {
                if is_selected {
                    s.bg(theme.selection_bg)
                } else {
                    s
                }
            };
            let row_area = Rect::new(
                list_inner.x,
                list_inner.y + visible_row,
                list_inner.width,
                rows_used as u16,
            );
            // Rows clipped off the top of this card by scrolling.
            let clip = clipped_top;

            if msg.is_video_share() {
                // A partially-scrolled card renders only its visible rows;
                // text rows are offset by clipped_top. The cover image is
                // skipped when clipped (the image widget cannot crop from
                // the top, and re-drawing it overlaps the rows above).
                let clip = clipped_top;
                // Card: left cover (7 rows), right title + stats + desc,
                // sized like the list cards in video-detail collections.
                let title = msg
                    .video_title
                    .as_deref()
                    .unwrap_or("视频分享")
                    .chars()
                    .take(50)
                    .collect::<String>();
                let mut stats = Vec::new();
                if let Some(view) = msg.video_view {
                    stats.push(format!("播放 {}", format_count(view)));
                }
                if let Some(dm) = msg.video_danmaku {
                    stats.push(format!("弹幕 {}", format_count(dm)));
                }
                let time = msg.format_time();
                if !time.is_empty() {
                    stats.push(time);
                }
                // Left: cover image or placeholder (24 cols wide like the
                // collection list cards). Skipped when the card is clipped.
                if clip == 0 {
                    let cover_h = 7u16.min(bottom.saturating_sub(row_area.y));
                    let cover_area = Rect::new(row_area.x, row_area.y, 24, cover_h);
                    if let Some(ref mut protocol) = self.cover_protocols[i] {
                        let image = StatefulImage::new();
                        frame.render_stateful_widget(image, cover_area, protocol);
                    } else if self.pending_covers.contains(&i) {
                        let ph = Paragraph::new(Line::from(Span::styled(
                            "◌ 封面加载中",
                            card_style(Style::default().fg(theme.fg_muted)),
                        )));
                        frame.render_widget(ph, cover_area);
                    } else {
                        let ph = Paragraph::new(Line::from(Span::styled(
                            "▶ 视频",
                            card_style(Style::default().fg(theme.fg_muted)),
                        )));
                        frame.render_widget(ph, cover_area);
                    }
                }
                // Right: title (2 rows), meta (1 row), desc (up to 3 rows).
                let info_x = row_area.x + 26;
                let info_w = row_area.width.saturating_sub(27).max(1);
                // Title occupies card rows 0..2.
                if clip <= 1 {
                    let title_line = Line::from(vec![
                        Span::styled(
                            if is_selected { "▶ " } else { "  " },
                            card_style(Style::default().fg(if is_selected {
                                theme.bilibili_pink
                            } else {
                                theme.info
                            })),
                        ),
                        Span::styled(
                            format!("[视频] {}", title),
                            card_style(
                                Style::default()
                                    .fg(theme.fg_primary)
                                    .add_modifier(if is_selected {
                                        Modifier::BOLD
                                    } else {
                                        Modifier::empty()
                                    }),
                            ),
                        ),
                    ]);
                    let ty = row_area.y + clip;
                    if ty < bottom {
                        let title_h = (2u16 - clip).min(bottom - ty);
                        frame.render_widget(
                            Paragraph::new(title_line).wrap(Wrap { trim: true }),
                            Rect::new(info_x, ty, info_w, title_h),
                        );
                    }
                }
                // Meta occupies card row 2.
                if clip <= 2 {
                    let meta_line = Line::from(Span::styled(
                        stats.join(" · "),
                        card_style(Style::default().fg(theme.fg_secondary)),
                    ));
                    let my = row_area.y + 2 - clip;
                    if my < bottom {
                        frame.render_widget(
                            Paragraph::new(meta_line),
                            Rect::new(info_x, my, info_w, 1),
                        );
                    }
                }
                // Description occupies card rows 3..6.
                if clip <= 6 {
                    if let Some(desc) = msg.video_desc.as_deref() {
                        let desc_text: String = desc.chars().take(120).collect();
                        let per_line = (info_w as usize).max(10);
                        for r in 0..3u16 {
                            let start = r as usize * per_line;
                            let chunk: String = desc_text
                                .chars()
                                .skip(start)
                                .take(per_line)
                                .collect();
                            let dy = row_area.y + 3 + r - clip;
                            if !chunk.is_empty() && dy < bottom {
                                let desc_line = Line::from(Span::styled(
                                    chunk,
                                    card_style(Style::default().fg(theme.fg_muted)),
                                ));
                                frame.render_widget(
                                    Paragraph::new(desc_line),
                                    Rect::new(info_x, dy, info_w, 1),
                                );
                            }
                        }
                    }
                }
                continue;
            }

            // Ordinary message: `sender: content`, wrapping over as many
            // rows as the content needs. The timestamp sits one row below
            // the last wrapped content row.
            let sender = if is_me { "我" } else { name.as_str() };
            let badge = msg.type_badge();
            let content: String = msg.content.chars().take(120).collect();
            let time = msg.format_time();
            let has_time = !time.is_empty();
            let content_rows = rows_used.saturating_sub(if has_time { 1 } else { 0 });
            // Content occupies card rows 0..content_rows (wrapped).
            if (clip as usize) < content_rows {
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}: ", sender),
                        s_style(
                            Style::default()
                                .fg(if is_me { theme.success } else { theme.info })
                                .add_modifier(Modifier::BOLD),
                        ),
                    ),
                    Span::styled(
                        badge,
                        s_style(
                            Style::default()
                                .fg(theme.bilibili_pink)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ),
                    Span::styled(content, s_style(Style::default().fg(theme.fg_primary))),
                ]);
                let cy = row_area.y + clip;
                if cy < bottom {
                    let ch = (content_rows - clip as usize).min((bottom - cy) as usize) as u16;
                    frame.render_widget(
                        Paragraph::new(line).wrap(Wrap { trim: true }),
                        Rect::new(row_area.x, cy, row_area.width, ch),
                    );
                }
            }
            // Timestamp occupies the row right after the wrapped content.
            if has_time && (clip as usize) <= content_rows {
                let time_line = Line::from(Span::styled(
                    format!("    {}", time),
                    s_style(Style::default().fg(theme.fg_secondary)),
                ));
                let ty = row_area.y + content_rows as u16 - clip;
                if ty < bottom {
                    frame.render_widget(
                        Paragraph::new(time_line),
                        Rect::new(row_area.x, ty, row_area.width, 1),
                    );
                }
            }
        }

        self.draw_chat_bottom(frame, chunks[2], theme);
    }

    /// Draw either the input box (input mode) or the hint bar (non-input
    /// mode) at the bottom of the chat view.
    fn draw_chat_bottom(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.chat_input_active {
            self.draw_chat_input(frame, area, theme);
        } else {
            let hint_para = Paragraph::new(Line::from(Span::styled(
                "按 / 输入消息",
                Style::default().fg(theme.fg_secondary),
            )))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_subtle)),
            );
            frame.render_widget(hint_para, area);
        }
    }

    /// Draw the chat input box (shared by loading/empty/content paths).
    fn draw_chat_input(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block_title = if self.chat_sending {
            "发送中...".to_string()
        } else {
            self.message
                .clone()
                .unwrap_or_else(|| "输入消息，Enter 发送".to_string())
        };
        let mut input_spans = vec![Span::styled(
            format!("> {}", self.chat_input),
            Style::default().fg(theme.fg_primary),
        )];
        if !self.chat_sending {
            input_spans.push(Span::styled("▌", Style::default().fg(theme.fg_secondary)));
        }
        let input_para = Paragraph::new(Line::from(input_spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_subtle))
                .title(block_title),
        );
        frame.render_widget(input_para, area);
    }
}

/// Format a view/danmaku count with 万/亿 units (e.g. 674306 -> 67.4万).
fn format_count(count: i64) -> String {
    if count >= 100_000_000 {
        format!("{:.1}亿", count as f64 / 100_000_000.0)
    } else if count >= 10_000 {
        format!("{:.1}万", count as f64 / 10_000.0)
    } else {
        count.to_string()
    }
}

/// Download a user avatar, crop it to a centered square and build a
/// terminal-graphics protocol. Returns `None` on any failure (the caller
/// falls back to a placeholder).
async fn download_avatar(url: &str, picker: &Arc<Picker>) -> Option<StatefulProtocol> {
    let response = reqwest::get(url).await.ok()?;
    let bytes = response.bytes().await.ok()?;
    let mut img: DynamicImage = image::load_from_memory(&bytes).ok()?;
    let side = img.width().min(img.height());
    let x = (img.width() - side) / 2;
    let y = (img.height() - side) / 2;
    img = img.crop_imm(x, y, side, side);
    img = img.resize(192, 192, image::imageops::FilterType::Triangle);
    Some(picker.new_resize_protocol(img))
}

/// Download a video-share cover, keeping the original aspect ratio but
/// capping the longest side so the terminal protocol stays small.
async fn download_cover(url: &str, picker: &Arc<Picker>) -> Option<StatefulProtocol> {
    let response = reqwest::get(url).await.ok()?;
    let bytes = response.bytes().await.ok()?;
    let img: DynamicImage = image::load_from_memory(&bytes).ok()?;
    let max_side = 256u32;
    let (w, h) = (img.width(), img.height());
    let scale = max_side as f32 / w.max(h).max(1) as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    let img = img.resize(nw, nh, image::imageops::FilterType::Triangle);
    Some(picker.new_resize_protocol(img))
}
