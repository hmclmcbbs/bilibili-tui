use crate::api::{
    ApiClient,
    article::ArticleData,
    bangumi::{SeasonRankItem, SeasonResult},
    comment::{CommentItem, CommentType},
    dynamic::DynamicItem,
    dynamic::UpListItem,
    favorite::{
        CollectedFolder, FavoriteFolder, FavoriteOrder, FavoriteResourceData, FavoriteSource,
        SeasonArchivesData, WatchLaterData,
    },
    history::{HistoryCursor, HistoryData, HistoryKey},
    live::LiveRoom,
    recommend::{HomeFeed, VideoItem},
    search::HotwordItem,
    search::SearchVideoItem,
    space::{RelationStat, SpaceInfo, SpaceVideoData, SpaceVideoOrder, parse_length_to_seconds},
    video::RelatedVideoItem,
    video::VideoInfo,
};
use crate::domain::playback::{PlayOrder, PlaylistItem, PlaylistSource};
use crate::presentation::tui::DynamicTab;
use futures_util::{StreamExt, stream};
use std::collections::HashMap;
use std::sync::{Arc, mpsc};

#[derive(Debug)]
pub enum NetworkCommand {
    CancelPending,
    LoadHome {
        req_id: u64,
        feed: HomeFeed,
        use_guest_feed: bool,
        rid: i64,
    },
    LoadHomeMore {
        req_id: u64,
        fresh_idx: i32,
        feed: HomeFeed,
        use_guest_feed: bool,
        rid: i64,
    },
    LoadSectionRanking {
        req_id: u64,
        rid: i64,
    },
    LoadHotwords {
        req_id: u64,
    },
    Search {
        req_id: u64,
        keyword: String,
        page: i32,
    },
    SearchUsers {
        req_id: u64,
        keyword: String,
        page: i32,
    },
    LoadDynamicInit {
        req_id: u64,
        tab: DynamicTab,
        host_mid: Option<i64>,
    },
    LoadDynamicRefresh {
        req_id: u64,
        tab: DynamicTab,
        host_mid: Option<i64>,
    },
    LoadDynamicMore {
        req_id: u64,
        offset: String,
        tab: DynamicTab,
        host_mid: Option<i64>,
    },
    LoadHistoryInit {
        req_id: u64,
    },
    LoadHistoryMore {
        req_id: u64,
        cursor: HistoryCursor,
    },
    DeleteHistory {
        req_id: u64,
        keys: Vec<HistoryKey>,
    },
    LoadArticle {
        req_id: u64,
        cvid: i64,
    },
    LoadLiveInit {
        req_id: u64,
    },
    LoadLiveMore {
        req_id: u64,
    },
    LoadVideoDetail {
        req_id: u64,
        bvid: String,
        aid: i64,
    },
    ProbeVideoStreams {
        req_id: u64,
        bvid: String,
        cid: i64,
    },
    LoadUpPage {
        req_id: u64,
        mid: i64,
        order: SpaceVideoOrder,
    },
    LoadUpVideos {
        req_id: u64,
        mid: i64,
        page: i32,
        order: SpaceVideoOrder,
    },
    LoadFavoriteResources {
        req_id: u64,
        owner_mid: i64,
        media_id: i64,
        page: i32,
        order: FavoriteOrder,
    },
    LoadSeriesList {
        req_id: u64,
        mid: i64,
        page: i32,
    },
    LoadSeriesArchives {
        req_id: u64,
        mid: i64,
        series_id: i64,
        is_series: bool,
        page: i32,
    },
    BuildUpPlaylist {
        req_id: u64,
        mid: i64,
        name: String,
        video_order: SpaceVideoOrder,
        play_order: PlayOrder,
    },
    BuildFavoritePlaylist {
        req_id: u64,
        media_id: i64,
        title: String,
        favorite_order: FavoriteOrder,
        play_order: PlayOrder,
    },
    LoadFavoritesInit {
        req_id: u64,
        mid: i64,
    },
    /// Reload only the folder lists (created + collected) without reloading
    /// the page content.  Used after creating / deleting a folder.
    RefreshFavoriteFolders {
        req_id: u64,
        mid: i64,
    },
    LoadFavoritesContent {
        req_id: u64,
        source: FavoriteSource,
        page: i32,
    },
    LoadDynamicDetail {
        req_id: u64,
        dynamic_id: String,
    },
    LoadBangumiIndex {
        req_id: u64,
    },
    LoadBangumiFollowList {
        req_id: u64,
        mid: i64,
    },
    LoadBangumiDetail {
        req_id: u64,
        season_id: i64,
    },
    LoadBangumiSearch {
        req_id: u64,
        keyword: String,
        page: i32,
    },
    LoadNotificationsInit {
        req_id: u64,
        feed_type: i32,
    },
    LoadNotificationsMore {
        req_id: u64,
        feed_type: i32,
        page: i32,
    },
    LoadChatSessions {
        req_id: u64,
    },
    LoadChatDetail {
        req_id: u64,
        talker_id: i64,
    },
    SendChatMessage {
        req_id: u64,
        talker_id: i64,
        content: String,
    },
    LoadMallOrders {
        req_id: u64,
        /// true = 只加载工房（B站小店/GF_DZ）数字商品订单，false = 会员购全部订单。
        gf_only: bool,
    },
    LoadMallExpress {
        req_id: u64,
        order_id: i64,
    },
    LoadMallExpressTrack {
        req_id: u64,
        order_id: i64,
    },
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum NetworkEvent {
    HomeLoaded {
        req_id: u64,
        feed: HomeFeed,
        videos: Vec<VideoItem>,
    },
    HomeMoreLoaded {
        req_id: u64,
        feed: HomeFeed,
        videos: Vec<VideoItem>,
    },
    SectionRankingLoaded {
        req_id: u64,
        rid: i64,
        videos: Vec<VideoItem>,
    },
    HotwordsLoaded {
        req_id: u64,
        hotwords: Vec<HotwordItem>,
    },
    HistoryLoaded {
        req_id: u64,
        append: bool,
        data: HistoryData,
    },
    NotificationsLoaded {
        req_id: u64,
        append: bool,
        feed_type: i32,
        items: Vec<crate::api::msg::NotificationItem>,
        unread: (i32, i32, i32, i32),
    },
    ChatSessionsLoaded {
        req_id: u64,
        sessions: Vec<crate::api::msg::ChatSession>,
    },
    ChatDetailLoaded {
        req_id: u64,
        talker_id: i64,
        messages: Vec<crate::api::msg::ChatMessage>,
    },
    ChatMessageSent {
        req_id: u64,
        talker_id: i64,
        ok: bool,
        error: Option<String>,
    },
    SearchLoaded {
        req_id: u64,
        keyword: String,
        page: i32,
        results: Vec<SearchVideoItem>,
        total: i32,
    },
    SearchUsersLoaded {
        req_id: u64,
        keyword: String,
        page: i32,
        results: Vec<crate::api::search::SearchUserItem>,
        total: i32,
    },
    DynamicLoaded {
        req_id: u64,
        append: bool,
        up_list: Option<Vec<UpListItem>>,
        items: Vec<crate::api::dynamic::DynamicItem>,
        offset: Option<String>,
        has_more: bool,
    },
    HistoryDeleted {
        req_id: u64,
        successful: Vec<HistoryKey>,
        failed: Vec<(HistoryKey, String)>,
    },
    ArticleLoaded {
        req_id: u64,
        cvid: i64,
        article: ArticleData,
        comments: Vec<CommentItem>,
    },
    LiveLoaded {
        req_id: u64,
        append: bool,
        rooms: Vec<LiveRoom>,
    },
    VideoDetailLoaded {
        req_id: u64,
        bvid: String,
        video_info: VideoInfo,
        comments: Vec<CommentItem>,
        has_more_comments: bool,
        related_videos: Vec<RelatedVideoItem>,
        hdr_supported: Option<bool>,
        hires_supported: Option<bool>,
        liked: bool,
        coined: i32,
        favorited: bool,
        in_watch_later: bool,
        default_media_id: Option<i64>,
        interaction_error: Option<String>,
    },
    VideoStreamSupportLoaded {
        req_id: u64,
        bvid: String,
        hdr_supported: Option<bool>,
        hires_supported: Option<bool>,
    },
    UpPageLoaded {
        req_id: u64,
        mid: i64,
        order: SpaceVideoOrder,
        profile: SpaceInfo,
        relation: Option<RelationStat>,
        is_followed: Option<bool>,
        videos: SpaceVideoData,
        folders: Vec<FavoriteFolder>,
    },
    UpVideosLoaded {
        req_id: u64,
        mid: i64,
        page: i32,
        order: SpaceVideoOrder,
        videos: SpaceVideoData,
    },
    FavoriteResourcesLoaded {
        req_id: u64,
        owner_mid: i64,
        media_id: i64,
        page: i32,
        order: FavoriteOrder,
        resources: FavoriteResourceData,
    },
    SeriesListLoaded {
        req_id: u64,
        mid: i64,
        page: i32,
        data: crate::api::space::SeriesListData,
    },
    SeriesArchivesLoaded {
        req_id: u64,
        mid: i64,
        series_id: i64,
        is_series: bool,
        page: i32,
        data: crate::api::space::SeriesArchivesData,
    },
    PlaylistLoaded {
        req_id: u64,
        items: Vec<PlaylistItem>,
        source: PlaylistSource,
        start_index: usize,
        order: PlayOrder,
    },
    FavoritesInitLoaded {
        req_id: u64,
        mid: i64,
        watch_later: WatchLaterData,
        created: Vec<FavoriteFolder>,
        collected: Vec<CollectedFolder>,
    },
    /// Folder lists refreshed (after create / delete) – no content change.
    FavoriteFoldersRefreshed {
        req_id: u64,
        created: Vec<FavoriteFolder>,
        collected: Vec<CollectedFolder>,
    },
    FavoritesWatchLaterLoaded {
        req_id: u64,
        page: i32,
        data: WatchLaterData,
    },
    FavoritesCreatedLoaded {
        req_id: u64,
        media_id: i64,
        page: i32,
        data: FavoriteResourceData,
    },
    FavoritesCollectedLoaded {
        req_id: u64,
        season_id: i64,
        page: i32,
        data: SeasonArchivesData,
    },
    DynamicDetailLoaded {
        req_id: u64,
        dynamic_id: String,
        dynamic_item: DynamicItem,
        comments: Vec<CommentItem>,
        has_more_comments: bool,
        image_urls: Vec<String>,
    },
    BangumiFollowListLoaded {
        req_id: u64,
        items: Vec<crate::api::space::SeriesInfo>,
    },
    BangumiIndexLoaded {
        req_id: u64,
        items: Vec<SeasonRankItem>,
    },
    BangumiDetailLoaded {
        req_id: u64,
        season_id: i64,
        season: SeasonResult,
        /// Resolved follow state for the current user. When the season view's
        /// user_status.login is 1 this comes from user_status.follow; otherwise
        /// it is checked against the real follow list (login=0 is unreliable).
        followed: bool,
    },
    BangumiSearchLoaded {
        req_id: u64,
        keyword: String,
        items: Vec<crate::api::search::SearchBangumiItem>,
    },
    MallOrdersLoaded {
        req_id: u64,
        orders: Vec<crate::api::mall::MallOrder>,
    },
    MallOrdersFailed {
        req_id: u64,
        error: String,
    },
    MallExpressLoaded {
        req_id: u64,
        order_id: i64,
        express: Option<crate::api::mall::MallExpressSummary>,
    },
    MallExpressFailed {
        req_id: u64,
        order_id: i64,
        error: String,
    },
    MallExpressTrackLoaded {
        req_id: u64,
        order_id: i64,
        express: Option<crate::api::mall::MallExpress>,
    },
    MallExpressTrackFailed {
        req_id: u64,
        order_id: i64,
        error: String,
    },
    RequestFailed {
        req_id: u64,
        target: &'static str,
        error: String,
    },
}

pub struct NetworkBridge {
    pub command_tx: mpsc::Sender<NetworkCommand>,
    pub event_rx: mpsc::Receiver<NetworkEvent>,
}

pub fn start_network_worker(api_client: Arc<ApiClient>) -> NetworkBridge {
    let (command_tx, command_rx) = mpsc::channel::<NetworkCommand>();
    let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>();

    std::thread::Builder::new()
        .name("bilibili-network-worker".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return,
            };

            let mut cancellations =
                HashMap::<&'static str, tokio_util::sync::CancellationToken>::new();
            while let Ok(command) = command_rx.recv() {
                if matches!(command, NetworkCommand::CancelPending) {
                    for (_, token) in cancellations.drain() {
                        token.cancel();
                    }
                    continue;
                }
                let key = command.cancel_key();
                let token = tokio_util::sync::CancellationToken::new();
                if let Some(previous) = cancellations.insert(key, token.clone()) {
                    previous.cancel();
                }
                let api_client = api_client.clone();
                let event_tx = event_tx.clone();
                runtime.spawn(async move {
                    tokio::select! {
                        _ = token.cancelled() => {}
                        event = handle_command(api_client, command) => {
                            let _ = event_tx.send(event);
                        }
                    }
                });
            }
        })
        .expect("failed to spawn network worker");

    NetworkBridge {
        command_tx,
        event_rx,
    }
}

