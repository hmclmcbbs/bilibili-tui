//! 下载管理页面（侧边栏入口）。
//!
//! 显示当前下载批次的实时进度，以及已下载到本地
//! `~/bilibili-tui-downloads` 的 mp4 文件。
//!
//! 布局与 UP 主页「合集」一致（封面卡片网格），选中后按确认键直接用 mpv
//! 播放本地文件；`/` 进入本地文件名过滤；`Tab`/`Shift+Tab` 切换侧边栏。

use super::{Component, Theme, shortcut_footer};
use crate::application::AppAction;
use crate::infrastructure::download::{current_status, download_dir};
use crate::storage::Keybindings;
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::*,
};
use ratatui_image::{StatefulImage, picker::Picker, protocol::StatefulProtocol};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// A completed mp4 file in the download directory.
#[derive(Clone)]
pub struct DownloadedFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub struct DownloadsPage {
    /// All files currently on disk (before filtering).
    all_files: Vec<DownloadedFile>,
    /// Files shown after applying `filter` (if any).
    files: Vec<DownloadedFile>,
    selected: usize,
    scroll_row: usize,
    columns: usize,
    visible_rows: usize,
    /// Local cover images (aligned with `files`), loaded from `<name>.jpg`.
    covers: Vec<Option<StatefulProtocol>>,
    picker: Arc<Picker>,
    filter: String,
    input_mode: bool,
    message: Option<String>,
}

impl DownloadsPage {
    pub fn new() -> Self {
        let mut page = Self {
            all_files: Vec::new(),
            files: Vec::new(),
            selected: 0,
            scroll_row: 0,
            columns: 3,
            visible_rows: 3,
            covers: Vec::new(),
            picker: Arc::new(
                Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()),
            ),
            filter: String::new(),
            input_mode: false,
            message: None,
        };
        page.refresh_files();
        page
    }

    pub fn refresh_files(&mut self) {
        self.all_files = scan_downloads();
        self.apply_filter();
        self.message = None;
    }

    fn apply_filter(&mut self) {
        let filter = self.filter.trim().to_lowercase();
        if filter.is_empty() {
            self.files = self.all_files.clone();
        } else {
            self.files = self
                .all_files
                .iter()
                .filter(|file| {
                    file.path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(&filter))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
        }
        if self.selected >= self.files.len() {
            self.selected = self.files.len().saturating_sub(1);
        }
        self.covers = self
            .files
            .iter()
            .map(|file| load_cover(&file.path, &self.picker))
            .collect();
        self.scroll_row = 0;
    }

    pub fn delete_selected(&mut self) {
        let Some(file) = self.files.get(self.selected) else {
            self.message = Some("没有可删除的文件".to_string());
            return;
        };
        match std::fs::remove_file(&file.path) {
            Ok(()) => {
                let name = file
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.message = Some(format!("已删除: {name}"));
                self.refresh_files();
            }
            Err(e) => {
                self.message = Some(format!("删除失败: {e}"));
            }
        }
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.files.get(self.selected).map(|f| f.path.clone())
    }

    fn columns(width: u16) -> usize {
        // Compact 3-column grid (same as UP 主页「合集」).
        if width >= 120 {
            3
        } else if width >= 80 {
            2
        } else {
            1
        }
    }

    fn card_height() -> u16 {
        16
    }
}

fn load_cover(path: &PathBuf, picker: &Picker) -> Option<StatefulProtocol> {
    // Same multi-dot caveat: never use with_extension here.
    let mp4_name = path.to_string_lossy();
    let stem = mp4_name.strip_suffix(".mp4").unwrap_or(&mp4_name);
    let cover_path = PathBuf::from(format!("{stem}.jpg"));
    if !cover_path.exists() {
        return None;
    }
    let img = image::open(&cover_path).ok()?;
    // Keep the original cover resolution (like VideoCard does for video
    // covers); shrinking it makes the rendered image tiny inside the card.
    Some(picker.new_resize_protocol(img))
}

fn scan_downloads() -> Vec<DownloadedFile> {
    let dir = download_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<DownloadedFile> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "mp4")
                .unwrap_or(false)
        })
        .map(|entry| {
            let meta = entry.metadata().ok();
            DownloadedFile {
                path: entry.path(),
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta.and_then(|m| m.modified().ok()),
            }
        })
        .collect();
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    files
}

fn format_size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= MB as u64 {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

/// Extract a percentage from messages like "下载中 47%".
fn parse_percent_from_msg(msg: &str) -> Option<f32> {
    msg.split_whitespace()
        .find(|token| token.ends_with('%'))
        .and_then(|token| token.trim_end_matches('%').parse::<f32>().ok())
}

fn format_time(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "未知时间".to_string())
}

