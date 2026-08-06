//! Left sidebar navigation component

use super::Theme;
use ratatui::{prelude::*, widgets::*};

/// Navigation menu items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Home,
    Search,
    Sections,
    Dynamic,
    History,
    Favorites,
    Live,
    Bangumi,
    Notifications,
    Settings,
}
impl NavItem {
    pub fn label(&self) -> &'static str {
        match self {
            NavItem::Home => "🏠 首页",
            NavItem::Search => "🔍 搜索",
            NavItem::Sections => "📊 分区",
            NavItem::Dynamic => "📺 动态",
            NavItem::History => "📜 历史",
            NavItem::Favorites => "⭐ 收藏夹",
            NavItem::Live => "📡 直播",
            NavItem::Bangumi => "🎬 番剧",
            NavItem::Notifications => "🔔 消息",
            NavItem::Settings => "⚙️ 设置",
        }
    }

    pub fn all() -> &'static [NavItem] {
        &[
            NavItem::Home,
            NavItem::Sections,
            NavItem::Dynamic,
            NavItem::History,
            NavItem::Favorites,
            NavItem::Live,
            NavItem::Bangumi,
            NavItem::Notifications,
            NavItem::Settings,
        ]
    }
}

pub struct Sidebar {
    pub selected: NavItem,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            selected: NavItem::Home,
        }
    }

    pub fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        user: Option<(&crate::api::auth::CurrentUser, &mut Option<ratatui_image::protocol::StatefulProtocol>)>,
    ) {
        // Main block with subtle right border
        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(theme.border_subtle));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into header (brand + user info), separator, and nav items
        let header_h = if user.is_some() { 9 } else { 4 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_h), // Header with branding + user info
                Constraint::Length(1),        // Separator
                Constraint::Min(5),           // Nav items
                Constraint::Length(1),        // Footer separator
            ])
            .split(inner);

        // Inner header split: brand (4) / divider (1) / user info (4)
        let header_chunks = if user.is_some() {
            let hc = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4), // Brand
                    Constraint::Length(1), // Divider between logo and user info
                    Constraint::Length(4), // User info (avatar + name/level + exp bar)
                ])
                .split(chunks[0]);
            Some(hc)
        } else {
            None
        };
        let brand_area = header_chunks
            .as_ref()
            .map(|c| c[0])
            .unwrap_or(chunks[0]);

        // Bilibili branding header with modern styling
        let brand_lines = vec![
            Line::raw(""),
            Line::from(vec![
                Span::styled(
                    "  ▌",
                    Style::default()
                        .fg(theme.bilibili_pink)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "B",
                    Style::default()
                        .fg(theme.bilibili_pink)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "ilibili",
                    Style::default()
                        .fg(theme.fg_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![Span::styled(
                "   TUI Client",
                Style::default()
                    .fg(theme.fg_muted)
                    .add_modifier(Modifier::ITALIC),
            )]),
        ];
        let brand = Paragraph::new(brand_lines);
        frame.render_widget(brand, brand_area);

        // Divider line between the logo and the user info
        if let Some(hc) = &header_chunks {
            let divider = Paragraph::new(Line::from(vec![Span::styled(
                "─".repeat(area.width.saturating_sub(2) as usize),
                Style::default().fg(theme.border_subtle),
            )]));
            frame.render_widget(divider, hc[1]);
        }

        // User info (avatar, name, level) below the brand when logged in
        if let Some((user, avatar)) = user {
            let area = header_chunks
                .as_ref()
                .map(|c| c[2])
                .unwrap_or(chunks[0]);
            let lines = user_lines(theme, user);
            if let Some(protocol) = avatar.as_mut() {
                // 5-column avatar on the left
                let avatar_area = Rect {
                    x: area.x + 1,
                    y: area.y,
                    width: 5,
                    height: 3,
                };
                use ratatui_image::StatefulImage;
                let image = StatefulImage::new();
                frame.render_stateful_widget(image, avatar_area, protocol);
                // Text to the right of the avatar
                let text_area = Rect {
                    x: area.x + 7,
                    y: area.y,
                    width: area.width.saturating_sub(8),
                    height: 2,
                };
                let text = Paragraph::new(lines).style(Style::default());
                frame.render_widget(text, text_area);
            } else {
                let text = Paragraph::new(lines).style(Style::default());
                frame.render_widget(text, area);
            }

            // Exp progress bar below the avatar
            let exp_area = Rect {
                x: area.x + 1,
                y: area.y + 3,
                width: area.width.saturating_sub(2),
                height: 1,
            };
            frame.render_widget(exp_bar(theme, user), exp_area);
        }

        // Separator line with gradient effect
        let separator = Paragraph::new(Line::from(vec![Span::styled(
            "─".repeat(area.width.saturating_sub(2) as usize),
            Style::default().fg(theme.border_subtle),
        )]));
        frame.render_widget(separator, chunks[1]);

        // Nav items with modern block selection indicator
        let items: Vec<ListItem> = NavItem::all()
            .iter()
            .map(|item| {
                let is_selected = *item == self.selected;
                let style = if is_selected {
                    Style::default()
                        .fg(theme.bilibili_pink)
                        .add_modifier(Modifier::BOLD)
                        .bg(theme.bg_highlight)
                } else {
                    Style::default().fg(theme.fg_secondary)
                };

                // Use block indicator for selection instead of arrow
                let prefix = if is_selected { " ▌" } else { "  " };
                let suffix = if is_selected { " " } else { "" };
                ListItem::new(format!("{}{}{}", prefix, item.label(), suffix)).style(style)
            })
            .collect();

        let list = List::new(items).highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(list, chunks[2]);
    }

    pub fn next(&mut self) {
        let items = NavItem::all();
        let current_idx = items.iter().position(|i| *i == self.selected).unwrap_or(0);
        let next_idx = (current_idx + 1) % items.len();
        self.selected = items[next_idx];
    }

    pub fn prev(&mut self) {
        let items = NavItem::all();
        let current_idx = items.iter().position(|i| *i == self.selected).unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            items.len() - 1
        } else {
            current_idx - 1
        };
        self.selected = items[prev_idx];
    }

    pub fn select(&mut self, item: NavItem) {
        self.selected = item;
    }
}

