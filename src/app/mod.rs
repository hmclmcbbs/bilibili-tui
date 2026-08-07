mod actions;
mod network_events;
mod runtime;

use crate::api::auth::CurrentUser;
use crate::application::network;
use crate::domain::playback::{PlaybackEvent, PlaybackState};
use crate::infrastructure::{
    bilibili::{ApiClient, LiveDanmakuHub},
    persistence::{self, AppConfig, Credentials, Keybindings},
};
use crate::presentation::tui::{BangumiPage, DEFAULT_THEME_ID, HomePage, Page, Sidebar, Theme};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use tokio::sync::watch;

#[derive(Default)]
struct RequestTracker {
    sequence: u64,
    pending: HashMap<&'static str, u64>,
}

impl RequestTracker {
    fn next(&mut self, key: &'static str) -> u64 {
        self.sequence = self.sequence.saturating_add(1);
        self.pending.insert(key, self.sequence);
        self.sequence
    }

    fn is_latest(&self, key: &'static str, request_id: u64) -> bool {
        self.pending
            .get(key)
            .is_some_and(|latest| *latest == request_id)
    }
}

/// Previous page for back navigation
#[derive(Clone)]
pub enum PreviousPage {
    Home,
    Search,
    Dynamic,
    History,
    Favorites,
    Live,
    Bangumi,
}

/// Main application state
pub struct App {
    pub current_page: Page,
    pub should_quit: bool,
    pub api_client: Arc<ApiClient>,
    pub credentials: Option<Credentials>,
    pub sidebar: Sidebar,
    pub show_sidebar: bool,

    /// Currently logged-in user profile shown in the sidebar.
    pub current_user: Option<CurrentUser>,
    /// Terminal-graphics protocol for the user's avatar (rendered in sidebar).
    pub user_avatar: Option<StatefulProtocol>,
    user_avatar_pending: bool,
    avatar_picker: Arc<Picker>,
    avatar_tx: tokio::sync::mpsc::Sender<Option<StatefulProtocol>>,
    avatar_rx: tokio::sync::mpsc::Receiver<Option<StatefulProtocol>>,

    pub previous_page: Option<PreviousPage>,
    /// Full page instances for nested detail navigation (list -> video -> UP).
    pub navigation_stack: Vec<Page>,
    pub theme: Theme,
    pub theme_id: String,
    pub config: AppConfig,
    pub keybindings: Keybindings,
    pub pending_home_notice: Option<String>,
    pub playback: PlaybackState,
    pub live_danmaku_hub: Option<Arc<LiveDanmakuHub>>,
    danmaku_config_tx: watch::Sender<crate::storage::DanmakuConfig>,
    playback_event_tx: mpsc::Sender<PlaybackEvent>,
    playback_event_rx: mpsc::Receiver<PlaybackEvent>,
    auto_return_after_playback: Option<(u64, String)>,
    next_playback_session_id: u64,
    pending_playlist: Option<(
        Vec<crate::domain::playback::PlaylistItem>,
        crate::domain::playback::PlaylistSource,
        usize,
        crate::domain::playback::PlayOrder,
    )>,

    /// Cached home page to avoid refresh when switching tabs
    pub cached_home: Option<HomePage>,
    /// Cached bangumi page to avoid refresh when switching tabs
    pub cached_bangumi: Option<BangumiPage>,
    network_command_tx: mpsc::Sender<network::NetworkCommand>,
    network_event_rx: mpsc::Receiver<network::NetworkEvent>,
    request_tracker: RequestTracker,
}

impl App {
    pub fn new() -> Self {
        let credentials = persistence::load_credentials().ok();
        let api_client = if let Some(ref creds) = credentials {
            ApiClient::with_cookies(creds)
        } else {
            ApiClient::new()
        };
        let api_client = Arc::new(api_client);
        let bridge = network::start_network_worker(api_client.clone());
        let (playback_event_tx, playback_event_rx) = mpsc::channel();

        // Load config and apply saved theme
        let config = persistence::load_config().unwrap_or_default();
        let (danmaku_config_tx, _) = watch::channel(config.danmaku.clone());
        let keybindings = config.keybindings.clone();
        let configured_theme_id = config.theme.clone();
        let (theme, used_fallback) = Theme::load_or_default(&configured_theme_id);
        let theme_id = if used_fallback {
            DEFAULT_THEME_ID.to_string()
        } else {
            configured_theme_id
        };

        // Always start from home. Login is now an optional flow.
        let current_page = Page::Home(HomePage::new());

        let avatar_picker = Arc::new(
            Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()),
        );
        let (avatar_tx, avatar_rx) = tokio::sync::mpsc::channel(4);

