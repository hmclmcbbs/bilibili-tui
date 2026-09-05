//! Left sidebar navigation component

use super::Theme;
use ratatui::{prelude::*, widgets::*};

/// Navigation menu items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Home,
    Search,
    /// Collapsible group header for the Bilibili "hot" feeds. It does not
    /// open a page itself; selecting it expands/collapses the group below.
    HotGroup,
    /// 综合热门
    Popular,
    /// 每周必看
    Weekly,
    /// 排行榜
    Ranking,
    /// 入站必刷
    MustWatch,
    Sections,
    Dynamic,
    History,
    Favorites,
    Downloads,
    Live,
    Bangumi,
    Notifications,
    Mall,
    Settings,
}
impl NavItem {
    pub fn label(&self) -> &'static str {
        match self {
            NavItem::Home => "🏠 首页",
            NavItem::Search => "🔍 搜索",
            NavItem::HotGroup => "🔥 热门",
            NavItem::Popular => "综合热门",
            NavItem::Weekly => "每周必看",
            NavItem::Ranking => "排行榜",
            NavItem::MustWatch => "入站必刷",
            NavItem::Sections => "📊 分区",
            NavItem::Dynamic => "📺 动态",
            NavItem::History => "📜 历史",
            NavItem::Favorites => "⭐ 收藏夹",
            NavItem::Downloads => "⬇️ 下载管理",
            NavItem::Live => "📡 直播",
            NavItem::Bangumi => "🎬 番剧",
            NavItem::Notifications => "🔔 消息",
            NavItem::Mall => "🛍️ 会员购",
            NavItem::Settings => "⚙️ 设置",
        }
    }

    pub fn all() -> &'static [NavItem] {
        &[
            NavItem::Home,
            NavItem::Search,
            NavItem::HotGroup,
            NavItem::Sections,
            NavItem::Dynamic,
            NavItem::History,
            NavItem::Favorites,
            NavItem::Downloads,
            NavItem::Live,
            NavItem::Bangumi,
            NavItem::Notifications,
            NavItem::Mall,
            NavItem::Settings,
        ]
    }

    /// The home feed this sidebar item opens, if any.
    pub fn home_feed(self) -> Option<crate::api::recommend::HomeFeed> {
        use crate::api::recommend::HomeFeed;
        match self {
            NavItem::Popular => Some(HomeFeed::Popular),
            NavItem::Weekly => Some(HomeFeed::Weekly),
            NavItem::Ranking => Some(HomeFeed::Ranking),
            NavItem::MustWatch => Some(HomeFeed::MustWatch),
            _ => None,
        }
    }

    /// True for the four items nested inside the 热门 group.
    pub fn is_hot_feed(self) -> bool {
        matches!(
            self,
            NavItem::Popular | NavItem::Weekly | NavItem::Ranking | NavItem::MustWatch
        )
    }
}

