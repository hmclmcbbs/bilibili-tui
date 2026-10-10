//! Application-level playback queue state.

/// Per-video playback preferences (quality / HDR / Hi-Res).
///
/// `quality` is the Bilibili `qn` value; 0 means auto (use the best stream
/// the server returns). HDR and Hi-Res are preference flags applied while
/// selecting video/audio streams from the playurl response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackOptions {
    pub quality: i64,
    pub prefer_hdr: bool,
    pub prefer_hires: bool,
}

/// Anime4K 档位 (设置页超分栏目调; A=官方 CTRL+1 …)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anime4kLevel {
    /// 1080p 动画: Restore_L → Upscale×2 (线稿重建)。
    #[default]
    A,
    /// 720p 动画: Restore_Soft_L → Upscale×2 (去振铃/抗锯齿)。
    B,
    /// 480p/无损图源: Upscale_Denoise_L (高保真轻处理)。
    C,
}

impl Anime4kLevel {
    pub fn next(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::C,
            Self::C => Self::A,
        }
    }

    /// 状态行补充标签 ("A·1080p动画" 等)。
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A·1080p动画",
            Self::B => "B·720p动画",
            Self::C => "C·480p高保真",
        }
    }
}

/// NNEDI3 神经元数量档 (越大越强越贵, 官方 hook 覆盖 16-256)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Nnedi3Nns {
    N16,
    N32,
    #[default]
    N64,
    N128,
    N256,
}

impl Nnedi3Nns {
    pub fn next(self) -> Self {
        use Nnedi3Nns::*;
        match self {
            N16 => N32,
            N32 => N64,
            N64 => N128,
            N128 => N256,
            N256 => N16,
        }
    }

    pub fn as_u32(self) -> u32 {
        use Nnedi3Nns::*;
        match self {
            N16 => 16,
            N32 => 32,
            N64 => 64,
            N128 => 128,
            N256 => 256,
        }
    }

    pub fn label(self) -> &'static str {
        use Nnedi3Nns::*;
        match self {
            N16 => "16 (最快)",
            N32 => "32",
            N64 => "64 (默认)",
            // 实测 (4060, 1080p→1600p 全屏 1.33×): 放宽门控后 2× 预缩
            // 中间帧 3840×2160, nns≥128 过载卡死 (ratio≈0.002, GPU 75%),
            // nns64 满速 49% — 重档标注场景限制, 低倍上缩请留 nns≤64。
            N128 => "128 (1.5×+场景)",
            N256 => "256 (≥1.5×专用)",
        }
    }
}

/// NNEDI3 邻域窗口 (8x4 水平窗 / 8x6 全窗; 全窗质量更好略贵)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nnedi3Window {
    #[default]
    Win8x4,
    Win8x6,
}

impl Nnedi3Window {
    pub fn next(self) -> Self {
        match self {
            Self::Win8x4 => Self::Win8x6,
            Self::Win8x6 => Self::Win8x4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Win8x4 => "8x4 (轻)",
            Self::Win8x6 => "8x6 (重)",
        }
    }
}

/// FSRCNNX 滤镜数 (8=轻量档, 16=官方默认重档)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FsrcnnxFilters {
    #[default]
    F16,
    F8,
}

impl FsrcnnxFilters {
    pub fn next(self) -> Self {
        match self {
            Self::F16 => Self::F8,
            Self::F8 => Self::F16,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::F16 => "16 (默认)",
            Self::F8 => "8 (轻量)",
        }
    }
}

/// RAVU 变体色彩模式 (yuv=YUV 平面直算, rgb=RGB 域, plain=亮度)。
/// 默认 yuv: bjin 官方 README 推荐视频播放用 yuv 变体 (色度同步超分,
/// 比 plain 亮度-only 锐利, 比 rgb 少一次域转换)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RavuVariant {
    #[default]
    Yuv,
    Rgb,
    Plain,
}

impl RavuVariant {
    pub fn next(self) -> Self {
        use RavuVariant::*;
        match self {
            Yuv => Rgb,
            Rgb => Plain,
            Plain => Yuv,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Yuv => "YUV (默认)",
            Self::Rgb => "RGB",
            Self::Plain => "亮度",
        }
    }
}