/// Build the sidebar user-info text lines (name + level).
fn user_lines(
    theme: &Theme,
    user: &crate::api::auth::CurrentUser,
) -> Vec<Line<'static>> {
    let name = if user.uname.is_empty() {
        format!("用户{}", user.mid)
    } else {
        user.uname.clone()
    };
    vec![
        Line::from(vec![Span::styled(
            name,
            Style::default()
                .fg(theme.bilibili_pink)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            format!("Lv.{}", user.level),
            Style::default().fg(theme.fg_muted),
        )]),
    ]
}

/// Build the exp progress bar line for the sidebar user card.
fn exp_bar(theme: &Theme, user: &crate::api::auth::CurrentUser) -> Paragraph<'static> {
    let total = (user.next_exp - user.current_min).max(1);
    let cur = (user.current_exp - user.current_min).clamp(0, total);
    let pct = cur as f64 / total as f64;
    let width = 16usize;
    let filled = ((pct * width as f64).round() as usize).min(width);
    let mut spans = Vec::new();
    spans.push(Span::styled(
        "经验 ",
        Style::default()
            .fg(theme.bilibili_pink)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        "█".repeat(filled),
        Style::default().fg(theme.bilibili_pink),
    ));
    spans.push(Span::styled(
        "░".repeat(width - filled),
        Style::default().fg(theme.fg_muted),
    ));
    spans.push(Span::styled(
        format!(" {}%", (pct * 100.0).round() as i64),
        Style::default().fg(theme.fg_muted),
    ));
    Paragraph::new(Line::from(spans))
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
