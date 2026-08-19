//! Credential storage and persistence

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COOKIE_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn write_private_file(path: &std::path::Path, content: &[u8]) -> Result<()> {
    let sequence = COOKIE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let write_result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// User credentials from Bilibili login
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub sessdata: String,
    pub bili_jct: String,
    pub dede_user_id: String,
    pub dede_user_id_ckmd5: Option<String>,
    pub refresh_token: Option<String>,
}

impl Credentials {
    pub fn from_cookies(
        cookies: &[(String, String)],
        refresh_token: Option<String>,
    ) -> Option<Self> {
        // ✅ 使用闭包和迭代器查找 cookie
        let get_cookie = |name: &str| -> Option<String> {
            cookies
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
        };

        Some(Credentials {
            sessdata: get_cookie("SESSDATA")?,
            bili_jct: get_cookie("bili_jct")?,
            dede_user_id: get_cookie("DedeUserID")?,
            dede_user_id_ckmd5: get_cookie("DedeUserID__ckMd5"),
            refresh_token,
        })
    }
}

/// Keybindings configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybindings {
    // Global actions
    pub quit: String,
    pub confirm: String,
    pub back: String,
    pub refresh: String,

    // Navigation
    pub nav_up: String,
    pub nav_down: String,
    pub nav_left: String,
    pub nav_right: String,
    pub nav_next_page: String,
    pub nav_prev_page: String,
    /// Move the focused content list by one viewport.
    pub page_down: String,
    pub page_up: String,

    // Section/Tab navigation
    pub section_prev: String,
    pub section_next: String,
    pub tab_1: String,
    pub tab_2: String,
    pub tab_3: String,

    // Actions
    pub next_theme: String,
    pub play: String,
    pub open_settings: String,
    pub search_focus: String,

    // Comments
    pub comment: String,
    pub toggle_replies: String,

    // Dynamic page specific
    pub up_prev: String,
    pub up_next: String,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            // Global
            quit: "q".to_string(),
            confirm: "Enter".to_string(),
            back: "Esc".to_string(),
            refresh: "r".to_string(),

            // Navigation
            nav_up: "k".to_string(),
            nav_down: "j".to_string(),
            nav_left: "h".to_string(),
            nav_right: "l".to_string(),
            nav_next_page: "Tab".to_string(),
            nav_prev_page: "BackTab".to_string(),
            page_down: "PageDown".to_string(),
            page_up: "PageUp".to_string(),

            // Section/Tab
            section_prev: "[".to_string(),
            section_next: "]".to_string(),
            tab_1: "1".to_string(),
            tab_2: "2".to_string(),
            tab_3: "3".to_string(),

            // Actions
            next_theme: "t".to_string(),
            play: "p".to_string(),
            open_settings: "s".to_string(),
            search_focus: "i".to_string(),

            // Comments
            comment: "c".to_string(),
            toggle_replies: "r".to_string(),

            // Dynamic page
            up_prev: "[".to_string(),
            up_next: "]".to_string(),
        }
    }
}

use ratatui::crossterm::event::KeyCode;

impl Keybindings {
    /// Parse a string representation into a KeyCode
    pub fn parse_keycode(s: &str) -> Option<KeyCode> {
        let s = s.trim();
        match s.to_lowercase().as_str() {
            "enter" | "return" => Some(KeyCode::Enter),
            "esc" | "escape" => Some(KeyCode::Esc),
            "tab" => Some(KeyCode::Tab),
            "backtab" | "shift+tab" => Some(KeyCode::BackTab),
            "backspace" => Some(KeyCode::Backspace),
            "delete" | "del" => Some(KeyCode::Delete),
            "insert" | "ins" => Some(KeyCode::Insert),
            "home" => Some(KeyCode::Home),
            "end" => Some(KeyCode::End),
            "pageup" | "pgup" => Some(KeyCode::PageUp),
            "pagedown" | "pgdn" => Some(KeyCode::PageDown),
            "up" | "↑" => Some(KeyCode::Up),
            "down" | "↓" => Some(KeyCode::Down),
            "left" | "←" => Some(KeyCode::Left),
            "right" | "→" => Some(KeyCode::Right),
            "space" | " " => Some(KeyCode::Char(' ')),
            "f1" => Some(KeyCode::F(1)),
            "f2" => Some(KeyCode::F(2)),
            "f3" => Some(KeyCode::F(3)),
            "f4" => Some(KeyCode::F(4)),
            "f5" => Some(KeyCode::F(5)),
            "f6" => Some(KeyCode::F(6)),
            "f7" => Some(KeyCode::F(7)),
            "f8" => Some(KeyCode::F(8)),
            "f9" => Some(KeyCode::F(9)),
            "f10" => Some(KeyCode::F(10)),
            "f11" => Some(KeyCode::F(11)),
            "f12" => Some(KeyCode::F(12)),
            _ => {
                // Single character
                let chars: Vec<char> = s.chars().collect();
                if chars.len() == 1 {
                    Some(KeyCode::Char(chars[0]))
                } else {
                    None
                }
            }
        }
    }