/// 超分辨率算法 (详情页 `e` 循环切换, 持久化于
/// `AppConfig::super_res.algorithm`; 着色器清单见 player::super_res_files)。
///
/// 12 种算法全部为 mpv 原生 GLSL hook (//!HOOK 格式, gpu-next/Vulkan 直接
/// 加载): 动画向 CNN (Anime4K/FSRCNNX/NNEDI3/RAVU)、通用实时 SR
/// (FSR/NIS)、自适应锐化 (CAS/AdaptiveSharpen/LumaSharpen) 与混合管线
/// (Anime4K-Ultra)。关闭 (Off) 时零参数, 由 mpv 内建 scaler 处理。
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SuperResAlgorithm {
    /// 不启用 (mpv 内建 spline36/lanczos 缩放)。
    #[default]
    Off,
    /// Anime4K 官方 CNN (档位 A/B/C, 设置页调; 动画线稿修复)。
    Anime4k,
    /// NNEDI3 神经元插值 (nns 16-256, 窗口 8x4/8x6; 动漫/低清最强重
    /// 建, 也最贵)。
    Nnedi3,
    /// RAVU-r2 轻量学习型超分 (动画向, 中等成本)。
    RavuR2,
    /// RAVU-r4 新一代 (同族质量/成本比最优)。
    RavuR4,
    /// RAVU-lite 最轻量档 (低消耗场景/低端 GPU)。
    RavuLite,
    /// FSRCNNX (igv 手调 x2 CNN, 细节重建, 8/16 滤镜可调)。
    Fsrcnnx,
    /// AMD FidelityFX Super Resolution 1.0 (EASU+RCAS, 通用内容)。
    Fsr,
    /// AMD CAS 对比度自适应锐化 (1:1/上缩锐化, 极低成本)。
    Cas,
    /// AdaptiveSharpen (动态裁剪锐化, 保边缘不振铃)。
    AdaptiveSharpen,
    /// NVIDIA Image Scaling (NIS, 通用锐化+上缩)。
    Nis,
    /// LumaSharpen (经典亮度锐化, 最廉价)。
    LumaSharpen,
    /// Anime4K-Ultra (FSR v1.0.2 + 改良 Anime4K 线细化混合管线)。
    Anime4kUltra,
}

impl SuperResAlgorithm {
    /// Off → 12 算法 → Off 循环。
    pub fn next(self) -> Self {
        use SuperResAlgorithm::*;
        match self {
            Off => Anime4k,
            Anime4k => Nnedi3,
            Nnedi3 => RavuR2,
            RavuR2 => RavuR4,
            RavuR4 => RavuLite,
            RavuLite => Fsrcnnx,
            Fsrcnnx => Fsr,
            Fsr => Cas,
            Cas => AdaptiveSharpen,
            AdaptiveSharpen => Nis,
            Nis => LumaSharpen,
            LumaSharpen => Anime4kUltra,
            Anime4kUltra => Off,
        }
    }

    /// 反向循环 (设置页 ← 键)。
    pub fn prev(self) -> Self {
        use SuperResAlgorithm::*;
        match self {
            Off => Anime4kUltra,
            Anime4k => Off,
            Nnedi3 => Anime4k,
            RavuR2 => Nnedi3,
            RavuR4 => RavuR2,
            RavuLite => RavuR4,
            Fsrcnnx => RavuLite,
            Fsr => Fsrcnnx,
            Cas => Fsr,
            AdaptiveSharpen => Cas,
            Nis => AdaptiveSharpen,
            LumaSharpen => Nis,
            Anime4kUltra => LumaSharpen,
        }
    }