impl Component for DownloadsPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(if current_status().is_some() { 6 } else { 1 }),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(area);

        // Title.
        let title_block = Block::bordered()
            .title(Span::styled(
                " 下载管理 ",
                Style::default()
                    .fg(theme.fg_accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(Style::default().fg(theme.border_focused));
        frame.render_widget(title_block, chunks[0]);

        // Active download status.
        if let Some(status) = current_status() {
            let status_block = Block::bordered()
                .title(" 当前下载 ")
                .border_style(Style::default().fg(theme.border_unfocused));
            let text = if status.total > 0 {
                format!(
                    " [{}/{}] {} — {}",
                    status.done, status.total, status.current_title, status.current_msg
                )
            } else {
                format!(" {} — {}", status.current_title, status.current_msg)
            };
            let inner = status_block.inner(chunks[1]);
            frame.render_widget(status_block, chunks[1]);
            let bar_width = inner.width.saturating_sub(2).max(8) as usize;
            let bar = match parse_percent_from_msg(&status.current_msg) {
                Some(pct) => {
                    let filled = ((pct / 100.0) * bar_width as f32).round() as usize;
                    format!(
                        "[{}{}] {:3.0}%",
                        "█".repeat(filled.min(bar_width)),
                        "░".repeat(bar_width.saturating_sub(filled)),
                        pct
                    )
                }
                None => {
                    let done = status.done;
                    let total = status.total.max(1);
                    let filled = (done * bar_width) / total;
                    format!(
                        "[{}{}] {}/{}",
                        "█".repeat(filled),
                        "░".repeat(bar_width.saturating_sub(filled)),
                        done,
                        status.total
                    )
                }
            };
            let lines = Text::from(vec![
                Line::from(Span::styled(
                    text,
                    Style::default().fg(theme.fg_primary),
                )),
                Line::from(Span::styled(
                    bar,
                    Style::default().fg(theme.info),
                )),
            ]);
            frame.render_widget(Paragraph::new(lines), inner);
        } else {
            frame.render_widget(Paragraph::new(""), chunks[1]);
        }

        // File grid (like UP 主页「合集」).
        let list_title = if self.filter.is_empty() {
            format!(" 已下载 ({} 个) ", self.files.len())
        } else {
            format!(
                " 已下载 ({} 个，过滤: {}) ",
                self.files.len(),
                self.filter
            )
        };
        let grid_block = Block::bordered()
            .title(Span::styled(
                list_title,
                Style::default().fg(theme.fg_secondary),
            ))
            .border_style(Style::default().fg(theme.border_unfocused));
        let grid_area = grid_block.inner(chunks[2]);
        frame.render_widget(grid_block, chunks[2]);

        if self.files.is_empty() {
            let empty = Paragraph::new(Line::from(vec![
                Span::styled(
                    if self.filter.is_empty() {
                        "暂无已下载文件。"
                    } else {
                        "没有匹配的文件。"
                    },
                    Style::default().fg(theme.fg_muted),
                ),
                Span::raw(" 在视频/番剧详情页按 D 开始下载，按 / 过滤。"),
            ]));
            frame.render_widget(empty, grid_area);
            self.render_footer(frame, chunks[3], theme, keys);
            return;
        }

        self.columns = Self::columns(grid_area.width).max(1);
        let cols = self.columns;
        let rows = (self.files.len() + cols - 1) / cols;
        self.visible_rows = (grid_area.height / Self::card_height()).max(1) as usize;
        if self.scroll_row > rows.saturating_sub(self.visible_rows) {
            self.scroll_row = rows.saturating_sub(self.visible_rows);
        }

        // Render only the visible window of rows.
        let first_row = self.scroll_row;
        let last_row = (first_row + self.visible_rows).min(rows);
        for row in first_row..last_row {
            let row_y = (row - first_row) as u16 * Self::card_height();
            let row_area = Rect {
                x: grid_area.x,
                y: grid_area.y + row_y,
                width: grid_area.width,
                height: Self::card_height().min(grid_area.height.saturating_sub(row_y)),
            };
            if row_area.height == 0 {
                continue;
            }
            let cols_area = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, cols as u32); cols])
                .split(row_area);
            for col in 0..cols {
                let index = row * cols + col;
                let Some(file) = self.files.get(index) else {
                    break;
                };
                let selected = index == self.selected;
                let border = if selected {
                    Style::default().fg(theme.fg_accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.border_subtle)
                };
                let name = file
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let card_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(border);
                let inner = card_block.inner(cols_area[col]);
                frame.render_widget(card_block, cols_area[col]);

                let card_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(inner.height.saturating_sub(3)),
                        Constraint::Length(3),
                    ])
                    .split(inner);

                // Cover area.
                let cover_area = card_chunks[0];
                if let Some(cover) = self.covers.get_mut(index).and_then(|c| c.as_mut()) {
                    let image_widget = StatefulImage::new();
                    frame.render_stateful_widget(image_widget, cover_area, cover);
                } else {
                    let placeholder = Paragraph::new("🎬")
                        .style(Style::default().fg(theme.fg_muted))
                        .alignment(Alignment::Center);
                    frame.render_widget(placeholder, cover_area);
                }

                // Info area: name + size/date.
                let info_area = card_chunks[1];
                let max_len = (info_area.width.saturating_sub(2)) as usize;
                let display_name: String = if name.chars().count() > max_len {
                    name.chars().take(max_len.saturating_sub(2)).collect::<String>() + "…"
                } else {
                    name
                };
                let info_text = Text::from(vec![
                    Line::from(Span::styled(
                        display_name,
                        Style::default().fg(if selected { theme.fg_accent } else { theme.fg_primary })
                            .add_modifier(if selected { Modifier::BOLD } else { Modifier::default() }),
                    )),
                    Line::from(Span::styled(
                        format!(
                            "{} · {}",
                            format_size(file.size),
                            format_time(file.modified.unwrap_or(SystemTime::UNIX_EPOCH))
                        ),
                        Style::default().fg(theme.fg_muted),
                    )),
                ]);
                frame.render_widget(
                    Paragraph::new(info_text),
                    info_area,
                );
            }
        }

        self.render_footer(frame, chunks[3], theme, keys);
    }

    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        // Tab navigation always wins (switches sidebar selection).
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }

        if self.input_mode {
            match key {
                KeyCode::Esc => {
                    self.input_mode = false;
                    self.filter.clear();
                    self.apply_filter();
                    self.message = None;
                }
                KeyCode::Enter => {
                    self.input_mode = false;
                    self.message = None;
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.apply_filter();
                }
                KeyCode::Char(c) if c.is_ascii() => {
                    self.filter.push(c);
                    self.apply_filter();
                }
                _ => {}
            }
            return None;
        }

        match key {
            KeyCode::Char('/') => {
                self.input_mode = true;
                self.filter.clear();
                self.message = None;
            }
            KeyCode::Char('r') => {
                self.refresh_files();
            }
            KeyCode::Char('d') => {
                self.delete_selected();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let cols = self.columns.max(1);
                if self.selected >= cols {
                    self.selected -= cols;
                }
                self.ensure_visible();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let cols = self.columns.max(1);
                if !self.files.is_empty() && self.selected + cols < self.files.len() {
                    self.selected += cols;
                }
                self.ensure_visible();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                self.ensure_visible();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if !self.files.is_empty() && self.selected + 1 < self.files.len() {
                    self.selected += 1;
                }
                self.ensure_visible();
            }
            KeyCode::Enter => {
                if let Some(path) = self.selected_path() {
                    return Some(AppAction::PlayLocalFile {
                        path: path.to_string_lossy().into_owned(),
                    });
                }
            }
            _ => {}
        }
        None
    }
}

impl DownloadsPage {
    fn ensure_visible(&mut self) {
        let cols = self.columns.max(1);
        let row = self.selected / cols;
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + self.visible_rows {
            self.scroll_row = row + 1 - self.visible_rows;
        }
    }

    fn render_footer(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        _keys: &Keybindings,
    ) {
        let items = if self.input_mode {
            vec![
                ("Esc".to_string(), "取消".into(), theme.fg_secondary),
                ("Enter".to_string(), "完成".into(), theme.success),
                ("!".to_string(), format!("过滤: {}", self.filter), theme.info),
            ]
        } else {
            let mut v = vec![
                ("↑↓←→".to_string(), "选择".into(), theme.fg_secondary),
                ("Enter".to_string(), "播放".into(), theme.success),
                ("/".to_string(), "过滤".into(), theme.info),
                ("r".to_string(), "刷新".into(), theme.info),
                ("d".to_string(), "删除".into(), theme.error),
            ];
            if let Some(msg) = &self.message {
                v.push(("!".to_string(), msg.clone(), theme.warning));
            }
            v
        };
        frame.render_widget(shortcut_footer(theme, items), area);
    }
}
