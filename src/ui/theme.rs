use opaline::OpalineColor;
use ratatui::style::Color;

pub const DEFAULT_THEME_ID: &str = "silkcircuit-neon";

const BUILTIN_THEME_IDS: [&str; 40] = [
    "silkcircuit-neon",
    "silkcircuit-soft",
    "silkcircuit-glow",
    "silkcircuit-vibrant",
    "silkcircuit-dawn",
    "catppuccin-mocha",
    "catppuccin-macchiato",
    "catppuccin-frappe",
    "catppuccin-latte",
    "dracula",
    "nord",
    "tokyo-night",
    "tokyo-night-storm",
    "tokyo-night-moon",
    "rose-pine",
    "rose-pine-moon",
    "rose-pine-dawn",
    "kanagawa-wave",
    "kanagawa-dragon",
    "kanagawa-lotus",
    "everforest-dark",
    "everforest-light",
    "gruvbox-dark",
    "gruvbox-light",
    "solarized-dark",
    "solarized-light",
    "one-dark",
    "one-light",
    "monokai-pro",
    "github-dark-dimmed",
    "github-light",
    "night-owl",
    "light-owl",
    "ayu-dark",
    "ayu-mirage",
    "ayu-light",
    "flexoki-dark",
    "flexoki-light",
    "palenight",
    "matugen",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeChoice {
    pub id: String,
    pub label: String,
}

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        return Some(Color::Rgb(r, g, b));
    }
    if s.len() == 8 {
        // #AARRGGBB — drop the alpha byte.
        let r = u8::from_str_radix(&s[2..4], 16).ok()?;
        let g = u8::from_str_radix(&s[4..6], 16).ok()?;
        let b = u8::from_str_radix(&s[6..8], 16).ok()?;
        return Some(Color::Rgb(r, g, b));
    }
    None
}


#[derive(Debug, Clone)]
pub struct Theme {
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_modal: Color,
    pub bg_card: Color,
    pub bg_highlight: Color,
    pub bg_overlay: Color,

    pub fg_primary: Color,
    pub fg_secondary: Color,
    pub fg_accent: Color,
    pub fg_muted: Color,

    pub border_focused: Color,
    pub border_unfocused: Color,
    pub border_subtle: Color,

    pub selection_bg: Color,
    pub selection_fg: Color,

    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    pub bilibili_pink: Color,
    pub bilibili_blue: Color,
    pub bilibili_cyan: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::load_or_default(DEFAULT_THEME_ID).0
    }

}

impl Theme {