    /// Convert a KeyCode to its string representation
    pub fn keycode_to_string(key: KeyCode) -> String {
        match key {
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "BackTab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::F(n) => format!("F{}", n),
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            _ => "Unknown".to_string(),
        }
    }

    /// Check if a key matches the configured keybinding (including arrow key alternatives)
    pub fn matches(&self, binding: &str, key: KeyCode) -> bool {
        if let Some(configured_key) = Self::parse_keycode(binding)
            && key == configured_key
        {
            return true;
        }
        false
    }

    // Convenience methods for common keybindings
    pub fn matches_quit(&self, key: KeyCode) -> bool {
        self.matches(&self.quit, key)
    }

    pub fn matches_confirm(&self, key: KeyCode) -> bool {
        self.matches(&self.confirm, key)
    }

    pub fn matches_back(&self, key: KeyCode) -> bool {
        self.matches(&self.back, key)
    }

    pub fn matches_refresh(&self, key: KeyCode) -> bool {
        self.matches(&self.refresh, key)
    }

    pub fn matches_up(&self, key: KeyCode) -> bool {
        self.matches(&self.nav_up, key) || key == KeyCode::Up
    }

    pub fn matches_down(&self, key: KeyCode) -> bool {
        self.matches(&self.nav_down, key) || key == KeyCode::Down
    }

    pub fn matches_left(&self, key: KeyCode) -> bool {
        self.matches(&self.nav_left, key) || key == KeyCode::Left
    }

    pub fn matches_right(&self, key: KeyCode) -> bool {
        self.matches(&self.nav_right, key) || key == KeyCode::Right
    }

    pub fn matches_nav_next(&self, key: KeyCode) -> bool {
        self.matches(&self.nav_next_page, key)
    }

    pub fn matches_nav_prev(&self, key: KeyCode) -> bool {
        self.matches(&self.nav_prev_page, key)
    }

    pub fn matches_page_down(&self, key: KeyCode) -> bool {
        self.matches(&self.page_down, key)
    }

    pub fn matches_page_up(&self, key: KeyCode) -> bool {
        self.matches(&self.page_up, key)
    }

    pub fn matches_next_theme(&self, key: KeyCode) -> bool {
        self.matches(&self.next_theme, key)
    }

    pub fn matches_play(&self, key: KeyCode) -> bool {
        self.matches(&self.play, key)
    }

    pub fn matches_open_settings(&self, key: KeyCode) -> bool {
        self.matches(&self.open_settings, key)
    }

    pub fn matches_search_focus(&self, key: KeyCode) -> bool {
        self.matches(&self.search_focus, key)
    }

    pub fn matches_section_prev(&self, key: KeyCode) -> bool {
        self.matches(&self.section_prev, key)
    }

    pub fn matches_section_next(&self, key: KeyCode) -> bool {
        self.matches(&self.section_next, key)
    }

    pub fn matches_comment(&self, key: KeyCode) -> bool {
        self.matches(&self.comment, key)
    }

    pub fn matches_toggle_replies(&self, key: KeyCode) -> bool {
        self.matches(&self.toggle_replies, key)
    }

    pub fn matches_tab_1(&self, key: KeyCode) -> bool {
        self.matches(&self.tab_1, key)
    }

    pub fn matches_tab_2(&self, key: KeyCode) -> bool {
        self.matches(&self.tab_2, key)
    }

    pub fn matches_tab_3(&self, key: KeyCode) -> bool {
        self.matches(&self.tab_3, key)
    }

    pub fn matches_up_prev(&self, key: KeyCode) -> bool {
        self.matches(&self.up_prev, key) || key == KeyCode::Char('[')
    }

    pub fn matches_up_next(&self, key: KeyCode) -> bool {
        self.matches(&self.up_next, key) || key == KeyCode::Char(']')
    }

    pub fn get_nav_keys_display(&self) -> String {
        format!(
            "{}{}{}{}",
            self.nav_left, self.nav_up, self.nav_down, self.nav_right
        )
    }