    /// 状态显示用短标签。
    pub fn label(self) -> &'static str {
        use SuperResAlgorithm::*;
        match self {
            Off => "关",
            Anime4k => "Anime4K",
            Nnedi3 => "NNEDI3",
            RavuR2 => "RAVU-r2",
            RavuR4 => "RAVU-r4",
            RavuLite => "RAVU-lite",
            Fsrcnnx => "FSRCNNX",
            Fsr => "FSR",
            Cas => "CAS",
            AdaptiveSharpen => "ASharpen",
            Nis => "NIS",
            LumaSharpen => "LumaS",
            Anime4kUltra => "A4K-Ultra",
        }
    }

    /// 每算法一句说明 (详情页状态行内联展示)。
    pub fn desc(self) -> &'static str {
        use SuperResAlgorithm::*;
        match self {
            Off => "",
            Anime4k => "动画·线稿重建CNN",
            Nnedi3 => "动漫·最强重建(贵)",
            RavuR2 => "动画·轻量CNN",
            RavuR4 => "动画·新一代CNN",
            RavuLite => "动画·极轻量CNN",
            Fsrcnnx => "通用·细节重建CNN",
            Fsr => "通用·AMD实时SR",
            Cas => "通用·自适应锐化",
            AdaptiveSharpen => "保边·动态锐化",
            Nis => "通用·NVIDIA锐化",
            LumaSharpen => "轻量·亮度锐化",
            Anime4kUltra => "混合·FSR+线细化",
        }
    }

    /// 是否需要着色器文件 (Off 恒 false; 其余 true — 缺文件时播放静默
    /// 降级, 见 player::super_res_shader_args)。
    pub fn needs_shaders(self) -> bool {
        self != Self::Off
    }
}

#[cfg(test)]
mod super_res_tests {
    use super::SuperResAlgorithm as S;

    #[test]
    fn super_res_algorithm_cycles_through_all_thirteen_states() {
        let mut cur = S::Off;
        let mut seen = vec![cur];
        for _ in 0..12 {
            cur = cur.next();
            seen.push(cur);
        }
        assert_eq!(cur.next(), S::Off, "12 算法后必须回到 Off");
        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), seen.len(), "循环中出现重复状态");
        assert_eq!(seen.len(), 13);
        // prev 与 next 互逆。
        for s in seen {
            assert_eq!(s.next().prev(), s);
            assert_eq!(s.prev().next(), s);
        }
    }

    #[test]
    fn super_res_labels_and_descs_are_distinct() {
        let all = [
            S::Off,
            S::Anime4k,
            S::Nnedi3,
            S::RavuR2,
            S::RavuR4,
            S::RavuLite,
            S::Fsrcnnx,
            S::Fsr,
            S::Cas,
            S::AdaptiveSharpen,
            S::Nis,
            S::LumaSharpen,
            S::Anime4kUltra,
        ];
        let labels: std::collections::HashSet<_> = all.iter().map(|s| s.label()).collect();
        assert_eq!(labels.len(), all.len());
        let descs: std::collections::HashSet<_> = all
            .iter()
            .filter(|s| **s != S::Off)
            .map(|s| s.desc())
            .collect();
        assert_eq!(descs.len(), all.len() - 1);
        assert_eq!(S::Off.desc(), "");
        assert!(!S::Off.needs_shaders());
        assert!(S::Anime4k.needs_shaders());
    }

    #[test]
    fn super_res_algorithm_serde_uses_snake_case() {
        assert_eq!(
            serde_json::from_str::<S>("\"anime4k_ultra\"").unwrap(),
            S::Anime4kUltra
        );
        assert_eq!(serde_json::from_str::<S>("\"off\"").unwrap(), S::Off);
        assert_eq!(serde_json::to_string(&S::RavuR2).unwrap(), "\"ravu_r2\"");
    }
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            quality: 0,
            prefer_hdr: false,
            prefer_hires: false,
        }
    }
}

