//! Offline download of Bilibili videos / bangumi episodes via `yt-dlp`.
//!
//! The Rust side only orchestrates `yt-dlp` (already used by the player for
//! URL resolution); ffmpeg (also a runtime dependency) merges the DASH
//! video+audio streams into a single mp4. Progress is streamed back over an
//! mpsc channel so the TUI can show a live percentage.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc::Sender;

/// Live, aggregated download status for the UI (bottom bar).
/// Tracks how many items in a batch are done and the current item's phase.
#[derive(Debug, Clone)]
pub struct DownloadSummary {
    /// Total items in the current batch.
    pub total: usize,
    /// Items finished (success or error).
    pub done: usize,
    /// Title of the item currently downloading.
    pub current_title: String,
    /// Human-readable phase text for the current item (e.g. "下载中 47%").
    pub current_msg: String,
}

/// Shared, process-global download status so both the background worker and
/// the UI (which draws without an `App` reference) can read/update it.
static DOWNLOAD_STATUS: OnceLock<std::sync::Arc<Mutex<Option<DownloadSummary>>>> = OnceLock::new();

fn status_cell() -> &'static std::sync::Arc<Mutex<Option<DownloadSummary>>> {
    DOWNLOAD_STATUS.get_or_init(|| std::sync::Arc::new(Mutex::new(None)))
}

/// Begin a batch with `total` items. Clears any previous status.
pub fn begin_batch(total: usize) {
    *status_cell().lock().unwrap() = Some(DownloadSummary {
        total,
        done: 0,
        current_title: String::new(),
        current_msg: "准备中…".to_string(),
    });
}

/// Update the current item's phase text (called from the per-item worker).
pub fn set_status(title: &str, status: &str) {
    let mut g = status_cell().lock().unwrap();
    if let Some(s) = g.as_mut() {
        s.current_title = title.to_string();
        s.current_msg = status.to_string();
    } else {
        *g = Some(DownloadSummary {
            total: 1,
            done: 0,
            current_title: title.to_string(),
            current_msg: status.to_string(),
        });
    }
}

/// Mark one item as finished (success or error).
pub fn mark_done() {
    let mut g = status_cell().lock().unwrap();
    if let Some(s) = g.as_mut() {
        s.done += 1;
    }
}

/// Take the latest aggregated download status (UI polling).
pub fn current_status() -> Option<DownloadSummary> {
    status_cell().lock().unwrap().clone()
}

/// Live download status reported to the UI.
#[derive(Debug, Clone)]
pub enum DownloadPhase {
    /// Resolving formats / fetching metadata.
    Preparing,
    /// Downloading a stream; `percent` is 0..100.
    Downloading(f32),
    /// ffmpeg is merging video+audio into the final mp4.
    Merging,
    /// Finished successfully; carries the output path.
    Done(PathBuf),
    /// Failed; carries the error message.
    Error(String),
}

/// Default download directory: `~/bilibili-tui-downloads`.
pub fn download_dir() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("bilibili-tui-downloads")
}

/// Convert a `VideoQuality` into a yt-dlp format selector. We cap by height
/// and always ask for bestvideo+bestaudio so ffmpeg can mux them.
pub fn quality_to_format(height: u16) -> String {
    // Strictly cap the height: the `/b` fallback must also respect the cap,
    // otherwise yt-dlp may pick a higher-resolution best stream when no
    // lower-height stream exists (making the selection appear to not work).
    format!("bv[height<={height}]+ba/b[height<={height}]")
}

/// What kind of media we are downloading (changes the source URL).
#[derive(Clone)]
pub enum DownloadTarget {
    Video { bvid: String },
    Bangumi { ep_id: i64 },
}