    pub fn get_arrow_keys_display(&self) -> String {
        "←↑↓→".to_string()
    }

    /// Get all keybinding labels for display in settings
    pub fn get_all_labels(&self) -> Vec<(&'static str, &str)> {
        vec![
            // Global actions
            ("退出", &self.quit),
            ("确认", &self.confirm),
            ("返回", &self.back),
            ("刷新", &self.refresh),
            // Navigation
            ("向上", &self.nav_up),
            ("向下", &self.nav_down),
            ("向左", &self.nav_left),
            ("向右", &self.nav_right),
            ("下一页面", &self.nav_next_page),
            ("上一页面", &self.nav_prev_page),
            ("内容下翻页", &self.page_down),
            ("内容上翻页", &self.page_up),
            // Section/Tab
            ("上一分区", &self.section_prev),
            ("下一分区", &self.section_next),
            ("标签1", &self.tab_1),
            ("标签2", &self.tab_2),
            ("标签3", &self.tab_3),
            // Actions
            ("切换主题", &self.next_theme),
            ("播放", &self.play),
            ("设置", &self.open_settings),
            ("搜索", &self.search_focus),
            // Comments
            ("评论", &self.comment),
            ("展开回复", &self.toggle_replies),
            // Dynamic page
            ("上一UP", &self.up_prev),
            ("下一UP", &self.up_next),
        ]
    }

