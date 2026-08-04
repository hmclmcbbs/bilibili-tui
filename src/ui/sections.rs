//! Section (分区) browsing page: a standalone sidebar page that lists
//! Bilibili ranking sections on the left and their top videos on the right.

use super::video_card::{VideoCard, VideoCardGrid};
use super::{Component, Theme, shortcut_footer};
use crate::api::recommend::VideoItem;
use crate::application::AppAction;
use crate::storage::Keybindings;
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

/// Common Bilibili ranking sections: (rid, label).
pub const SECTION_LIST: &[(i64, &str)] = &[
    (0, "全站"),
    (1, "动画"),
    (3, "音乐"),
    (4, "游戏"),
    (5, "娱乐"),
    (36, "知识"),
    (160, "生活"),
    (119, "鬼畜"),
    (155, "时尚"),
    (181, "影视"),
    (188, "科技"),
    (234, "舞蹈"),
];

pub struct SectionPage {
    /// Current selected section index into SECTION_LIST.
    pub selected: usize,
    /// Videos of the current section (loaded).
    pub videos: VideoCardGrid,
    /// Whether the left section list has focus (true) or the video grid (false).
    pub focus_sections: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub loaded_rid: Option<i64>,
}

impl SectionPage {
    pub fn new() -> Self {
        Self {
            selected: 0,
            videos: VideoCardGrid::new(),
            focus_sections: true,
            loading: true,
            error: None,
            loaded_rid: None,
        }
    }

    pub fn selected_rid(&self) -> i64 {
        SECTION_LIST
            .get(self.selected)
            .map(|(rid, _)| *rid)
            .unwrap_or(0)
    }

    pub fn section_label(&self) -> &'static str {
        SECTION_LIST
            .get(self.selected)
            .map(|(_, label)| *label)
            .unwrap_or("全站")
    }

    pub fn begin_load(&mut self) {
        self.loading = true;
        self.error = None;
    }

    pub fn apply_videos(&mut self, rid: i64, videos: Vec<VideoItem>) {
        self.videos.clear();
        for video in videos {
            let card = VideoCard::new(
                video.bvid.clone(),
                Some(video.id),
                video
                    .title
                    .clone()
                    .unwrap_or_else(|| "(无标题)".to_string()),
                video
                    .owner
                    .as_ref()
                    .map(|o| o.name.clone())
                    .unwrap_or_default(),
                format_play(video.stat.as_ref().and_then(|s| s.view)),
                format_duration(video.duration),
                video.pic.clone(),
            )
            .with_uploader_mid(video.owner.as_ref().map(|o| o.mid));
            self.videos.add_card(card);
        }
        self.loaded_rid = Some(rid);
        self.loading = false;
        self.error = None;
    }

    pub fn apply_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    fn draw_section_list(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = SECTION_LIST
            .iter()
            .enumerate()
            .map(|(idx, (_, label))| {
                let selected = idx == self.selected;
                let style = if selected {
                    Style::default()
                        .fg(theme.bilibili_pink)
                        .add_modifier(Modifier::BOLD)
                        .bg(theme.bg_highlight)
                } else {
                    Style::default().fg(theme.fg_secondary)
                };
                let prefix = if selected { " ▌" } else { "  " };
                ListItem::new(format!("{prefix}{label}")).style(style)
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(Span::styled(
                " 分区 ",
                Style::default().fg(theme.fg_accent).add_modifier(Modifier::BOLD),
            ));
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        frame.render_widget(list, area);
    }

    fn draw_videos(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(Span::styled(
                format!(" {} 排行榜 ", self.section_label()),
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.loading {
            frame.render_widget(
                Paragraph::new("⏳ 加载中…")
                    .style(Style::default().fg(theme.warning))
                    .alignment(Alignment::Center),
                inner,
            );
        } else if let Some(error) = &self.error {
            frame.render_widget(
                Paragraph::new(format!("❌ {error}"))
                    .style(Style::default().fg(theme.error))
                    .alignment(Alignment::Center),
                inner,
            );
        } else if self.videos.cards.is_empty() {
            frame.render_widget(
                Paragraph::new("📭 暂无视频")
                    .style(Style::default().fg(theme.fg_secondary))
                    .alignment(Alignment::Center),
                inner,
            );
        } else {
            self.videos.render(frame, inner, theme);
        }
    }
}

impl Default for SectionPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for SectionPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(8),
                Constraint::Length(2),
            ])
            .split(area);

        // Header line
        let header = Line::from(vec![
            Span::styled(" 分区 ", Style::default().fg(theme.fg_accent)),
            Span::styled(
                format!("[{}]", self.section_label()),
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        let content = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(14), Constraint::Min(26)])
            .split(chunks[1]);
        self.draw_section_list(frame, content[0], theme);
        self.draw_videos(frame, content[1], theme);

        frame.render_widget(
            Paragraph::new(shortcut_footer(
                theme,
                [
                    (
                        format!("{}/{}", keys.nav_up, keys.nav_down),
                        "选择".into(),
                        theme.fg_accent,
                    ),
                    (keys.confirm.clone(), "打开/切换".into(), theme.success),
                    (keys.back.clone(), "返回".into(), theme.info),
                    (keys.refresh.clone(), "刷新".into(), theme.info),
                ],
            ))
            .alignment(Alignment::Center),
            chunks[2],
        );
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        // Tab navigation always wins, including while either pane is loading.
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }
        if self.focus_sections {
            match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                    return Some(AppAction::None);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected + 1 < SECTION_LIST.len() {
                        self.selected += 1;
                    }
                    return Some(AppAction::None);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.focus_sections = false;
                    return Some(AppAction::None);
                }
                _ => {}
            }
            if keys.matches_confirm(key) || keys.matches_play(key) {
                return Some(AppAction::SelectSection(self.selected_rid()));
            }
            return None;
        }

        // Video grid focus
        match key {
            KeyCode::Left | KeyCode::Char('h') => {
                self.focus_sections = true;
                return Some(AppAction::None);
            }
            _ => {}
        }
        if keys.matches_confirm(key) || keys.matches_play(key) {
            let Some(card) = self.videos.selected_card() else {
                return Some(AppAction::None);
            };
            let Some(bvid) = card.bvid.clone() else {
                return Some(AppAction::None);
            };
            let aid = card.aid.unwrap_or(0);
            return Some(AppAction::OpenVideoDetail(bvid, aid));
        }
        if keys.matches_up(key) {
            self.videos.move_up();
            return Some(AppAction::None);
        }
        if keys.matches_down(key) {
            self.videos.move_down();
            return Some(AppAction::None);
        }
        if keys.matches_page_up(key) {
            self.videos.move_page_up();
            return Some(AppAction::None);
        }
        if keys.matches_page_down(key) {
            self.videos.move_page_down();
            return Some(AppAction::None);
        }
        if keys.matches_back(key) {
            self.focus_sections = true;
            return Some(AppAction::None);
        }
        if keys.matches_refresh(key) {
            return Some(AppAction::SelectSection(self.selected_rid()));
        }
        None
    }
}

fn format_play(value: Option<i64>) -> String {
    match value {
        Some(v) if v >= 10_000 => format!("{:.1}万", v as f64 / 10_000.0),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

fn format_duration(seconds: Option<i64>) -> String {
    match seconds {
        Some(s) => format!("{:02}:{:02}", s / 60, s % 60),
        None => String::new(),
    }
}