impl NetworkCommand {
    fn cancel_key(&self) -> &'static str {
        match self {
            Self::LoadHome { .. } | Self::LoadHomeMore { .. } => "home",
            Self::LoadSectionRanking { .. } => "sections",
            Self::LoadFavoriteResources { .. }
            | Self::LoadFavoritesInit { .. }
            | Self::RefreshFavoriteFolders { .. }
            | Self::LoadFavoritesContent { .. } => "favorites",
            Self::BuildUpPlaylist { .. } | Self::BuildFavoritePlaylist { .. } => "playlist",
            Self::CancelPending => "cancel",
            Self::LoadHotwords { .. } | Self::Search { .. } | Self::SearchUsers { .. } => "search",
            Self::LoadDynamicInit { .. }
            | Self::LoadDynamicRefresh { .. }
            | Self::LoadDynamicMore { .. } => "dynamic",
            Self::LoadHistoryInit { .. } | Self::LoadHistoryMore { .. } => "history",
            Self::DeleteHistory { .. } => "history_delete",
            Self::LoadArticle { .. } => "article_detail",
            Self::LoadLiveInit { .. } | Self::LoadLiveMore { .. } => "live",
            Self::LoadVideoDetail { .. } | Self::ProbeVideoStreams { .. } => "video_detail",
            Self::LoadUpPage { .. }
            | Self::LoadUpVideos { .. }
            | Self::LoadSeriesList { .. }
            | Self::LoadSeriesArchives { .. } => "up",
            Self::LoadDynamicDetail { .. } => "dynamic_detail",
            Self::LoadBangumiIndex { .. } | Self::LoadBangumiDetail { .. } => "bangumi",
            Self::LoadBangumiFollowList { .. } => "bangumi_follow_list",
            Self::LoadBangumiSearch { .. } => "bangumi_search",
            Self::LoadNotificationsInit { .. } | Self::LoadNotificationsMore { .. } => {
                "notifications"
            }
            Self::LoadChatSessions { .. } | Self::LoadChatDetail { .. } => "chat",
            Self::SendChatMessage { .. } => "chat_send",
            Self::LoadMallOrders { .. } => "mall",
            Self::LoadMallExpress { .. } => "mall_express",
            Self::LoadMallExpressTrack { .. } => "mall_express_track",
        }
    }
}

