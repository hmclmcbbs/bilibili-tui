//! 会员购 (Bilibili mall) 订单与物流页面。
//!
//! 显示当前登录用户的会员购订单列表，选中订单后可查看物流概要，
//! 按 `t` 查看具体运输过程（物流轨迹），按 `g` 查看商品图片。

use super::{Component, Theme, shortcut_footer};
use crate::api::mall::{MallExpress, MallExpressSummary, MallOrder};
use crate::application::AppAction;
use crate::storage::Keybindings;
use image::DynamicImage;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyModifiers},
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::*,
};
use ratatui_image::{StatefulImage, picker::Picker, protocol::StatefulProtocol};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Completed product-image download.
struct ProductImageResult {
    order_id: i64,
    protocol: Option<StatefulProtocol>,
}

pub struct MallPage {
    pub orders: Vec<MallOrder>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected: usize,
    /// order_id -> express summary (None = no express info).
    pub expresses: HashMap<i64, Option<MallExpressSummary>>,
    /// order_id -> currently loading.
    pub express_loading: HashMap<i64, bool>,
    /// order_id -> error message.
    pub express_errors: HashMap<i64, String>,
    /// order_id -> express trace (None = no trace info).
    pub tracks: HashMap<i64, Option<MallExpress>>,
    /// order_id -> currently loading trace.
    pub track_loading: HashMap<i64, bool>,
    /// order_id -> trace error message.
    pub track_errors: HashMap<i64, String>,
    pub message: Option<String>,

    /// Product thumbnail images by order_id.
    product_images: HashMap<i64, Option<StatefulProtocol>>,
    pending_product_downloads: HashSet<i64>,
    product_tx: mpsc::Sender<ProductImageResult>,
    product_rx: mpsc::Receiver<ProductImageResult>,
    picker: Arc<Picker>,

    /// Full-screen transport trace view.
    track_view: bool,
    track_scroll: usize,
    /// Set when the user pressed `t` but the trace was still loading; enter
    /// the full-screen view as soon as the data arrives.
    track_pending_view: bool,
}

impl MallPage {
    pub fn new() -> Self {
        let picker = Arc::new(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
        let (product_tx, product_rx) = mpsc::channel(64);
        Self {
            orders: Vec::new(),
            loading: true,
            error: None,
            selected: 0,
            expresses: HashMap::new(),
            express_loading: HashMap::new(),
            express_errors: HashMap::new(),
            tracks: HashMap::new(),
            track_loading: HashMap::new(),
            track_errors: HashMap::new(),
            message: None,
            product_images: HashMap::new(),
            pending_product_downloads: HashSet::new(),
            product_tx,
            product_rx,
            picker,
            track_view: false,
            track_scroll: 0,
            track_pending_view: false,
        }
    }

    pub fn start_loading(&mut self) {
        self.loading = true;
        self.error = None;
        self.message = None;
    }

    pub fn apply_orders(&mut self, orders: Vec<MallOrder>) {
        self.orders = orders;
        self.loading = false;
        self.error = None;
        self.message = None;
        if self.selected >= self.orders.len() {
            self.selected = 0;
        }
        self.start_product_downloads();
    }

    pub fn apply_orders_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
        self.message = None;
    }

    pub fn start_express_loading(&mut self, order_id: i64) {
        self.express_loading.insert(order_id, true);
        self.express_errors.remove(&order_id);
        self.message = Some("正在获取物流信息...".to_string());
    }

    pub fn apply_express(
        &mut self,
        order_id: i64,
        express: Option<MallExpressSummary>,
    ) {
        self.express_loading.remove(&order_id);
        self.express_errors.remove(&order_id);
        self.message = match &express {
            Some(e) if !e.com_v.is_empty() && !e.sno.is_empty() => Some(format!(
                "物流: {} {} ({})",
                e.com_v, e.sno, e.status_v
            )),
            Some(_) => Some("该订单暂无物流信息".to_string()),
            None => Some("该订单暂无物流信息".to_string()),
        };
        self.expresses.insert(order_id, express);
    }

    pub fn apply_express_error(&mut self, order_id: i64, error: String) {
        self.express_loading.remove(&order_id);
        self.express_errors.insert(order_id, error.clone());
        self.message = Some(format!("物流获取失败: {error}"));
    }

    pub fn start_track_loading(&mut self, order_id: i64) {
        self.track_loading.insert(order_id, true);
        self.track_errors.remove(&order_id);
        self.message = Some("正在获取运输过程...".to_string());
    }

