//! Bilibili API Client with cookie management and WBI signing

use super::wbi;
use crate::storage::{Credentials, VideoQuality};
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use reqwest::Client;
use reqwest::header::{COOKIE, HeaderMap, HeaderValue, ORIGIN, REFERER, USER_AGENT};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::io::Write;
use std::sync::RwLock;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub enum BilibiliApiDomain {
    Main,
    Passport,
    Live,
}

impl BilibiliApiDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            BilibiliApiDomain::Main => "https://api.bilibili.com",
            BilibiliApiDomain::Passport => "https://passport.bilibili.com",
            BilibiliApiDomain::Live => "https://api.live.bilibili.com",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    #[allow(dead_code)]
    pub ttl: Option<i32>,
    pub data: Option<T>,
}

/// WBI keys for signing requests
#[derive(Debug, Clone)]
pub struct WbiKeys {
    pub img_key: String,
    pub sub_key: String,
}

pub struct ApiClient {
    client: Client,
    cookies: RwLock<Option<String>>,
    wbi_keys: RwLock<Option<WbiKeys>>,
}

impl ApiClient {
    fn safe_url(url: &str) -> String {
        reqwest::Url::parse(url)
            .map(|url| {
                format!(
                    "{}://{}{}",
                    url.scheme(),
                    url.host_str().unwrap_or(""),
                    url.path()
                )
            })
            .unwrap_or_else(|_| "<invalid URL>".to_string())
    }