/// Probe a video's stream list and report whether HDR and Hi-Res are
/// available.  Returns `(None, None)` when the playurl request fails so the
/// detail page can show an "unknown" state instead of a wrong "no".
async fn probe_stream_support(
    api_client: &ApiClient,
    bvid: &str,
    cid: i64,
) -> (Option<bool>, Option<bool>) {
    let options = crate::domain::playback::PlaybackOptions::default();
    match api_client.get_play_url(bvid, cid, options).await {
        Ok(data) => {
            // 125 = HDR, 126 = Dolby Vision, 127 = 8K.
            let hdr = data.dash.video.iter().any(|stream| stream.id >= 125);
            let hires = data.dash.audio.iter().any(|stream| stream.id == 30252)
                || data.dash.flac.as_ref().and_then(|flac| flac.audio.as_ref()).is_some();
            (Some(hdr), Some(hires))
        }
        Err(_) => (None, None),
    }
}

async fn handle_command(api_client: Arc<ApiClient>, command: NetworkCommand) -> NetworkEvent {
    match command {
        NetworkCommand::CancelPending => unreachable!("handled by worker"),
        NetworkCommand::LoadHome {
            req_id,
            feed,
            use_guest_feed,
            rid,
        } => {
            let result = match (feed, use_guest_feed) {
                (HomeFeed::Recommended, false) => api_client.get_recommendations().await,
                (HomeFeed::Recommended, true) | (HomeFeed::Popular, _) => {
                    api_client.get_popular_videos(1, 20).await
                }
                _ => api_client.get_home_feed(feed, 1, 20, rid).await,
            };
            match result {
                Ok(mut videos) => {
                    enrich_followers(&api_client, &mut videos).await;
                    NetworkEvent::HomeLoaded {
                        req_id,
                        feed,
                        videos,
                    }
                }
                Err(e) => failed(req_id, "home", e),
            }
        }
        NetworkCommand::LoadHomeMore {
            req_id,
            fresh_idx,
            feed,
            use_guest_feed,
            rid,
        } => {
            let result = match (feed, use_guest_feed) {
                (HomeFeed::Recommended, false) => {
                    api_client.get_recommendations_paged(fresh_idx).await
                }
                (HomeFeed::Recommended, true) | (HomeFeed::Popular, _) => {
                    api_client.get_popular_videos(fresh_idx, 20).await
                }
                _ => api_client.get_home_feed(feed, fresh_idx, 20, rid).await,
            };
            match result {
                Ok(mut videos) => {
                    enrich_followers(&api_client, &mut videos).await;
                    NetworkEvent::HomeMoreLoaded {
                        req_id,
                        feed,
                        videos,
                    }
                }
                Err(e) => failed(req_id, "home_more", e),
            }
        }
        NetworkCommand::LoadSectionRanking { req_id, rid } => {
            match api_client.get_ranking(rid).await {
                Ok(mut videos) => {
                    enrich_followers(&api_client, &mut videos).await;
                    NetworkEvent::SectionRankingLoaded {
                        req_id,
                        rid,
                        videos,
                    }
                }
                Err(e) => failed(req_id, "sections", e),
            }
        }
        NetworkCommand::LoadHotwords { req_id } => match api_client.get_hot_search().await {
            Ok(hotwords) => NetworkEvent::HotwordsLoaded { req_id, hotwords },
            Err(e) => failed(req_id, "hotwords", e),
        },
        NetworkCommand::Search {
            req_id,
            keyword,
            page,
        } => match api_client.search_videos(&keyword, page).await {
            Ok(data) => NetworkEvent::SearchLoaded {
                req_id,
                keyword,
                page,
                results: data.result.unwrap_or_default(),
                total: data.num_results.unwrap_or(0),
            },
            Err(e) => failed(req_id, "search", e),
        },
        NetworkCommand::SearchUsers {
            req_id,
            keyword,
            page,
        } => match api_client.search_users(&keyword, page).await {
            Ok(data) => NetworkEvent::SearchUsersLoaded {
                req_id,
                keyword,
                page,
                results: data.result.unwrap_or_default(),
                total: data.num_results.unwrap_or(0),
            },
            Err(e) => failed(req_id, "search_users", e),
        },
        NetworkCommand::LoadUpPage { req_id, mid, order } => {
            match api_client.get_space_info(mid).await {
                Ok(profile) => {
                    let relation = api_client.get_relation_stat(mid).await.ok();
                    let is_followed = api_client.get_follow_status(mid).await.ok();
                    let folders = api_client
                        .get_favorite_folders(mid)
                        .await
                        .unwrap_or_default();
                    match api_client.get_space_videos(mid, 1, 40, order).await {
                        Ok(videos) => NetworkEvent::UpPageLoaded {
                            req_id,
                            mid,
                            order,
                            profile,
                            relation,
                            is_followed,
                            videos,
                            folders,
                        },
                        Err(error) => failed(req_id, "up_page", error),
                    }
                }
                Err(error) => failed(req_id, "up_page", error),
            }
        }
        NetworkCommand::LoadUpVideos {
            req_id,
            mid,
            page,
            order,
        } => match api_client.get_space_videos(mid, page, 40, order).await {
            Ok(videos) => NetworkEvent::UpVideosLoaded {
                req_id,
                mid,
                page,
                order,
                videos,
            },
            Err(error) => failed(req_id, "up_videos", error),
        },
        NetworkCommand::LoadFavoriteResources {
            req_id,
            owner_mid,
            media_id,
            page,
            order,
        } => match api_client
            .get_favorite_resources(media_id, page, 40, order)
            .await
        {
            Ok(resources) => NetworkEvent::FavoriteResourcesLoaded {
                req_id,
                owner_mid,
                media_id,
                page,
                order,
                resources,
            },
            Err(error) => failed(req_id, "favorite_resources", error),
        },
        NetworkCommand::LoadSeriesList { req_id, mid, page } => {
            match api_client.get_series_list(mid, page, 20).await {
                Ok(data) => NetworkEvent::SeriesListLoaded {
                    req_id,
                    mid,
                    page,
                    data,
                },
                Err(error) => failed(req_id, "series_list", error),
            }
        }
            NetworkCommand::LoadSeriesArchives {
                req_id,
                mid,
                series_id,
                is_series,
                page,
            } => match api_client
                .get_series_archives(mid, series_id, is_series, page, 30)
                .await
            {
                Ok(data) => NetworkEvent::SeriesArchivesLoaded {
                    req_id,
                    mid,
                    series_id,
                    is_series,
                    page,
                    data,
                },
            Err(error) => failed(req_id, "series_archives", error),
        },
        NetworkCommand::BuildUpPlaylist {
            req_id,
            mid,
            name,
            video_order,
            play_order,
        } => {
            let mut items = Vec::new();
            let mut page = 1;
            loop {
                let data = match api_client
                    .get_space_videos(mid, page, 40, video_order)
                    .await
                {
                    Ok(data) => data,
                    Err(error) => return failed(req_id, "playlist_build", error),
                };
                let total = data.page.count as usize;
                let empty = data.list.vlist.is_empty();
                items.extend(data.list.vlist.into_iter().map(|video| PlaylistItem {
                    bvid: video.bvid,
                    aid: video.aid,
                    cid: None,
                    title: video.title,
                    uploader_mid: Some(video.mid.unwrap_or(mid)),
                    duration: video
                        .length
                        .as_deref()
                        .and_then(parse_length_to_seconds)
                        .or(video.duration),
                    page: None,
                }));
                if empty || items.len() >= total {
                    break;
                }
                page += 1;
            }
            let start_index = if play_order == PlayOrder::Reverse {
                items.len().saturating_sub(1)
            } else {
                0
            };
            NetworkEvent::PlaylistLoaded {
                req_id,
                items,
                source: PlaylistSource::Uploader { mid, name },
                start_index,
                order: play_order,
            }
        }
        NetworkCommand::BuildFavoritePlaylist {
            req_id,
            media_id,
            title,
            favorite_order,
            play_order,
        } => {
            let mut items = Vec::new();
            let mut page = 1;
            loop {
                let data = match api_client
                    .get_favorite_resources(media_id, page, 40, favorite_order)
                    .await
                {
                    Ok(data) => data,
                    Err(error) => return failed(req_id, "playlist_build", error),
                };
                let has_more = data.has_more.unwrap_or(false);
                items.extend(data.medias.into_iter().filter_map(|media| {
                    Some(PlaylistItem {
                        bvid: media.bvid?,
                        aid: media.id,
                        cid: None,
                        title: media.title,
                        uploader_mid: media.upper.as_ref().map(|upper| upper.mid),
                        duration: media.duration,
                        page: None,
                    })
                }));
                if !has_more {
                    break;
                }
                page += 1;
            }
            let start_index = if play_order == PlayOrder::Reverse {
                items.len().saturating_sub(1)
            } else {
                0
            };
            NetworkEvent::PlaylistLoaded {
                req_id,
                items,
                source: PlaylistSource::Favorites { media_id, title },
                start_index,
                order: play_order,
            }
        }
        NetworkCommand::LoadFavoritesInit { req_id, mid } => {
            match api_client.get_watch_later(1, 20).await {
                Ok(watch_later) => {
                    let created = api_client
                        .get_favorite_folders(mid)
                        .await
                        .unwrap_or_default();
                    match api_client.get_collected_folders(mid, 1, 50).await {
                        Ok(collected) => NetworkEvent::FavoritesInitLoaded {
                            req_id,
                            mid,
                            watch_later,
                            created,
                            collected: collected.list,
                        },
                        Err(error) => failed(req_id, "favorites_init", error),
                    }
                }
                Err(error) => failed(req_id, "favorites_init", error),
            }
        }
        NetworkCommand::RefreshFavoriteFolders { req_id, mid } => {
            let created = api_client
                .get_favorite_folders(mid)
                .await
                .unwrap_or_default();
            let collected = match api_client.get_collected_folders(mid, 1, 50).await {
                Ok(collected) => collected.list,
                Err(_) => Vec::new(),
            };
            NetworkEvent::FavoriteFoldersRefreshed {
                req_id, created, collected,
            }
        }
        NetworkCommand::LoadFavoritesContent {
            req_id,
            source,
            page,
        } => match source {
            FavoriteSource::WatchLater => match api_client.get_watch_later(page, 20).await {
                Ok(data) => NetworkEvent::FavoritesWatchLaterLoaded { req_id, page, data },
                Err(error) => failed(req_id, "favorites_content", error),
            },
            FavoriteSource::Created { media_id, .. } => match api_client
                .get_favorite_resources(media_id, page, 40, FavoriteOrder::RecentlyFavorited)
                .await
            {
                Ok(data) => NetworkEvent::FavoritesCreatedLoaded {
                    req_id,
                    media_id,
                    page,
                    data,
                },
                Err(error) => failed(req_id, "favorites_content", error),
            },
            FavoriteSource::Collected { season_id, mid, .. } => match api_client
                .get_collected_season_videos(mid, season_id, page, 30)
                .await
            {
                Ok(data) => NetworkEvent::FavoritesCollectedLoaded {
                    req_id,
                    season_id,
                    page,
                    data,
                },
                Err(error) => failed(req_id, "favorites_content", error),
            },
        },
        NetworkCommand::LoadDynamicInit {
            req_id,
            tab,
            host_mid,
        } => {
            let up_list = match api_client.get_dynamic_portal().await {
                Ok(portal) => portal.up_list,
                Err(_) => None,
            };
            let feed_type = tab.get_feed_type();
            match api_client.get_dynamic_feed(None, feed_type, host_mid).await {
                Ok(data) => NetworkEvent::DynamicLoaded {
                    req_id,
                    append: false,
                    up_list,
                    items: data.items.unwrap_or_default(),
                    offset: data.offset,
                    has_more: data.has_more.unwrap_or(false),
                },
                Err(e) => failed(req_id, "dynamic_init", e),
            }
        }
        NetworkCommand::LoadDynamicRefresh {
            req_id,
            tab,
            host_mid,
        } => {
            let feed_type = tab.get_feed_type();
            match api_client.get_dynamic_feed(None, feed_type, host_mid).await {
                Ok(data) => NetworkEvent::DynamicLoaded {
                    req_id,
                    append: false,
                    up_list: None,
                    items: data.items.unwrap_or_default(),
                    offset: data.offset,
                    has_more: data.has_more.unwrap_or(false),
                },
                Err(e) => failed(req_id, "dynamic_refresh", e),
            }
        }
        NetworkCommand::LoadDynamicMore {
            req_id,
            offset,
            tab,
            host_mid,
        } => {
            let feed_type = tab.get_feed_type();
            match api_client
                .get_dynamic_feed(Some(&offset), feed_type, host_mid)
                .await
            {
                Ok(data) => NetworkEvent::DynamicLoaded {
                    req_id,
                    append: true,
                    up_list: None,
                    items: data.items.unwrap_or_default(),
                    offset: data.offset,
                    has_more: data.has_more.unwrap_or(false),
                },
                Err(e) => failed(req_id, "dynamic_more", e),
            }
        }
        NetworkCommand::LoadHistoryInit { req_id } => {
            match api_client.get_history(None, None, None).await {
                Ok(data) => NetworkEvent::HistoryLoaded {
                    req_id,
                    append: false,
                    data,
                },
                Err(e) => failed(req_id, "history_init", e),
            }
        }
        NetworkCommand::LoadNotificationsInit { req_id, feed_type } => {
            match api_client.get_msg_feed(feed_type, 1).await {
                Ok(items) => {
                    let unread = api_client.get_msg_unread().await.unwrap_or(
                        crate::api::msg::UnreadData {
                            at: 0,
                            chat: 0,
                            like: 0,
                            reply: 0,
                            sys_msg: 0,
                        },
                    );
                    NetworkEvent::NotificationsLoaded {
                        req_id,
                        append: false,
                        feed_type,
                        items,
                        unread: (
                            unread.reply,
                            unread.at,
                            unread.like,
                            unread.sys_msg,
                        ),
                    }
                }
                Err(e) => failed(req_id, "notifications_init", e),
            }
        }
        NetworkCommand::LoadNotificationsMore {
            req_id,
            feed_type,
            page,
        } => match api_client.get_msg_feed(feed_type, page).await {
            Ok(items) => NetworkEvent::NotificationsLoaded {
                req_id,
                append: true,
                feed_type,
                items,
                unread: (0, 0, 0, 0),
            },
            Err(e) => failed(req_id, "notifications_more", e),
        },
        NetworkCommand::LoadChatSessions { req_id } => {
            match api_client.get_msg_sessions().await {
                Ok(sessions) => NetworkEvent::ChatSessionsLoaded { req_id, sessions },
                Err(e) => failed(req_id, "chat_sessions", e),
            }
        }
        NetworkCommand::LoadChatDetail { req_id, talker_id } => {
            match api_client.get_chat_detail(talker_id).await {
                Ok(messages) => NetworkEvent::ChatDetailLoaded {
                    req_id,
                    talker_id,
                    messages,
                },
                Err(e) => failed(req_id, "chat_detail", e),
            }
        }
        NetworkCommand::SendChatMessage {
            req_id,
            talker_id,
            content,
        } => match api_client.send_chat_message(talker_id, &content).await {
            Ok(()) => NetworkEvent::ChatMessageSent {
                req_id,
                talker_id,
                ok: true,
                error: None,
            },
            Err(e) => NetworkEvent::ChatMessageSent {
                req_id,
                talker_id,
                ok: false,
                error: Some(format!("{e:#}")),
            },
        },
        NetworkCommand::LoadMallOrders { req_id, gf_only } => {
            match api_client.get_mall_orders().await {
                Ok(mut orders) => {
                    if gf_only {
                        // 工房页面只显示 B站小店/GF_DZ 数字商品订单（order_type=9），
                        // 不显示会员购实体商品订单（order_type=2 等）。
                        orders.retain(|o| o.order_type == 9);
                    }
                    NetworkEvent::MallOrdersLoaded { req_id, orders }
                }
                Err(e) => failed(req_id, "mall_orders", e),
            }
        }
        NetworkCommand::LoadMallExpress { req_id, order_id } => {
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                api_client.get_mall_express(order_id),
            )
            .await
            {
                Ok(Ok(express)) => NetworkEvent::MallExpressLoaded {
                    req_id,
                    order_id,
                    express,
                },
                Ok(Err(e)) => failed(req_id, "mall_express", e),
                Err(_) => failed(
                    req_id,
                    "mall_express",
                    anyhow::anyhow!("物流查询超时(10s)，请重试"),
                ),
            }
        }
        NetworkCommand::LoadMallExpressTrack { req_id, order_id } => {
            let timeout_dur = std::time::Duration::from_secs(10);
            match tokio::time::timeout(timeout_dur, api_client.get_mall_express_track(order_id)).await
            {
                Ok(Ok(express)) => NetworkEvent::MallExpressTrackLoaded {
                    req_id,
                    order_id,
                    express,
                },
                Ok(Err(e)) => NetworkEvent::MallExpressTrackFailed {
                    req_id,
                    order_id,
                    error: redact_url_queries(&e.to_string()),
                },
                Err(_) => NetworkEvent::MallExpressTrackFailed {
                    req_id,
                    order_id,
                    error: "物流轨迹查询超时(10s)，请重试".to_string(),
                },
            }
        }
        NetworkCommand::LoadHistoryMore { req_id, cursor } => match api_client
            .get_history(
                Some(cursor.max),
                Some(cursor.view_at),
                Some(cursor.business.as_str()),
            )
            .await
        {
            Ok(data) => NetworkEvent::HistoryLoaded {
                req_id,
                append: true,
                data,
            },
            Err(e) => failed(req_id, "history_more", e),
        },
        NetworkCommand::DeleteHistory { req_id, keys } => {
            let mut successful = Vec::new();
            let mut failed_keys = Vec::new();
            for key in keys {
                match api_client.delete_history_item(&key).await {
                    Ok(()) => successful.push(key),
                    Err(error) => failed_keys.push((key, error.to_string())),
                }
            }
            NetworkEvent::HistoryDeleted {
                req_id,
                successful,
                failed: failed_keys,
            }
        }
        NetworkCommand::LoadArticle { req_id, cvid } => match api_client.get_article(cvid).await {
            Ok(article) => {
                let comment_oid = if article.id > 0 { article.id } else { cvid };
                let comments = api_client
                    .get_dynamic_comments(comment_oid, CommentType::Article.as_i32(), 1)
                    .await
                    .ok()
                    .map(|data| {
                        let mut comments = data.hots.unwrap_or_default();
                        for comment in data.replies.unwrap_or_default() {
                            if !comments
                                .iter()
                                .any(|existing| existing.rpid == comment.rpid)
                            {
                                comments.push(comment);
                            }
                        }
                        comments
                    })
                    .unwrap_or_default();
                NetworkEvent::ArticleLoaded {
                    req_id,
                    cvid,
                    article,
                    comments,
                }
            }
            Err(error) => failed(req_id, "article_detail", error),
        },
        NetworkCommand::LoadLiveInit { req_id } => {
            let rooms = match api_client.get_live_home_rooms().await {
                Ok(rooms) => Ok(rooms),
                Err(_) => api_client.get_live_recommendations().await,
            };
            match rooms {
                Ok(rooms) => NetworkEvent::LiveLoaded {
                    req_id,
                    append: false,
                    rooms,
                },
                Err(e) => failed(req_id, "live_init", e),
            }
        }
        NetworkCommand::LoadLiveMore { req_id } => {
            match api_client.get_live_recommendations().await {
                Ok(rooms) => NetworkEvent::LiveLoaded {
                    req_id,
                    append: true,
                    rooms,
                },
                Err(e) => failed(req_id, "live_more", e),
            }
        }
        NetworkCommand::LoadVideoDetail { req_id, bvid, aid } => {
            let video_info = match api_client.get_video_info(&bvid).await {
                Ok(info) => info,
                Err(e) => return failed(req_id, "video_detail", e),
            };
            let cid = video_info
                .pages
                .as_ref()
                .and_then(|pages| pages.first())
                .map(|page| page.cid)
                .unwrap_or(video_info.cid);
            let (
                (comments, has_more_comments),
                related_videos,
                (hdr_supported, hires_supported),
                like_result,
                coin_result,
                fav_result,
                watch_later_result,
            ) =
                tokio::join!(
                    async {
                        match api_client.get_comments(aid, 1).await {
                            Ok(data) => {
                                let comments = data.replies.unwrap_or_default();
                                let has_more = data
                                    .page
                                    .map(|p| p.count.unwrap_or(0) > comments.len() as i32)
                                    .unwrap_or(false);
                                (comments, has_more)
                            }
                            Err(_) => (Vec::new(), false),
                        }
                    },
                    async { api_client.get_related_videos(&bvid).await.unwrap_or_default() },
                    async { probe_stream_support(&api_client, &bvid, cid).await },
                    async {
                        match api_client.get_video_like_status(&bvid).await {
                            Ok(v) => Ok(v),
                            Err(_) => Err("点赞状态: 需要登录"),
                        }
                    },
                    async {
                        match api_client.get_video_coin_status(&bvid).await {
                            Ok(v) => Ok(v),
                            Err(_) => Err("投币状态: 需要登录"),
                        }
                    },
                    async {
                        match api_client.get_default_favorite_folder(aid).await {
                            Ok((mid, fav)) => Ok((mid, fav)),
                            Err(_) => Err("收藏状态: 需要登录"),
                        }
                    },
                    async {
                        match api_client.get_watch_later_status(aid).await {
                            Ok(v) => Ok(v),
                            Err(_) => Err("稍后再看状态: 需要登录"),
                        }
                    },
                );
            let liked = like_result.unwrap_or(false);
            let coined = coin_result.unwrap_or(0);
            let in_watch_later = watch_later_result.unwrap_or(false);
            let (default_media_id, favorited) = match fav_result {
                Ok((mid, fav)) => (Some(mid), fav),
                Err(_) => (None, false),
            };
            let mut interaction_errors = Vec::new();
            if let Err(e) = &like_result { interaction_errors.push(*e); }
            if let Err(e) = &coin_result { interaction_errors.push(*e); }
            if let Err(e) = &fav_result { interaction_errors.push(*e); }
            if let Err(e) = &watch_later_result { interaction_errors.push(*e); }
            let interaction_error = if interaction_errors.is_empty() {
                None
            } else {
                Some(interaction_errors.join(", "))
            };
            NetworkEvent::VideoDetailLoaded {
                req_id,
                bvid,
                video_info,
                comments,
                has_more_comments,
                related_videos,
                hdr_supported,
                hires_supported,
                liked,
                coined,
                favorited,
                in_watch_later,
                default_media_id,
                interaction_error,
            }
        }
        NetworkCommand::ProbeVideoStreams { req_id, bvid, cid } => {
            let (hdr_supported, hires_supported) =
                probe_stream_support(&api_client, &bvid, cid).await;
            NetworkEvent::VideoStreamSupportLoaded {
                req_id,
                bvid,
                hdr_supported,
                hires_supported,
            }
        }
        NetworkCommand::LoadDynamicDetail { req_id, dynamic_id } => {
            let dynamic_item = match api_client.get_dynamic_detail(&dynamic_id).await {
                Ok(item) => item,
                Err(e) => return failed(req_id, "dynamic_detail", e),
            };
            let comment_type = dynamic_item.comment_type();
            let comment_oid = dynamic_item.comment_oid(&dynamic_id);
            let (comments, has_more_comments) = if let Some(oid) = comment_oid {
                match api_client.get_dynamic_comments(oid, comment_type, 1).await {
                    Ok(data) => {
                        let comments = data.replies.unwrap_or_default();
                        let has_more = data
                            .page
                            .map(|p| p.count.unwrap_or(0) > comments.len() as i32)
                            .unwrap_or(false);
                        (comments, has_more)
                    }
                    Err(_) => (Vec::new(), false),
                }
            } else {
                (Vec::new(), false)
            };
            let mut image_urls = Vec::new();
            if dynamic_item.is_draw() {
                image_urls.extend(
                    dynamic_item
                        .draw_images()
                        .into_iter()
                        .map(|s| s.to_string()),
                );
            }
            if dynamic_item.is_opus() {
                image_urls.extend(
                    dynamic_item
                        .opus_images()
                        .into_iter()
                        .map(|s| s.to_string()),
                );
            }
            NetworkEvent::DynamicDetailLoaded {
                req_id,
                dynamic_id,
                dynamic_item,
                comments,
                has_more_comments,
                image_urls,
            }
        }
        NetworkCommand::LoadBangumiIndex { req_id } => match api_client.get_bangumi_rank().await {
            Ok(items) => NetworkEvent::BangumiIndexLoaded { req_id, items },
            Err(e) => failed(req_id, "bangumi_index", e),
        },
        NetworkCommand::LoadBangumiFollowList { req_id, mid } => {
            match api_client.get_bangumi_follow_list(mid, 1, 1).await {
                Ok(items) => NetworkEvent::BangumiFollowListLoaded { req_id, items },
                Err(e) => failed(req_id, "bangumi_follow_list", e),
            }
        }
        NetworkCommand::LoadBangumiSearch { req_id, keyword, page } => {
            match api_client.search_bangumi(&keyword, page).await {
                Ok(items) => NetworkEvent::BangumiSearchLoaded { req_id, keyword, items },
                Err(e) => failed(req_id, "bangumi_search", e),
            }
        }
        NetworkCommand::LoadBangumiDetail { req_id, season_id } => {
            match api_client.get_bangumi_season(season_id).await {
                Ok(season) => {
                    // The pgc season view's user_status.follow is unreliable:
                    // it can report login=1 + follow=0 even when the season is
                    // in the user's real follow list (observed on this account).
                    // Only trust follow=1 as a fast path; follow=0 must be
                    // cross-checked against the real follow list.
                    let status_follow = season
                        .user_status
                        .as_ref()
                        .map(|u| u.follow == 1)
                        .unwrap_or(false);
                    let followed = if status_follow {
                        true
                    } else {
                        api_client
                            .is_bangumi_followed(season_id)
                            .await
                            .unwrap_or(false)
                    };
                    NetworkEvent::BangumiDetailLoaded {
                        req_id,
                        season_id,
                        season,
                        followed,
                    }
                }
                Err(e) => failed(req_id, "bangumi_detail", e),
            }
        }
    }
}