    /// Matugen color file path (`~/.config/bilibili-tui/matugen.json`).
    pub fn matugen_path() -> std::path::PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        base.join("bilibili-tui").join("matugen.json")
    }

    fn from_matugen_or_default() -> Self {
        Self::from_matugen_file().unwrap_or_else(|| {
            let fallback = opaline::load_by_name(DEFAULT_THEME_ID).unwrap_or_default();
            Self::from_opaline(&fallback)
        })
    }

    /// Build a Theme from matugen's flattened `{role: "#hex"}` JSON.
    fn from_matugen_file() -> Option<Self> {
        let path = Self::matugen_path();
        let text = std::fs::read_to_string(path).ok()?;
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&text).ok()?;
        let c = |key: &str, fallback: Color| -> Color {
            map.get(key)
                .and_then(|v| v.as_str())
                .and_then(parse_hex)
                .unwrap_or(fallback)
        };
        let dflt = Self::default();
        let bg = c("background", dflt.bg_primary);
        let surface = c("surface", dflt.bg_secondary);
        let surface_container = c("surface_container", surface);
        let surface_high = c("surface_container_high", surface_container);
        let surface_highest = c("surface_container_highest", surface_high);
        let primary = c("primary", dflt.fg_accent);
        let secondary = c("secondary", dflt.bilibili_blue);
        let tertiary = c("tertiary", dflt.bilibili_pink);
        Some(Self {
            bg_primary: bg,
            bg_secondary: surface,
            bg_modal: surface_high,
            bg_card: surface_container,
            bg_highlight: surface_high,
            bg_overlay: c("surface_dim", surface),

            fg_primary: c("on_background", dflt.fg_primary),
            fg_secondary: c("on_surface_variant", dflt.fg_secondary),
            fg_accent: primary,
            fg_muted: c("on_surface_variant", dflt.fg_muted),

            border_focused: primary,
            border_unfocused: c("outline_variant", dflt.border_unfocused),
            border_subtle: c("outline_variant", dflt.border_subtle),

            // Selection: medium-dark container background + light text is far
            // more readable/comfortable than a bright primary fill.
            selection_bg: c("primary_container", primary),
            selection_fg: c("on_primary_container", dflt.selection_fg),

            success: c("tertiary", tertiary),
            warning: c("tertiary_container", tertiary),
            error: c("error", dflt.error),
            info: secondary,

            // Selection/brand accents: primary is the brightest material role
            // and keeps selected items clearly visible on dark backgrounds.
            bilibili_pink: primary,
            bilibili_blue: primary,
            bilibili_cyan: c("on_primary_container", secondary),
        })
    }
    pub fn available_theme_choices() -> Vec<ThemeChoice> {
        let mut choices: Vec<ThemeChoice> = BUILTIN_THEME_IDS
            .iter()
            .filter_map(|id| {
                opaline::load_by_name(id).map(|theme| ThemeChoice {
                    id: (*id).to_string(),
                    label: theme.meta.name.clone(),
                })
            })
            .collect();
        // matugen is dynamic (not an opaline theme), but it must be selectable
        // so opening/saving Settings does not silently reset it.
        choices.push(ThemeChoice {
            id: "matugen".to_string(),
            label: "Matugen (动态跟随壁纸)".to_string(),
        });
        choices
    }

    pub fn next_theme_id(current_theme_id: &str) -> String {
        if let Some(idx) = BUILTIN_THEME_IDS
            .iter()
            .position(|id| *id == current_theme_id)
        {
            let next = (idx + 1) % BUILTIN_THEME_IDS.len();
            BUILTIN_THEME_IDS[next].to_string()
        } else {
            BUILTIN_THEME_IDS[0].to_string()
        }
    }

    pub fn load(theme_id: &str) -> Option<Self> {
        if theme_id == "matugen" {
            // matugen is dynamic: the file may not exist yet, so always
            // resolve to *something* (matugen colors when available, otherwise
            // the default theme) and let the runtime keep polling for changes.
            return Some(Self::from_matugen_or_default());
        }
        opaline::load_by_name(theme_id).map(|theme| Self::from_opaline(&theme))
    }

    pub fn load_or_default(theme_id: &str) -> (Self, bool) {
        if theme_id == "matugen" {
            return (Self::from_matugen_or_default(), false);
        }
        match Self::load(theme_id) {
            Some(theme) => (theme, false),
            None => {
                let fallback = opaline::load_by_name(DEFAULT_THEME_ID).unwrap_or_default();
                (Self::from_opaline(&fallback), true)
            }
        }
    }

    fn from_opaline(theme: &opaline::Theme) -> Self {
        let bg_base = theme.color("bg.base");

        Self {
            bg_primary: Self::to_ratatui(bg_base),
            bg_secondary: Self::color(theme, "bg.panel"),
            bg_modal: Self::color(theme, "bg.code"),
            bg_card: Self::color(theme, "bg.panel"),
            bg_highlight: Self::color(theme, "bg.highlight"),
            bg_overlay: Self::to_ratatui(bg_base.darken(0.2)),

            fg_primary: Self::color(theme, "text.primary"),
            fg_secondary: Self::color(theme, "text.secondary"),
            fg_accent: Self::color(theme, "accent.primary"),
            fg_muted: Self::color(theme, "text.muted"),

            border_focused: Self::color(theme, "border.focused"),
            border_unfocused: Self::color(theme, "border.unfocused"),
            border_subtle: Self::color(theme, "text.dim"),

            selection_bg: Self::color(theme, "bg.selection"),
            selection_fg: Self::color(theme, "text.primary"),

            success: Self::color(theme, "success"),
            warning: Self::color(theme, "warning"),
            error: Self::color(theme, "error"),
            info: Self::color(theme, "info"),

            bilibili_pink: Self::color(theme, "accent.primary"),
            bilibili_blue: Self::color(theme, "accent.secondary"),
            bilibili_cyan: Self::color(theme, "accent.tertiary"),
        }
    }

    fn color(theme: &opaline::Theme, token: &str) -> Color {
        Self::to_ratatui(theme.color(token))
    }

    fn to_ratatui(color: OpalineColor) -> Color {
        Color::Rgb(color.r, color.g, color.b)
    }
}