    pub fn apply_track(&mut self, order_id: i64, track: Option<MallExpress>) {
        self.track_loading.remove(&order_id);
        self.track_errors.remove(&order_id);
        self.message = match &track {
            Some(t) if !t.traces.is_empty() => Some(format!(
                "运输过程: {} 条物流记录",
                t.traces.len()
            )),
            Some(_) => Some("该订单暂无运输过程".to_string()),
            None => Some("该订单暂无运输过程".to_string()),
        };
        self.tracks.insert(order_id, track);
        if self.track_pending_view {
            self.track_pending_view = false;
            self.track_view = true;
            self.track_scroll = 0;
        }
    }

    pub fn apply_track_error(&mut self, order_id: i64, error: String) {
        self.track_loading.remove(&order_id);
        self.track_errors.insert(order_id, error.clone());
        self.message = Some(format!("运输过程获取失败: {error}"));
    }

    /// Kick off thumbnail downloads for every order that has a product logo
    /// and hasn't been requested yet.
    fn start_product_downloads(&mut self) {
        for order in &self.orders {
            let Some(logo) = order
                .rows
                .iter()
                .find(|row| !row.logo.is_empty())
                .map(|row| row.logo.clone())
            else {
                continue;
            };
            if self.pending_product_downloads.contains(&order.order_id)
                || self.product_images.contains_key(&order.order_id)
            {
                continue;
            }
            self.pending_product_downloads.insert(order.order_id);
            let order_id = order.order_id;
            let tx = self.product_tx.clone();
            let picker = Arc::clone(&self.picker);
            tokio::spawn(async move {
                let protocol = Self::download_product_image(&logo, &picker).await;
                let _ = tx.send(ProductImageResult { order_id, protocol }).await;
            });
        }
    }

    /// Poll for completed product image downloads.
    fn poll_product_results(&mut self) {
        while let Ok(result) = self.product_rx.try_recv() {
            self.pending_product_downloads.remove(&result.order_id);
            self.product_images.insert(result.order_id, result.protocol);
        }
    }

    async fn download_product_image(
        url: &str,
        picker: &Arc<Picker>,
    ) -> Option<StatefulProtocol> {
        // Mall logo URLs are scheme-less (`//i0.hdslb.com/...`); reqwest needs a
        // full URL, so prepend `https:` when the protocol is missing.
        let full_url = if url.starts_with("//") {
            format!("https:{url}")
        } else {
            url.to_string()
        };
        let response = reqwest::get(&full_url).await.ok()?;
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

    pub fn selected_order(&self) -> Option<&MallOrder> {
        self.orders.get(self.selected)
    }
    fn format_time(ts_seconds: i64) -> String {
        if ts_seconds <= 0 {
            return "—".to_string();
        }
        match chrono::DateTime::from_timestamp(ts_seconds, 0) {
            Some(dt) => dt
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            None => "—".to_string(),
        }
    }
}

impl Default for MallPage {
    fn default() -> Self {
        Self::new()
    }
}

impl MallPage {
    /// Render the full-screen transport trace view.
    fn draw_track_view(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Span::styled(
                " 🚚 运输过程 (Esc 返回) ",
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        if let Some(order) = self.selected_order() {
            let order_id = order.order_id;
            lines.push(Line::from(vec![
                Span::styled(
                    if order.shop_name.is_empty() {
                        format!("订单 {}", order.order_id)
                    } else {
                        order.shop_name.clone()
                    },
                    Style::default().fg(theme.bilibili_pink).add_modifier(Modifier::BOLD),
                ),
            ]));
            // Show the first product name for this order.
            if let Some(row) = order
                .rows
                .iter()
                .find(|row| !row.name.is_empty())
            {
                lines.push(Line::from(vec![
                    Span::styled("商品: ", Style::default().fg(theme.fg_secondary)),
                    Span::styled(
                        format!("{} x{}", row.name, row.count.max(1)),
                        Style::default().fg(theme.fg_primary).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            if let Some(Some(track)) = self.tracks.get(&order_id) {
                if !track.com_v.is_empty() || !track.sno.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("快递: {}  单号: {}", track.com_v, track.sno),
                            Style::default().fg(theme.fg_secondary),
                        ),
                    ]));
                }
                lines.push(Line::from(""));
                if track.traces.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "暂无运输记录",
                        Style::default().fg(theme.fg_secondary),
                    )));
                } else {
                    for trace in &track.traces {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", trace.time),
                            Style::default().fg(theme.fg_accent),
                        )));
                        lines.push(Line::from(Span::styled(
                            format!("  {}", trace.context),
                            Style::default().fg(theme.fg_primary),
                        )));
                        lines.push(Line::from(""));
                    }
                }
            } else if self.track_loading.get(&order_id).copied().unwrap_or(false) {
                lines.push(Line::from(Span::styled(
                    "运输过程加载中...",
                    Style::default().fg(theme.fg_secondary),
                )));
            } else if let Some(err) = self.track_errors.get(&order_id) {
                lines.push(Line::from(Span::styled(
                    format!("运输过程获取失败: {err}"),
                    Style::default().fg(theme.error),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "暂无运输记录",
                    Style::default().fg(theme.fg_secondary),
                )));
            }
        }

        // Scroll the lines.
        let visible = inner.height as usize;
        let scroll = self.track_scroll.min(lines.len().saturating_sub(visible));
        self.track_scroll = scroll;
        let start = scroll.min(lines.len());
        let slice: Vec<Line> = lines.into_iter().skip(start).take(visible).collect();
        let para = Paragraph::new(slice).scroll((0, 0));
        frame.render_widget(para, inner);
    }
}