impl PlaybackOptions {
    /// Bilibili quality label for a given qn value.
    pub fn quality_label(qn: i64) -> &'static str {
        match qn {
            0 => "自动",
            127 => "8K",
            126 => "杜比视界",
            125 => "HDR",
            120 => "4K",
            116 => "1080P60",
            112 => "1080P+",
            80 => "1080P",
            64 => "720P",
            32 => "480P",
            16 => "360P",
            _ => "未知",
        }
    }

    /// Quality cycle used by the detail page `m` key.
    pub fn cycle_quality(&mut self) {
        const CYCLE: [i64; 8] = [0, 120, 116, 112, 80, 64, 32, 16];
        let next = CYCLE
            .iter()
            .position(|&qn| qn == self.quality)
            .map(|idx| CYCLE[(idx + 1) % CYCLE.len()])
            .unwrap_or(0);
        self.quality = next;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistItem {
    pub bvid: String,
    pub aid: i64,
    pub cid: Option<i64>,
    pub title: String,
    pub uploader_mid: Option<i64>,
    pub duration: Option<i64>,
    pub page: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlayOrder {
    #[default]
    Forward,
    Reverse,
    Shuffle,
}

/// What happens when playback reaches the end of a video / playlist.
///
/// Persisted in `AppConfig::playback_loop` (toggled with `l` on the UP page);
/// every playback entry point reads the config value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayLoop {
    /// Play to the end of the list, then stop (single videos auto-advance
    /// through their source list first — see `decide_auto_next`).
    #[default]
    Stop,
    /// Restart the list from the beginning when it ends.
    List,
    /// Repeat the current item forever (implemented as mpv `--loop-file=inf`).
    Item,
}

/// 补帧方法 (视频详情页 `i` 循环切换, 持久化于 `AppConfig::interpolation_mode`)。
///
/// 8 态: 关 → 混合 (mitchell) → 5 种 mpv 时间插值核 (第8轮新增, 实测
/// 满速) → Smooth Motion → 关。除 Off/Blend/SmoothMotion 外的变体各自
/// 固定一个 `--tscale` 核 (见 [`InterpolationMode::tscale`]), 与 Blend
/// 共用 `--interpolation=yes` + display-resample 链。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterpolationMode {
    /// 不补帧 (仅 display-resample 保持 vsync 节奏, 内容仍按源帧率步进)。
    #[default]
    Off,
    /// mpv 时间混合插值 (display-resample + --interpolation/--tscale)。
    /// 零额外依赖; 相邻帧加权平均, 快速运动有轻微拖影。核由手改
    /// `interpolation_tscale` 决定 (默认 mitchell, 白名单校验)。
    /// alias: 旧配置值 "rife" (光流功能已移除) 映射为混合, 旧配置可读。
    #[serde(alias = "rife")]
    Blend,
    /// 时间核 sinc — 最锐利, 运动边缘振铃风险最高。
    Sinc,
    /// 时间核 lanczos — 锐利与平滑均衡, 轻微振铃。
    Lanczos,
    /// 时间核 gaussian — 最平滑, 无振铃, 拖影略多。
    Gaussian,
    /// 时间核 catmull_rom — 中等锐度, 线条/动画向观感。
    CatmullRom,
    /// 时间核 oversample — 时域超采样, 零拖影零鬼影 (只平滑呈现节奏,
    /// 内容混合最轻)。
    Oversample,
    /// NVIDIA Smooth Motion: 驱动级插帧。启用环境变量
    /// `NVPRESENT_ENABLE_SMOOTH_MOTION=1` 加载 VK_LAYER_NV_present
    /// 隐式 Vulkan 层, 由驱动 AI 在呈现层补帧 (RTX 40 系+, 仅 Vulkan;
    /// Wayland 下 mpv 自动选 vulkan)。mpv 自身不叠加插值, 避免双重补帧。
    SmoothMotion,
}

impl InterpolationMode {
    /// Off → Blend → Sinc → Lanczos → Gaussian → CatmullRom → Oversample
    /// → SmoothMotion → Off 循环。
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Blend,
            Self::Blend => Self::Sinc,
            Self::Sinc => Self::Lanczos,
            Self::Lanczos => Self::Gaussian,
            Self::Gaussian => Self::CatmullRom,
            Self::CatmullRom => Self::Oversample,
            Self::Oversample => Self::SmoothMotion,
            Self::SmoothMotion => Self::Off,
        }
    }

    /// `next` 的逆 (设置页 ← 键回退)。
    pub fn prev(self) -> Self {
        match self {
            Self::Off => Self::SmoothMotion,
            Self::Blend => Self::Off,
            Self::Sinc => Self::Blend,
            Self::Lanczos => Self::Sinc,
            Self::Gaussian => Self::Lanczos,
            Self::CatmullRom => Self::Gaussian,
            Self::Oversample => Self::CatmullRom,
            Self::SmoothMotion => Self::Oversample,
        }
    }

    /// 固定时间核 (仅 5 种新方法返回 Some; Blend 核手改配置, Off/Smooth
    /// Motion 无核)。
    pub fn tscale(self) -> Option<&'static str> {
        match self {
            Self::Sinc => Some("sinc"),
            Self::Lanczos => Some("lanczos"),
            Self::Gaussian => Some("gaussian"),
            Self::CatmullRom => Some("catmull_rom"),
            Self::Oversample => Some("oversample"),
            Self::Off | Self::Blend | Self::SmoothMotion => None,
        }
    }

    /// 状态行/设置行标签 ("关"/"混合"/"sinc·最锐"/…/"Smooth Motion")。
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "关",
            Self::Blend => "混合",
            Self::Sinc => "sinc·最锐",
            Self::Lanczos => "lanczos·锐利",
            Self::Gaussian => "gaussian·平滑",
            Self::CatmullRom => "catmullrom·均衡",
            Self::Oversample => "oversample·零拖影",
            Self::SmoothMotion => "Smooth Motion",
        }
    }
}