/// Start a `yt-dlp` download. The function resolves the final file path and
/// streams progress through `tx`. It blocks until yt-dlp exits.
///
/// `whole_playlist`: when true, do NOT pass `--no-playlist`, so a bangumi
/// season downloads as a whole. Off by default to avoid rate-limiting.
///
/// `user_agent`: passed to yt-dlp. Must match the app's UA or Bilibili may
/// answer with HTTP 412 (risk control).
pub async fn download(
    target: DownloadTarget,
    quality_height: u16,
    out_dir: PathBuf,
    title: String,
    cookie_path: Option<PathBuf>,
    whole_playlist: bool,
    user_agent: String,
    tx: Sender<DownloadPhase>,
) -> Result<PathBuf> {
    let url = match target {
        DownloadTarget::Video { bvid } => format!("https://www.bilibili.com/video/{bvid}"),
        DownloadTarget::Bangumi { ep_id } => {
            format!("https://www.bilibili.com/bangumi/play/ep{ep_id}")
        }
    };

    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("创建下载目录失败: {}", out_dir.display()))?;

    // yt-dlp output template. The merged file ends with `.mp4`; intermediate
    // video/audio parts use the `.fNNNNN` suffix before ffmpeg merges them.
    let safe_title: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect::<String>()
        .trim()
        .to_string();
    if safe_title.is_empty() {
        return Err(anyhow::anyhow!("下载标题为空，无法生成文件名"));
    }
    let out_template = out_dir
        .join(format!("{safe_title}.%(ext)s"))
        .to_string_lossy()
        .into_owned();

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-f")
        .arg(quality_to_format(quality_height))
        .arg("--no-warnings")
        .arg("--user-agent")
        .arg(user_agent)
        .arg(if whole_playlist { "--yes-playlist" } else { "--no-playlist" })
        .arg("-o")
        .arg(&out_template)
        .arg("--merge-output-format")
        .arg("mp4")
        .arg(&url);
    if let Some(cookie) = cookie_path {
        cmd.arg("--cookies").arg(cookie);
    }

    let mut child = cmd
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .context("启动 yt-dlp 失败（请确认系统已安装 yt-dlp）")?;

    let _ = tx.send(DownloadPhase::Preparing).await;

    let mut stderr = child
        .stderr
        .take()
        .context("无法读取 yt-dlp 输出")?;
    let mut buf = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new();
    let mut last_percent: f32 = -1.0;

    loop {
        let n = stderr.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        pending.extend_from_slice(&buf[..n]);
        // yt-dlp refreshes progress with '\r' and only writes '\n' at the end.
        // Split on both so percentages stream live instead of stalling until
        // the process exits.
        loop {
            let Some(sep) = pending
                .iter()
                .position(|b| *b == b'\r' || *b == b'\n')
            else {
                break;
            };
            let chunk_bytes: Vec<u8> = pending.drain(..sep).collect();
            pending.drain(..1);
            let chunk = String::from_utf8_lossy(&chunk_bytes);
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            if let Some(pct) = parse_percent(chunk) {
                if (pct - last_percent).abs() >= 1.0 {
                    last_percent = pct;
                    let _ = tx.send(DownloadPhase::Downloading(pct)).await;
                }
            } else if chunk.contains("[Merger] Merging") || chunk.contains("Merging formats") {
                let _ = tx.send(DownloadPhase::Merging).await;
            }
        }
    }
    // Handle any trailing progress without a final newline.
    if !pending.is_empty() {
        let chunk = String::from_utf8_lossy(&pending);
        let chunk = chunk.trim();
        if !chunk.is_empty() {
            if let Some(pct) = parse_percent(chunk) {
                if (pct - last_percent).abs() >= 1.0 {
                    let _ = tx.send(DownloadPhase::Downloading(pct)).await;
                }
            } else if chunk.contains("[Merger] Merging") || chunk.contains("Merging formats") {
                let _ = tx.send(DownloadPhase::Merging).await;
            }
        }
    }

    let status = child.wait().await.context("等待 yt-dlp 结束失败")?;
    if !status.success() {
        let _ = tx
            .send(DownloadPhase::Error(format!(
                "yt-dlp 退出码 {:?}",
                status.code()
            )))
            .await;
        return Err(anyhow::anyhow!("yt-dlp 下载失败 (exit {:?})", status.code()));
    }

    // The final merged file is `<safe_title>.mp4`.
    let final_path = out_dir.join(format!("{safe_title}.mp4"));
    if !final_path.exists() {
        // Fall back: yt-dlp may have kept a single-stream file with another ext.
        return Err(anyhow::anyhow!(
            "下载完成但未找到输出文件: {}",
            final_path.display()
        ));
    }
    let _ = tx.send(DownloadPhase::Done(final_path.clone())).await;
    Ok(final_path)
}

