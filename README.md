# bilibili-tui

<div align="center">

![Rust](https://img.shields.io/badge/Rust-stable-orange)
![License](https://img.shields.io/badge/License-MIT-blue)
![GitHub release (latest by date)](https://img.shields.io/github/v/release/maredevi/bilibili-tui?label=version&color=green)

一个基于终端用户界面（TUI）的 Bilibili 客户端，使用 Rust 构建，提供轻量级且高效的 Bilibili 浏览体验。

[功能特性](#功能特性) • [安装指南](#安装指南) • [使用方法](#使用方法) • [开发指南](#开发指南)

</div>

## 📖 项目简介

bilibili-tui 是一个现代化的终端应用程序，让你能够在终端中舒适地浏览 Bilibili 平台的内容。它采用了 Ratatui 框架构建美观的界面，并集成了完整的 Bilibili API 功能。

### ✨ 核心特点

- 🚀 **高性能**: 基于 Rust 和 Tokio 异步运行时，响应迅速
- 🎨 **美观界面**: 基于 Opaline 主题引擎，支持 39 款内置主题
- 📱 **完整功能**: 支持视频播放、动态浏览、搜索、弹幕等核心功能
- 🔐 **安全认证**: 二维码登录，Cookie 本地持久化存储
- ⌨️ **Vim 风格**: 直观的键位绑定，适合终端用户
- 🖱️ **鼠标支持**: 全面支持鼠标点击和滚轮交互

## � 应用截图

| 首页                                        | 搜索页                                        |
| ------------------------------------------- | --------------------------------------------- |
| ![首页推荐](screenshots/home.png)           | ![搜索页面](screenshots/search.png)           |
| **动态页**                                  | **动态详情**                                  |
| ![关注动态](screenshots/dynamic.png)        | ![动态详情](screenshots/dynamic%20detail.png) |
| **视频详情**                                | **视频播放**                                  |
| ![视频详情](screenshots/video%20detail.png) | ![视频播放](screenshots/video%20playing.png)  |
| **设置页**                                  |                                               |
| ![设置页面](screenshots/settings.png)       |                                               |

## �🚀 功能特性

### 🔑 认证系统

- **二维码登录**: 扫描二维码快速登录 Bilibili 账号
- **凭证持久化**: 自动保存登录状态到本地配置目录
- **状态管理**: 实时检测登录状态，自动处理认证过期

### 🏠 浏览功能

- **首页推荐**: 个性化视频推荐网格，支持封面图片预览
- **动态系统**: 浏览关注的 UP 主动态，支持图片和文字动态
- **UP 主门户**: 常看 UP 主快速访问
- **分页加载**: 智能分页，流畅浏览大量内容

### 🔍 搜索功能

- **全文搜索**: 搜索 Bilibili 平台的所有视频内容
- **智能排序**: 支持按热度、时间等多种排序方式
- **结果筛选**: 精确的搜索结果展示
- **分页浏览**: 高效的分页加载机制

### 🎬 视频播放

- **MPV 集成**: 使用强大的 MPV 播放器进行视频播放
- **yt-dlp 支持**: 自动提取和播放 Bilibili 视频流
- **认证播放**: 支持播放会员专属和登录后可见的内容
- **Cookie 同步**: 自动同步登录状态到播放器

### 📝 互动功能

- **评论系统**: 查看、点赞和回复评论，支持多级评论展开
- **相关推荐**: 智能推荐相关视频内容
- **动态互动**: 查看和浏览动态详情

### 🎨 主题系统

支持 Opaline 内置主题（39 款），包括：

- **SilkCircuit** 系列（Neon / Soft / Glow / Vibrant / Dawn）
- **Catppuccin** 系列（Mocha / Macchiato / Frappé / Latte）
- **Nord / Dracula / Tokyo Night / Rose Pine / Gruvbox / Solarized / One / GitHub / Ayu / Flexoki** 等

### ⚙️ 设置和定制

- **键位绑定**: Vim 风格的导航键位
- **主题切换**: 实时切换界面主题
- **配置持久化**: 自动保存用户偏好设置
- **账户管理**: 登录/登出账户管理

## 🛠️ 技术栈

- **UI 框架**: [Ratatui](https://github.com/ratatui-org/ratatui) v0.30.0
- **异步运行时**: [Tokio](https://tokio.rs/) (full features)
- **HTTP 客户端**: [reqwest](https://docs.rs/reqwest/) (JSON + cookies)
- **图片处理**: [image](https://docs.rs/image/) crate
- **QR 码生成**: [qrcode](https://docs.rs/qrcode/), [tui-qrcode](https://docs.rs/tui-qrcode/)
- **主题系统**: [Opaline](https://github.com/hyperb1iss/opaline)
- **开发工具**: [mise](https://mise.jdx.dev/), pre-commit hooks

## 📁 项目结构

```
src/
├── api/          # Bilibili API 交互模块
│   ├── auth.rs   # 二维码认证实现
│   ├── client.rs # 核心 API 客户端
│   ├── video.rs  # 视频信息接口
│   ├── search.rs # 搜索功能接口
│   ├── dynamic.rs # 动态系统接口
│   ├── comment.rs # 评论系统接口
│   ├── recommend.rs # 推荐算法接口
│   └── wbi.rs    # WBI 签名实现
├── app/          # 应用逻辑和状态管理
│   ├── action.rs # 应用动作定义
│   └── mod.rs    # 主应用结构
├── player/       # MPV 播放器集成
│   └── mod.rs    # 播放器控制逻辑
├── storage/      # 数据持久化模块
│   └── mod.rs    # 凭证和配置存储
├── ui/           # UI 组件和页面
│   ├── login.rs  # 登录页面组件
│   ├── home.rs   # 首页推荐组件
│   ├── search.rs # 搜索页面组件
│   ├── dynamic.rs # 动态页面组件
│   ├── video_detail.rs # 视频详情组件
│   ├── dynamic_detail.rs # 动态详情组件
│   ├── settings.rs # 设置页面组件
│   ├── sidebar.rs # 侧边栏导航
│   ├── theme.rs  # 主题系统实现
│   └── video_card.rs # 视频卡片组件
├── lib.rs        # 模块声明
└── main.rs       # 应用程序入口
```

## 🚀 安装指南

### 前置要求

#### 系统依赖

- **Rust 工具链**: 稳定版 (stable)
- **Cargo**: Rust 包管理器

#### 外部工具

- **MPV**: 强大的视频播放器

  ```bash
  # Ubuntu/Debian
  sudo apt install mpv

  # macOS
  brew install mpv

  # Arch Linux
  sudo pacman -S mpv
  ```

  > **跳转精度：** Bilibili DASH 的视频和音频是独立流。MPV 默认的相对跳转可能回退到视频关键帧，造成跳转后短暂无声。应用启动 MPV 时已自动传入 `--hr-seek=yes`，无需额外配置；只有在应用之外单独使用 MPV 时，才需要在 `~/.config/mpv/mpv.conf` 中加入 `hr-seek=yes`。

  > **弹幕顺滑度：** 应用启动 MPV 时传入 `--deinterlace=yes --video-sync=display-resample`：前者让 30fps 片源按场重建为 60fps 输出，后者把呈现节奏交给显示器的 vsync 时钟（低延迟 profile 会把 `video-sync` 改回 `audio`，应用已在 profile 之后重新覆盖）。弹幕 overlay 只在"呈现"时刻上屏，实测两项配合把位置更新从约 60 次/秒提升到约 120 次/秒，滚动糊影（画面冻结时长）从约 10.6ms 降到约 2.1ms。代价是 bob 重建会轻微软化逐行素材的垂直细节；如某片源不希望处理，可用 `mpv_extra_args` 传 `--deinterlace=no` 覆盖。

  > **CDN 隐私：** 应用会对维护目录中的 CDN 裸域名执行无 Cookie、无媒体路径、无签名参数的限时可达性探测，并将结果作为本地排名的少量先验。实际播放只使用 Bilibili `playurl` API 授权返回的地址，不会把签名 URL 改写到目录中的其他主机。

- **yt-dlp**: 视频提取工具（MPV 内置支持）

  ```bash
  # Ubuntu/Debian
  sudo apt install yt-dlp

  # macOS
  brew install yt-dlp

  # Arch Linux
  sudo pacman -S yt-dlp
  ```

- **MPV Bilibili 弹幕**

  请参考[MPV-Play-BiliBili-Comments](https://github.com/itKelis/MPV-Play-BiliBili-Comments)

- **MPV Bilibili SponsorBlock（可选）**

  如需在播放时自动跳过 Bilibili 视频中的恰饭广告，可安装
  [mpv_sponsorblock_for_bilibili](https://github.com/MareDevi/mpv_sponsorblock_for_bilibili)。

#### 推荐终端

支持以下终端图形协议以获得最佳图片预览体验：

- **Kitty**: graphics protocol
- **Sixel**: 图形协议支持
- **iTerm2**: graphics protocol
- **其他终端**: 将自动回退到 ASCII 艺术模式

### 安装方法

#### 方法一：从 AUR 安装（Arch Linux 推荐）

对于 Arch Linux 用户，推荐通过 AUR 安装预编译的二进制包：

```bash
# 使用 yay
# 源代码构建
yay -S bilibili-tui
# 二进制预编译
yay -S bilibili-tui-bin

# 或使用 paru
paru -S bilibili-tui
paru -S bilibili-tui-bin
```

#### 方法二：使用 Homebrew (macOS/Linux)

```bash
brew install maredevi/tap/bilibili-tui
```

#### 方法三：使用 Cargo

```bash
# 克隆仓库
git clone https://github.com/maredevi/bilibili-tui.git
cd bilibili-tui

# 构建发布版本
cargo build --release

# 运行应用
./target/release/bilibili-tui
```

#### 方法四：从源码编译（开发版本）

```bash
# 从 git 仓库克隆并构建
git clone https://github.com/maredevi/bilibili-tui.git
cd bilibili-tui
cargo install --path .
```

> 注意：当前版本尚未发布到 crates.io

#### 方法五：使用 mise（推荐用于开发）

```bash
# 安装 mise（如果未安装）
# macOS: brew install mise
# Linux: 参考官方文档 https://mise.jdx.dev/

# 克隆仓库
git clone https://github.com/maredevi/bilibili-tui.git
cd bilibili-tui

# 安装项目依赖
mise install

# 构建
mise exec cargo build

# 运行
mise exec cargo run
```

#### 方法六：使用 Nix Flake（NixOS / Nix 用户）

```bash
# 克隆仓库
git clone https://github.com/maredevi/bilibili-tui.git
cd bilibili-tui

# 进入开发环境 / 安装依赖
nix develop

# 构建
cargo build --release

# 构建静态版本
cargo build --release --target x86_64-unknown-linux-musl
```

> 注意：确保系统中安装了 `mpv` 和 `yt-dlp` 作为运行时依赖。

## 📖 使用方法

### 键位绑定

应用采用 Vim 风格的键位绑定，熟悉 Vim 的用户可以快速上手：

| 功能           | 键位                | 说明                           |
| -------------- | ------------------- | ------------------------------ |
| **导航**       |                     |                                |
| 向上移动       | `k` / `↑`           | 在列表中向上移动               |
| 向下移动       | `j` / `↓`           | 在列表中向下移动               |
| 向左移动       | `h` / `←`           | 向左导航                       |
| 向右移动       | `l` / `→`           | 向右导航                       |
| 内容上翻页     | `PageUp`            | 按可见区域向上移动             |
| 内容下翻页     | `PageDown`          | 按可见区域向下移动             |
| **操作**       |                     |                                |
| 确认选择       | `Enter`             | 打开选中项                     |
| 返回上级       | `Esc`               | 返回上一页面                   |
| 退出应用       | `q`                 | 退出程序                       |
| 播放视频       | `p`                 | 播放选中的视频                 |
| 刷新页面       | `r`                 | 刷新当前页面内容               |
| 切换主题       | `t`                 | 循环切换主题                   |
| 打开设置       | `s`                 | 打开设置页面                   |
| **搜索**       |                     |                                |
| 开始搜索       | `/` 或 `i`          | 进入搜索输入模式               |
| **页面切换**   |                     |                                |
| 切换页面       | `Tab` / `Shift+Tab` | 所有页面统一：切换侧边栏导航   |
| **动态页**     |                     |                                |
| 切换动态标签   | `1` / `2` / `3`     | 快速跳转到全部/视频/图文标签   |
| 切换 UP 主     | `[` / `]`           | 在常看 UP 主列表中左右切换     |
| **设置页**     |                     |                                |
| 切换分类       | `[` / `]`           | 在主题/弹幕/播放/超分/快捷键/账户间切换 |
| **视频详情页** |                     |                                |
| 切换焦点       | `Tab`               | 在评论和相关推荐区域间切换     |
| 展开收起回复   | `r`                 | 展开/收起评论回复              |
| 补帧三态       | `i`                 | 循环 关→混合 (mpv 时间插值)→Smooth Motion (NVIDIA 驱动插帧) →关; 状态在「播放选项」块 (页头不放提示), 持久化 |
| 超分选择       | `e`                 | 循环 关→12 种超分算法 (Anime4K/NNEDI3/RAVU/FSRCNNX/FSR/CAS/…); 状态与说明在「播放选项」块, 细调在设置页「🔍 超分」栏目 |

#### 🖼️ 超分辨率（12 算法） & 🎞 NVIDIA Smooth Motion

详情页「播放选项」块（画质/HDR 下方）显示 `补帧:` 与 `超分:` 状态，
快捷键与画质键 `m` 同组。**补帧三态实测真值**（同场 1080p24 全屏、nvdec、
Wayland commit 计数 + IPC 属性）：

  | 模式 | 内容更新率 | GPU | CPU | 备注 |
  |------|-----------|-----|-----|------|
  | 关（display-resample） | 165 提交/秒，内容仍 24Hz 步进 | 29% | 23% | 呈现循环 = 弹幕平滑的代价（audio 节奏基线仅 20%/9%） |
  | 混合（mpv 时间插值） | **165Hz 真补帧** | 30% | 24% | 插值本身 ≈ 免费（+1pp GPU、+1pp CPU，drops 0 / mist ≤3） |
  | Smooth Motion（NVPRESENT） | **≈48Hz**（2× 设计） | 15-21% | 26% | GPU 最省；**到不了刷新率**（见下） |
  | SM + display-resample | 坏档：有效 82Hz、12s 掉 70 帧 | — | — | 驱动层与 vsync 节奏冲突，禁止组合 |


- **超分选择**（`e` 循环 关→12 算法→关）：Anime4K 原三档选项升级为
  **12 种超分/画质算法**（第7轮，用户选定"11 新增 + Anime4K"）。全部为
  mpv 原生 GLSL hook（`//!HOOK` 格式，gpu-next/Vulkan 直接加载，79 个
  文件 46MB，来源：bjin/mpv-prescalers、igv FSRCNNX、iwalton3 FSR、
  agyild CAS/NIS、boned101 LumaSharpen/AdaptiveSharpen、
  Th-Underscore Anime4K-Ultra）。资产在
  `~/.local/share/bilibili-tui/superres/<算法>/`，缺文件整体静默降级
  （可播性优先）：

  | 算法 | 定位 | GPU（1080p→1600p 全屏, off=21%） |
  |------|------|--------------------------------|
  | 关（内建 spline36/lanczos） | 基线 | 21% |
  | CAS / NIS / FSR | 通用实时 SR+锐化（AMD/NVIDIA） | 22-23%（+1-2pp） |
  | RAVU-lite / RAVU-r4 / RAVU-r2 | 动画向学习型 CNN（bjin） | 21-25%（+0-4pp） |
  | NNEDI3 (nns16-256) | 神经元插值，低清重建最强 | 25%（+4pp） |
  | Anime4K（L 档链） | 动画线稿重建 CNN | 28%（+7pp） |
  | Anime4K-Ultra | FSR+线细化混合 | 24%（+3pp） |
  | FSRCNNX (8/16) | igv 手调 x2 CNN | 30-37%（+9-16pp） |
  | AdaptiveSharpen / LumaSharpen | 自适应锐化（非超分） | 21-25% |

  **参数细调**（设置页「🔍 超分」栏目，随算法动态显示）：Anime4K 档位
  A/B/C、NNEDI3 神经元数（16-256）与窗口（8x4/8x6）、FSRCNNX 滤镜数
  （16/8）、RAVU 色彩变体（YUV/RGB/亮度）、锐化族强度（1-100，50=官方
  默认，按百分比缩放写 `.s<N>.glsl` 补丁副本，原文件不动）。bjin README
  推荐全屏视频用 `-rgb` 变体（合并 chroma 上缩），已设为默认。
  旧 `anime4k_mode` 配置加载时自动迁移（a/b/c → Anime4K 对应档位），
  双向同步保持旧版本二进制兼容。

  **下缩/自适应门控（"像之前一样优化"）**：12 算法 79 文件勘察后分三类
  — ① nnedi3/ravu/fsrcnnx/fsr/cas/nis **自带上缩门控 WHEN**
  （`HOOKED/OUTPUT < 0.7071` 即 ≥1.414×、`OUTPUT/LUMA > 1.0/1.3`），下缩
  自动禁用零成本；② LumaSharpen（HOOK LUMA 无 WHEN）与 Anime4K-Ultra
  的 9 个无条件 Thin/CNN 块由 `guard_shader_downscale` 补官方同款
  `0.999` WHEN（`.guarded.glsl` 副本）；③ adaptive-sharpen（HOOK SCALED，
  输出分辨率后锐化）保留。实测 4K24 下缩场景全部算法 GPU ≈ off 基线
  （守卫生效）。**NNEDI3/RAVU 单轴行为实测无变形**：1.33× 全屏时高度轴
  （1080/1600=0.675<0.707）触发 2× 后 mpv 轻微下采，宽度轴不触发 — 行/
  列投影位移=0、r≥0.979，是 bjin 的完美 2× 预缩器设计（≥1.414× 时双轴
  全开）。1.5× 上缩质量口径（720p→1080p vs 参考）：多数算法 PSNR 同值
  （SR 生成新高频而非复原像素，MSE 弱区分），Anime4K-Ultra −0.8dB、
  AdaptiveSharpen −5.6dB（锐化过冲），其余与内建缩放持平。

  - **Anime4K 三档**（原关→A→B→C，现为算法内档位参数）：
    三模式结构取自官方模板 CTRL+1/2/3，**档位说明随选择即时显示**：

  | 模式 | 定位 | 行内说明 |
  |------|------|----------|
  | A | 1080p 动画 | `1080p动画·线稿重建` — 重建退化线条/去压缩伪影，感知质量最高 |
  | B | 720p 动画 | `720p动画·去振铃抗锯齿` — 中度修复，适合轻度模糊与下采样伪影 |
  | C | 480p/无损图 | `480p·高保真轻处理` — 最高 PSNR，只降噪不重修线条 |

  着色器变体按实测定档（4060 笔记本，1080p 全屏 15s，无 shader 基线
  ratio ≈1.000 / GPU 29%，drops 全程 0；质量 = 对锐利参考的重建
  PSNR/SSIM，另辅以文字/网格区域肉眼对比）：

  | 变体 | 实测（实时比 / GPU / vo-delayed） | 质量 PSNR/SSIM | 结论 |
  |------|----------------------------------|----------------|------|
  | **L 降档（当前）** | 干净口径逐帧 +2pp GPU（UL 是 +16pp）；用户实际模式（drs+插值）GPU 33-37%、**dispFPS 164.6 满呈现**、vo-delayed 7-8、mist 2-3 | A 36.188 · B 37.215 · C 44.736（q924 全屏放大同窗） | **采用**：三档全降 L — **省 10-18pp GPU 且让 165Hz 补帧恢复**，A 仅 −0.87dB（肉眼不可分）、B 反超 UL +0.78dB、C 与 UL 同值 |
  | UL 升级档（上一轮） | 干净口径 +16pp；drs 下挤爆管线：GPU 45-50% / **dispFPS 148** / vo-delayed 145-158 / mist 52-60 | A 37.055 · B 36.433 · C 44.736 | 被 L 替代（呈现拥塞直接违背"补帧到刷新率"） |
  | VL 升级档 | 0.999-1.000 / 40-43% / 15-44（旧环境噪声内，误判与 UL 同成本） | 26.84/.824（旧口径）· 35.499（新同窗） | 被 L 替代（L 同价更低噪、质量反超） |
  | 全 VL HQ 套 | 0.997 / 50% / **206** | 26.98/.832 | present 拥塞（vo-delayed 10×），质量仅 +0.14dB，弃 |
  | 官方 Fast (M)（曾用） | 0.999 / 34% / 12-30 | 26.64/.802 | 修复力度偏弱，被 VL 升级档替代 |
  | 全 S 套（曾用） | 0.972-0.982 | — | 修复力度弱（"没效果"），弃 |
  | GAN 上采样 | 0.998 / 48% / 178 | 26.40/.793 | 质量与 present 双差，弃 |
  | 无 shader（spline36） | 1.000 / 29% / 13 | 27.58/.837 | 参考行：MSE 指标偏爱保守缩放，线稿感知清晰度以肉眼为准 |

  **消耗结构（第4轮拆分实测）**：成本几乎全在修复/降噪阶段的 CNN 卷积
  （含 Clamp 全链 45/46pp GPU），上采样+Auto 收尾合计 ≈+1pp、Clamp
  ≈2pp。**Restore 档位成本曲线**（干净 audio 口径 off=19%）：UL=+16pp、
  VL=+9pp、M=+5pp、S=+3pp、**L=+2pp** — UL 是 L 的 8 倍。display-resample
  会把 UL 的成本进一步放大到 +16~17pp 并挤爆呈现管线（dispFPS 148 /
  vo-delayed 150），**换成 L 后放大项一起消失**（L 在 drs 下 164.6Hz /
  vo-delayed 7）。CPU：audio 9%、display-resample 23% — 后者的 +14pp
  是 165Hz 呈现循环（弹幕平滑所必需，与 Anime4K 无关）。
  已否决的降耗路径（有据）：前置降采样 hook（0.64× 窗口画质 −3.0dB，
  官方"先修复后缩小"顺序是对的）、vo=gpu（同场无优势）、阶段3 降 S
  （−0.13dB 且不省）。

  **下缩守卫（第5轮，4K 源 GPU 100% 问题的修复）**：视频源比输出大
  （4K→2560 屏 0.667×、1080p→小窗）时，上采样/收口 pass 的官方 WHEN
  全不触发，唯一在跑的 Restore 卷积在源分辨率白跑 — 实测 4K24 全屏
  SM+Anime4K **GPU 72% vs 关 31%（+41pp，叠加背景负载即用户报告的
  100%）**，且真下缩渲染质量 −1.4dB（修复后再下缩不如 mpv 直接高质量
  下缩）。修复：`guard_downscale` 给 `Anime4K_Restore_CNN_*` 的每个
  hook 块补官方同款 `//!WHEN OUTPUT/MAIN > 0.999`（1:1 与上缩照常修复，
  下缩整段跳过），生成 `.guarded.glsl` 副本、官方资产不动。实测守卫后
  **GPU 72→35%**、质量回基线（35.44 ≈ 无 shader 35.38，原链 33.94）、
  1080p 上缩场景无回归（WHEN 为真照常跑）。
  详情页 Anime4K 状态行在 4K/8K 画质（qn 120/127）时显示 **"⚠4K源不
  建议开启"**（mpv 侧守卫已自动跳过，别再手动开）；HDR(125)/杜比(126)
  分辨率不定不判断，跟随画质无数据不提示（守卫仍生效）。

  **HDR（第6轮，"开启 hdr 后 mpv 没有显示 hdr 内容"的两层根因）**：
  ① 选流 bug（已修 `pick_video`）：实测 B 站 playurl 在任何 qn 下都下发
  HDR 流（qn=80 仍返回 id=125, 3840×2160 PQ），但旧逻辑让 quality cap
  (id ≤ quality) 把 125 掐掉再"静默回退 SDR" — 状态行 HDR:开、实际播
  SDR。现在 HDR 开启时 HDR 家族流优先于普通画质上限（普通上限只约束非
  HDR 候选；无 HDR 流才按上限落回）。
  ② 显示链路（环境限制，无法在应用层修）：niri (v26.04) 未实现
  `wp_color_manager_v1`（mpv 实测日志 "Compositor doesn't support"），
  mpv 无法 HDR 直通输出，只能 tone-map 到 SDR — 修完 ① 后 mpv 拿到真
  HDR(PQ/BT.2020) 流并 tone-map 呈现（这是合成器支持 HDR 前的上限）。
  tone-map 成本实测仅 +1pp GPU（23→24%，auto=spline + hdr-compute-peak
  auto 为 libplacebo 工程化默认，无需覆盖）。**Anime4K × HDR**：B 站
  HDR 流全是 4K，在 2560 屏上恒为下缩 → 已有下缩守卫自动跳过整条链
  （真流实测 off vs L链像素差 0.1% = 守卫生效），CNN 不会在 PQ 域运行。

  **60fps 决策（用户选定保留修复）**：1080p60 全屏 + Anime4K 实测
  GPU 70%（关 42%），成本 ×2.5 但用户要保留修复效果。附带澄清：
  SM 模式下 `frame-drop-count≈450` 是 NVPRESENT 接管呈现后的计数失真
  （所有变体恒定、ratio=1.0），Wayland commit 实测 165/s 满速呈现，
  观感不卡。
  缺任一着色器文件时应用整体不启用（mpv 直接吃缺失文件实测退化到
  ~0.55 实时，存在性检查已拦）。**增强覆盖全部播放路径**：单视频、
  多 P 播放列表、番剧、本地文件（番剧/本地曾漏挂，导致"没效果"）。
  着色器目录缺失/缺文件时静默不启用，播放不受影响。重建：
  `git clone --depth 1 https://github.com/bloc97/Anime4K.git ~/.local/share/bilibili-tui/anime4k`
- **混合**（`i` 循环到第二态）：mpv 时间插值补到刷新率 —— 基础参数
  `display-resample` 定住 vsync 节奏，追加 `--interpolation=yes` + tscale。
  **tscale 默认 mitchell**（本轮改，原默认 oversample）：mpv 手册按
  "平滑↑模糊↑"排序，oversample 最锐最不平滑 ≈ 只有呈现节奏、不做内容
  补帧，默认它等于混合模式白开；mitchell 居中。手改 `interpolation_tscale`
  可选 linear/mitchell 等。实测：Wayland commit 满 165/s（真补帧）、
  GPU 30% / CPU 24%（对关闭 29%/23% ≈ 免费）。
- **Smooth Motion**（`i` 循环到第三态）：NVIDIA 驱动级插帧。播放进程注入
  `NVPRESENT_ENABLE_SMOOTH_MOTION=1` 启用 `VK_LAYER_NV_present` 隐式层，
  驱动 AI 在呈现层补帧（**RTX 40 系+，需 Vulkan** — 除环境变量外还
  **显式追加 `--gpu-api=vulkan`** 锁定层依赖，防回退会话落到 opengl 时
  隐式层静默不加载）。该模式下 mpv 不叠加自身插值，避免双重补帧；且呈现
  节奏让位驱动层：**追加 `--video-sync=audio` 覆盖基础参数里的
  display-resample**（仅本模式；其他模式的 display-resample 不动，弹幕
  平滑不受影响）——对帧生成也是最优输入：只在新帧时提交 present，驱动在
  真实帧间插值，vsync 节奏的同帧重复 present 会让插值退化。
  实测收益（1080p24）：GPU 41%→31%、vo-delayed 11→0、实时比
  0.999→1.003；与 A 模式（VL 升级档）同时开 1.003 / GPU 28% /
  vo-delayed 0（audio 节奏连 display-resample 的开销也一并省去）。
  **生成帧生效实证**（mpv 进程 Wayland commit 计数，8s）：全屏
  开338/关176、**平铺窗口开340/关177，均稳定 2×** — 窗口形态不影响
  生成（平铺亦生效，无需全屏）。**能力边界（实测+官方文档）**：NVIDIA
  官方定义就是"两帧之间推断一帧"（2×，README nvpresent 章），**没有
  目标帧率开关** — 24fps 源稳定输出 ≈48Hz（commit 49/s），**到不了显示
  器刷新率**；要满 165Hz 用混合模式。SM 与 display-resample 组合实测
  坏档（有效 82Hz、12s 掉 70 帧），audio 覆盖不可去掉。
  排查：`NVPRESENT_LOG_LEVEL=4`（stderr 日志）、`VK_LOADER_DEBUG=layer`
  （层加载），见驱动 README "NVIDIA Smooth Motion" 章。
- **启动黑屏治理**：`--force-window=no`（原 `immediate`）——网络打开、
  探测、缓冲期间**不创建 mpv 窗口**，TUI 全程可见，画面就绪才弹窗。
  fifo 实测：`immediate` 阻塞打开时秒建黑窗（Vulkan 立即初始化），
  `no` 则推迟到真正播放才建窗；视频加载黑屏时间由"整个加载期"缩到
  "首帧渲染 ~0.5-1s"。

### 🖱️ 鼠标操作

应用全面支持鼠标交互，提供更直观的操作体验：

- **左键点击**: 选中列表项、切换标签页、点击功能按钮
- **滚轮滚动**: 浏览列表、查看长文本、翻页

### 页面导航

应用采用侧边栏导航设计，包含以下主要页面：

#### 🏠 首页

- 显示个性化推荐视频
- 支持封面图片预览
- 自动分页加载更多内容

#### 🔍 搜索页

- 输入关键词搜索视频
- 显示搜索结果列表
- 支持分页浏览更多结果

#### 📱 动态页

- 浏览关注的 UP 主动态
- 支持多种动态类型（视频、图文、纯文字）
- **标签切换**：按 `1` / `2` / `3` 快速跳转到全部/视频/图文
- **UP 主导航**：按 `[` / `]` 切换常看 UP 主
- 快速访问常看 UP 主列表

#### ⚙️ 设置页

- 查看和修改键位绑定
- 切换界面主题
- 账户管理（登出功能）
- **分类切换**：按 `[` / `]` 在主题/快捷键/账户间切换

#### 🎬 视频详情页

- 查看视频信息和评论区
- 支持相关推荐
- **焦点切换**：按 `Tab` 在评论和相关推荐区域间切换
- **评论操作**：按 `r` 展开/收起回复

### 主要功能说明

#### 二维码登录流程

1. 启动应用后自动显示登录页面
2. 使用 Bilibili 手机客户端扫描二维码
3. 确认登录后自动保存凭证到本地
4. 登录成功后跳转到首页

#### 视频播放

1. 在视频列表中选择视频
2. 按回车键进入视频详情；详情加载完成后会自动启动 MPV
3. 也可以在详情页面按 `p` 键手动启动播放
4. MPV 正常结束后，自动返回进入详情前的列表页面

#### 图片预览

- 支持的终端协议：Kitty、iTerm2、Sixel
- 自动检测终端能力
- 不支持时回退到 ASCII 艺术

## ⚙️ 配置说明

### 配置文件位置

配置文件存储在系统用户配置目录（Linux 通常为
`~/.config/bilibili-tui/`，macOS 通常为
`~/Library/Application Support/bilibili-tui/`）：

```
~/.config/bilibili-tui/
├── credentials.json  # 登录凭证
├── config.json      # 应用配置
└── cookies-<pid>-<序号>.txt  # 播放期间临时生成
```

> 临时 cookie 文件仅用于 MPV/yt-dlp 认证，权限为 `0600`，播放器退出后会自动删除。不要将配置目录或调试日志提交到版本库。

### 配置文件格式

#### `credentials.json`

```json
{
  "sessdata": "your_sessdata_token",
  "bili_jct": "your_bili_jct_token",
  "dede_user_id": "your_user_id",
  "dede_user_id_ckmd5": "optional_md5_hash",
  "refresh_token": "optional_refresh_token"
}
```

#### `config.json`

```json
{
  "theme": "silkcircuit-neon",
  "keybindings": {
    "quit": "q",
    "nav_up": "k",
    "nav_down": "j",
    "nav_left": "h",
    "nav_right": "l",
    "confirm": "Enter",
    "back": "Esc",
    "next_theme": "t",
    "play": "p",
    "refresh": "r",
    "open_settings": "s"
  }
}
```

#### MPV 相关配置

以下字段均为可选，直接写在 `config.json` 顶层：

| 字段             | 说明                                                                                                   |
| ---------------- | ------------------------------------------------------------------------------------------------------ |
| `mpv_vo`         | 覆盖 MPV 视频输出。留空使用默认外部窗口；设为 `kitty` / `tct` 可在终端内渲染。                          |
| `mpv_hwdec`      | 覆盖硬件解码模式。留空自动选择（NVIDIA 默认 `nvdec`，其他 `auto-safe`）；可显式设为 `vaapi`、`vulkan`、`no` 等。 |
| `mpv_extra_args` | 追加到每次 MPV 命令行末尾的额外参数，按空白分隔，支持单/双引号包裹含空格的值。**最后追加，可覆盖应用自身设置的同名选项。** |

示例：

```json
{
  "mpv_hwdec": "auto-safe",
  "mpv_extra_args": "--loop-playback=inf --audio-pitch-correction=no"
}
```

### 主题配置

`theme` 字段使用 Opaline 主题 ID（kebab-case），例如：

- `"silkcircuit-neon"`
- `"catppuccin-mocha"`
- `"tokyo-night"`
- `"rose-pine"`
- `"nord"`

完整列表可在设置页主题分类中查看与切换。

## 🏗️ 架构说明

### 设计模式

- **事件驱动**: UI 组件返回 `AppAction`，中央处理器统一处理
- **异步 I/O**: 所有 API 调用和 I/O 操作都是异步的
- **组件化 UI**: 每个页面实现 `Component` trait
- **状态管理**: 集中式状态管理，支持状态持久化

### 模块交互流程

```
main.rs → App::run() → 事件循环 → UI 组件 → AppActions → App::handle_action()
                                    ↓
                            API 客户端 ← Storage (凭证/配置)
                                    ↓
                            播放器 (外部 MPV)
```

### API 安全

- **用户代理**: 使用标准 Chrome 用户代理
- **Referer 头**: 始终包含 bilibili.com referer
- **WBI 签名**: 搜索和推荐 API 的强制签名机制
- **速率限制**: 通过正确的 API 使用模式实现隐式速率限制

## 🧪 开发指南

### 开发环境设置

#### 1. 克隆仓库

```bash
git clone https://github.com/maredevi/bilibili-tui.git
cd bilibili-tui
```

#### 2. 安装开发依赖

```bash
# 使用 mise（推荐）
mise install

# 或手动安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### 3. 设置 Pre-commit Hooks

```bash
# 安装 git hooks
pre-commit install

# 手动运行检查
pre-commit run --all-files
```

### 开发工作流

#### 常用命令

```bash
# 代码检查
cargo check

# 格式化代码
cargo fmt

# 代码 Lint
cargo clippy -- -D warnings

# 运行测试
cargo test

# 构建项目
cargo build

# 构建发布版本
cargo build --release

# 运行应用
cargo run
```

#### Pre-commit 配置

项目配置了以下 pre-commit hooks：

- **代码格式**: `rustfmt` 自动格式化
- **语法检查**: `cargo check` 快速检查
- **代码质量**: `clippy` 高级 lint
- **构建测试**: `cargo build` 确保可构建
- **单元测试**: `cargo test` 运行测试

### 调试技巧

#### 启用调试日志

```bash
RUST_LOG=debug cargo run
```

#### 常见问题

1. **图片不显示**: 检查终端是否支持图形协议
2. **播放失败**: 确认 MPV 和 yt-dlp 已正确安装
3. **登录失败**: 检查网络连接和防火墙设置

## 🤝 贡献指南

我们欢迎所有形式的贡献！请遵循以下步骤：

### 贡献流程

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

### 代码规范

- 遵循 Rust 官方代码风格
- 使用 `cargo fmt` 格式化代码
- 通过 `cargo clippy` 检查
- 添加适当的单元测试
- 更新相关文档

### Issue 报告

使用 GitHub Issues 报告问题，请包含：

- 详细的问题描述
- 复现步骤
- 环境信息（操作系统、终端等）
- 错误日志（如有）

## 📄 许可证

MIT License

## 🙏 致谢

感谢以下开源项目的支持：

- [Ratatui](https://github.com/ratatui-org/ratatui) - 现代化的 Rust TUI 框架
- [Tokio](https://tokio.rs/) - 异步运行时
- [Opaline](https://github.com/hyperb1iss/opaline) - Rust 主题引擎与内置主题库
- [MPV](https://mpv.io/) - 强大的媒体播放器
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - 视频下载工具
- [MPV-Play-BiliBili-Comments](https://github.com/itKelis/MPV-Play-BiliBili-Comments) - MPV Bilibili 弹幕
- [mpv_sponsorblock](https://github.com/po5/mpv_sponsorblock) - MPV 恰饭广告自动跳过
- [bilibili-API-collect](https://github.com/SocialSisterYi/bilibili-API-collect) - Bilibili API 集合

## 📞 联系方式

- 项目主页: [GitHub Repository](https://github.com/maredevi/bilibili-tui)
- 问题反馈: [GitHub Issues](https://github.com/maredevi/bilibili-tui/issues)
- 功能请求: [GitHub Discussions](https://github.com/maredevi/bilibili-tui/discussions)

---

<div align="center">

**🌟 如果这个项目对你有帮助，请给个 Star 支持！**

Made with ❤️ by MareDevi

</div>