pub struct Sidebar {
    pub selected: NavItem,
    /// Whether the 热门 group is expanded. Defaults to expanded so all the
    /// special feeds are visible; clicking the group header collapses it.
    pub hot_expanded: bool,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            selected: NavItem::Home,
            hot_expanded: true,
        }
    }

    /// The navigation rows currently visible, in render/navigation order.
    /// The 热门 group header is kept as a clickable row; keyboard Tab skips
    /// it and expands the group automatically when it is encountered while
    /// collapsed.
    pub fn visible_items(&self) -> Vec<NavItem> {
        let mut items = vec![NavItem::Home, NavItem::Search];
        if self.hot_expanded {
            items.push(NavItem::HotGroup);
            items.push(NavItem::Popular);
            items.push(NavItem::Weekly);
            items.push(NavItem::Ranking);
            items.push(NavItem::MustWatch);
        } else {
            items.push(NavItem::HotGroup);
        }
        items.extend([
            NavItem::Sections,
            NavItem::Dynamic,
            NavItem::History,
            NavItem::Favorites,
            NavItem::Downloads,
            NavItem::Live,
            NavItem::Bangumi,
            NavItem::Notifications,
            NavItem::Mall,
            NavItem::Settings,
        ]);
        items
    }

    /// Scroll offset that keeps the currently selected row visible inside a
    /// nav area of `visible_rows` rows. Pure function of the current state so
    /// both drawing and mouse hit-testing agree without storing extra state.
    pub fn scroll_for_selected(&self, visible_rows: usize) -> usize {
        let items = self.visible_items();
        if items.is_empty() || visible_rows == 0 {
            return 0;
        }
        let selected = items
            .iter()
            .position(|item| *item == self.selected)
            .unwrap_or(0);
        let max_scroll = items.len().saturating_sub(visible_rows);
        // Keep the selection near the middle of the visible window.
        let half = visible_rows / 2;
        selected.saturating_sub(half).min(max_scroll)
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

        // Nav items with modern block selection indicator. The 热门 group
        // header is a section title (click to expand/collapse), while the four
        // nested feeds are indented rows under it. When the sidebar is shorter
        // than the full item list, render only the window around the current
        // selection so every item (including 每周必看/排行榜/入站必刷) can be
        // reached by keyboard/scroll.
        let all_items = self.visible_items();
        let nav_rows = chunks[2].height.max(1) as usize;
        let scroll = self.scroll_for_selected(nav_rows);
        let visible_end = (scroll + nav_rows).min(all_items.len());
        let items: Vec<ListItem> = all_items[scroll..visible_end]
            .iter()
            .map(|item| {
                let is_selected = *item == self.selected;
                let style = if *item == NavItem::HotGroup {
                    Style::default()
                        .fg(theme.fg_muted)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default()
                        .fg(theme.bilibili_pink)
                        .add_modifier(Modifier::BOLD)
                        .bg(theme.bg_highlight)
                } else {
                    Style::default().fg(theme.fg_secondary)
                };

                let arrow = if *item == NavItem::HotGroup {
                    if self.hot_expanded { "▾" } else { "▸" }
                } else {
                    " "
                };
                let indent = if item.is_hot_feed() { "  " } else { "" };
                let prefix = if is_selected { " ▌" } else { "  " };
                let suffix = if is_selected { " " } else { "" };
                let label = if *item == NavItem::HotGroup {
                    format!("🔥 热门 {arrow}")
                } else {
                    format!("{indent}{}{}", item.label(), arrow)
                };
                ListItem::new(format!("{prefix}{label}{suffix}")).style(style)
            })
            .collect();

        let list = List::new(items).highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(list, chunks[2]);
    }

    pub fn next(&mut self) {
        // If the cursor is sitting on the collapsed group header, Tab expands
        // the group and lands on the first feed instead of moving away.
        if self.selected == NavItem::HotGroup && !self.hot_expanded {
            self.hot_expanded = true;
            self.selected = NavItem::Popular;
            return;
        }
        let items = self.visible_items();
        let current_idx = items.iter().position(|i| *i == self.selected).unwrap_or(0);
        for step in 1..=items.len() {
            let idx = (current_idx + step) % items.len();
            let item = items[idx];
            if item == NavItem::HotGroup {
                // Keyboard never stops on the group header. When it is
                // collapsed, reaching it expands the group into its feeds.
                if !self.hot_expanded {
                    self.hot_expanded = true;
                    self.selected = NavItem::Popular;
                    return;
                }
                continue;
            }
            self.selected = item;
            return;
        }
    }

    pub fn prev(&mut self) {
        if self.selected == NavItem::HotGroup && !self.hot_expanded {
            self.hot_expanded = true;
            self.selected = NavItem::MustWatch;
            return;
        }
        let items = self.visible_items();
        let current_idx = items.iter().position(|i| *i == self.selected).unwrap_or(0);
        for step in 1..=items.len() {
            let idx = if current_idx >= step {
                current_idx - step
            } else {
                items.len() - (step - current_idx)
            };
            let item = items[idx];
            if item == NavItem::HotGroup {
                if !self.hot_expanded {
                    self.hot_expanded = true;
                    self.selected = NavItem::MustWatch;
                    return;
                }
                continue;
            }
            self.selected = item;
            return;
        }
    }

    /// Move the highlight one row down inside the *visible* list, skipping
    /// the 热门 group header without expanding it. Used by the sidebar
    /// hjkl mode, where j/k only move the cursor and Enter opens the page.
    pub fn next_highlight(&mut self) {
        let items = self.visible_items();
        if items.is_empty() {
            return;
        }
        let current = items
            .iter()
            .position(|item| *item == self.selected)
            .unwrap_or(0);
        for step in 1..=items.len() {
            let item = items[(current + step) % items.len()];
            if item != NavItem::HotGroup {
                self.selected = item;
                return;
            }
        }
    }

    /// Move the highlight one row up inside the *visible* list, skipping the
    /// 热门 group header without expanding it.
    pub fn prev_highlight(&mut self) {
        let items = self.visible_items();
        if items.is_empty() {
            return;
        }
        let current = items
            .iter()
            .position(|item| *item == self.selected)
            .unwrap_or(0);
        for step in 1..=items.len() {
            let idx = if current >= step {
                current - step
            } else {
                items.len() - (step - current)
            };
            let item = items[idx];
            if item != NavItem::HotGroup {
                self.selected = item;
                return;
            }
        }
    }

    pub fn select(&mut self, item: NavItem) {
        if item == NavItem::HotGroup {
            self.hot_expanded = !self.hot_expanded;
            self.selected = if self.hot_expanded {
                NavItem::Popular
            } else {
                NavItem::HotGroup
            };
            return;
        }
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
    let is_vip = user.vip_status == 1 && user.vip_type > 0;
    // B 站网页版大会员的等级数字是粉色高亮，旁边还有粉色徽章；
    // 非大会员等级是灰色。
    let level_style = if is_vip {
        Style::default()
            .fg(theme.bilibili_pink)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg_muted)
    };
    // B 站网页版：有大会员时等级和徽章粉色高亮，非大会员灰色显示徽章。
    let vip_style = if is_vip {
        Style::default()
            .fg(theme.bilibili_pink)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg_muted)
    };
    let mut level_spans = vec![Span::styled(format!("Lv.{}", user.level), level_style)];
    level_spans.push(Span::styled(" ✦大会员", vip_style));
    vec![
        Line::from(vec![Span::styled(
            name,
            Style::default()
                .fg(theme.bilibili_pink)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(level_spans),
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
    Paragraph::new(Line::from(spans))
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_group_is_expanded_by_default_and_contains_feeds() {
        let sidebar = Sidebar::new();
        assert!(sidebar.hot_expanded);
        let items = sidebar.visible_items();
        assert!(items.contains(&NavItem::Popular));
        assert!(items.contains(&NavItem::Weekly));
        assert!(items.contains(&NavItem::Ranking));
        assert!(items.contains(&NavItem::MustWatch));
    }

    #[test]
    fn keyboard_next_skips_group_header() {
        let mut sidebar = Sidebar::new();
        sidebar.selected = NavItem::Search;
        sidebar.next();
        assert_eq!(sidebar.selected, NavItem::Popular);
    }

    #[test]
    fn keyboard_prev_from_first_feed_goes_back_to_search() {
        let mut sidebar = Sidebar::new();
        sidebar.selected = NavItem::Popular;
        sidebar.prev();
        assert_eq!(sidebar.selected, NavItem::Search);
    }

    #[test]
    fn collapsing_hides_feeds_and_next_expands_into_first_feed() {
        let mut sidebar = Sidebar::new();
        sidebar.hot_expanded = false;
        sidebar.selected = NavItem::HotGroup;
        assert!(!sidebar.visible_items().contains(&NavItem::Popular));

        sidebar.next();
        assert!(sidebar.hot_expanded);
        assert_eq!(sidebar.selected, NavItem::Popular);
    }

    #[test]
    fn select_toggles_group() {
        let mut sidebar = Sidebar::new();
        sidebar.select(NavItem::HotGroup);
        assert!(!sidebar.hot_expanded);
        assert_eq!(sidebar.selected, NavItem::HotGroup);

        sidebar.select(NavItem::HotGroup);
        assert!(sidebar.hot_expanded);
        assert_eq!(sidebar.selected, NavItem::Popular);
    }
}