/// Save offline sidecars next to the downloaded mp4:
///   - `<name>.jpg`         : cover image
///   - `<name>.danmaku.json`: parsed danmaku for offline playback
///
/// Both are best-effort: a missing cover URL or cid just skips that sidecar.
pub async fn save_cover_and_danmaku(
    api_client: &crate::api::client::ApiClient,
    final_path: &Path,
    pic_url: &str,
    cid: i64,
    aid: i64,
    duration_secs: i64,
    bvid: &str,
    ep_id: i64,
    quality: crate::storage::VideoQuality,
) -> Result<()> {
    // Path::with_extension truncates multi-dot Chinese titles
    // ("a.b.c.mp4" -> "a.b.jpg"), which breaks sidecar lookup. Build the
    // sidecar names by stripping only the trailing ".mp4".
    let mp4_name = final_path.to_string_lossy();
    let stem = mp4_name.strip_suffix(".mp4").unwrap_or(&mp4_name);

    // Cover image.
    if !pic_url.is_empty() {
        let cover_path = PathBuf::from(format!("{stem}.jpg"));
        if !cover_path.exists() {
            let resp = reqwest::Client::new()
                .get(pic_url)
                .header("User-Agent", crate::api::client::UA)
                .send()
                .await;
            if let Ok(resp) = resp {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = tokio::fs::write(&cover_path, bytes).await;
                }
            }
        }
    }

    // Danmaku.
    if cid > 0 {
        let danmaku_path = PathBuf::from(format!("{stem}.danmaku.json"));
        if !danmaku_path.exists() {
            let danmaku = api_client
                .get_video_danmaku(cid, Some(aid), duration_secs)
                .await
                .unwrap_or_default();
            if let Ok(json) = serde_json::to_vec(&danmaku) {
                let _ = tokio::fs::write(&danmaku_path, json).await;
            }
        }
    }

    // Subtitle (best-effort): save the Chinese track as `<name>.srt`.
    let srt_path = PathBuf::from(format!("{stem}.srt"));
    if !srt_path.exists() {
        let mut cues = Vec::new();
        if ep_id > 0 {
            // Bangumi subtitles ride along in the playurl response.
            if let Ok(play_url) = api_client.get_bangumi_play_url(ep_id, quality).await
                && let Some(block) = play_url.subtitle
                && let Some(track) = block
                    .subtitles
                    .into_iter()
                    .find(|t| t.lan.to_lowercase().contains("zh"))
                && let Ok(c) = api_client.fetch_subtitle_cues(&track.subtitle_url).await
            {
                cues = c;
            }
        } else if !bvid.is_empty() {
            // Regular video: player/wbi/v2 lists the tracks.
            if let Ok(tracks) = api_client.get_video_subtitles(bvid, cid).await
                && let Some(track) = tracks
                    .into_iter()
                    .find(|t| t.lan.to_lowercase().contains("zh"))
                && let Ok(c) = api_client.fetch_subtitle_cues(&track.subtitle_url).await
            {
                cues = c;
            }
        }
        if !cues.is_empty() {
            let srt = crate::api::subtitle::render_srt(&cues);
            let _ = tokio::fs::write(&srt_path, srt).await;
        }
    }
    Ok(())
}

/// Parse a percentage like " 23.7% of 50.53MiB at ..." into a float.
fn parse_percent(line: &str) -> Option<f32> {
    if !line.contains("[download]") {
        return None;
    }
    let pct = line
        .split("download]")
        .nth(1)?
        .trim()
        .trim_end_matches('%')
        .split_whitespace()
        .next()?;
    pct.parse::<f32>().ok()
}