impl Component for MallPage {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings) {
        self.poll_product_results();
        // Full-screen transport trace view.
        if self.track_view {
            self.draw_track_view(frame, area, theme);
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Span::styled(
                " 🛍️ 会员购 ",
                Style::default()
                    .fg(theme.bilibili_pink)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Status
                Constraint::Min(5),    // Order list / express
                Constraint::Length(1), // Footer
            ])
            .split(inner);

        // Status line
        let status = if self.loading {
            Line::from(Span::styled(
                "正在加载订单...",
                Style::default().fg(theme.fg_secondary),
            ))
        } else if let Some(err) = &self.error {
            Line::from(Span::styled(
                format!("加载失败: {err}"),
                Style::default().fg(theme.error),
            ))
        } else {
            Line::from(Span::styled(
                format!("共 {} 个订单", self.orders.len()),
                Style::default().fg(theme.fg_secondary),
            ))
        };
        frame.render_widget(Paragraph::new(status), chunks[0]);

        // Content
        if self.loading && self.orders.is_empty() {
            let hint = Paragraph::new(Line::from(Span::styled(
                "加载中...",
                Style::default().fg(theme.fg_secondary),
            )));
            frame.render_widget(hint, chunks[1]);
        } else if self.orders.is_empty() {
            let hint = Paragraph::new(Line::from(Span::styled(
                "暂无订单",
                Style::default().fg(theme.fg_secondary),
            )));
            frame.render_widget(hint, chunks[1]);
        } else {
            // Orders list (left) + express detail (right)
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(chunks[1]);

            // Order list
            let list_height = cols[0].height as usize;
            let scroll_start = self
                .selected
                .saturating_sub(list_height.saturating_sub(1))
                .min(self.orders.len().saturating_sub(1));
            let mut lines: Vec<Line> = Vec::new();
            for (idx, order) in self.orders.iter().enumerate().skip(scroll_start).take(list_height) {
                let selected = idx == self.selected;
                let name = if order.shop_name.is_empty() {
                    format!("订单 {}", order.order_id)
                } else {
                    order.shop_name.clone()
                };
                let money = format!("¥{:.2}", order.pay_money as f64 / 100.0);
                let time = Self::format_time(order.order_ctime);
                let status_name = if order.status_name.is_empty() {
                    "—".to_string()
                } else {
                    order.status_name.clone()
                };
                let line = if selected {
                    Line::from(vec![
                        Span::styled(
                            format!(" {name} "),
                            Style::default()
                                .fg(theme.fg_primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" [{status_name}] "),
                            Style::default().fg(theme.info),
                        ),
                        Span::styled(
                            format!(" {money} "),
                            Style::default().fg(theme.fg_accent),
                        ),
                        Span::styled(
                            format!(" {time}"),
                            Style::default().fg(theme.fg_secondary),
                        ),
                    ])
                    .style(Style::default().bg(theme.selection_bg))
                } else {
                    Line::from(vec![
                        Span::styled(
                            format!(" {name} "),
                            Style::default().fg(theme.fg_primary),
                        ),
                        Span::styled(
                            format!(" [{status_name}] "),
                            Style::default().fg(theme.fg_secondary),
                        ),
                        Span::styled(
                            format!(" {money} "),
                            Style::default().fg(theme.fg_secondary),
                        ),
                        Span::styled(
                            format!(" {time}"),
                            Style::default().fg(theme.fg_secondary),
                        ),
                    ])
                };
                lines.push(line);
            }
            frame.render_widget(Paragraph::new(lines), cols[0]);

            // Express detail for selected order
            let mut expr_lines: Vec<Line> = Vec::new();
            if let Some(order) = self.selected_order() {
                let order_id = order.order_id;
                // Product thumbnail + name for the selected order.
                let product = order
                    .rows
                    .iter()
                    .find(|row| !row.logo.is_empty() || !row.name.is_empty());
                if let Some(row) = product {
                    if !row.name.is_empty() {
                        expr_lines.push(Line::from(vec![
                            Span::styled(
                                format!("商品: {} x{}", row.name, row.count.max(1)),
                                Style::default().fg(theme.fg_primary).add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                }
                if self.express_loading.get(&order_id).copied().unwrap_or(false) {
                    expr_lines.push(Line::from(Span::styled(
                        "物流加载中...",
                        Style::default().fg(theme.fg_secondary),
                    )));
                } else if let Some(err) = self.express_errors.get(&order_id) {
                    expr_lines.push(Line::from(Span::styled(
                        format!("物流获取失败: {err}"),
                        Style::default().fg(theme.error),
                    )));
                } else if let Some(Some(expr)) = self.expresses.get(&order_id) {
                    expr_lines.push(Line::from(Span::styled(
                        "── 物流信息 ──",
                        Style::default().fg(theme.bilibili_pink).add_modifier(Modifier::BOLD),
                    )));
                    expr_lines.push(Line::from(vec![
                        Span::styled("快递公司: ", Style::default().fg(theme.fg_secondary)),
                        Span::styled(
                            if expr.com_v.is_empty() { "—".to_string() } else { expr.com_v.clone() },
                            Style::default().fg(theme.fg_primary),
                        ),
                    ]));
                    expr_lines.push(Line::from(vec![
                        Span::styled("快递单号: ", Style::default().fg(theme.fg_secondary)),
                        Span::styled(
                            if expr.sno.is_empty() { "—".to_string() } else { expr.sno.clone() },
                            Style::default().fg(theme.fg_primary),
                        ),
                    ]));
                    expr_lines.push(Line::from(vec![
                        Span::styled("物流状态: ", Style::default().fg(theme.fg_secondary)),
                        Span::styled(
                            if expr.state_v.is_empty() { "—".to_string() } else { expr.state_v.clone() },
                            Style::default().fg(theme.success),
                        ),
                    ]));
                    expr_lines.push(Line::from(vec![
                        Span::styled("订单状态: ", Style::default().fg(theme.fg_secondary)),
                        Span::styled(
                            if expr.status_v.is_empty() { "—".to_string() } else { expr.status_v.clone() },
                            Style::default().fg(theme.fg_primary),
                        ),
                    ]));
                    expr_lines.push(Line::from(""));
                    expr_lines.push(Line::from(Span::styled(
                        "按 Enter 刷新物流",
                        Style::default().fg(theme.fg_secondary),
                    )));
                    expr_lines.push(Line::from(Span::styled(
                        "按 t 全屏查看运输过程",
                        Style::default().fg(theme.fg_secondary),
                    )));
                } else {
                    expr_lines.push(Line::from(Span::styled(
                        "按 Enter 查看物流信息",
                        Style::default().fg(theme.fg_secondary),
                    )));
                }
            }
            let expr_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(Span::styled(
                    " 物流 ",
                    Style::default().fg(theme.info),
                ));
            let expr_inner = expr_block.inner(cols[1]);
            frame.render_widget(expr_block, cols[1]);

            // Split the express panel: product image on top, text below.
            if let Some(order) = self.selected_order() {
                let order_id = order.order_id;
                if let Some(protocol) = self.product_images.get_mut(&order_id).and_then(|p| p.as_mut()) {
                    let img_h = 14u16.min(expr_inner.height.saturating_sub(1));
                    let text_h = expr_inner.height.saturating_sub(img_h);
                    if text_h > 0 {
                        let chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([Constraint::Length(img_h), Constraint::Min(text_h)])
                            .split(expr_inner);
                        let image = StatefulImage::new();
                        frame.render_stateful_widget(image, chunks[0], protocol);
                        frame.render_widget(Paragraph::new(expr_lines), chunks[1]);
                    } else {
                        frame.render_widget(Paragraph::new(expr_lines), expr_inner);
                    }
                } else {
                    frame.render_widget(Paragraph::new(expr_lines), expr_inner);
                }
            } else {
                frame.render_widget(Paragraph::new(expr_lines), expr_inner);
            }
        }

        // Message
        if let Some(msg) = &self.message {
            let msg_line = Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(theme.success),
            ));
            // show on the status area's second line, not covering the status
            let msg_area = Rect {
                x: chunks[0].x,
                y: chunks[0].y.saturating_add(1),
                width: chunks[0].width,
                height: chunks[0].height.saturating_sub(1),
            };
            if msg_area.height > 0 {
                frame.render_widget(Paragraph::new(msg_line), msg_area);
            }
        }

        // Footer
        let footer = shortcut_footer(
            theme,
            vec![
                ("j/k".to_string(), "选择".to_string(), theme.fg_accent),
                ("Enter".to_string(), "物流".to_string(), theme.fg_accent),
                ("t".to_string(), "运输过程全屏".to_string(), theme.fg_accent),
                ("r".to_string(), "刷新".to_string(), theme.fg_accent),
                ("Tab".to_string(), "侧边栏".to_string(), theme.fg_secondary),
                ("o".to_string(), "网页".to_string(), theme.fg_secondary),
                ("q".to_string(), "退出".to_string(), theme.fg_secondary),
            ],
        );
        frame.render_widget(footer, chunks[2]);
    }


    fn handle_input_with_modifiers(
        &mut self,
        key: KeyCode,
        _modifiers: KeyModifiers,
        keys: &Keybindings,
    ) -> Option<AppAction> {
        // Full-screen track view key handling.
        if self.track_view {
            match key {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.track_view = false;
                    self.track_scroll = 0;
                    None
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.track_scroll = self.track_scroll.saturating_add(1);
                    None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.track_scroll = self.track_scroll.saturating_sub(1);
                    None
                }
                _ => None,
            }
        } else {
        // Tab / BackTab switches to the sidebar, same as other pages.
        if keys.matches_nav_next(key) {
            return Some(AppAction::NavNext);
        }
        if keys.matches_nav_prev(key) {
            return Some(AppAction::NavPrev);
        }
        match key {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.orders.is_empty() {
                    let next = (self.selected + 1).min(self.orders.len() - 1);
                    if next != self.selected {
                        self.selected = next;
                        // Auto-load express for the newly selected order.
                        if let Some(order) = self.selected_order() {
                            return Some(AppAction::LoadMallExpress {
                                order_id: order.order_id,
                            });
                        }
                    }
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.orders.is_empty() {
                    let next = self.selected.saturating_sub(1);
                    if next != self.selected {
                        self.selected = next;
                        // Auto-load express for the newly selected order.
                        if let Some(order) = self.selected_order() {
                            return Some(AppAction::LoadMallExpress {
                                order_id: order.order_id,
                            });
                        }
                    }
                }
                None
            }
            KeyCode::Char('r') => Some(AppAction::RefreshMall),
            KeyCode::Char('t') => {
                if let Some(order) = self.selected_order() {
                    let order_id = order.order_id;
                    if self.tracks.contains_key(&order_id)
                        && !self.track_loading.get(&order_id).copied().unwrap_or(false)
                    {
                        self.track_view = true;
                        self.track_scroll = 0;
                        self.track_pending_view = false;
                        None
                    } else {
                        self.track_pending_view = true;
                        Some(AppAction::LoadMallExpressTrack { order_id })
                    }
                } else {
                    None
                }
            }
            KeyCode::Enter => {
                if let Some(order) = self.selected_order() {
                    Some(AppAction::LoadMallExpress {
                        order_id: order.order_id,
                    })
                } else {
                    None
                }
            }
            KeyCode::Char('o') => {
                self.message = Some("已在浏览器中打开 会员购 (mall.bilibili.com)".to_string());
                Some(AppAction::OpenExternalUrl(
                    "https://mall.bilibili.com/".to_string(),
                ))
            }
            _ if keys.matches_back(key) || keys.matches_quit(key) => None,
            _ => None,
        }
        }
    }
}