impl PlayLoop {
    pub fn next(self) -> Self {
        match self {
            PlayLoop::Stop => PlayLoop::List,
            PlayLoop::List => PlayLoop::Item,
            PlayLoop::Item => PlayLoop::Stop,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PlayLoop::Stop => "播完停止",
            PlayLoop::List => "列表循环",
            PlayLoop::Item => "单曲循环",
        }
    }
}

/// One video a list page can hand to the auto-continue chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoNextTarget {
    pub bvid: String,
    pub aid: i64,
}

/// Where the current video sits inside a list page's loaded items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoNextOutcome {
    /// The finished video is not part of this list (or the page has no
    /// suitable list): auto-continue must not use this page.
    NotMember,
    /// Another video follows the finished one.
    Next(AutoNextTarget),
    /// The finished video was the last one. `first` (when present and
    /// different from the finished video) is where a List-loop wraps to.
    End { first: Option<AutoNextTarget> },
}

impl AutoNextOutcome {
    /// Build the outcome from a list page's loaded targets, in display
    /// order. Targets without ids (ads, user cards, …) are skipped.
    pub fn from_targets(
        targets: impl IntoIterator<Item = Option<(String, i64)>>,
        finished_bvid: &str,
    ) -> Self {
        let list: Vec<(String, i64)> = targets.into_iter().flatten().collect();
        match list.iter().position(|(bvid, _)| bvid == finished_bvid) {
            None => AutoNextOutcome::NotMember,
            Some(index) if index + 1 < list.len() => AutoNextOutcome::Next(AutoNextTarget {
                bvid: list[index + 1].0.clone(),
                aid: list[index + 1].1,
            }),
            Some(_) => AutoNextOutcome::End {
                first: list.first().map(|(bvid, aid)| AutoNextTarget {
                    bvid: bvid.clone(),
                    aid: *aid,
                }),
            },
        }
    }
}

/// The next hop of the single-video auto-continue chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoHop {
    /// Play the next part of the current multi-part video.
    NextPart(usize),
    /// Open (and auto-play) the next video from the source list.
    NewVideo(AutoNextTarget),
}