    /// Update a keybinding by index (for settings page)
    pub fn update_by_index(&mut self, index: usize, new_key: String) {
        match index {
            // Global actions
            0 => self.quit = new_key,
            1 => self.confirm = new_key,
            2 => self.back = new_key,
            3 => self.refresh = new_key,
            // Navigation
            4 => self.nav_up = new_key,
            5 => self.nav_down = new_key,
            6 => self.nav_left = new_key,
            7 => self.nav_right = new_key,
            8 => self.nav_next_page = new_key,
            9 => self.nav_prev_page = new_key,
            10 => self.page_down = new_key,
            11 => self.page_up = new_key,
            // Section/Tab
            12 => self.section_prev = new_key,
            13 => self.section_next = new_key,
            14 => self.tab_1 = new_key,
            15 => self.tab_2 = new_key,
            16 => self.tab_3 = new_key,
            // Actions
            17 => self.next_theme = new_key,
            18 => self.play = new_key,
            19 => self.open_settings = new_key,
            20 => self.search_focus = new_key,
            // Comments
            21 => self.comment = new_key,
            22 => self.toggle_replies = new_key,
            // Dynamic page
            23 => self.up_prev = new_key,
            24 => self.up_next = new_key,
            _ => {}
        }
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DanmakuConfig {
    pub enabled: bool,
    pub display_area: f64,
    pub opacity: f64,
    pub font_scale: f64,
    pub duration: f64,
    pub stroke_width: f64,
    pub line_height: f64,
    pub massive_mode: bool,
    pub font_family: String,
    /// Horizontal pixel offset for advanced (mode 7/8) danmaku.
    /// Positive shifts right, negative shifts left. 0 = official position.
    #[serde(default)]
    pub advanced_offset_x: f64,
    /// Vertical pixel offset for advanced danmaku.
    /// Positive shifts down, negative shifts up. 0 = official position.
    #[serde(default)]
    pub advanced_offset_y: f64,
    /// Uniform scale for advanced danmaku around the video center.
    /// 1.0 = official size/position, 0.9 pulls everything toward the center.
    #[serde(default = "default_one")]
    pub advanced_scale: f64,
    /// Text fragments (case-insensitive) that hide a danmaku entirely.
    /// Works for both video and live danmaku.
    #[serde(default)]
    pub blocked_keywords: Vec<String>,
    /// Sender UIDs to hide (live danmaku only — Bilibili's video danmaku
    /// feed does not carry a sender id).
    #[serde(default)]
    pub blocked_users: Vec<i64>,
    /// Live danmaku batch window in milliseconds. Higher values coalesce more
    /// messages per IPC write (less overhead under heavy chat), at the cost of
    /// slightly higher display latency. Range clamped to 16-1000 ms.
    #[serde(default = "default_sixteen")]
    pub live_batch_ms: u64,
    /// Per-type visibility toggles. None of these disable the renderer; they
    /// just skip enqueueing a given mode (1/4/5/6 scroll+top+bottom, 7/8
    /// advanced). All default to on.
    #[serde(default = "default_true")]
    pub show_roll: bool,
    #[serde(default = "default_true")]
    pub show_top: bool,
    #[serde(default = "default_true")]
    pub show_bottom: bool,
    #[serde(default = "default_true")]
    pub show_advanced: bool,
    /// Edge fade for rolling/top/bottom danmaku: 0 = off, 1 = full fade so
    /// comments are dim at the screen edges and brightest mid-flight. Pure
    /// cosmetic, applied on the Lua side.
    #[serde(default)]
    pub fade_edges: f64,
}

impl Default for DanmakuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            display_area: 0.85,
            opacity: 1.0,
            font_scale: 1.0,
            duration: 7.0,
            stroke_width: if cfg!(target_os = "macos") { 2.0 } else { 2.5 },
            line_height: 1.6,
            massive_mode: false,
            font_family: if cfg!(target_os = "macos") {
                "Yuanti SC"
            } else if cfg!(target_os = "windows") {
                "Microsoft YaHei UI"
            } else {
                "Noto Sans CJK SC"
            }
            .to_string(),
            advanced_offset_x: 0.0,
            advanced_offset_y: 0.0,
            advanced_scale: 1.0,
            blocked_keywords: Vec::new(),
            blocked_users: Vec::new(),
            live_batch_ms: 16,
            show_roll: true,
            show_top: true,
            show_bottom: true,
            show_advanced: true,
            fade_edges: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: String,
    pub keybindings: Keybindings,
    #[serde(default)]
    pub danmaku: DanmakuConfig,
    #[serde(default = "default_true")]
    pub auto_play: bool,
    /// mpv video output override. Empty/unset means mpv's default (external
    /// window); "kitty" / "tct" draw inside the terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mpv_vo: Option<String>,
    /// mpv hardware decoding override. Unset means auto-detect: NVIDIA GPUs
    /// default to "nvdec", everything else to "auto-safe". Set explicitly
    /// (e.g. "vaapi", "nvdec", "vulkan", "no") to force a mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mpv_hwdec: Option<String>,
    #[serde(default)]
    pub video_quality: VideoQuality,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "silkcircuit-neon".to_string(),
            keybindings: Keybindings::default(),
            danmaku: DanmakuConfig::default(),
            auto_play: true,
            mpv_vo: None,
            mpv_hwdec: None,
            video_quality: VideoQuality::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VideoQuality {
    #[default]
    Best,
    Q4k,
    Q1080pHigh,
    Q1080p,
    Q720p,
    Q480p,
    Q360p,
}

impl VideoQuality {
    pub const ALL: [Self; 7] = [
        Self::Best,
        Self::Q4k,
        Self::Q1080pHigh,
        Self::Q1080p,
        Self::Q720p,
        Self::Q480p,
        Self::Q360p,
    ];

    pub const fn qn(self) -> i64 {
        match self {
            Self::Best => 127,
            Self::Q4k => 120,
            Self::Q1080pHigh => 116,
            Self::Q1080p => 80,
            Self::Q720p => 64,
            Self::Q480p => 32,
            Self::Q360p => 16,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Best => "最佳",
            Self::Q4k => "4K",
            Self::Q1080pHigh => "1080P+/60",
            Self::Q1080p => "1080P",
            Self::Q720p => "720P",
            Self::Q480p => "480P",
            Self::Q360p => "360P",
        }
    }

    pub fn cycle(self, direction: i32) -> Self {
        let index = Self::ALL
            .iter()
            .position(|quality| *quality == self)
            .unwrap_or(0);
        if direction >= 0 {
            Self::ALL[(index + 1) % Self::ALL.len()]
        } else {
            Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
        }
    }

    pub const fn max_height(self) -> Option<u16> {
        match self {
            Self::Best => None,
            Self::Q4k => Some(2160),
            Self::Q1080pHigh | Self::Q1080p => Some(1080),
            Self::Q720p => Some(720),
            Self::Q480p => Some(480),
            Self::Q360p => Some(360),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_sixteen() -> u64 {
    16
}

fn default_one() -> f64 {
    1.0
}


/// Get the config directory path
fn get_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
        .join("bilibili-tui");

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    Ok(config_dir)
}

/// Get the credentials file path
fn get_credentials_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("credentials.json"))
}

/// Get the config file path
fn get_config_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("config.json"))
}

/// Save credentials to disk
pub fn save_credentials(credentials: &Credentials) -> Result<()> {
    let path = get_credentials_path()?;
    let json = serde_json::to_string_pretty(credentials)?;
    write_private_file(&path, json.as_bytes())?;
    Ok(())
}