async fn enrich_followers(api_client: &Arc<ApiClient>, videos: &mut [VideoItem]) {
    let mids = videos
        .iter()
        .filter_map(|video| video.owner.as_ref().map(|owner| owner.mid))
        .filter(|mid| *mid > 0)
        .collect::<std::collections::BTreeSet<_>>();
    let followers = stream::iter(mids)
        .map(|mid| {
            let client = Arc::clone(api_client);
            async move {
                let follower = client
                    .get_relation_stat(mid)
                    .await
                    .ok()
                    .and_then(|stat| stat.follower);
                (mid, follower)
            }
        })
        .buffer_unordered(6)
        .collect::<HashMap<_, _>>()
        .await;
    for video in videos {
        if let Some(owner) = video.owner.as_mut() {
            owner.follower = followers.get(&owner.mid).copied().flatten();
        }
    }
}

fn failed(req_id: u64, target: &'static str, error: anyhow::Error) -> NetworkEvent {
    let safe_error = redact_url_queries(&error.to_string());
    if let Some(mut dir) = dirs::config_dir() {
        dir.push("bilibili-tui");
        let log_path = dir.join("debug.log");
        if std::fs::create_dir_all(&dir).is_ok()
            && let Ok(mut log) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
        {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o600));
            }
            use std::io::Write;
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let safe_log_error = redact_url_queries(&format!("{error:#}"));
            let _ = writeln!(
                log,
                "[{timestamp}] Network request failed\nTarget: {target}\nRequest ID: {req_id}\nError: {safe_log_error}\n"
            );
        }
    }

    NetworkEvent::RequestFailed {
        req_id,
        target,
        error: safe_error,
    }
}

pub(crate) fn redact_url_queries(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(ch) = chars.next() {
        output.push(ch);
        if ch == '?' {
            output.push_str("<redacted>");
            while chars
                .peek()
                .is_some_and(|next| !next.is_whitespace() && !matches!(next, '"' | '\''))
            {
                chars.next();
            }
        }
    }
    output
}

#[cfg(test)]
mod security_tests {
    use super::redact_url_queries;

    #[test]
    fn network_log_redacts_url_query_parameters() {
        let value =
            redact_url_queries("GET https://cdn.example/video?token=secret&expires=1 failed");
        assert_eq!(value, "GET https://cdn.example/video?<redacted> failed");
        assert!(!value.contains("secret"));
    }
}