/// Decide whether a naturally-finished single video should auto-continue.
///
/// Pure so the mode matrix is unit-testable. Guards:
/// * The loop mode is `Item` → no chain (Item is handled by mpv
///   `--loop-file=inf`; a Finished there means the user quit).
///
/// NOTE: deliberately NOT gated on `config.auto_play` — that setting means
/// "autoplay when a detail page is opened"; auto-continue happens after the
/// user already watched something and must work with auto-play off (the
/// chain forces the next page itself via `chain_play`).
/// * `dwell_ok` false (video ended suspiciously fast, e.g. history seek
///   landed at the very end) → no chain, so already-watched lists cannot
///   be ripped through in a storm.
/// * Multi-part videos advance part-by-part first; only after the last
///   part does the source list decide (next video / List-wrap / stop).
pub fn decide_auto_next(
    loop_mode: PlayLoop,
    dwell_ok: bool,
    next_part: Option<usize>,
    origin: &AutoNextOutcome,
    finished_bvid: &str,
) -> Option<AutoHop> {
    if !dwell_ok || loop_mode == PlayLoop::Item {
        return None;
    }
    if let Some(index) = next_part {
        return Some(AutoHop::NextPart(index));
    }
    match origin {
        AutoNextOutcome::Next(target) => Some(AutoHop::NewVideo(target.clone())),
        AutoNextOutcome::End { first: Some(first) }
            if loop_mode == PlayLoop::List && first.bvid != finished_bvid =>
        {
            Some(AutoHop::NewVideo(first.clone()))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistSource {
    Manual,
    Uploader { mid: i64, name: String },
    Favorites { media_id: i64, title: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackStatus {
    #[default]
    Idle,
    Starting,
    Playing,
    Paused,
    Failed,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackEvent {
    ItemChanged {
        session_id: u64,
        index: usize,
        bvid: String,
    },
    Started {
        session_id: u64,
        bvid: Option<String>,
        cid: Option<i64>,
        success: bool,
        error: Option<String>,
    },
    Finished {
        session_id: u64,
        bvid: Option<String>,
        /// True only when mpv reached a real end-of-file. `q` / errors also
        /// end a session with a Finished event — those must not trigger the
        /// auto-continue chain (quitting means "stop watching").
        natural_end: bool,
    },
    Failed {
        session_id: u64,
        error: String,
    },
}

#[derive(Debug, Default)]
pub struct PlaybackState {
    pub queue: Vec<PlaylistItem>,
    pub current_index: Option<usize>,
    pub order: PlayOrder,
    pub source: Option<PlaylistSource>,
    pub status: PlaybackStatus,
    pub last_error: Option<String>,
    pub session_id: Option<u64>,
}

impl PlaybackState {
    pub fn replace_queue(&mut self, source: PlaylistSource, items: Vec<PlaylistItem>) {
        self.queue = items;
        self.source = Some(source);
        self.current_index = (!self.queue.is_empty()).then_some(0);
        self.status = PlaybackStatus::Idle;
        self.last_error = None;
        self.session_id = None;
    }

    pub fn begin_session(&mut self, session_id: u64) {
        self.session_id = Some(session_id);
        self.status = PlaybackStatus::Playing;
        self.last_error = None;
    }

    /// Whether an mpv session is currently active and owns the terminal input.
    ///
    /// While this is true the TUI must not poll/read keyboard events, otherwise
    /// keys like `q` or `Space` would be consumed by both mpv (which inherits
    /// our stdin) and the TUI, causing mpv to quit AND the TUI to navigate away.
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            PlaybackStatus::Starting | PlaybackStatus::Playing | PlaybackStatus::Paused
        )
    }

    pub fn apply_event(&mut self, event: &PlaybackEvent) -> bool {
        let session_id = match event {
            PlaybackEvent::ItemChanged { session_id, .. }
            | PlaybackEvent::Started { session_id, .. }
            | PlaybackEvent::Finished { session_id, .. }
            | PlaybackEvent::Failed { session_id, .. } => *session_id,
        };
        if self.session_id != Some(session_id) {
            return false;
        }
        match event {
            PlaybackEvent::ItemChanged { index, bvid, .. } => {
                self.current_index = if self
                    .queue
                    .get(*index)
                    .is_some_and(|item| item.bvid == *bvid)
                {
                    Some(*index)
                } else {
                    self.queue.iter().position(|item| item.bvid == *bvid)
                };
            }
            PlaybackEvent::Started { success, error, .. } => {
                if *success {
                    self.status = PlaybackStatus::Playing;
                    self.last_error = None;
                } else {
                    self.status = PlaybackStatus::Failed;
                    self.last_error = error.clone();
                    self.session_id = None;
                }
            }
            PlaybackEvent::Finished { .. } => {
                self.status = PlaybackStatus::Finished;
                self.session_id = None;
            }
            PlaybackEvent::Failed { error, .. } => {
                self.status = PlaybackStatus::Failed;
                self.last_error = Some(error.clone());
                self.session_id = None;
            }
        }
        true
    }

    pub fn play_from(&mut self, index: usize) -> bool {
        if index >= self.queue.len() {
            return false;
        }
        self.current_index = Some(index);
        self.status = PlaybackStatus::Starting;
        self.last_error = None;
        true
    }

    pub fn advance(&mut self) -> bool {
        let Some(current) = self.current_index else {
            return false;
        };
        let next = match self.order {
            PlayOrder::Forward => current
                .checked_add(1)
                .filter(|next| *next < self.queue.len()),
            PlayOrder::Reverse => current.checked_sub(1),
            PlayOrder::Shuffle => current
                .checked_add(1)
                .filter(|next| *next < self.queue.len()),
        };
        if let Some(next) = next {
            self.current_index = Some(next);
            self.status = PlaybackStatus::Starting;
            true
        } else {
            self.status = PlaybackStatus::Finished;
            false
        }
    }
}

/// Anime4K 增强模式 (详情页 `e` 循环切换, 持久化于
/// `AppConfig::anime4k_mode`; 着色器清单见 player::anime4k_mode_files)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anime4kMode {
    /// 不启用。
    #[default]
    Off,
    /// Mode A (官方 CTRL+1): Restore→Upscale×2 — 优化 1080p 动画。
    A,
    /// Mode B (官方 CTRL+2): Restore_Soft→Upscale×2 — 优化 720p/低模糊源。
    B,
    /// Mode C (官方 CTRL+3): Upscale_Denoise→Upscale — 480p/无损图源。
    C,
}

impl Anime4kMode {
    /// Off → A → B → C → Off 循环。
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::A,
            Self::A => Self::B,
            Self::B => Self::C,
            Self::C => Self::Off,
        }
    }

    /// 状态显示用短标签 ("关"/"A"/"B"/"C")。
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "关",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }

    /// 每模式说明 (播放选项块内联展示, 定位取自官方 Modes 文档):
    /// - A: 1080p 动画, 重建退化线稿, 感知质量最高 (副作用也最明显)
    /// - B: 720p 动画, 去振铃/抗锯齿, 中度修复
    /// - C: 480p/无损图, 最高 PSNR, 只降噪不重修线条
    pub fn desc(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::A => "1080p动画·线稿重建",
            Self::B => "720p动画·去振铃抗锯齿",
            Self::C => "480p·高保真轻处理",
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn anime4k_mode_cycles() {
        use super::Anime4kMode as M;
        assert_eq!(M::Off.next(), M::A);
        assert_eq!(M::A.next(), M::B);
        assert_eq!(M::B.next(), M::C);
        assert_eq!(M::C.next(), M::Off);
        assert_eq!(M::A.label(), "A");
        assert_eq!(M::A.desc(), "1080p动画·线稿重建");
        assert_eq!(M::B.desc(), "720p动画·去振铃抗锯齿");
        assert_eq!(M::C.desc(), "480p·高保真轻处理");
        assert_eq!(M::Off.desc(), "");
        assert_eq!(serde_json::from_str::<M>("\"c\"").unwrap(), M::C);
    }

    #[test]
    fn legacy_rife_value_deserializes_to_blend() {
        // 光流已移除: 旧配置里的 "rife" 不应让整个配置反序列化失败。
        let mode: super::InterpolationMode = serde_json::from_str("\"rife\"").unwrap();
        assert_eq!(mode, super::InterpolationMode::Blend);
        assert_eq!(serde_json::to_string(&mode).unwrap(), "\"blend\"");
    }

    use super::*;

    fn item(id: i64) -> PlaylistItem {
        PlaylistItem {
            bvid: format!("BV{id}"),
            aid: id,
            cid: Some(id),
            title: format!("video {id}"),
            uploader_mid: Some(1),
            duration: Some(60),
            page: None,
        }
    }

    #[test]
    fn replacing_queue_resets_position_and_error() {
        let mut state = PlaybackState {
            last_error: Some("old error".into()),
            ..PlaybackState::default()
        };
        state.replace_queue(PlaylistSource::Manual, vec![item(1), item(2)]);
        assert_eq!(state.current_index, Some(0));
        assert_eq!(state.last_error, None);
    }

    #[test]
    fn forward_queue_finishes_after_last_item() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1), item(2)]);
        assert!(state.advance());
        assert_eq!(state.current_index, Some(1));
        assert!(!state.advance());
        assert_eq!(state.status, PlaybackStatus::Finished);
    }

    #[test]
    fn play_from_rejects_out_of_bounds_index() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1)]);
        assert!(!state.play_from(1));
        assert_eq!(state.current_index, Some(0));
    }

    #[test]
    fn ignores_events_from_an_old_session() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1), item(2)]);
        state.begin_session(2);
        assert!(!state.apply_event(&PlaybackEvent::Finished {
            session_id: 1,
            bvid: None,
            natural_end: false,
        }));
        assert_eq!(state.status, PlaybackStatus::Playing);
        assert!(state.apply_event(&PlaybackEvent::ItemChanged {
            session_id: 2,
            index: 1,
            bvid: "BV2".into()
        }));
        assert_eq!(state.current_index, Some(1));
    }

    #[test]
    fn failed_player_marks_active_session_failed() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1)]);
        state.begin_session(7);
        assert!(state.apply_event(&PlaybackEvent::Failed {
            session_id: 7,
            error: "decoder failed".into(),
        }));
        assert_eq!(state.status, PlaybackStatus::Failed);
        assert_eq!(state.last_error.as_deref(), Some("decoder failed"));
        assert_eq!(state.session_id, None);
    }

    #[test]
    fn shuffled_queue_advances_in_its_materialized_order() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1), item(2)]);
        state.order = PlayOrder::Shuffle;
        assert!(state.advance());
        assert_eq!(state.current_index, Some(1));
        assert!(!state.advance());
    }

    #[test]
    fn play_loop_cycles_and_labels() {
        assert_eq!(PlayLoop::default(), PlayLoop::Stop);
        assert_eq!(PlayLoop::Stop.next(), PlayLoop::List);
        assert_eq!(PlayLoop::List.next(), PlayLoop::Item);
        assert_eq!(PlayLoop::Item.next(), PlayLoop::Stop);
        assert_eq!(PlayLoop::Stop.label(), "播完停止");
        assert_eq!(PlayLoop::List.label(), "列表循环");
        assert_eq!(PlayLoop::Item.label(), "单曲循环");
    }

    fn target(id: i64) -> AutoNextTarget {
        AutoNextTarget {
            bvid: format!("BV{id}"),
            aid: id,
        }
    }

    fn origin_of(ids: &[i64], finished: &str) -> AutoNextOutcome {
        AutoNextOutcome::from_targets(
            ids.iter().map(|id| Some((format!("BV{id}"), *id))),
            finished,
        )
    }

    #[test]
    fn auto_next_outcome_walks_then_ends_then_wraps() {
        assert_eq!(
            origin_of(&[1, 2, 3], "BV2"),
            AutoNextOutcome::Next(target(3))
        );
        assert_eq!(
            origin_of(&[1, 2, 3], "BV3"),
            AutoNextOutcome::End {
                first: Some(target(1))
            }
        );
        assert_eq!(origin_of(&[1, 2, 3], "BV9"), AutoNextOutcome::NotMember);
        // Non-video cards (None entries) are skipped when building the list.
        let mixed = AutoNextOutcome::from_targets(
            vec![Some(("BV1".into(), 1)), None, Some(("BV2".into(), 2))],
            "BV1",
        );
        assert_eq!(mixed, AutoNextOutcome::Next(target(2)));
    }

    #[test]
    fn decide_auto_next_mode_matrix() {
        let end = AutoNextOutcome::End {
            first: Some(target(1)),
        };
        let next = AutoNextOutcome::Next(target(2));
        // Stop: advance through the list, stop at its end.
        assert_eq!(
            decide_auto_next(PlayLoop::Stop, true, None, &next, "BV1"),
            Some(AutoHop::NewVideo(target(2)))
        );
        assert_eq!(
            decide_auto_next(PlayLoop::Stop, true, None, &end, "BV1"),
            None
        );
        // List: wrap to the first video at the end.
        assert_eq!(
            decide_auto_next(PlayLoop::List, true, None, &end, "BV3"),
            Some(AutoHop::NewVideo(target(1)))
        );
        // Never wrap a single-item list into itself (seek-to-end storm).
        assert_eq!(
            decide_auto_next(PlayLoop::List, true, None, &end, "BV1"),
            None
        );
        // Item: mpv loops the file; a Finished is a user quit → no chain.
        assert_eq!(
            decide_auto_next(PlayLoop::Item, true, None, &next, "BV1"),
            None
        );
        // Suspiciously short dwell / not a member.
        assert_eq!(
            decide_auto_next(PlayLoop::Stop, false, None, &next, "BV1"),
            None
        );
        assert_eq!(
            decide_auto_next(
                PlayLoop::Stop,
                true,
                None,
                &AutoNextOutcome::NotMember,
                "BV1"
            ),
            None
        );
        // Multi-part videos advance part-by-part before consulting the list.
        assert_eq!(
            decide_auto_next(PlayLoop::Stop, true, Some(2), &next, "BV1"),
            Some(AutoHop::NextPart(2))
        );
        assert_eq!(
            decide_auto_next(PlayLoop::List, true, Some(2), &end, "BV1"),
            Some(AutoHop::NextPart(2))
        );
    }
}