/// Load credentials from disk
pub fn load_credentials() -> Result<Credentials> {
    let path = get_credentials_path()?;
    let json = fs::read_to_string(path)?;
    let credentials: Credentials = serde_json::from_str(&json)?;
    Ok(credentials)
}

/// Delete credentials (logout)
pub fn delete_credentials() -> Result<()> {
    let path = get_credentials_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Save app config to disk
pub fn save_config(config: &AppConfig) -> Result<()> {
    let path = get_config_path()?;
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

/// Load app config from disk
pub fn load_config() -> Result<AppConfig> {
    let path = get_config_path()?;
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let mut config: AppConfig = serde_json::from_str(&json)?;
        // Backward compatibility for dynamic UP navigation defaults:
        // old defaults were h/l, and a previous regression used [ and /.
        if (config.keybindings.up_prev == "h" && config.keybindings.up_next == "l")
            || (config.keybindings.up_prev == "[" && config.keybindings.up_next == "/")
        {
            config.keybindings.up_prev = "[".to_string();
            config.keybindings.up_next = "]".to_string();
        }
        Ok(config)
    } else {
        Ok(AppConfig::default())
    }
}

/// Export cookies in Netscape format for yt-dlp
pub fn export_cookies_for_ytdlp(credentials: &Credentials) -> Result<PathBuf> {
    let sequence = COOKIE_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = get_config_dir()?.join(format!("cookies-{}-{sequence}.txt", std::process::id()));

    let content = format!(
        "# Netscape HTTP Cookie File\n\
        .bilibili.com\tTRUE\t/\tTRUE\t0\tSESSDATA\t{}\n\
        .bilibili.com\tTRUE\t/\tFALSE\t0\tbili_jct\t{}\n\
        .bilibili.com\tTRUE\t/\tFALSE\t0\tDedeUserID\t{}\n",
        credentials.sessdata, credentials.bili_jct, credentials.dede_user_id
    );

    write_private_file(&path, content.as_bytes())?;
    Ok(path)
}

/// Remove a temporary cookie export. This is safe to call on error paths and
/// succeeds when the file has already been removed.
pub fn remove_cookie_export(path: &std::path::Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

const SEARCH_HISTORY_LIMIT: usize = 20;

fn get_search_history_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("search_history.json"))
}

/// Load previously searched keywords, newest first. Returns an empty vec when
/// the file does not exist or is malformed.
pub fn load_search_history() -> Vec<String> {
    let path = match get_search_history_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let Ok(json) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&json).unwrap_or_default()
}

/// Record a searched keyword. Deduplicates case-insensitively, keeps the most
/// recent entry first and caps the list at SEARCH_HISTORY_LIMIT entries.
pub fn save_search_history(keyword: &str) {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return;
    }
    let mut list = load_search_history();
    list.retain(|k| !k.eq_ignore_ascii_case(keyword));
    list.insert(0, keyword.to_string());
    list.truncate(SEARCH_HISTORY_LIMIT);
    let Ok(path) = get_search_history_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = fs::write(path, json);
    }
}

/// Remove every recorded search keyword. Used by the "clear history" action
/// in the search picker.
pub fn clear_search_history() {
    let Ok(path) = get_search_history_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string_pretty(&Vec::<String>::new()) {
        let _ = fs::write(path, json);
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn legacy_config_receives_default_danmaku_settings() {
        let value = serde_json::json!({
            "theme": "silkcircuit-neon",
            "keybindings": Keybindings::default(),
        });
        let config: AppConfig = serde_json::from_value(value).expect("legacy config");
        assert_eq!(config.danmaku, DanmakuConfig::default());
        assert_eq!(config.video_quality, VideoQuality::Best);
    }

    #[test]
    fn video_quality_has_stable_qn_mapping_and_round_trips() {
        assert_eq!(VideoQuality::Q4k.qn(), 120);
        assert_eq!(VideoQuality::Q1080pHigh.qn(), 116);
        let value = serde_json::to_value(VideoQuality::Q720p).unwrap();
        assert_eq!(value, "q720p");
        assert_eq!(
            serde_json::from_value::<VideoQuality>(value).unwrap(),
            VideoQuality::Q720p
        );
    }

    #[test]
    fn platform_default_danmaku_font_is_explicit() {
        let font = DanmakuConfig::default().font_family;
        if cfg!(target_os = "macos") {
            assert_eq!(font, "Yuanti SC");
        } else if cfg!(target_os = "windows") {
            assert_eq!(font, "Microsoft YaHei UI");
        } else {
            assert_eq!(font, "Noto Sans CJK SC");
        }
    }
}
