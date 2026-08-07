use crate::api::favorite::{FavoriteOrder, FavoriteSource};
use crate::api::history::HistoryKey;
use crate::api::recommend::HomeFeed;
use crate::api::space::SpaceVideoOrder;
use crate::api::video::VideoPage;
use crate::domain::playback::{PlayOrder, PlaylistItem, PlaylistSource};
use crate::domain::playback::PlaybackOptions;
use crate::infrastructure::persistence::{Credentials, DanmakuConfig, Keybindings, VideoQuality};
use crate::presentation::tui::DynamicTab;

/// Actions that can be triggered from UI components
#[derive(Debug, Clone)]
pub enum AppAction {
    /// Quit the application
    Quit,
    /// Switch to home page
    SwitchToHome,
    /// Refresh home page recommendations (force reload)
    RefreshHome,
    SwitchHomeFeed(HomeFeed),
    /// Switch the ranking section (rid) for the ranking feed.
    SwitchRankingRid(i64),
    /// Switch to the standalone sections (分区) page.
    SwitchToSections,
    /// Load videos for a section (rid) in the sections page.
    SelectSection(i64),
    /// Switch to the message notification center.
    SwitchToNotifications,
    /// Switch the notification tab (reply / at / like / sys).
    SwitchNotifTab(crate::presentation::tui::NotifTab),
    /// Refresh the current notification tab.
    RefreshNotifications,
    /// Load more notifications for the current tab.
    LoadMoreNotifications,
    /// Open a private-message conversation with a user.
    OpenChat(i64),
    /// Return from chat detail view back to the session list.
    BackToChatList,
    /// Send a private message.
    SendChatMessage {
        talker_id: i64,
        content: String,
    },
    /// Open the most recent video-share message in the open chat.
    OpenChatVideo(String),
    /// Switch to login page
    SwitchToLogin,
    /// Switch to settings page
    SwitchToSettings,
    /// Open an external URL with the system browser (xdg-open).
    OpenExternalUrl(String),
    /// Switch to the 会员购 (Bilibili mall) page.
    SwitchToMall,
    /// Refresh the mall order list.
    RefreshMall,
    /// Load express info for a mall order.
    LoadMallExpress {
        order_id: i64,
    },
    /// Load express trace (物流轨迹) for a mall order.
    LoadMallExpressTrack {
        order_id: i64,
    },
    /// Switch to history page
    SwitchToHistory,
    /// Login was successful with credentials
    LoginSuccess(Credentials),
    /// Play a video with metadata (bvid, aid, cid, duration)
    PlayVideo {
        bvid: String,
        aid: i64,
        cid: i64,
        duration: i64,
        playback: PlaybackOptions,
    },
    /// Play a video with page info for auto-play next episode
    PlayVideoWithPages {
        bvid: String,
        aid: i64,
        pages: Vec<VideoPage>,
        current_index: usize,
        playback: PlaybackOptions,
    },
    PlayPlaylist {
        items: Vec<PlaylistItem>,
        source: PlaylistSource,
        start_index: usize,
        order: PlayOrder,
    },
    PlayUpAll {
        mid: i64,
        name: String,
        video_order: SpaceVideoOrder,
        play_order: PlayOrder,
    },
    PlayFavoriteAll {
        media_id: i64,
        title: String,
        favorite_order: FavoriteOrder,
        play_order: PlayOrder,
    },
    /// Navigate to next sidebar item
    NavNext,
    /// Navigate to previous sidebar item
    NavPrev,
    CancelPendingLoads,
    /// Search for videos
    Search(String),
    /// Search for users (UP主)
    SearchUsers(String),
    /// Refresh dynamic feed
    RefreshDynamic,
    /// Open video detail page (bvid, aid)
    OpenVideoDetail(String, i64),
    /// Open an uploader's public space by member ID.
    OpenUpPage(i64),
    RefreshUpPage,
    SwitchUpVideoOrder(SpaceVideoOrder),
    LoadMoreUpVideos,
    OpenFavoriteFolder(i64),
    SwitchFavoriteOrder(FavoriteOrder),
    LoadMoreFavoriteResources,
    OpenSeriesFolder(i64),
    LoadMoreSeriesVideos,
    SelectFavoriteSource(FavoriteSource),
    LoadMoreFavorites,
    /// Open dynamic detail page for image/text dynamics (dynamic_id)
    OpenDynamicDetail(String),
    /// Go back to previous page
    BackToList,
    /// Load more recommendations
    LoadMoreRecommendations,
    /// Load more search results
    LoadMoreSearch,
    /// Load more user search results
    LoadMoreSearchUsers,
    /// Load more dynamic items
    LoadMoreDynamic,
    /// Load more history items
    LoadMoreHistory,
    DeleteHistoryItems(Vec<HistoryKey>),
    OpenArticle(i64),
    OpenHistoryBangumi {
        season_id: i64,
        ep_id: i64,
    },
    /// Load more comments in video detail page
    LoadMoreComments,
    /// Toggle comment replies expansion
    ToggleCommentReplies,
    /// Switch dynamic tab
    SwitchDynamicTab(DynamicTab),
    /// Select UP master (0 = all, 1+ = specific UP)
    SelectUpMaster(usize),
    /// Switch to next theme variant
    NextTheme,
    /// Set a specific theme by Opaline theme ID
    SetTheme(String),
    /// Save keybindings to config
    SaveKeybindings(Box<Keybindings>),
    /// Save live/video danmaku rendering settings.
    SaveDanmakuConfig(Box<DanmakuConfig>),
    /// Save the auto-play-on-video-open preference.
    SaveAutoPlay(bool),
    SaveVideoQuality(VideoQuality),
    /// Logout and return to login page
    Logout,
    /// Like or unlike a comment (oid, rpid, comment_type)
    LikeComment {
        oid: i64,
        rpid: i64,
        comment_type: i32,
    },
    /// Like or unlike the current video (bvid, aid)
    LikeVideo {
        bvid: String,
        aid: i64,
    },
    /// Give one coin to the current video (bvid, aid)
    CoinVideo {
        bvid: String,
        aid: i64,
    },
    /// Add or remove the current video from the default favorite folder (bvid, aid)
    FavoriteVideo {
        bvid: String,
        aid: i64,
    },
    /// Load the user's favorite folders list (for the folder picker in video detail).
    LoadUserFavoriteFolders,
    /// Toggle the current video in the user's watch-later list (aid)
    ToggleWatchLater { aid: i64 },
    /// Remove a video from the watch-later list (used from the favorites page)
    RemoveFromWatchLater { aid: i64 },
    /// Add a comment (oid, comment_type, message, optional root rpid for replies)
    AddComment {
        oid: i64,
        comment_type: i32,
        message: String,
        root: Option<i64>,
    },
    /// Toggle follow/unfollow an uploader (mid)
    ToggleFollow { mid: i64 },
    /// Toggle follow/unfollow a bangumi season (追番)
    ToggleBangumiFollow { season_id: i64 },
    /// Switch to live page
    SwitchToLive,
    /// Open live room detail
    OpenLiveDetail(i64),
    /// Refresh live recommendations
    RefreshLive,
    /// Load more live rooms
    LoadMoreLive,
    /// Play live stream
    PlayLive {
        room_id: i64,
        title: String,
    },
    /// Send a danmaku to a live room
    SendLiveDanmaku {
        room_id: i64,
        msg: String,
    },
    /// Publish a text dynamic
    PostDynamic {
        content: String,
    },
    /// Switch to bangumi page
    SwitchToBangumi,
    /// Refresh bangumi timeline
    RefreshBangumi,
    /// Switch bangumi tab
    SwitchBangumiTab(BangumiTab),
    /// Search bangumi by keyword
    SearchBangumi { keyword: String },
    /// Open bangumi detail page
    OpenBangumiDetail(i64),
    /// Load more bangumi index items
    LoadMoreBangumi,
    /// Play a bangumi episode
    PlayBangumiEpisode {
        ep_id: i64,
        season_id: i64,
        title: String,
    },
    /// Create a new favorite folder (title, intro, privacy)
    CreateFavoriteFolder {
        title: String,
        intro: String,
        privacy: i32,
    },
    /// Delete a favorite folder by media_id
    DeleteFavoriteFolder(i64),
    /// Rename a favorite folder (media_id, new title)
    RenameFavoriteFolder {
        media_id: i64,
        title: String,
    },
    /// Add or remove a video from a specific favorite folder (aid, media_id, add)
    FavoriteVideoInFolder {
        aid: i64,
        media_id: i64,
        add: bool,
    },
    /// No action
    None,
}

/// Bangumi page tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BangumiTab {
    Timeline,
    Index,
    Follow,
}