        Self {
            current_page,
            should_quit: false,
            api_client,
            credentials,
            sidebar: Sidebar::new(),
            show_sidebar: true,
            current_user: None,
            user_avatar: None,
            user_avatar_pending: false,
            avatar_picker,
            avatar_tx,
            avatar_rx,
            previous_page: None,
            navigation_stack: Vec::new(),
            theme,
            theme_id,
            config,
            keybindings,
            pending_home_notice: used_fallback
                .then_some("⚠ 旧主题配置无效，请前往设置页重新选择主题".to_string()),
            playback: PlaybackState::default(),
            live_danmaku_hub: None,
            danmaku_config_tx,
            playback_event_tx,
            playback_event_rx,
            auto_return_after_playback: None,
            next_playback_session_id: 1,
            pending_playlist: None,
            cached_home: None,
            cached_bangumi: None,
            network_command_tx: bridge.command_tx,
            network_event_rx: bridge.event_rx,
            request_tracker: RequestTracker::default(),
        }
    }

    fn next_request_id(&mut self, key: &'static str) -> u64 {
        self.request_tracker.next(key)
    }

    fn is_latest_request(&self, key: &'static str, req_id: u64) -> bool {
        self.request_tracker.is_latest(key, req_id)
    }

    fn send_network_command(&self, command: network::NetworkCommand) {
        let _ = self.network_command_tx.send(command);
    }

    fn allocate_playback_session(&mut self) -> u64 {
        let id = self.next_playback_session_id;
        self.next_playback_session_id = self.next_playback_session_id.saturating_add(1);
        id
    }

    /// Fetch the current user profile (if logged in) and refresh the sidebar.
    pub async fn refresh_current_user(&mut self) {
        if self.credentials.is_none() {
            self.current_user = None;
            self.user_avatar = None;
            return;
        }
        match self.api_client.get_current_user().await {
            Ok(Some(user)) => {
                let changed = self
                    .current_user
                    .as_ref()
                    .map(|u| u.mid != user.mid || u.face != user.face)
                    .unwrap_or(true);
                self.current_user = Some(user);
                if changed {
                    self.user_avatar = None;
                    self.user_avatar_pending = false;
                    self.start_avatar_download();
                }
            }
            Ok(None) => {
                self.current_user = None;
                self.user_avatar = None;
            }
            Err(_) => {
                // Keep the previous profile on transient failure.
            }
        }
    }

    /// Kick off a background avatar download if we have a face URL and no
    /// avatar is in flight yet.
    fn start_avatar_download(&mut self) {
        if self.user_avatar_pending || self.user_avatar.is_some() {
            return;
        }
        let Some(user) = self.current_user.clone() else {
            return;
        };
        if user.face.is_empty() {
            return;
        }
        self.user_avatar_pending = true;
        let picker = Arc::clone(&self.avatar_picker);
        let tx = self.avatar_tx.clone();
        tokio::spawn(async move {
            let protocol = download_avatar(&user.face, &picker).await;
            let _ = tx.send(protocol).await;
        });
    }

    /// Poll for a completed avatar download (called every tick).
    pub fn poll_user_avatar(&mut self) {
        while let Ok(protocol) = self.avatar_rx.try_recv() {
            self.user_avatar = protocol;
            self.user_avatar_pending = false;
        }
    }
}

/// Download a user avatar, crop it to a centered square and build a
/// terminal-graphics protocol. Returns `None` on any failure.
async fn download_avatar(url: &str, picker: &Picker) -> Option<StatefulProtocol> {
    let response = reqwest::get(url).await.ok()?;
    let bytes = response.bytes().await.ok()?;
    let mut img: image::DynamicImage = image::load_from_memory(&bytes).ok()?;
    let side = img.width().min(img.height());
    let x = (img.width() - side) / 2;
    let y = (img.height() - side) / 2;
    img = img.crop_imm(x, y, side, side);
    img = img.resize(96, 96, image::imageops::FilterType::Triangle);
    // B 站很多头像 jpg 是白底，在深色终端里渲染出来像一块突兀的白色色块。
    // 如果图片本身不带透明通道（jpg），把接近纯白的背景像素转成透明，
    // 这样头像边缘以外不再有不透明的实色块。
    if !has_alpha(&img) {
        img = make_white_transparent(img);
    }
    Some(picker.new_resize_protocol(img))
}

/// Check whether the image carries an alpha channel with any transparency.
fn has_alpha(img: &image::DynamicImage) -> bool {
    match img.color() {
        image::ColorType::Rgba8 => {
            let rgba = img.to_rgba8();
            rgba.pixels().any(|p| p.0[3] < 250)
        }
        _ => false,
    }
}