    fn write_decode_diagnostic(url: &str, error: &serde_json::Error) {
        let Some(mut dir) = dirs::config_dir() else {
            return;
        };
        dir.push("bilibili-tui");
        if fs::create_dir_all(&dir).is_err() {
            return;
        }

        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let safe_url = Self::safe_url(url);
        let log_path = dir.join("debug.log");
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        if let Ok(mut log) = options.open(&log_path) {
            let _ = writeln!(
                log,
                "[{timestamp}] JSON decode failed\nURL: {safe_url}\nError: {error}\n"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&log_path, fs::Permissions::from_mode(0o600));
            }
        }
    }

    pub fn new() -> Self {
        // Older builds persisted complete failed API responses. Remove that
        // legacy diagnostic because it can contain account-specific data.
        if let Some(mut path) = dirs::config_dir() {
            path.push("bilibili-tui");
            path.push("last-decode-error.json");
            let _ = fs::remove_file(path);
        }
        Self {
            client: Client::builder()
                .default_headers(Self::default_headers())
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("Failed to create HTTP client"),
            cookies: RwLock::new(None),
            wbi_keys: RwLock::new(None),
        }
    }

    pub fn with_cookies(credentials: &Credentials) -> Self {
        let client = Self::new();
        client.set_credentials(credentials);
        client
    }

    fn default_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(
            REFERER,
            HeaderValue::from_static("https://www.bilibili.com/"),
        );
        headers
    }

    pub fn set_credentials(&self, credentials: &Credentials) {
        let cookie_str = format!(
            "SESSDATA={}; bili_jct={}; DedeUserID={}",
            credentials.sessdata, credentials.bili_jct, credentials.dede_user_id
        );
        *self.cookies.write().expect("cookies lock poisoned") = Some(cookie_str);
    }

    pub fn clear_credentials(&self) {
        *self.cookies.write().expect("cookies lock poisoned") = None;
    }

    fn build_url(&self, domain: BilibiliApiDomain, endpoint: &str) -> String {
        format!("{}{}", domain.as_str(), endpoint)
    }

    fn check_code(value: &serde_json::Value) -> Result<()> {
        let code = value.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let msg = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("API error ({}): {}", code, msg));
        }
        Ok(())
    }

    /// Make a GET request
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<ApiResponse<T>> {
        // A Bilibili CDN connection can occasionally close while the compressed
        // response body is being read. Retrying a read-only GET once prevents a
        // transient truncated body from surfacing as a dynamic-page failure.
        let safe_url = Self::safe_url(url);
        for attempt in 0..2 {
            let mut req = self.client.get(url);
            if let Some(ref cookies) = *self.cookies.read().expect("cookies lock poisoned") {
                req = req.header(COOKIE, cookies.as_str());
            }

            let resp = match req.send().await {
                Ok(resp) => resp,
                Err(_) if attempt == 0 => {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    continue;
                }
                Err(error) => {
                    return Err(error).with_context(|| format!("request failed for {safe_url}"));
                }
            };
            let status = resp.status();
            let body = match resp.bytes().await {
                Ok(body) => body,
                Err(_) if attempt == 0 => continue,
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to read response body from {safe_url}"));
                }
            };

            if !status.is_success() {
                if attempt == 0 && (status.is_server_error() || status.as_u16() == 429) {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    continue;
                }
                return Err(anyhow!("HTTP {status} from {safe_url}"));
            }

            let response: ApiResponse<T> = serde_json::from_slice(&body).map_err(|error| {
                Self::write_decode_diagnostic(url, &error);
                anyhow!("invalid JSON response from {safe_url}: {error}")
            })?;
            if response.code != 0 {
                return Err(anyhow!(
                    "API error ({}): {}",
                    response.code,
                    response.message
                ));
            }
            return Ok(response);
        }

        unreachable!("GET retry loop always returns")
    }

    /// Make a GET request and return raw JSON
    pub async fn get_json(&self, url: &str) -> Result<serde_json::Value> {
        let mut req = self.client.get(url);
        if let Some(ref cookies) = *self.cookies.read().expect("cookies lock poisoned") {
            req = req.header(COOKIE, cookies.as_str());
        }
        let resp = req.send().await?.error_for_status()?;
        let value: serde_json::Value = resp.json().await?;
        Ok(value)
    }

    /// Make a POST request with form data
    pub async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        form_data: Vec<(&str, String)>,
    ) -> Result<ApiResponse<T>> {
        let owned: Vec<(String, String)> = form_data
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        self.post_with_owned(url, owned).await
    }

    /// Internal POST that accepts owned key-value pairs (avoids lifetime
    /// issues when callers need to add dynamically computed params).
    async fn post_with_owned<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        form_data: Vec<(String, String)>,
    ) -> Result<ApiResponse<T>> {
        // Interaction endpoints (like/coin/fav/relation) are risk-controlled:
        // without the buvid3/buvid4 fingerprint cookies Bilibili answers 412.
        self.ensure_buvid_cookies().await?;
        // The web front-end also sends b_lsid/b_nut local-session cookies on
        // every interaction; a missing pair can also produce 412.
        self.ensure_lsid_cookies();

        let mut req = self.client.post(url);

        // 使用块作用域确保锁在 await 之前释放
        let params = {
            let cookies = self.cookies.read().expect("cookies lock poisoned");
            if let Some(ref cookie_str) = *cookies {
                req = req.header(COOKIE, cookie_str.as_str());
            }

            let has_csrf = cookies
                .as_ref()
                .map(|c| c.contains("bili_jct"))
                .unwrap_or(false);

            let mut params = form_data;

            if has_csrf
                && !params.iter().any(|(k, _)| k == "csrf")
                && let Some(cookie_str) = cookies.as_ref()
                && let Some(csrf) = cookie_str.split(';').find_map(|part| {
                    let part = part.trim();
                    part.split_once('=')
                        .filter(|(name, _)| *name == "bili_jct")
                        .map(|(_, value)| value.to_string())
                })
            {
                params.push(("csrf".to_string(), csrf));
            }
            params
        }; // 锁在此处释放

        // B站 interaction endpoints require Referer/Origin headers.
        req = req
            .header(REFERER, "https://www.bilibili.com")
            .header(ORIGIN, "https://www.bilibili.com");
        req = req.form(&params);
        let resp = req.send().await?.error_for_status()?;
        let api_resp: ApiResponse<T> = resp.json().await?;
        if api_resp.code != 0 {
            return Err(anyhow!(
                "API error ({}): {}",
                api_resp.code,
                api_resp.message
            ));
        }
        Ok(api_resp)
    }

    /// Make a WBI-signed GET request
    pub async fn get_with_wbi<T: for<'de> Deserialize<'de>>(
        &self,
        base_url: &str,
        params: Vec<(&str, String)>,
    ) -> Result<ApiResponse<T>> {
        // Ensure we have WBI keys
        self.ensure_wbi_keys().await?;

        let query = {
            let keys = self.wbi_keys.read().expect("wbi_keys lock poisoned");
            let keys = keys
                .as_ref()
                .expect("WBI keys should be set after ensure_wbi_keys");
            wbi::encode_wbi(params, &keys.img_key, &keys.sub_key)
        };
        let url = format!("{}?{}", base_url, query);

        self.get(&url).await
    }

    /// Make a WBI-signed POST request with form data (csrf is appended
    /// automatically like `post`). Bilibili requires WBI signatures on
    /// interaction endpoints (like / coin / favorite) since 2024.
    ///
    /// For POST requests the WBI params (`wts` + `w_rid`) must be in the
    /// POST body, NOT in the URL query string.
    pub async fn post_with_wbi<T: for<'de> Deserialize<'de>>(
        &self,
        base_url: &str,
        form_data: Vec<(&str, String)>,
    ) -> Result<ApiResponse<T>> {
        // Ensure we have WBI keys
        self.ensure_wbi_keys().await?;

        let signed_data = {
            let keys = self.wbi_keys.read().expect("wbi_keys lock poisoned");
            let keys = keys
                .as_ref()
                .expect("WBI keys should be set after ensure_wbi_keys");
            let mixin_key = wbi::get_mixin_key(&keys.img_key, &keys.sub_key);

            // Add wts (timestamp)
            let cur_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();
            let mut params: Vec<(String, String)> = form_data
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
            params.push(("wts".to_string(), cur_time.to_string()));

            // Sort by key, build query string for w_rid computation
            params.sort_by(|a, b| a.0.cmp(&b.0));
            let query: String = params
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        k.chars()
                            .map(|c| {
                                if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
                                    c.to_string()
                                } else {
                                    format!("%{:02X}", c as u8)
                                }
                            })
                            .collect::<String>(),
                        v.chars()
                            .map(|c| {
                                if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
                                    c.to_string()
                                } else {
                                    format!("%{:02X}", c as u8)
                                }
                            })
                            .collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join("&");

            // Calculate w_rid (MD5 of sorted query + mixin_key)
            let w_rid = format!("{:?}", md5::compute(query + &mixin_key));
            params.push(("w_rid".to_string(), w_rid));

            params
        };

        match self.post_with_owned(base_url, signed_data.clone()).await {
            Ok(resp) => Ok(resp),
            Err(err)
                if err.to_string().contains("-403")
                    || err.to_string().contains("-412")
                    || err.to_string().contains("412") =>
            {
                // Risk control answers 412/403 when the request looks
                // automated. Do NOT refresh the buvid fingerprint here:
                // swapping the device fingerprint on every failure makes
                // Bilibili treat the account as anomalous and escalates the
                // risk-control window. Replay the exact same payload once;
                // if it still fails, surface the error to the user.
                self.post_with_owned(base_url, signed_data).await
            }
            Err(err) => Err(err),
        }
    }

    /// Fetch WBI keys from nav API
    async fn ensure_wbi_keys(&self) -> Result<()> {
        if self
            .wbi_keys
            .read()
            .expect("wbi_keys lock poisoned")
            .is_some()
        {
            return Ok(());
        }

        #[derive(Deserialize)]
        struct WbiImg {
            img_url: String,
            sub_url: String,
        }

        #[derive(Deserialize)]
        struct NavData {
            wbi_img: WbiImg,
        }

        let url = self.build_url(BilibiliApiDomain::Main, "/x/web-interface/nav");
        let resp: ApiResponse<NavData> = self.get(&url).await?;

        if let Some(data) = resp.data {
            let img_key = wbi::extract_key_from_url(&data.wbi_img.img_url)
                .ok_or_else(|| anyhow::anyhow!("Failed to extract img_key"))?;
            let sub_key = wbi::extract_key_from_url(&data.wbi_img.sub_url)
                .ok_or_else(|| anyhow::anyhow!("Failed to extract sub_key"))?;

            *self.wbi_keys.write().expect("wbi_keys lock poisoned") =
                Some(WbiKeys { img_key, sub_key });
        }

        Ok(())
    }

    // Auth APIs

    /// Fetch the currently logged-in user profile (mid, uname, face, level).
    /// Returns `None` when the request succeeds but the account is not logged in.
    pub async fn get_current_user(&self) -> Result<Option<super::auth::CurrentUser>> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/web-interface/nav");
        let resp: ApiResponse<serde_json::Value> = self.get(&url).await?;
        let Some(data) = resp.data else {
            return Ok(None);
        };
        let is_login = data
            .get("isLogin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_login {
            return Ok(None);
        }
        let mid = data.get("mid").and_then(|v| v.as_i64()).unwrap_or(0);
        let uname = data
            .get("uname")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let face = data
            .get("face")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let level_info = data.get("level_info");
        let level = level_info
            .and_then(|v| v.get("current_level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let current_min = level_info
            .and_then(|v| v.get("current_min"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let current_exp = level_info
            .and_then(|v| v.get("current_exp"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let next_exp = level_info
            .and_then(|v| v.get("next_exp"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        Ok(Some(super::auth::CurrentUser {
            mid,
            uname,
            face,
            level,
            current_min,
            current_exp,
            next_exp,
        }))
    }

    pub async fn get_qrcode_data(&self) -> Result<super::auth::QrcodeData> {
        let url = self.build_url(
            BilibiliApiDomain::Passport,
            "/x/passport-login/web/qrcode/generate",
        );
        let resp: ApiResponse<super::auth::QrcodeData> = self.get(&url).await?;
        resp.data
            .ok_or_else(|| anyhow::anyhow!("No data in QR code response"))
    }

    pub async fn poll_qrcode(&self, qrcode_key: &str) -> Result<super::auth::QrcodePollResult> {
        let url = format!(
            "{}/x/passport-login/web/qrcode/poll?qrcode_key={}",
            BilibiliApiDomain::Passport.as_str(),
            qrcode_key
        );

        let mut req = self.client.get(&url);
        if let Some(ref cookies) = *self.cookies.read().unwrap() {
            req = req.header(COOKIE, cookies.as_str());
        }

        let resp = req.send().await?.error_for_status()?;

        // Extract cookies from response headers
        let mut new_cookies = Vec::new();
        for cookie in resp.cookies() {
            new_cookies.push((cookie.name().to_string(), cookie.value().to_string()));
        }

        let api_resp: ApiResponse<super::auth::QrcodePollData> = resp.json().await?;

        Ok(super::auth::QrcodePollResult {
            data: api_resp.data,
            cookies: new_cookies,
        })
    }

    // Recommendation API
    pub async fn get_recommendations(&self) -> Result<Vec<super::recommend::VideoItem>> {
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/web-interface/wbi/index/top/feed/rcmd",
        );

        let params = vec![
            ("fresh_type", "4".to_string()),
            ("ps", "20".to_string()),
            ("fresh_idx", "1".to_string()),
            ("fresh_idx_1h", "1".to_string()),
        ];

        let resp: ApiResponse<super::recommend::RecommendData> =
            self.get_with_wbi(&url, params).await?;

        Ok(resp
            .data
            .map(|d| d.item.into_iter().filter(|v| v.bvid.is_some()).collect())
            .unwrap_or_default())
    }

    /// Guest homepage videos from popular feed
    pub async fn get_popular_videos(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<Vec<super::recommend::VideoItem>> {
        let url = format!(
            "{}/x/web-interface/popular?pn={}&ps={}",
            BilibiliApiDomain::Main.as_str(),
            page.max(1),
            page_size.max(1)
        );

        let mut req = self.client.get(&url);
        if let Some(ref cookies) = *self.cookies.read().expect("cookies lock poisoned") {
            req = req.header(COOKIE, cookies.as_str());
        }

        let value: serde_json::Value = req.send().await?.error_for_status()?.json().await?;
        let code = value
            .get("code")
            .and_then(|v| v.as_i64())
            .unwrap_or_default();
        if code != 0 {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("Popular API error {}: {}", code, message));
        }

        let list = value
            .get("data")
            .and_then(|d| d.get("list"))
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(parse_home_videos(list))
    }

    pub async fn get_home_feed(
        &self,
        feed: super::recommend::HomeFeed,
        page: i32,
        page_size: i32,
        rid: i64,
    ) -> Result<Vec<super::recommend::VideoItem>> {
        use super::recommend::HomeFeed;
        let path = match feed {
            HomeFeed::Recommended => {
                // The authenticated homepage recommendation is selected by the
                // network worker; this fallback keeps the public helper useful
                // for callers without a login context.
                return self.get_popular_videos(page, page_size).await;
            }
            HomeFeed::Popular => {
                return self.get_popular_videos(page, page_size).await;
            }
            HomeFeed::Weekly => {
                let list_url = format!(
                    "{}/x/web-interface/popular/series/list",
                    BilibiliApiDomain::Main.as_str()
                );
                let list: ApiResponse<serde_json::Value> = self
                    .get_with_wbi(&list_url, vec![("web_location", "333.934".to_string())])
                    .await?;
                let number = list
                    .data
                    .as_ref()
                    .and_then(|data| data.get("list"))
                    .and_then(|list| list.as_array())
                    .and_then(|list| list.first())
                    .and_then(|item| item.get("number"))
                    .and_then(|number| number.as_i64())
                    .ok_or_else(|| anyhow!("每周必看期数为空"))?;
                let one_url = format!(
                    "{}/x/web-interface/popular/series/one",
                    BilibiliApiDomain::Main.as_str()
                );
                let value: ApiResponse<serde_json::Value> = self
                    .get_with_wbi(
                        &one_url,
                        vec![
                            ("number", number.to_string()),
                            ("web_location", "333.934".to_string()),
                        ],
                    )
                    .await?;
                let list = value
                    .data
                    .as_ref()
                    .and_then(|data| data.get("list"))
                    .and_then(|list| list.as_array())
                    .cloned()
                    .unwrap_or_default();
                return Ok(parse_home_videos(list));
            }
            HomeFeed::Ranking => {
                format!("/x/web-interface/ranking/v2?rid={rid}&type=all")
            }
            HomeFeed::MustWatch => "/x/web-interface/popular/precious".to_string(),
        };
        let url = format!("{}{}", BilibiliApiDomain::Main.as_str(), path);
        let value: ApiResponse<serde_json::Value> = self.get(&url).await?;
        let list = value
            .data
            .as_ref()
            .and_then(|data| data.get("list"))
            .and_then(|list| list.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(parse_home_videos(list))
    }

    // Video API
    pub async fn get_video_info(&self, bvid: &str) -> Result<super::video::VideoInfo> {
        let url = format!(
            "{}/x/web-interface/view?bvid={}",
            BilibiliApiDomain::Main.as_str(),
            bvid
        );
        let resp: ApiResponse<super::video::VideoInfo> = self.get(&url).await?;
        resp.data
            .ok_or_else(|| anyhow::anyhow!("No data in video info response"))
    }

    pub async fn get_play_url(
        &self,
        bvid: &str,
        cid: i64,
        options: crate::domain::playback::PlaybackOptions,
    ) -> Result<super::cdn::PlayUrlData> {
        let qn = if options.quality > 0 {
            options.quality
        } else {
            127 // auto: request the full stream list and pick locally
        };
        let url = self.build_url(BilibiliApiDomain::Main, "/x/player/wbi/playurl");
        let resp: ApiResponse<super::cdn::PlayUrlData> = self
            .get_with_wbi(
                &url,
                vec![
                    ("bvid", bvid.to_string()),
                    ("cid", cid.to_string()),
                    ("qn", qn.to_string()),
                    ("fnver", "0".to_string()),
                    ("fnval", "4048".to_string()),
                    ("fourk", "1".to_string()),
                ],
            )
            .await?;
        if resp.code != 0 {
            return Err(anyhow!("playurl API error {}: {}", resp.code, resp.message));
        }
        resp.data
            .ok_or_else(|| anyhow!("playurl response has no data"))
    }

    pub async fn get_bangumi_play_url(
        &self,
        ep_id: i64,
        quality: crate::storage::VideoQuality,
    ) -> Result<super::cdn::PlayUrlData> {
        let url = format!(
            "{}/pgc/player/web/v2/playurl?ep_id={ep_id}&qn={}&otype=json&fnval=4048&fourk=1&from_client=BROWSER&is_main_page=false&need_fragment=false&isGaiaAvoided=true&web_location=1315873",
            BilibiliApiDomain::Main.as_str(),
            quality.qn()
        );
        let value = self.get_json(&url).await?;
        Self::parse_bangumi_play_url(&value)
    }

    fn parse_bangumi_play_url(value: &serde_json::Value) -> Result<super::cdn::PlayUrlData> {
        let mut payload = value;
        if payload.get("code").is_some() {
            Self::check_code(payload)?;
        }
        if let Some(raw) = payload.get("raw") {
            payload = raw;
        }
        if let Some(data) = payload.get("data") {
            payload = data;
            if payload.get("code").is_some() {
                Self::check_code(payload)?;
            }
        }
        if let Some(result) = payload.get("result") {
            payload = result;
        }
        if let Some(video_info) = payload.get("video_info") {
            payload = video_info;
        }
        serde_json::from_value(payload.clone()).context("invalid bangumi playurl response")
    }

    pub async fn get_video_danmaku(
        &self,
        cid: i64,
        duration_secs: i64,
    ) -> Result<Vec<super::danmaku::VideoDanmaku>> {
        // Two endpoints complement each other:
        //   - seg.so (segmented protobuf) returns the full danmaku history
        //     in 6-minute segments, but for this video it is dominated by
        //     positioned (mode 7/8) danmaku - only a handful of regular
        //     rolling comments show up there.
        //   - The XML endpoint returns the most recent ~600 comments, which
        //     is where the bulk of regular (mode 1/4/5) danmaku lives.
        // Fetch both and merge: XML entries that are not already present in
        // the segmented history (same time + text) get appended.
        //
        // Segment requests are fetched concurrently (8 at a time) instead of
        // serially - for a long video the old loop waited on up to 64
        // round-trips before mpv could even start.
        let segment_count = ((duration_secs.max(0) as usize).div_ceil(360)).clamp(1, 64);
        let mut all = Vec::new();
        for chunk in (1..=segment_count).collect::<Vec<_>>().chunks(8) {
            let fetched = futures_util::stream::iter(chunk.iter().copied())
                .map(|segment_index| {
                    let client = self.client.clone();
                    async move {
                        let url = format!(
                            "https://api.bilibili.com/x/v2/dm/web/seg.so?type=1&oid={cid}&segment_index={segment_index}"
                        );
                        let response = client.get(&url).send().await?;
                        if !response.status().is_success() {
                            return Ok::<Vec<super::danmaku::VideoDanmaku>, reqwest::Error>(
                                Vec::new(),
                            );
                        }
                        let bytes = response.bytes().await?;
                        Ok(super::danmaku::parse_seg_protobuf(&bytes))
                    }
                })
                .buffered(8)
                .collect::<Vec<_>>()
                .await;
            for parsed in fetched {
                if let Ok(parsed) = parsed {
                    all.extend(parsed);
                }
            }
        }

        // Always also pull the XML endpoint for the regular danmaku it
        // carries (the segmented endpoint can be almost all mode 7/8).
        let xml_url = format!("https://comment.bilibili.com/{cid}.xml");
        if let Ok(response) = self.client.get(&xml_url).send().await
            && response.status().is_success()
        {
            let encoding = response
                .headers()
                .get(reqwest::header::CONTENT_ENCODING)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let bytes = response.bytes().await?;
            let body = if encoding.contains("deflate") {
                let mut decoded = Vec::new();
                if flate2::read::DeflateDecoder::new(bytes.as_ref())
                    .read_to_end(&mut decoded)
                    .is_err()
                {
                    decoded.clear();
                    flate2::read::ZlibDecoder::new(bytes.as_ref()).read_to_end(&mut decoded)?;
                }
                String::from_utf8(decoded)?
            } else {
                String::from_utf8(bytes.to_vec())?
            };
            if let Ok(xml_items) = super::danmaku::parse_xml(&body) {
                for item in xml_items {
                    let duplicate = all.iter().any(|existing| {
                        (existing.time - item.time).abs() < 0.5 && existing.text == item.text
                    });
                    if !duplicate {
                        all.push(item);
                    }
                }
            }
        }

        all.sort_by(|left, right| left.time.total_cmp(&right.time));
        Ok(all)
    }

    /// Fetch the AI subtitle track list for a video. Returns an empty list
    /// when the video has no AI subtitles (or the player API hides them).
    pub async fn get_video_subtitles(
        &self,
        bvid: &str,
        cid: i64,
    ) -> Result<Vec<super::subtitle::SubtitleInfo>> {
        #[derive(serde::Deserialize)]
        struct PlayerV2Data {
            subtitle: PlayerSubtitle,
        }
        #[derive(serde::Deserialize)]
        struct PlayerSubtitle {
            #[serde(default)]
            subtitles: Vec<super::subtitle::SubtitleInfo>,
        }
        let url = self.build_url(BilibiliApiDomain::Main, "/x/player/wbi/v2");
        let resp: ApiResponse<PlayerV2Data> = self
            .get_with_wbi(
                &url,
                vec![("bvid", bvid.to_string()), ("cid", cid.to_string())],
            )
            .await?;
        if resp.code != 0 {
            return Err(anyhow!("player v2 API error {}: {}", resp.code, resp.message));
        }
        Ok(resp
            .data
            .map(|data| data.subtitle.subtitles)
            .unwrap_or_default())
    }

    /// Fetch the cue list of one subtitle track by its (usually relative)
    /// subtitle URL. Empty body means no cues (bad track).
    pub async fn fetch_subtitle_cues(
        &self,
        subtitle_url: &str,
    ) -> Result<Vec<super::subtitle::SubtitleCue>> {
        let url = if subtitle_url.starts_with("//") {
            format!("https:{subtitle_url}")
        } else if subtitle_url.starts_with("http") {
            subtitle_url.to_string()
        } else {
            format!("https://{subtitle_url}")
        };
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Ok(Vec::new());
        }
        let text = response.text().await?;
        super::subtitle::parse_subtitle_body(&text).map_err(anyhow::Error::from)
    }

    /// Load the public profile shown at space.bilibili.com/{mid}.
    pub async fn get_space_info(&self, mid: i64) -> Result<super::space::SpaceInfo> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/space/wbi/acc/info");
        let resp: ApiResponse<super::space::SpaceInfo> = self
            .get_with_wbi(&url, vec![("mid", mid.to_string())])
            .await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "space info API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("space info response has no data"))
    }

    pub async fn get_relation_stat(&self, mid: i64) -> Result<super::space::RelationStat> {
        let url = format!(
            "{}/x/relation/stat?vmid={mid}",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::space::RelationStat> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "relation API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("relation response has no data"))
    }

    /// Load an UP's submissions using the same `pubdate`/`click` order values
    /// as the web space's 最新发布/最多播放 controls.
    pub async fn get_space_videos(
        &self,
        mid: i64,
        page: i32,
        page_size: i32,
        order: super::space::SpaceVideoOrder,
    ) -> Result<super::space::SpaceVideoData> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/space/wbi/arc/search");
        let params = vec![
            ("mid", mid.to_string()),
            ("pn", page.to_string()),
            ("ps", page_size.to_string()),
            ("tid", "0".to_string()),
            ("special_type", String::new()),
            ("order", order.api_value().to_string()),
            ("index", "0".to_string()),
            ("keyword", String::new()),
            ("order_avoided", "true".to_string()),
            ("platform", "web".to_string()),
        ];
        let resp: ApiResponse<super::space::SpaceVideoData> =
            self.get_with_wbi(&url, params).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "space videos API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("space videos response has no data"))
    }

    /// List public favorite folders created by a user. Private folders remain
    /// visible only when the authenticated account has permission.
    pub async fn get_favorite_folders(
        &self,
        owner_mid: i64,
    ) -> Result<Vec<super::favorite::FavoriteFolder>> {
        let url = format!(
            "{}/x/v3/fav/folder/created/list-all?up_mid={owner_mid}",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::favorite::FavoriteFolderData> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "favorite folders API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        Ok(resp.data.map(|data| data.list).unwrap_or_default())
    }

    /// Load one page of a favorite folder in the folder's web order.
    pub async fn get_favorite_resources(
        &self,
        media_id: i64,
        page: i32,
        page_size: i32,
        order: super::favorite::FavoriteOrder,
    ) -> Result<super::favorite::FavoriteResourceData> {
        let url = format!(
            "{}/x/v3/fav/resource/list?media_id={media_id}&pn={page}&ps={page_size}&order={}&type=0&tid=0&platform=web",
            BilibiliApiDomain::Main.as_str(),
            order.api_value()
        );
        let resp: ApiResponse<super::favorite::FavoriteResourceData> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "favorite resources API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("favorite resources response has no data"))
    }

    /// List video series (合集) created by an uploader.
    pub async fn get_series_list(
        &self,
        mid: i64,
        page: i32,
        page_size: i32,
    ) -> Result<super::space::SeriesListData> {
        let url = format!(
            "{}/x/polymer/web-space/seasons_series_list?mid={mid}&page_num={page}&page_size={page_size}",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::space::SeriesListData> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "series list API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("series list response has no data"))
    }

    /// Load one page of videos from a series (合集).
    pub async fn get_series_archives(
        &self,
        mid: i64,
        season_id: i64,
        page: i32,
        page_size: i32,
    ) -> Result<super::space::SeriesArchivesData> {
        let url = format!(
            "{}/x/polymer/web-space/seasons_archives_list?mid={mid}&season_id={season_id}&page_num={page}&page_size={page_size}",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::space::SeriesArchivesData> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "series archives API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("series archives response has no data"))
    }

    pub async fn get_watch_later(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<super::favorite::WatchLaterData> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/history/toview/web");
        let resp: ApiResponse<super::favorite::WatchLaterData> = self
            .get_with_wbi(
                &url,
                vec![
                    ("pn", page.to_string()),
                    ("ps", page_size.to_string()),
                    ("viewed", "0".to_string()),
                    ("key", String::new()),
                    ("asc", "false".to_string()),
                    ("need_split", "true".to_string()),
                ],
            )
            .await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "watch later API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("watch later response has no data"))
    }

    /// Add a video (by aid) to the user's watch-later list.
    pub async fn add_to_watch_later(&self, aid: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/history/toview/add");
        let _: ApiResponse<serde_json::Value> = self
            .post(&url, vec![("aid", aid.to_string())])
            .await?;
        Ok(())
    }

    /// Remove a video (by aid) from the user's watch-later list.
    pub async fn remove_from_watch_later(&self, aid: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/history/toview/del");
        let _: ApiResponse<serde_json::Value> = self
            .post(&url, vec![("aid", aid.to_string())])
            .await?;
        Ok(())
    }

    /// Check whether a video (by aid) is in the user's watch-later list.
    /// Uses the first page of the toview list; good enough for detail-page status.
    pub async fn get_watch_later_status(&self, aid: i64) -> Result<bool> {
        let data = self.get_watch_later(1, 50).await?;
        Ok(data.list.iter().any(|item| item.aid == aid))
    }

    /// Look up the last watch progress (seconds) for a video by bvid from the
    /// most recent history page. Returns None when there is no record.
    pub async fn get_video_history_progress(&self, bvid: &str) -> Result<Option<i64>> {
        let data = self.get_history(None, None, Some("archive")).await?;
        Ok(data
            .list
            .iter()
            .find(|item| item.get_bvid() == Some(bvid))
            .map(|item| item.progress))
    }

    pub async fn get_collected_folders(
        &self,
        mid: i64,
        page: i32,
        page_size: i32,
    ) -> Result<super::favorite::CollectedFolderData> {
        let url = format!(
            "{}/x/v3/fav/folder/collected/list?pn={page}&ps={page_size}&up_mid={mid}&platform=web",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::favorite::CollectedFolderData> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "collected folders API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("collected folders response has no data"))
    }

    pub async fn get_collected_season_videos(
        &self,
        mid: i64,
        season_id: i64,
        page: i32,
        page_size: i32,
    ) -> Result<super::favorite::SeasonArchivesData> {
        let url = format!(
            "{}/x/polymer/web-space/seasons_archives_list?mid={mid}&season_id={season_id}&sort_reverse=false&page_num={page}&page_size={page_size}",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::favorite::SeasonArchivesData> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "season archives API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("season archives response has no data"))
    }

    // Search API
    pub async fn search_videos(
        &self,
        keyword: &str,
        page: i32,
    ) -> Result<super::search::SearchData> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/web-interface/wbi/search/type");

        let params = vec![
            ("search_type", "video".to_string()),
            ("keyword", keyword.to_string()),
            ("page", page.to_string()),
            ("order", "totalrank".to_string()),
        ];

        let resp: ApiResponse<super::search::SearchData> = self.get_with_wbi(&url, params).await?;
        Ok(resp.data.unwrap_or(super::search::SearchData {
            result: None,
            num_results: Some(0),
            page: Some(page),
            pagesize: Some(20),
        }))
    }

    /// Search for users (UP主) by keyword
    pub async fn search_users(
        &self,
        keyword: &str,
        page: i32,
    ) -> Result<super::search::SearchUserData> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/web-interface/wbi/search/type");

        let params = vec![
            ("search_type", "bili_user".to_string()),
            ("keyword", keyword.to_string()),
            ("page", page.to_string()),
            ("order", "totalrank".to_string()),
        ];

        let resp: ApiResponse<super::search::SearchUserData> = self.get_with_wbi(&url, params).await?;
        Ok(resp.data.unwrap_or(super::search::SearchUserData {
            result: None,
            num_results: Some(0),
            page: Some(page),
            pagesize: Some(20),
        }))
    }

    /// Search bangumi/season by keyword (search_type=media_bangumi)
    pub async fn search_bangumi(
        &self,
        keyword: &str,
        page: i32,
    ) -> Result<Vec<super::search::SearchBangumiItem>> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/web-interface/wbi/search/type");

        let params = vec![
            ("search_type", "media_bangumi".to_string()),
            ("keyword", keyword.to_string()),
            ("page", page.to_string()),
            ("order", "totalrank".to_string()),
        ];

        #[derive(serde::Deserialize)]
        struct BangumiSearchData {
            result: Option<Vec<super::search::SearchBangumiItem>>,
        }

        let resp: ApiResponse<BangumiSearchData> = self.get_with_wbi(&url, params).await?;
        Ok(resp.data.and_then(|d| d.result).unwrap_or_default())
    }

    /// Fetch hot search keywords (web)
    pub async fn get_hot_search(&self) -> Result<Vec<super::search::HotwordItem>> {
        const HOTWORD_URL: &str = "https://s.search.bilibili.com/main/hotword";

        let mut req = self.client.get(HOTWORD_URL);

        if let Some(ref cookies) = *self.cookies.read().expect("cookies lock poisoned") {
            req = req.header(COOKIE, cookies.as_str());
        }

        let resp = req.send().await?.error_for_status()?;
        let data: super::search::HotwordResponse = resp.json().await?;

        if let Some(code) = data.code
            && code != 0
        {
            let msg = data.message.unwrap_or_else(|| "unknown error".to_string());
            return Err(anyhow!("Hot search API error: {}", msg));
        }

        Ok(data.list.unwrap_or_default())
    }

    // Bangumi API
    pub async fn get_bangumi_timeline(&self) -> Result<Vec<super::bangumi::TimelineDay>> {
        let url = self.build_url(BilibiliApiDomain::Main, "/pgc/web/timeline?types=1");
        let value = self.get_json(&url).await?;
        Self::check_code(&value)?;
        let result = value
            .get("result")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Ok(result)
    }

    pub async fn get_bangumi_rank(&self) -> Result<Vec<super::bangumi::SeasonRankItem>> {
        let url = format!(
            "{}/pgc/web/rank/list?day=3&season_type=1",
            BilibiliApiDomain::Main.as_str(),
        );
        let value = self.get_json(&url).await?;
        Self::check_code(&value)?;
        let list = value
            .get("result")
            .and_then(|r| r.get("list"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Ok(list)
    }

    pub async fn get_bangumi_season(&self, season_id: i64) -> Result<super::bangumi::SeasonResult> {
        let url = format!(
            "{}/pgc/view/web/season?season_id={}",
            BilibiliApiDomain::Main.as_str(),
            season_id,
        );
        let value = self.get_json(&url).await?;
        Self::check_code(&value)?;
        let result_val = value
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("API 返回缺少 result 字段"))?;
        let result: super::bangumi::SeasonResult =
            serde_json::from_value(result_val).map_err(|e| anyhow!("解析番剧详情失败: {}", e))?;
        Ok(result)
    }

    /// Resolve a single bangumi episode by its ep id, returning the episode
    /// record (used for its cid, which is needed to fetch danmaku).
    pub async fn get_bangumi_episode_info(
        &self,
        ep_id: i64,
    ) -> Result<super::bangumi::BangumiEpisode> {
        let url = format!(
            "{}/pgc/view/web/season?ep_id={}",
            BilibiliApiDomain::Main.as_str(),
            ep_id,
        );
        let value = self.get_json(&url).await?;
        Self::check_code(&value)?;
        let result_val = value
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("API 返回缺少 result 字段"))?;
        let result: super::bangumi::SeasonResult =
            serde_json::from_value(result_val).map_err(|e| anyhow!("解析番剧详情失败: {}", e))?;
        for section in result.all_sections() {
            if let Some(episode) = section.episodes.iter().find(|ep| ep.id == ep_id) {
                return Ok(episode.clone());
            }
        }
        Err(anyhow!("未找到剧集 ep{ep_id}"))
    }

    // Dynamic Feed API
    pub async fn get_dynamic_feed(
        &self,
        offset: Option<&str>,
        feed_type: Option<&str>,
        host_mid: Option<i64>,
    ) -> Result<super::dynamic::DynamicFeedData> {
        let mut url = format!(
            "{}/x/polymer/web-dynamic/v1/feed/all",
            BilibiliApiDomain::Main.as_str()
        );

        let mut params = Vec::new();

        if let Some(ft) = feed_type {
            params.push(format!("type={}", ft));
        }

        if let Some(off) = offset {
            params.push(format!("offset={}", off));
        }

        if let Some(mid) = host_mid {
            params.push(format!("host_mid={}", mid));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let resp: ApiResponse<super::dynamic::DynamicFeedData> = self.get(&url).await?;
        Ok(resp.data.unwrap_or(super::dynamic::DynamicFeedData {
            items: None,
            offset: None,
            has_more: Some(false),
            update_num: Some(0),
        }))
    }

    // Dynamic Detail API
    pub async fn get_dynamic_detail(
        &self,
        dynamic_id: &str,
    ) -> Result<super::dynamic::DynamicItem> {
        let url = format!(
            "{}/x/polymer/web-dynamic/v1/detail?id={}",
            BilibiliApiDomain::Main.as_str(),
            dynamic_id
        );

        #[derive(Deserialize)]
        struct DynamicDetailData {
            item: super::dynamic::DynamicItem,
        }

        let resp: ApiResponse<DynamicDetailData> = self.get(&url).await?;
        resp.data
            .map(|d| d.item)
            .ok_or_else(|| anyhow::anyhow!("No data in dynamic detail response"))
    }

    // Get following users (关注列表)
    pub async fn get_followings(
        &self,
        vmid: i64,
        ps: i32,
        pn: i32,
    ) -> Result<super::dynamic::FollowingsData> {
        let url = format!(
            "{}/x/relation/followings?vmid={}&ps={}&pn={}",
            BilibiliApiDomain::Main.as_str(),
            vmid,
            ps,
            pn
        );

        let resp: ApiResponse<super::dynamic::FollowingsData> = self.get(&url).await?;
        Ok(resp.data.unwrap_or(super::dynamic::FollowingsData {
            list: None,
            total: Some(0),
        }))
    }

    /// Get dynamic portal with frequently watched UP masters (常看UP主)
    pub async fn get_dynamic_portal(&self) -> Result<super::dynamic::PortalData> {
        let url = format!(
            "{}/x/polymer/web-dynamic/v1/portal",
            BilibiliApiDomain::Main.as_str()
        );

         let resp: ApiResponse<super::dynamic::PortalData> = self.get(&url).await?;
         Ok(resp
             .data
             .unwrap_or(super::dynamic::PortalData { up_list: None }))
     }

     // Message notification API
     pub async fn get_msg_unread(&self) -> Result<super::msg::UnreadData> {
         let url = format!("{}/x/msgfeed/unread", BilibiliApiDomain::Main.as_str());
         let resp: ApiResponse<super::msg::UnreadData> = self.get(&url).await?;
         Ok(resp.data.unwrap_or(super::msg::UnreadData {
             at: 0,
             chat: 0,
             like: 0,
             reply: 0,
             sys_msg: 0,
         }))
     }

     /// Fetch a message feed tab. `feed_type`: 1=reply 2=at 3=like 6=system.
     pub async fn get_msg_feed(
         &self,
         feed_type: i32,
         page: i32,
     ) -> Result<Vec<super::msg::NotificationItem>> {
         if feed_type == 6 {
             return self.get_system_notices(page).await;
         }
         let (path, extra) = match feed_type {
             2 => ("/x/msgfeed/at".to_string(), String::new()),
             3 => ("/x/msgfeed/like".to_string(), String::new()),
             _ => ("/x/msgfeed/reply".to_string(), "&type=1".to_string()),
         };
         let url = format!(
             "{}{}?page={}&page_size=20{}",
             BilibiliApiDomain::Main.as_str(),
             path,
             page,
             extra
         );
         if feed_type == 3 {
             // The like feed nests items under data.latest / data.total.
             let resp: ApiResponse<super::msg::LikeFeedData> = self.get(&url).await?;
             let data = resp.data.unwrap_or(super::msg::LikeFeedData {
                 latest: None,
                 total: None,
             });
             let mut items: Vec<super::msg::NotificationItem> = Vec::new();
             if let Some(section) = &data.latest {
                 items.extend(
                     section
                         .items
                         .iter()
                         .filter_map(|v| super::msg::parse_feed_item(v, feed_type)),
                 );
             }
             if let Some(section) = &data.total {
                 items.extend(
                     section
                         .items
                         .iter()
                         .filter_map(|v| super::msg::parse_feed_item(v, feed_type)),
                 );
             }
             return Ok(items);
         }
         let resp: ApiResponse<super::msg::FeedData> = self.get(&url).await?;
         let data = resp.data.unwrap_or(super::msg::FeedData {
             items: Vec::new(),
             page: None,
         });
         Ok(data
             .items
             .iter()
             .filter_map(|v| super::msg::parse_feed_item(v, feed_type))
             .collect())
     }

     /// Fetch system notices from the unified notify endpoint.
     /// This lives on message.bilibili.com (not api.bilibili.com) and the
     /// legacy /x/msgfeed/sys and /x/msgfeed/notice are both unusable for
     /// listing: sys is retired (404) and notice only marks items read.
     pub async fn get_system_notices(&self, _page: i32) -> Result<Vec<super::msg::NotificationItem>> {
         let url = format!(
             "{}/x/sys-msg/query_unified_notify?page_size=20&build=0&mobi_app=web",
             "https://message.bilibili.com"
         );
         let resp: ApiResponse<super::msg::SystemNotifyData> = self.get(&url).await?;
         let data = resp.data.unwrap_or(super::msg::SystemNotifyData {
             system_notify_list: Vec::new(),
         });
         Ok(data
             .system_notify_list
             .iter()
             .filter_map(super::msg::parse_system_notify)
             .collect())
     }

      /// Fetch the private-message conversation list.
      /// GET https://api.vc.bilibili.com/session_svr/v1/session_svr/get_sessions
      /// (the old api.bilibili.com/x/session/v2/sessions was retired in 2026;
      /// the private-message service moved to the vc domain and requires WBI)
      ///
      /// The session endpoint only returns `talker_id` (no user name), so we
      /// resolve display names in parallel via the user card API.
      pub async fn get_msg_sessions(&self) -> Result<Vec<super::msg::ChatSession>> {
          let url = "https://api.vc.bilibili.com/session_svr/v1/session_svr/get_sessions";
          let params: Vec<(&str, String)> = vec![
              ("session_type", "1".to_string()),
              ("group_fold", "1".to_string()),
              ("unfollow_fold", "0".to_string()),
              ("sort_rule", "2".to_string()),
              ("build", "0".to_string()),
              ("mobi_app", "web".to_string()),
          ];
          let resp: ApiResponse<super::msg::SessionListData> =
              self.get_with_wbi(url, params).await?;
          let data = resp.data.unwrap_or(super::msg::SessionListData {
              session_list: Vec::new(),
              has_more: None,
          });
          let mut sessions: Vec<super::msg::ChatSession> = data
              .session_list
              .iter()
              .filter_map(super::msg::parse_session)
              .collect();

          // The session API has no user names; fill them from the user card
          // API in parallel (20 sessions -> 20 small GETs).
          let mids: Vec<i64> = sessions.iter().map(|s| s.talker_id).collect();
          let resolved = self.resolve_user_cards(&mids).await;
          for session in sessions.iter_mut() {
              if let Some(entry) = resolved.get(&session.talker_id) {
                  if !entry.0.is_empty() {
                      session.uname = entry.0.clone();
                  }
                  if session.face.is_none() && !entry.1.is_empty() {
                      session.face = Some(entry.1.clone());
                  }
              }
          }
          Ok(sessions)
      }

      /// Resolve `mid -> (name, face)` for a list of user ids.
      ///
      /// Strategy (the standalone `/x/web-interface/card` endpoint is
      /// frequently rate-limited to -352 from server IPs):
      ///   1. scan the current user's following list (returns uname+face
      ///      and is a stable endpoint);
      ///   2. fill remaining ids with the card API best-effort.
      /// Missing/private users are simply skipped (the caller keeps the
      /// fallback "用户{id}" name).
      async fn resolve_user_cards(&self, mids: &[i64]) -> HashMap<i64, (String, String)> {
          if mids.is_empty() {
              return HashMap::new();
          }
          let mut wanted: HashSet<i64> = mids.iter().copied().collect();
          let mut result: HashMap<i64, (String, String)> = HashMap::new();

          // 1. following list: 2 pages x 50 covers most private-message
          // contacts (people you follow). The endpoint returns uname/face.
          let my_uid = self
              .cookies
              .read()
              .expect("cookies lock poisoned")
              .as_ref()
              .and_then(|c| {
                  c.split(';').find_map(|part| {
                      let part = part.trim();
                      part.split_once('=')
                          .filter(|(name, _)| *name == "DedeUserID")
                          .map(|(_, value)| value.to_string())
                  })
              })
              .unwrap_or_default();
          if !my_uid.is_empty() {
              for pn in 1..=2u32 {
                  if wanted.is_empty() {
                      break;
                  }
                  let url = format!(
                      "{}/x/relation/followings?vmid={}&pn={}&ps=50&order=desc",
                      BilibiliApiDomain::Main.as_str(),
                      my_uid,
                      pn
                  );
                  match self.get::<serde_json::Value>(&url).await {
                      Ok(resp) => {
                          let Some(data) = resp.data else {
                              break;
                          };
                          let Some(list) = data.get("list").and_then(|l| l.as_array()) else {
                              break;
                          };
                          for u in list {
                              let Some(mid) = u.get("mid").and_then(|m| m.as_i64()) else {
                                  continue;
                              };
                              if wanted.remove(&mid) {
                                  let name = u
                                      .get("uname")
                                      .and_then(|n| n.as_str())
                                      .unwrap_or("")
                                      .to_string();
                                  let face = u
                                      .get("face")
                                      .and_then(|f| f.as_str())
                                      .unwrap_or("")
                                      .to_string();
                                  result.insert(mid, (name, face));
                              }
                          }
                      }
                      Err(_) => break,
                  }
              }
          }

          // 2. best-effort card lookup for the rest.
          let remaining: Vec<i64> = wanted.into_iter().collect();
          if remaining.is_empty() {
              return result;
          }
          let futures = remaining.iter().map(|mid| {
              let url = format!(
                  "{}/x/web-interface/card?mid={}",
                  BilibiliApiDomain::Main.as_str(),
                  mid
              );
              async move {
                  let resp: ApiResponse<serde_json::Value> = self.get(&url).await.ok()?;
                  let card = resp.data.as_ref()?.get("card")?;
                  let name = card.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                  let face = card.get("face").and_then(|f| f.as_str()).unwrap_or("").to_string();
                  Some((*mid, (name, face)))
              }
          });
          let results = futures_util::future::join_all(futures).await;
          for (mid, (name, face)) in results.into_iter().flatten() {
              result.insert(mid, (name, face));
          }
          result
      }

     /// Fetch chat history with one user.
     /// GET https://api.vc.bilibili.com/svr_sync/v1/svr_sync/fetch_session_msgs
     pub async fn get_chat_detail(&self, talker_id: i64) -> Result<Vec<super::msg::ChatMessage>> {
         let url = "https://api.vc.bilibili.com/svr_sync/v1/svr_sync/fetch_session_msgs";
         let params: Vec<(&str, String)> = vec![
             ("talker_id", talker_id.to_string()),
             ("session_type", "1".to_string()),
             ("size", "30".to_string()),
             ("build", "0".to_string()),
             ("mobi_app", "web".to_string()),
             ("sender_device_id", "1".to_string()),
         ];
         let resp: ApiResponse<super::msg::ChatDetailData> =
             self.get_with_wbi(url, params).await?;
         let data = resp.data.unwrap_or(super::msg::ChatDetailData {
             messages: Vec::new(),
             has_more: None,
         });
         Ok(data
             .messages
             .iter()
             .filter_map(super::msg::parse_chat_message)
             .collect())
     }

     /// Send a private message to `talker_id`.
     /// POST https://customerservice.bilibili.com/x/custom/msg_svr/v1/send_msg
     pub async fn send_chat_message(&self, talker_id: i64, content: &str) -> Result<()> {
         let my_uid = self
             .cookies
             .read()
             .expect("cookies lock poisoned")
             .as_ref()
             .and_then(|c| {
                 c.split(';').find_map(|part| {
                     let part = part.trim();
                     part.split_once('=')
                         .filter(|(name, _)| *name == "DedeUserID")
                         .map(|(_, value)| value.to_string())
                 })
             })
             .unwrap_or_default();
         // Bilibili web IM expects a UUID v4 (upper-case) as dev_id; the old
         // x/session/msg/send accepted a hex blob, the new endpoint validates it.
         let dev_id = uuid_v4_upper();
         let ts = std::time::SystemTime::now()
             .duration_since(std::time::UNIX_EPOCH)
             .map(|d| d.as_secs())
             .unwrap_or(0);
         let content_json = serde_json::json!({ "content": content }).to_string();
         let url = "https://customerservice.bilibili.com/x/custom/msg_svr/v1/send_msg";
         let form_data: Vec<(&str, String)> = vec![
             ("sender_uid", my_uid),
             ("receiver_id", talker_id.to_string()),
             ("receiver_type", "1".to_string()),
             ("msg_type", "1".to_string()),
             ("content", content_json),
             ("dev_id", dev_id),
             ("msg_status", "0".to_string()),
             ("msg_source", "6".to_string()),
             ("timestamp", ts.to_string()),
             ("build", "0".to_string()),
             ("mobi_app", "web".to_string()),
         ];
         let _resp: ApiResponse<serde_json::Value> = self.post_with_wbi(&url, form_data).await?;
         Ok(())
     }

     /// Send a private message to `talker_id` (alias kept for readability).

     // Comments API

    // Comments API
    pub async fn get_comments(&self, oid: i64, pn: i32) -> Result<super::comment::CommentData> {
        let url = format!(
            "{}/x/v2/reply?type=1&oid={}&sort=1&ps=20&pn={}",
            BilibiliApiDomain::Main.as_str(),
            oid,
            pn
        );

        let resp: ApiResponse<super::comment::CommentData> = self.get(&url).await?;
        Ok(resp.data.unwrap_or(super::comment::CommentData {
            page: None,
            replies: None,
            hots: None,
        }))
    }

    // Dynamic Comments API
    // type=11: 相簿（图片动态） - image/photo albums
    // type=17: 动态（纯文字动态&分享） - text dynamics and shares
    pub async fn get_dynamic_comments(
        &self,
        oid: i64,
        comment_type: i32,
        pn: i32,
    ) -> Result<super::comment::CommentData> {
        let url = format!(
            "{}/x/v2/reply?type={}&oid={}&sort=1&ps=20&pn={}",
            BilibiliApiDomain::Main.as_str(),
            comment_type,
            oid,
            pn
        );

        let resp: ApiResponse<super::comment::CommentData> = self.get(&url).await?;
        Ok(resp.data.unwrap_or(super::comment::CommentData {
            page: None,
            replies: None,
            hots: None,
        }))
    }

    // Comment replies API
    pub async fn get_comment_replies(
        &self,
        oid: i64,
        root: i64,
        pn: i32,
    ) -> Result<super::comment::CommentData> {
        let url = format!(
            "{}/x/v2/reply/reply?type=1&oid={}&root={}&ps=20&pn={}",
            BilibiliApiDomain::Main.as_str(),
            oid,
            root,
            pn
        );

        let resp: ApiResponse<super::comment::CommentData> = self.get(&url).await?;
        Ok(resp.data.unwrap_or(super::comment::CommentData {
            page: None,
            replies: None,
            hots: None,
        }))
    }

    // Related Videos API
    pub async fn get_related_videos(
        &self,
        bvid: &str,
    ) -> Result<Vec<super::video::RelatedVideoItem>> {
        let url = format!(
            "{}/x/web-interface/archive/related?bvid={}",
            BilibiliApiDomain::Main.as_str(),
            bvid
        );

        let resp: ApiResponse<Vec<super::video::RelatedVideoItem>> = self.get(&url).await?;
        Ok(resp.data.unwrap_or_default())
    }

    // Extended Recommendations API with pagination
    pub async fn get_recommendations_paged(
        &self,
        fresh_idx: i32,
    ) -> Result<Vec<super::recommend::VideoItem>> {
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/web-interface/wbi/index/top/feed/rcmd",
        );

        let params = vec![
            ("fresh_type", "4".to_string()),
            ("ps", "20".to_string()),
            ("fresh_idx", fresh_idx.to_string()),
            ("fresh_idx_1h", fresh_idx.to_string()),
        ];

        let resp: ApiResponse<super::recommend::RecommendData> =
            self.get_with_wbi(&url, params).await?;

        Ok(resp
            .data
            .map(|d| d.item.into_iter().filter(|v| v.bvid.is_some()).collect())
            .unwrap_or_default())
    }

    pub async fn get_history(
        &self,
        max: Option<i64>,
        view_at: Option<i64>,
        business: Option<&str>,
    ) -> Result<super::history::HistoryData> {
        let mut url = format!(
            "{}/x/web-interface/history/cursor",
            BilibiliApiDomain::Main.as_str()
        );

        let mut params = Vec::new();
        params.push("ps=20".to_string());

        if let Some(m) = max {
            params.push(format!("max={}", m));
        }
        if let Some(v) = view_at {
            params.push(format!("view_at={}", v));
        }
        if let Some(b) = business {
            params.push(format!("business={}", b));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let resp: ApiResponse<super::history::HistoryData> = self.get(&url).await?;
        resp.data
            .ok_or_else(|| anyhow::anyhow!("No data in history response"))
    }

    pub async fn delete_history_item(&self, key: &super::history::HistoryKey) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/history/delete");
        let _: ApiResponse<serde_json::Value> =
            self.post(&url, vec![("kid", key.api_value())]).await?;
        Ok(())
    }

    pub async fn get_article(&self, cvid: i64) -> Result<super::article::ArticleData> {
        let url = format!(
            "{}/x/article/view?id={cvid}",
            BilibiliApiDomain::Main.as_str()
        );
        let resp: ApiResponse<super::article::ArticleData> = self.get(&url).await?;
        resp.data
            .ok_or_else(|| anyhow!("article response has no data"))
    }

    // ========== Comment Action APIs ==========

    /// Add a comment (发表评论)
    /// - `oid`: Target ID (e.g., video aid)
    /// - `comment_type`: Comment area type (1=video, 17=dynamic, etc.)
    /// - `message`: Comment content
    /// - `root`: Root comment rpid for reply (None for top-level comment)
    /// - `parent`: Parent comment rpid for reply (None for top-level comment)
    pub async fn add_comment(
        &self,
        oid: i64,
        comment_type: i32,
        message: &str,
        root: Option<i64>,
        parent: Option<i64>,
    ) -> Result<super::comment::AddCommentResponse> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/reply/add");

        let mut form_data = vec![
            ("type", comment_type.to_string()),
            ("oid", oid.to_string()),
            ("message", message.to_string()),
            ("plat", "1".to_string()), // Web platform
        ];

        if let Some(r) = root {
            form_data.push(("root", r.to_string()));
        }
        if let Some(p) = parent {
            form_data.push(("parent", p.to_string()));
        }

        let resp: ApiResponse<super::comment::AddCommentResponse> =
            self.post_with_wbi(&url, form_data).await?;

        if resp.code != 0 {
            return Err(anyhow::anyhow!("Failed to add comment: {}", resp.message));
        }

        resp.data
            .ok_or_else(|| anyhow::anyhow!("No data in add comment response"))
    }

    /// Like or unlike a comment (点赞/取消点赞评论)
    /// - `action`: true = like, false = unlike
    pub async fn like_comment(
        &self,
        oid: i64,
        rpid: i64,
        comment_type: i32,
        action: bool,
    ) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/reply/action");

        let form_data = vec![
            ("type", comment_type.to_string()),
            ("oid", oid.to_string()),
            ("rpid", rpid.to_string()),
            ("action", if action { "1" } else { "0" }.to_string()),
        ];

        let resp: ApiResponse<serde_json::Value> = self.post_with_wbi(&url, form_data).await?;

        if resp.code != 0 {
            return Err(anyhow::anyhow!(
                "Failed to {} comment: {}",
                if action { "like" } else { "unlike" },
                resp.message
            ));
        }

        Ok(())
    }

    /// Dislike or un-dislike a comment (点踩/取消点踩评论)
    /// - `action`: true = dislike, false = un-dislike
    pub async fn dislike_comment(
        &self,
        oid: i64,
        rpid: i64,
        comment_type: i32,
        action: bool,
    ) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/reply/hate");

        let form_data = vec![
            ("type", comment_type.to_string()),
            ("oid", oid.to_string()),
            ("rpid", rpid.to_string()),
            ("action", if action { "1" } else { "0" }.to_string()),
        ];

        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;

        if resp.code != 0 {
            return Err(anyhow::anyhow!(
                "Failed to {} comment: {}",
                if action { "dislike" } else { "un-dislike" },
                resp.message
            ));
        }

        Ok(())
    }

    /// Delete a comment (删除评论)
    /// Only own comments can be deleted
    pub async fn delete_comment(&self, oid: i64, rpid: i64, comment_type: i32) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/reply/del");

        let form_data = vec![
            ("type", comment_type.to_string()),
            ("oid", oid.to_string()),
            ("rpid", rpid.to_string()),
        ];

        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;

        if resp.code != 0 {
            return Err(anyhow::anyhow!(
                "Failed to delete comment: {}",
                resp.message
            ));
        }

        Ok(())
    }

    // ========== Video interaction APIs (三连：点赞/投币/收藏) ==========

    /// Query whether the current user has liked a video (data: 0/1).
    pub async fn get_video_like_status(&self, bvid: &str) -> Result<bool> {
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/web-interface/archive/has/like",
        );
        let resp: ApiResponse<i32> = self
            .get_with_wbi(&url, vec![("bvid", bvid.to_string())])
            .await?;
        Ok(resp.data.unwrap_or(0) == 1)
    }

    /// Query how many coins the current user has given a video (0/1/2).
    pub async fn get_video_coin_status(&self, bvid: &str) -> Result<i32> {
        #[derive(Deserialize)]
        struct CoinsResp {
            multiply: Option<i32>,
        }
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/web-interface/archive/coins",
        );
        let resp: ApiResponse<CoinsResp> = self
            .get_with_wbi(&url, vec![("bvid", bvid.to_string())])
            .await?;
        Ok(resp.data.and_then(|d| d.multiply).unwrap_or(0))
    }

    /// Find the default favorite folder of the current logged-in user and
    /// whether the video is already in it. Returns `(media_id, favorited)`.
    pub async fn get_default_favorite_folder(&self, aid: i64) -> Result<(i64, bool)> {
        #[derive(Deserialize)]
        struct FolderListResp {
            list: Vec<FolderItem>,
        }
        #[derive(Deserialize)]
        struct FolderItem {
            id: i64,
            fav_state: Option<i32>,
            title: Option<String>,
        }
        // The `up_mid` of this endpoint must be the current logged-in user's
        // own mid (we query "my created folders"), not the video uploader's.
        let cookie_str = self
            .cookies
            .read()
            .expect("cookies lock poisoned")
            .clone()
            .ok_or_else(|| anyhow!("not logged in (no cookies)"))?;
        let up_mid: i64 = cookie_str
            .split(';')
            .filter_map(|part| {
                let mut it = part.trim().splitn(2, '=');
                match (it.next(), it.next()) {
                    (Some("DedeUserID"), Some(v)) => v.trim().parse::<i64>().ok(),
                    _ => None,
                }
            })
            .next()
            .ok_or_else(|| anyhow!("not logged in (no DedeUserID)"))?;
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/v3/fav/folder/created/list-all",
        );
        let resp: ApiResponse<FolderListResp> = self
            .get_with_wbi(
                &url,
                vec![
                    ("up_mid", up_mid.to_string()),
                    ("rid", aid.to_string()),
                    ("type", "2".to_string()),
                ],
            )
            .await?;
        let data = resp.data.ok_or_else(|| anyhow!("no favorite folder data"))?;
        let folder = data
            .list
            .iter()
            .find(|f| {
                f.title
                    .as_deref()
                    .map(|t| t.contains("默认") || t.contains("Default"))
                    .unwrap_or(false)
            })
            .or_else(|| data.list.first())
            .ok_or_else(|| anyhow!("no favorite folder"))?;
        Ok((folder.id, folder.fav_state.unwrap_or(0) == 1))
    }

    /// Get all user-created favorite folders with favorite status for the
    /// given video (for the folder picker in video detail).
    pub async fn get_default_favorite_folder_list(
        &self,
        aid: i64,
    ) -> Result<Vec<super::favorite::FavoriteFolder>> {
        let cookie_str = self
            .cookies
            .read()
            .expect("cookies lock poisoned")
            .clone()
            .ok_or_else(|| anyhow!("not logged in (no cookies)"))?;
        let up_mid: i64 = cookie_str
            .split(';')
            .filter_map(|part| {
                let mut it = part.trim().splitn(2, '=');
                match (it.next(), it.next()) {
                    (Some("DedeUserID"), Some(v)) => v.trim().parse::<i64>().ok(),
                    _ => None,
                }
            })
            .next()
            .ok_or_else(|| anyhow!("not logged in (no DedeUserID)"))?;
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/v3/fav/folder/created/list-all",
        );
        let resp: ApiResponse<super::favorite::FavoriteFolderData> = self
            .get_with_wbi(
                &url,
                vec![
                    ("up_mid", up_mid.to_string()),
                    ("rid", aid.to_string()),
                    ("type", "2".to_string()),
                ],
            )
            .await?;
        let data = resp
            .data
            .ok_or_else(|| anyhow!("no favorite folder data"))?;
        Ok(data.list)
    }

    /// Like (`like = true`) or unlike (`like = false`) a video.
    pub async fn like_video(&self, aid: i64, like: bool) -> Result<()> {
        let url =
            self.build_url(BilibiliApiDomain::Main, "/x/web-interface/archive/like");
        let form_data = vec![
            ("aid", aid.to_string()),
            ("like", if like { "1" } else { "2" }.to_string()),
        ];
        let _: ApiResponse<serde_json::Value> = self.post_with_wbi(&url, form_data).await?;
        Ok(())
    }

    /// Give coins to a video. `multiply` is 1 or 2 coins per request;
    /// `select_like` also likes the video at the same time (Bilibili default).
    pub async fn coin_video(&self, aid: i64, multiply: i32, select_like: bool) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/web-interface/coin/add");
        let form_data = vec![
            ("aid", aid.to_string()),
            ("multiply", multiply.to_string()),
            ("select_like", if select_like { "1" } else { "0" }.to_string()),
        ];
        let _: ApiResponse<serde_json::Value> = self.post_with_wbi(&url, form_data).await?;
        Ok(())
    }

    /// Add (`add = true`) or remove (`add = false`) a video from a favorite
    /// folder identified by `media_id`.
    pub async fn favorite_video(&self, aid: i64, media_id: i64, add: bool) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v3/fav/resource/deal");
        let mut form_data = vec![
            ("rid", aid.to_string()),
            ("type", "2".to_string()),
        ];
        if add {
            form_data.push(("add_media_ids", media_id.to_string()));
        } else {
            form_data.push(("del_media_ids", media_id.to_string()));
        }
        let _: ApiResponse<serde_json::Value> = self.post_with_wbi(&url, form_data).await?;
        Ok(())
    }

    // ========== Live Streaming APIs ==========

    /// Get live streaming recommendations
    pub async fn get_live_recommendations(&self) -> Result<Vec<super::live::LiveRoom>> {
        const LIVE_REC_URL: &str =
            "https://api.live.bilibili.com/xlive/web-interface/v1/webMain/getMoreRecList";

        let url = format!("{}?platform=web", LIVE_REC_URL);

        let mut req = self.client.get(&url);
        if let Some(ref cookies) = *self.cookies.read().expect("cookies lock poisoned") {
            req = req.header(COOKIE, cookies.as_str());
        }

        let resp = req.send().await?.error_for_status()?;
        let api_resp: ApiResponse<super::live::LiveRecommendData> = resp.json().await?;

        Ok(api_resp
            .data
            .map(|d| d.recommend_room_list)
            .unwrap_or_default())
    }

    /// Match the web live homepage: followed live rooms first, recommendations second.
    pub async fn get_live_home_rooms(&self) -> Result<Vec<super::live::LiveRoom>> {
        const URL: &str = "https://api.live.bilibili.com/xlive/web-interface/v1/index/getList";
        let resp: ApiResponse<super::live::LiveHomeData> = self
            .get_with_wbi(
                URL,
                vec![
                    ("platform", "web".to_string()),
                    ("web_location", "444.7".to_string()),
                ],
            )
            .await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "live homepage API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        let data = resp
            .data
            .ok_or_else(|| anyhow!("live homepage response has no data"))?;
        Ok(data.followed_then_recommended())
    }

    async fn get_live_play_info(
        &self,
        room_id: i64,
        quality: i64,
    ) -> Result<super::live::LivePlayInfoData> {
        const URL: &str = "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo";
        let resp: ApiResponse<super::live::LivePlayInfoData> = self
            .get_with_wbi(
                URL,
                vec![
                    ("room_id", room_id.to_string()),
                    ("protocol", "0,1".to_string()),
                    ("format", "0,1,2".to_string()),
                    ("codec", "0,1,2".to_string()),
                    ("qn", quality.to_string()),
                    ("platform", "web".to_string()),
                    ("ptype", "8".to_string()),
                    ("dolby", "5".to_string()),
                    ("panorama", "1".to_string()),
                    ("eotf", "0,1,2".to_string()),
                    ("req_reason", "0".to_string()),
                    ("supported_drms", "0,1,2,3".to_string()),
                    ("special_scenario", "2".to_string()),
                    ("web_location", "444.7".to_string()),
                ],
            )
            .await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "live play API error {}: {}",
                resp.code,
                resp.message
            ));
        }
        resp.data
            .ok_or_else(|| anyhow!("live play response has no data"))
    }

    pub async fn get_best_live_stream_urls(&self, room_id: i64) -> Result<Vec<String>> {
        let initial = self.get_live_play_info(room_id, 0).await?;
        if initial.live_status != 1 {
            return Err(anyhow!("直播间当前未开播"));
        }
        let best_quality = initial.highest_available_quality().unwrap_or_default();
        let selected = if best_quality > 0 {
            self.get_live_play_info(room_id, best_quality)
                .await
                .unwrap_or(initial)
        } else {
            initial
        };
        let urls = selected.stream_urls();
        if urls.is_empty() {
            return Err(anyhow!("最高画质没有可用播放地址"));
        }
        Ok(urls)
    }

    pub async fn get_default_live_stream_urls(&self, room_id: i64) -> Result<Vec<String>> {
        let info = self.get_live_play_info(room_id, 0).await?;
        if info.live_status != 1 {
            return Err(anyhow!("直播间当前未开播"));
        }
        let urls = info.default_stream_urls();
        if urls.is_empty() {
            return Err(anyhow!("直播默认播放地址为空"));
        }
        Ok(urls)
    }

    /// Get live room info
    pub async fn get_live_room_info(&self, room_id: i64) -> Result<super::live::LiveRoomInfo> {
        let url = format!(
            "https://api.live.bilibili.com/room/v1/Room/get_info?room_id={}",
            room_id
        );

        let mut req = self.client.get(&url);
        if let Some(ref cookies) = *self.cookies.read().expect("cookies lock poisoned") {
            req = req.header(COOKIE, cookies.as_str());
        }

        let resp = req.send().await?.error_for_status()?;
        let api_resp: ApiResponse<super::live::LiveRoomInfo> = resp.json().await?;

        api_resp
            .data
            .ok_or_else(|| anyhow::anyhow!("No data in live room info response"))
    }

    /// Get danmu info for WebSocket connection
    pub async fn get_danmu_info(&self, room_id: i64) -> Result<super::live_ws::DanmuInfoData> {
        let base_url = "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo";

        // WBI signature is REQUIRED since 2025-05-26
        self.ensure_wbi_keys().await?;

        // Helper to build signed URL with the current WBI keys
        let build_signed_url = |keys: &WbiKeys| {
            let query_string = wbi::encode_wbi(
                vec![
                    ("id", room_id.to_string()),
                    ("type", "0".to_string()),
                    ("web_location", "444.8".to_string()),
                ],
                &keys.img_key,
                &keys.sub_key,
            );
            format!("{}?{}", base_url, query_string)
        };

        // Try once, then refresh WBI keys and retry on signature error (-352)
        let mut attempt = 0;
        loop {
            attempt += 1;

            let keys = {
                let guard = self.wbi_keys.read().expect("wbi lock");
                guard
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("WBI keys unavailable"))?
            };

            let url = build_signed_url(&keys);

            let mut req = self.client.get(&url);
            if let Some(ref cookies) = *self.cookies.read().expect("cookies lock") {
                req = req.header(COOKIE, cookies.as_str());
            }

            let resp = req.send().await?.error_for_status()?;
            let resp_text = resp.text().await?;

            let api_resp: ApiResponse<super::live_ws::DanmuInfoData> =
                serde_json::from_str(&resp_text).map_err(|e| anyhow::anyhow!("解析失败: {e}"))?;

            if api_resp.code == 0 {
                return api_resp.data.ok_or_else(|| anyhow::anyhow!("响应无数据"));
            }

            // If signature failed, refresh keys and retry once
            if api_resp.code == -352 && attempt == 1 {
                *self.wbi_keys.write().expect("wbi lock") = None;
                self.ensure_wbi_keys().await?;
                continue;
            }

            return Err(anyhow::anyhow!(
                "API错误 {}: {}",
                api_resp.code,
                api_resp.message
            ));
        }
    }

    pub async fn get_buvid3(&self) -> Result<String> {
        let url = "https://api.bilibili.com/x/frontend/finger/spi";
        let response: ApiResponse<serde_json::Value> = self.get(url).await?;
        response
            .data
            .as_ref()
            .and_then(|data| data.get("b_3"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow!("buvid3 响应为空"))
    }

    /// Ensure the cookie string carries buvid3/buvid4 fingerprint cookies.
    /// Bilibili's risk control answers 412 on interaction POSTs (relation,
    /// like, coin, fav) when these are missing. Fetch once from the spi
    /// endpoint and append them; subsequent requests take the fast path.
    /// Returns true when the cookie string already carries a plausible
    /// buvid3/buvid4 pair (Bilibili fingerprint cookies). A too-short value
    /// means the fingerprint was captured mid-write or is a stale placeholder,
    /// in which case we refresh it.
    fn has_valid_buvid_cookies(cookie_str: &str) -> bool {
        let valid = |needle: &str| -> bool {
            cookie_str
                .split(';')
                .find_map(|part| {
                    let part = part.trim();
                    part.split_once('=')
                        .filter(|(name, _)| *name == needle)
                        .map(|(_, v)| v)
                })
                .map(|v| v.len() >= 20)
                .unwrap_or(false)
        };
        valid("buvid3") && valid("buvid4")
    }

    fn set_buvid_cookies(&self, b3: &str, b4: &str) {
        let mut cookies = self.cookies.write().expect("cookies lock poisoned");
        if let Some(c) = cookies.as_mut() {
            // Drop any existing buvid entries then append fresh ones.
            let kept: Vec<&str> = c
                .split(';')
                .map(str::trim)
                .filter(|part| {
                    !part.is_empty()
                        && !part.starts_with("buvid3=")
                        && !part.starts_with("buvid4=")
                })
                .collect();
            let mut joined = kept.join("; ");
            if !joined.is_empty() {
                joined.push_str("; ");
            }
            joined.push_str(&format!("buvid3={b3}; buvid4={b4}"));
            *c = joined;
        }
    }

    /// Generate a Bilibili `b_lsid` value: 8 random hex chars, underscore,
    /// then 11 random hex chars (mirrors the web front-end's genLsid()).
    fn generate_lsid() -> String {
        use std::fs::File;
        use std::io::Read;
        let mut buf = [0u8; 19];
        if let Ok(mut f) = File::open("/dev/urandom") {
            let _ = f.read_exact(&mut buf);
        }
        let hex = |bytes: &[u8]| -> String {
            bytes.iter().map(|b| format!("{b:02x}")).collect()
        };
        format!("{}_{}", &hex(&buf[..8]), &hex(&buf[8..]))
    }

    /// Ensure the cookie string carries the web-front-end local session
    /// fingerprint cookies `b_lsid` + `b_nut`. Risk control answers 412 on
    /// interaction POSTs when these are missing, even with buvid3/buvid4 set.
    fn ensure_lsid_cookies(&self) {
        let mut cookies = self.cookies.write().expect("cookies lock poisoned");
        if let Some(c) = cookies.as_mut() {
            let has_lsid = c
                .split(';')
                .any(|p| p.trim().starts_with("b_lsid="));
            let has_nut = c.split(';').any(|p| p.trim().starts_with("b_nut="));
            if has_lsid && has_nut {
                return;
            }
            let lsid = Self::generate_lsid();
            let nut = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_string());
            let kept: Vec<&str> = c
                .split(';')
                .map(str::trim)
                .filter(|part| {
                    !part.is_empty()
                        && !part.starts_with("b_lsid=")
                        && !part.starts_with("b_nut=")
                })
                .collect();
            let mut joined = kept.join("; ");
            if !joined.is_empty() {
                joined.push_str("; ");
            }
            joined.push_str(&format!("b_lsid={lsid}; b_nut={nut}"));
            *c = joined;
        }
    }

    /// Ensure the client carries Bilibili fingerprint cookies. When the
    /// existing buvid3/buvid4 are missing or implausible, fetch fresh ones
    /// from the official SPI endpoint.
    pub async fn ensure_buvid_cookies(&self) -> Result<()> {
        {
            let cookies = self.cookies.read().expect("cookies lock poisoned");
            if let Some(c) = cookies.as_ref() {
                if Self::has_valid_buvid_cookies(c) {
                    return Ok(());
                }
            }
        }
        self.refresh_buvid_cookies().await
    }

    /// Force-fetch fresh buvid3/buvid4 from the SPI endpoint and overwrite
    /// whatever fingerprint cookies are currently stored. Used both on first
    /// request and as a self-healing retry after a -403 risk-control answer.
    pub async fn refresh_buvid_cookies(&self) -> Result<()> {
        let url = "https://api.bilibili.com/x/frontend/finger/spi";
        let response: ApiResponse<serde_json::Value> = self.get(url).await?;
        let data = response.data.as_ref();
        let (Some(b3), Some(b4)) = (
            data.and_then(|d| d.get("b_3")).and_then(|v| v.as_str()),
            data.and_then(|d| d.get("b_4")).and_then(|v| v.as_str()),
        ) else {
            return Ok(());
        };
        self.set_buvid_cookies(b3, b4);
        Ok(())
    }

    /// Get live room history danmaku
    pub async fn get_history_danmaku(
        &self,
        room_id: i64,
    ) -> Result<super::live_ws::HistoryDanmakuData> {
        let url = format!(
            "https://api.live.bilibili.com/xlive/web-room/v1/dM/gethistory?roomid={}",
            room_id
        );

        let mut req = self.client.get(&url);
        if let Some(ref cookies) = *self.cookies.read().expect("cookies lock") {
            req = req.header(COOKIE, cookies.as_str());
        }

        let resp = req.send().await?.error_for_status()?;
        let api_resp: ApiResponse<super::live_ws::HistoryDanmakuData> = resp.json().await?;

        if api_resp.code != 0 {
            return Err(anyhow::anyhow!(
                "API错误 {}: {}",
                api_resp.code,
                api_resp.message
            ));
        }

        api_resp.data.ok_or_else(|| anyhow::anyhow!("响应无数据"))
    }

    // ========== Favorite Management APIs ==========

    /// Create a new favorite folder. Returns the new folder's `id`.
    pub async fn create_favorite_folder(
        &self,
        title: &str,
        intro: &str,
        privacy: i32,
    ) -> Result<i64> {
        let url =
            self.build_url(BilibiliApiDomain::Main, "/x/v3/fav/folder/add");
        let form_data = vec![
            ("title", title.to_string()),
            ("intro", intro.to_string()),
            ("privacy", privacy.to_string()),
        ];
        let resp: ApiResponse<serde_json::Value> =
            self.post_with_wbi(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "创建收藏夹失败 {}: {}",
                resp.code,
                resp.message
            ));
        }
        let id = resp
            .data
            .as_ref()
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        Ok(id)
    }

    /// Delete a favorite folder by its `media_id`.
    pub async fn delete_favorite_folder(&self, media_id: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v3/fav/folder/del");
        let form_data = vec![("media_ids".to_string(), media_id.to_string())];
        let resp: ApiResponse<serde_json::Value> =
            self.post_with_owned(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "删除收藏夹失败 {}: {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    // ========== Danmaku Send API ==========

    /// Send a danmaku (弹幕) to a video.
    /// - `bvid`: Video BV ID
    /// - `cid`: Video CID (page cid)
    /// - `msg`: Danmaku text content
    pub async fn send_danmaku(
        &self,
        bvid: &str,
        cid: i64,
        msg: &str,
    ) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/v2/dm/post");
        let rnd = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let form_data = vec![
            ("type", "1".to_string()),
            ("oid", cid.to_string()),
            ("msg", msg.to_string()),
            ("bvid", bvid.to_string()),
            ("color", "16777215".to_string()),
            ("fontsize", "25".to_string()),
            ("mode", "1".to_string()),
            ("rnd", rnd.to_string()),
        ];
        let resp: ApiResponse<serde_json::Value> =
            self.post_with_wbi(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "发送弹幕失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    /// Send a danmaku to a live room (直播弹幕).
    /// Uses the web live API (not the WebSocket send path).
    pub async fn send_live_danmaku(&self, room_id: i64, msg: &str) -> Result<()> {
        let url = self.build_url(
            BilibiliApiDomain::Live,
            "/xlive/web-interface/v1/danmaku/send",
        );
        let rnd = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let form_data = vec![
            ("room_id", room_id.to_string()),
            ("msg", msg.to_string()),
            ("color", "16777215".to_string()),
            ("fontsize", "25".to_string()),
            ("mode", "1".to_string()),
            ("rnd", rnd.to_string()),
            ("platform", "web".to_string()),
            ("buvid", self.get_buvid3().await.unwrap_or_default()),
        ];
        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "发送直播弹幕失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    /// Publish a text dynamic (发布动态/图文动态的文字内容).
    /// Uses the web dynamic door API with a JSON dyn_req payload.
    pub async fn post_dynamic(&self, content: &str) -> Result<()> {
        let url = self.build_url(
            BilibiliApiDomain::Main,
            "/x/polymer/web-dynamic/v1/door/post",
        );
        let dyn_req = serde_json::json!({
            "content": {
                "content": content,
                "at_uids": [],
                "ctrl": [],
                "topic_id": 0
            },
            "up_choose_topic": false,
            "up_choose_res": false,
            "dynamic_type": 0,
            "ext_info": {
                "emoji_type": 0
            }
        });
        let form_data = vec![
            ("dyn_req", dyn_req.to_string()),
            ("platform", "web".to_string()),
        ];
        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "发布动态失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    /// Follow a user (关注).
    pub async fn follow_user(&self, mid: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/relation/modify");
        // relation endpoints use plain csrf POST (no WBI). `act=1` means
        // follow, `re_src=11` is the space-page source the web client sends.
        // NOTE: `/x/relation/follow` was retired upstream (returns 404);
        // the web client now uses `/x/relation/modify`.
        let form_data = vec![
            ("fid", mid.to_string()),
            ("act", "1".to_string()),
            ("re_src", "11".to_string()),
        ];
        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "关注失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    /// Unfollow a user (取关).
    pub async fn unfollow_user(&self, mid: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/x/relation/modify");
        let form_data = vec![
            ("fid", mid.to_string()),
            ("act", "2".to_string()),
            ("re_src", "11".to_string()),
        ];
        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "取关失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    // ── Bangumi follow (追番) ────────────────────────────────────────────

    /// Follow a season (追番 / 追剧). Uses the current web client endpoint
    /// `pgc/web/follow/add` (the legacy `/pgc/season/follow` returns 404).
    pub async fn follow_bangumi(&self, season_id: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/pgc/web/follow/add");
        let form_data = vec![("season_id", season_id.to_string())];
        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "追番失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    /// Unfollow a season (取消追番).
    pub async fn unfollow_bangumi(&self, season_id: i64) -> Result<()> {
        let url = self.build_url(BilibiliApiDomain::Main, "/pgc/web/follow/del");
        let form_data = vec![("season_id", season_id.to_string())];
        let resp: ApiResponse<serde_json::Value> = self.post(&url, form_data).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "取消追番失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        Ok(())
    }

    /// Fetch the user's follow list (追番列表). `season_type`: 1=番剧, 2=影视.
    /// Returns a list of followed seasons (title, season_id, cover).
    /// Uses `/x/space/bangumi/follow/list`; the older
    /// `/x/polymer/web-space/seasons_series_list` returns an empty list for
    /// bangumi follows on the current API.
    pub async fn get_bangumi_follow_list(
        &self,
        mid: i64,
        season_type: i32,
        page: i32,
    ) -> Result<Vec<super::space::SeriesInfo>> {
        let url = format!(
            "{}/x/space/bangumi/follow/list?type={}&follow_status=0&pn={}&ps=20&vmid={}",
            BilibiliApiDomain::Main.as_str(),
            season_type,
            page,
            mid
        );
        let value = self.get_json(&url).await?;
        Self::check_code(&value)?;
        let list = value
            .get("data")
            .and_then(|d| d.get("list"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            let season_id = item.get("season_id").and_then(|v| v.as_i64());
            let Some(season_id) = season_id else { continue };
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("未知番剧")
                .to_string();
            let cover = item
                .get("cover")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
            let description = item
                .get("evaluate")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
            out.push(super::space::SeriesInfo {
                id: None,
                meta: Some(super::space::SeriesMeta {
                    season_id: Some(season_id),
                    name: Some(title.clone()),
                    title: Some(title),
                    description,
                    total: None,
                    cover,
                }),
                total: None,
            });
        }
        Ok(out)
    }

    /// Check whether the current user follows the given bangumi season.
    /// The pgc season view sometimes reports `user_status.login = 0` even for
    /// valid sessions (making `user_status.follow` unreliable), so this walks
    /// the real follow list instead. Returns `false` when not logged in.
    pub async fn is_bangumi_followed(&self, season_id: i64) -> Result<bool> {
        let mid = {
            let cookie_str = self.cookies.read().expect("cookies lock poisoned").clone();
            cookie_str
                .and_then(|c| {
                    c.split(';')
                        .filter_map(|part| {
                            let mut it = part.trim().splitn(2, '=');
                            match (it.next(), it.next()) {
                                (Some("DedeUserID"), Some(v)) => v.trim().parse::<i64>().ok(),
                                _ => None,
                            }
                        })
                        .next()
                })
        };
        let Some(mid) = mid else {
            return Ok(false);
        };
        // Walk up to 20 pages (ps=20 → 400 seasons). ps>20 is rejected by the
        // API with -400, which used to make every lookup fail and report
        // "not followed" even when the season was in the list.
        for page in 1..=20 {
            let url = format!(
                "{}/x/space/bangumi/follow/list?type=1&follow_status=0&pn={}&ps=20&vmid={}",
                BilibiliApiDomain::Main.as_str(),
                page,
                mid
            );
            let value = self.get_json(&url).await?;
            Self::check_code(&value)?;
            let list = value
                .get("data")
                .and_then(|d| d.get("list"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if list.iter().any(|item| {
                item.get("season_id")
                    .and_then(|v| v.as_i64())
                    .map(|sid| sid == season_id)
                    .unwrap_or(false)
            }) {
                return Ok(true);
            }
            let total = value
                .get("data")
                .and_then(|d| d.get("total"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if list.is_empty() || (page as i64) * 50 >= total {
                break;
            }
        }
        Ok(false)
    }

    /// Check whether the current user follows the given uploader.
    /// Returns `true` if followed (attribute & 1 != 0).
    pub async fn get_follow_status(&self, mid: i64) -> Result<bool> {
        let url = self.build_url(
            BilibiliApiDomain::Main,
            &format!("/x/relation?fid={}", mid),
        );
        let resp: ApiResponse<serde_json::Value> = self.get(&url).await?;
        if resp.code != 0 {
            return Err(anyhow!(
                "查询关注状态失败 ({}): {}",
                resp.code,
                resp.message
            ));
        }
        let attribute = resp
            .data
            .as_ref()
            .and_then(|data| data.get("attribute"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        // attribute: 0 = not followed, 1 = followed, 2 = mutual follow.
        // Any non-zero value means the current user follows this uploader.
        Ok(attribute != 0)
    }

    // ========== Ranking API ==========

    /// Get ranking videos for a specific section (分区排行榜).
    /// `rid`: Section ID (0 = all, 1 = anime, 3 = music, etc.)
    pub async fn get_ranking(
        &self,
        rid: i64,
    ) -> Result<Vec<super::recommend::VideoItem>> {
        let url = format!(
            "{}/x/web-interface/ranking/v2?rid={}&type=all",
            BilibiliApiDomain::Main.as_str(),
            rid,
        );
        let value = self.get_json(&url).await?;
        let code = value
            .get("code")
            .and_then(|v| v.as_i64())
            .unwrap_or_default();
        if code != 0 {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("Ranking API error {}: {}", code, message));
        }
        let list = value
            .get("data")
            .and_then(|d| d.get("list"))
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(parse_home_videos(list))
    }
}
fn parse_home_videos(items: Vec<serde_json::Value>) -> Vec<super::recommend::VideoItem> {
    items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<super::recommend::VideoItem>(item).ok())
        .filter(|video| video.id > 0 && video.bvid.is_some())
        .collect()
}

/// UUID v4 (upper-case), the `dev_id` format the Bilibili web IM expects.
fn uuid_v4_upper() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    // Prefer OS randomness; fall back to a time/pid mix if /dev/urandom is
    // unavailable. The endpoint only validates the shape, not the entropy.
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !ok {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let pid = std::process::id() as u64;
        let seed = ts ^ (pid << 32) ^ 0x9e3779b97f4a7c15;
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (seed.rotate_left((i as u32) * 7) >> ((i % 8) * 8)) as u8;
        }
    }
    // RFC 4122: version 4, variant 10xx
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().map(|b| format!("{:02X}", b)).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod live_contract_tests {
    use super::ApiClient;
    use crate::api::space::SpaceVideoOrder;
    use crate::storage::VideoQuality;

    #[test]
    fn bangumi_v2_playurl_extracts_nested_video_info() {
        let value = serde_json::json!({
            "code": 0,
            "result": {
                "video_info": {
                    "dash": {
                        "video": [{"id": 80, "bandwidth": 10}],
                        "audio": [{"id": 30280, "bandwidth": 5}],
                        "dolby": null,
                        "flac": null
                    }
                },
                "play_view_business_info": {"episode_info": {"cid": 1}}
            }
        });
        let playurl = ApiClient::parse_bangumi_play_url(&value).unwrap();
        assert_eq!(playurl.dash.video[0].id, 80);
    }

    #[tokio::test]
    #[ignore = "requires a logged-in account and network access"]
    async fn current_space_and_favorite_contracts_deserialize() {
        let credentials = crate::storage::load_credentials().expect("load credentials");
        let mid = credentials
            .dede_user_id
            .parse::<i64>()
            .expect("numeric DedeUserID");
        let client = ApiClient::with_cookies(&credentials);

        client.get_space_info(mid).await.expect("space info");
        client
            .get_space_videos(mid, 1, 10, SpaceVideoOrder::Latest)
            .await
            .expect("latest submissions");
        client
            .get_space_videos(mid, 1, 10, SpaceVideoOrder::Popular)
            .await
            .expect("popular submissions");
        let folders = client
            .get_favorite_folders(mid)
            .await
            .expect("favorite folders");
        if let Some(folder) = folders.first() {
            let resources = client
                .get_favorite_resources(
                    folder.id,
                    1,
                    10,
                    crate::api::favorite::FavoriteOrder::RecentlyFavorited,
                )
                .await
                .expect("favorite resources");
            if let Some(bvid) = resources
                .medias
                .iter()
                .find_map(|media| media.bvid.as_deref())
            {
                let info = client.get_video_info(bvid).await.expect("video info");
                let play_url = client
                    .get_play_url(bvid, info.cid, Default::default())
                    .await
                    .expect("playurl");
                crate::api::cdn::rank_streams(&play_url, Default::default())
                    .await
                    .expect("reachable CDN streams");
            }
        }

        let watch_later = client.get_watch_later(1, 2).await.expect("watch later");
        assert!(watch_later.count >= watch_later.list.len() as i64);
        let collected = client
            .get_collected_folders(mid, 1, 2)
            .await
            .expect("collected folders");
        if let Some(folder) = collected
            .list
            .iter()
            .find(|folder| folder.state.unwrap_or_default() == 0 && folder.mid != 0)
        {
            client
                .get_collected_season_videos(folder.mid, folder.id, 1, 2)
                .await
                .expect("collected season videos");
        }
        let live_rooms = client.get_live_home_rooms().await.expect("live homepage");
        if let Some(room) = live_rooms.first() {
            let urls = client
                .get_best_live_stream_urls(room.roomid)
                .await
                .expect("best live stream URLs");
            assert!(!urls.is_empty());
        }
    }
}