/// Turn near-white background pixels transparent while keeping the subject.
fn make_white_transparent(img: image::DynamicImage) -> image::DynamicImage {
    let mut rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    // 边缘像素往往是纯背景；用它们估一个背景阈值，避免误伤内容里的白色。
    let mut bg_sample = 0u64;
    let mut bg_count = 0u64;
    for x in 0..w {
        for y in [0u32, h - 1] {
            let p = rgba.get_pixel(x, y);
            bg_sample += p.0[0] as u64 + p.0[1] as u64 + p.0[2] as u64;
            bg_count += 3;
        }
    }
    for y in 0..h {
        for x in [0u32, w - 1] {
            let p = rgba.get_pixel(x, y);
            bg_sample += p.0[0] as u64 + p.0[1] as u64 + p.0[2] as u64;
            bg_count += 3;
        }
    }
    let avg = if bg_count > 0 {
        bg_sample / bg_count
    } else {
        255
    };
    // 背景平均亮度低于 220 就当作深色背景（深色主题下不明显），只处理浅色背景。
    if avg < 220 {
        return image::DynamicImage::ImageRgba8(rgba);
    }
    // 白底：RGB 三个通道都接近背景色（宽松一点），alpha 置 0。
    let threshold = (avg as i32 - 28).max(210) as u8;
    for p in rgba.pixels_mut() {
        let r = p.0[0] as i32;
        let g = p.0[1] as i32;
        let b = p.0[2] as i32;
        let min_c = r.min(g).min(b);
        let max_c = r.max(g).max(b);
        if min_c >= threshold as i32 && (max_c - min_c) <= 24 {
            p.0[3] = 0;
        }
    }
    image::DynamicImage::ImageRgba8(rgba)
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{App, RequestTracker};
    use crate::application::AppAction;
    use crate::domain::playback::PlaybackEvent;
    use crate::presentation::tui::{FavoritesPage, HomePage, NavItem, Page, VideoDetailPage};

    #[test]
    fn request_tracking_latest_wins_per_key() {
        let mut tracker = RequestTracker::default();
        let first = tracker.next("search");
        let second = tracker.next("search");

        assert!(!tracker.is_latest("search", first));
        assert!(tracker.is_latest("search", second));
    }

    #[test]
    fn request_tracking_isolated_by_key() {
        let mut tracker = RequestTracker::default();
        let search_id = tracker.next("search");
        let home_id = tracker.next("home");

        assert!(tracker.is_latest("search", search_id));
        assert!(tracker.is_latest("home", home_id));
        assert!(!tracker.is_latest("home", search_id));
    }

    #[tokio::test]
    async fn tab_continues_from_favorites_to_live() {
        let mut app = App::new();
        app.sidebar.select(NavItem::Favorites);
        app.current_page = Page::Favorites(FavoritesPage::new(1));
        app.handle_action(AppAction::NavNext).await;
        assert_eq!(app.sidebar.selected, NavItem::Live);
        assert!(matches!(app.current_page, Page::Live(_)));
    }

    #[tokio::test]
    async fn completed_auto_play_returns_to_the_previous_page() {
        let mut app = App::new();
        app.navigation_stack.push(Page::Home(HomePage::new()));
        app.current_page =
            Page::VideoDetail(Box::new(VideoDetailPage::new("BV1test".to_string(), 1)));
        app.playback.begin_session(7);
        app.auto_return_after_playback = Some((7, "BV1test".to_string()));
        app.playback_event_tx
            .send(PlaybackEvent::Finished {
                session_id: 7,
                bvid: Some("BV1test".to_string()),
            })
            .unwrap();

        app.tick().await;

        assert!(matches!(app.current_page, Page::Home(_)));
        assert!(app.navigation_stack.is_empty());
        assert!(app.auto_return_after_playback.is_none());
    }

    #[tokio::test]
    async fn stale_playback_session_does_not_return() {
        let mut app = App::new();
        app.navigation_stack.push(Page::Home(HomePage::new()));
        app.current_page =
            Page::VideoDetail(Box::new(VideoDetailPage::new("BV1test".to_string(), 1)));
        app.playback.begin_session(8);
        app.auto_return_after_playback = Some((8, "BV1test".to_string()));
        app.playback_event_tx
            .send(PlaybackEvent::Finished {
                session_id: 7,
                bvid: Some("BV1test".to_string()),
            })
            .unwrap();
        app.tick().await;
        assert!(matches!(app.current_page, Page::VideoDetail(_)));
        assert_eq!(
            app.playback.status,
            crate::domain::playback::PlaybackStatus::Playing
        );
    }
}
