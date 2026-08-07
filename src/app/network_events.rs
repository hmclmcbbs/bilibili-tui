use crate::app::App;
use crate::application::network;
use crate::presentation::tui::{Page, VideoCard};

impl App {
    pub(super) fn drain_network_events(&mut self) {
        while let Ok(event) = self.network_event_rx.try_recv() {
            self.handle_network_event(event);
        }
    }

    fn handle_network_event(&mut self, event: network::NetworkEvent) {
        match event {
            network::NetworkEvent::HomeLoaded {
                req_id,
                feed,
                videos,
            } => {
                if !self.is_latest_request("home", req_id) {
                    return;
                }
                if let Page::Home(page) = &mut self.current_page {
                    page.apply_recommendations(feed, videos);
                }
            }
            network::NetworkEvent::HomeMoreLoaded {
                req_id,
                feed,
                videos,
            } => {
                if !self.is_latest_request("home_more", req_id) {
                    return;
                }
                if let Page::Home(page) = &mut self.current_page {
                    page.apply_load_more(feed, videos);
                }
            }
            network::NetworkEvent::SectionRankingLoaded {
                req_id,
                rid,
                videos,
            } => {
                if !self.is_latest_request("sections", req_id) {
                    return;
                }
                if let Page::Sections(page) = &mut self.current_page {
                    page.apply_videos(rid, videos);
                }
            }
            network::NetworkEvent::HotwordsLoaded { req_id, hotwords } => {
                if !self.is_latest_request("hotwords", req_id) {
                    return;
                }
                match &mut self.current_page {
                    Page::Home(page) => page.search_mut().set_hotwords(hotwords),
                    Page::Search(page) => page.set_hotwords(hotwords),
                    _ => {}
                }
            }
            network::NetworkEvent::SearchLoaded {
                req_id,
                keyword,
                page,
                results,
                total,
            } => {
                if !self.is_latest_request("search", req_id) {
                    return;
                }
                match &mut self.current_page {
                    Page::Home(home_page) => {
                        let search_page = home_page.search_mut();
                        if search_page.query != keyword {
                            return;
                        }
                        if page <= 1 {
                            search_page.page = 1;
                            search_page.set_results(results, total);
                        } else {
                            search_page.page = page;
                            search_page.total_results = total;
                            search_page.append_results(results);
                        }
                    }
                    Page::Search(search_page) => {
                        if search_page.query != keyword {
                            return;
                        }
                        if page <= 1 {
                            search_page.page = 1;
                            search_page.set_results(results, total);
                        } else {
                            search_page.page = page;
                            search_page.total_results = total;
                            search_page.append_results(results);
                        }
                    }
                    _ => {}
                }
            }
            network::NetworkEvent::SearchUsersLoaded {
                req_id,
                keyword,
                page,
                results,
                total,
            } => {
                if !self.is_latest_request("search", req_id) {
                    return;
                }
                match &mut self.current_page {
                    Page::Home(home_page) => {
                        let search_page = home_page.search_mut();
                        if search_page.query != keyword {
                            return;
                        }
                        if page <= 1 {
                            search_page.user_page = 1;
                            search_page.set_user_results(results, total);
                        } else {
                            search_page.user_page = page;
                            search_page.user_total = total;
                            search_page.append_user_results(results);
                        }
                    }
                    Page::Search(search_page) => {
                        if search_page.query != keyword {
                            return;
                        }
                        if page <= 1 {
                            search_page.user_page = 1;
                            search_page.set_user_results(results, total);
                        } else {
                            search_page.user_page = page;
                            search_page.user_total = total;
                            search_page.append_user_results(results);
                        }
                    }
                    _ => {}
                }
            }
            network::NetworkEvent::DynamicLoaded {
                req_id,
                append,
                up_list,
                items,
                offset,
                has_more,
            } => {
                let key = if append {
                    "dynamic_more"
                } else {
                    "dynamic_refresh"
                };
                if !self.is_latest_request(key, req_id)
                    && !self.is_latest_request("dynamic_init", req_id)
                {
                    return;
                }
                if let Page::Dynamic(page) = &mut self.current_page {
                    if let Some(up_list) = up_list {
                        page.set_up_list(up_list);
                    }
                    if append {
                        page.append_feed(items, offset, has_more);
                        page.loading_more = false;
                    } else {
                        page.set_feed(items, offset, has_more);
                    }
                }
            }
            network::NetworkEvent::HistoryLoaded {
                req_id,
                append,
                data,
            } => {
                let key = if append {
                    "history_more"
                } else {
                    "history_init"
                };
                if !self.is_latest_request(key, req_id) {
                    return;
                }
                if let Page::History(page) = &mut self.current_page {
                    if append {
                        page.apply_history_more(data);
                    } else {
                        page.apply_history_init(data);
                    }
                }
            }
            network::NetworkEvent::NotificationsLoaded {
                req_id,
                append,
                feed_type,
                items,
                unread,
            } => {
                let key = if append {
                    "notifications_more"
                } else {
                    "notifications"
                };
                if !self.is_latest_request(key, req_id) {
                    return;
                }
                if let Page::Notifications(page) = &mut self.current_page {
                    if page.tab.feed_type() != feed_type {
                        return;
                    }
                    page.apply_items(items, append);
                    page.apply_unread(unread.0, unread.1, unread.2, unread.3);
                }
            }
            network::NetworkEvent::ChatSessionsLoaded { req_id, sessions } => {
                if !self.is_latest_request("notifications", req_id) {
                    return;
                }
                if let Page::Notifications(page) = &mut self.current_page {
                    page.apply_sessions(sessions);
                }
            }
            network::NetworkEvent::ChatDetailLoaded {
                req_id,
                talker_id,
                messages,
            } => {
                if !self.is_latest_request("chat_detail", req_id) {
                    return;
                }
                if let Page::Notifications(page) = &mut self.current_page {
                    page.apply_chat_messages(talker_id, messages);
                }
            }
            network::NetworkEvent::ChatMessageSent {
                req_id,
                talker_id: _,
                ok,
                error,
            } => {
                if !self.is_latest_request("chat_send", req_id) {
                    return;
                }
                if let Page::Notifications(page) = &mut self.current_page {
                    page.apply_chat_sent(ok, error);
                }
            }
            network::NetworkEvent::MallOrdersLoaded { req_id, orders } => {
                if !self.is_latest_request("mall_orders", req_id) {
                    return;
                }
                if let Page::Mall(page) = &mut self.current_page {
                    page.apply_orders(orders);
                    // Auto-load express for the first order so the user sees
                    // logistics without pressing Enter first.
                    if let Some(first) = page.selected_order().cloned() {
                        let req_id = self.next_request_id("mall_express");
                        self.send_network_command(network::NetworkCommand::LoadMallExpress {
                            req_id,
                            order_id: first.order_id,
                        });
                    }
                }
            }
            network::NetworkEvent::MallOrdersFailed { req_id, error } => {
                if !self.is_latest_request("mall_orders", req_id) {
                    return;
                }
                if let Page::Mall(page) = &mut self.current_page {
                    page.apply_orders_error(error);
                }
            }
            network::NetworkEvent::MallExpressLoaded {
                req_id: _,
                order_id,
                express,
            } => {
                if let Page::Mall(page) = &mut self.current_page {
                    page.apply_express(order_id, express);
                }
            }
            network::NetworkEvent::MallExpressFailed {
                req_id: _,
                order_id,
                error,
            } => {
                if let Page::Mall(page) = &mut self.current_page {
                    page.apply_express_error(order_id, error);
                }
            }
            network::NetworkEvent::MallExpressTrackLoaded {
                req_id: _,
                order_id,
                express,
            } => {
                if let Page::Mall(page) = &mut self.current_page {
                    page.apply_track(order_id, express);
                }
            }
            network::NetworkEvent::MallExpressTrackFailed {
                req_id: _,
                order_id,
                error,
            } => {
                if let Page::Mall(page) = &mut self.current_page {
                    page.apply_track_error(order_id, error);
                }
            }
            network::NetworkEvent::HistoryDeleted {
                req_id,
                successful,
                failed,
            } => {
                if !self.is_latest_request("history_delete", req_id) {
                    return;
                }
                if let Page::History(page) = &mut self.current_page {
                    page.apply_delete_result(successful, failed);
                }
            }
            network::NetworkEvent::ArticleLoaded {
                req_id,
                cvid,
                article,
                comments,
            } => {
                if !self.is_latest_request("article_detail", req_id) {
                    return;
                }
                if let Page::ArticleDetail(page) = &mut self.current_page
                    && page.cvid == cvid
                {
                    page.set_article(article, comments);
                }
            }
            network::NetworkEvent::LiveLoaded {
                req_id,
                append,
                rooms,
            } => {
                let key = if append { "live_more" } else { "live_init" };
                if !self.is_latest_request(key, req_id) {
                    return;
                }
                if let Page::Live(page) = &mut self.current_page {
                    if append {
                        page.apply_live_more(rooms);
                    } else {
                        page.apply_live_init(rooms);
                    }
                }
            }
            network::NetworkEvent::VideoDetailLoaded {
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
            } => {
                if !self.is_latest_request("video_detail", req_id) {
                    return;
                }
                if let Page::VideoDetail(page) = &mut self.current_page {
                    if page.bvid != bvid {
                        return;
                    }
                    page.video_info = Some(video_info);
                    page.comments = comments;
                    page.comment_page = 1;
                    page.has_more_comments = has_more_comments;
                    page.related_videos = related_videos.clone();
                    page.related_card_grid.clear();
                    for video in &related_videos {
                        let card = VideoCard::new(
                            video.bvid.clone(),
                            video.aid,
                            video.title.clone().unwrap_or_else(|| "无标题".to_string()),
                            video.author_name().to_string(),
                            video.format_views(),
                            video.format_duration(),
                            video.cover_url(),
                        )
                        .with_uploader_mid(video.owner.as_ref().and_then(|owner| owner.mid));
                        page.related_card_grid.add_card(card);
                    }
                    page.loading = false;
                    page.error_message = None;
                    page.hdr_supported = hdr_supported;
                    page.hires_supported = hires_supported;
                    page.streams_probing = false;
                    page.liked = liked;
                    page.coined = coined;
                    page.favorited = favorited;
                    page.in_watch_later = in_watch_later;
                    page.default_media_id = default_media_id;
                    if page.interaction_msg.is_none() {
                        if let Some(err) = interaction_error {
                            page.set_interaction_msg(err);
                        }
                    }
                }
            }
            network::NetworkEvent::VideoStreamSupportLoaded {
                req_id,
                bvid,
                hdr_supported,
                hires_supported,
            } => {
                if !self.is_latest_request("video_detail", req_id) {
                    return;
                }
                if let Page::VideoDetail(page) = &mut self.current_page {
                    if page.bvid != bvid {
                        return;
                    }
                    page.hdr_supported = hdr_supported;
                    page.hires_supported = hires_supported;
                    page.streams_probing = false;
                }
            }
            network::NetworkEvent::UpPageLoaded {
                req_id,
                mid,
                order,
                profile,
                relation,
                is_followed,
                videos,
                folders,
            } => {
                if !self.is_latest_request("up_page", req_id) {
                    return;
                }
                if let Page::Up(page) = &mut self.current_page
                    && page.mid == mid
                    && page.video_order == order
                {
                    page.apply_initial(profile, relation, is_followed, videos, folders);
                }
            }
            network::NetworkEvent::UpVideosLoaded {
                req_id,
                mid,
                page: loaded_page,
                order,
                videos,
            } => {
                if !self.is_latest_request("up_videos", req_id) {
                    return;
                }
                if let Page::Up(page) = &mut self.current_page
                    && page.mid == mid
                    && page.video_order == order
                {
                    if loaded_page == 1 {
                        page.videos.clear();
                    }
                    page.apply_more_videos(loaded_page, videos);
                    page.loading = false;
                }
            }
            network::NetworkEvent::FavoriteResourcesLoaded {
                req_id,
                owner_mid,
                media_id,
                page: loaded_page,
                order,
                resources,
            } => {
                if !self.is_latest_request("favorite_resources", req_id) {
                    return;
                }
                if let Page::Up(page) = &mut self.current_page
                    && page.mid == owner_mid
                    && page.favorite_order == order
                    && (page.pending_folder == Some(media_id)
                        || page.active_folder == Some(media_id))
                {
                    page.apply_favorite_resources(media_id, loaded_page, resources);
                }
            }
            network::NetworkEvent::SeriesListLoaded {
                req_id,
                mid,
                page: _loaded_page,
                data,
            } => {
                if !self.is_latest_request("series_list", req_id) {
                    return;
                }
                if let Page::Up(page) = &mut self.current_page
                    && page.mid == mid
                {
                    page.apply_series_list(data);
                }
            }
            network::NetworkEvent::SeriesArchivesLoaded {
                req_id,
                mid,
                series_id,
                page: loaded_page,
                data,
            } => {
                if !self.is_latest_request("series_archives", req_id) {
                    return;
                }
                if let Page::Up(page) = &mut self.current_page
                    && page.mid == mid
                    && (page.pending_series == Some(series_id)
                        || page.active_series == Some(series_id))
                {
                    page.apply_series_archives(series_id, loaded_page, data);
                }
            }
            network::NetworkEvent::PlaylistLoaded {
                req_id,
                items,
                source,
                start_index,
                order,
            } => {
                if self.is_latest_request("playlist_build", req_id) {
                    self.pending_playlist = Some((items, source, start_index, order));
                }
            }
            network::NetworkEvent::FavoritesInitLoaded {
                req_id,
                mid,
                watch_later,
                created,
                collected,
            } => {
                if !self.is_latest_request("favorites_init", req_id) {
                    return;
                }
                if let Page::Favorites(page) = &mut self.current_page
                    && page.mid == mid
                {
                    page.apply_initial(watch_later, created, collected);
                }
            }
            network::NetworkEvent::FavoriteFoldersRefreshed {
                req_id,
                created,
                collected,
            } => {
                if !self.is_latest_request("favorites_refresh", req_id) {
                    return;
                }
                if let Page::Favorites(page) = &mut self.current_page {
                    if let Some(media_id) =
                        page.apply_folder_list_refresh(created, collected)
                    {
                        // Auto-navigate to the newly created folder
                        let req_id = self.next_request_id("favorites_content");
                        self.send_network_command(
                            network::NetworkCommand::LoadFavoritesContent {
                                req_id,
                                source: crate::api::favorite::FavoriteSource::Created {
                                    media_id,
                                    title: String::new(),
                                },
                                page: 1,
                            },
                        );
                    }
                }
            }
            network::NetworkEvent::FavoritesWatchLaterLoaded { req_id, page, data } => {
                if !self.is_latest_request("favorites_content", req_id) {
                    return;
                }
                if let Page::Favorites(favorites) = &mut self.current_page
                    && matches!(
                        favorites.active_source,
                        crate::api::favorite::FavoriteSource::WatchLater
                    )
                {
                    favorites.apply_watch_later(page, data);
                }
            }
            network::NetworkEvent::FavoritesCreatedLoaded {
                req_id,
                media_id,
                page,
                data,
            } => {
                if !self.is_latest_request("favorites_content", req_id) {
                    return;
                }
                if let Page::Favorites(favorites) = &mut self.current_page
                    && matches!(
                        favorites.active_source,
                        crate::api::favorite::FavoriteSource::Created {
                            media_id: active_id,
                            ..
                        } if active_id == media_id
                    )
                {
                    favorites.apply_created(page, data);
                }
            }
            network::NetworkEvent::FavoritesCollectedLoaded {
                req_id,
                season_id,
                page,
                data,
            } => {
                if !self.is_latest_request("favorites_content", req_id) {
                    return;
                }
                if let Page::Favorites(favorites) = &mut self.current_page
                    && matches!(
                        favorites.active_source,
                        crate::api::favorite::FavoriteSource::Collected {
                            season_id: active_id,
                            ..
                        } if active_id == season_id
                    )
                {
                    favorites.apply_collected(page, data);
                }
            }
            network::NetworkEvent::DynamicDetailLoaded {
                req_id,
                dynamic_id,
                dynamic_item,
                comments,
                has_more_comments,
                image_urls,
            } => {
                if !self.is_latest_request("dynamic_detail", req_id) {
                    return;
                }
                if let Page::DynamicDetail(page) = &mut self.current_page {
                    if page.dynamic_id != dynamic_id {
                        return;
                    }
                    page.dynamic_item = Some(dynamic_item);
                    page.comments = comments;
                    page.comment_page = 1;
                    page.has_more_comments = has_more_comments;
                    page.image_urls = image_urls;
                    page.image_protocols = (0..page.image_urls.len()).map(|_| None).collect();
                    page.loading = false;
                    page.error_message = None;
                }
            }
            network::NetworkEvent::BangumiIndexLoaded { req_id, items } => {
                if !self.is_latest_request("bangumi_index", req_id) {
                    return;
                }
                if let Page::Bangumi(page) = &mut self.current_page {
                    page.set_index_items(items);
                }
            }
            network::NetworkEvent::BangumiFollowListLoaded { req_id, items } => {
                if !self.is_latest_request("bangumi_follow_list", req_id) {
                    return;
                }
                if let Page::Bangumi(page) = &mut self.current_page {
                    page.set_follow_items(items);
                }
            }
            network::NetworkEvent::BangumiSearchLoaded { req_id, keyword, items } => {
                if !self.is_latest_request("bangumi_search", req_id) {
                    return;
                }
                if let Page::Bangumi(page) = &mut self.current_page {
                    if page.search_query() != keyword {
                        return;
                    }
                    page.set_search_items(items);
                }
            }
            network::NetworkEvent::BangumiDetailLoaded {
                req_id,
                season_id,
                season,
                followed,
            } => {
                if !self.is_latest_request("bangumi_detail", req_id) {
                    return;
                }
                if let Page::BangumiDetail(page) = &mut self.current_page {
                    if page.season_id != season_id {
                        return;
                    }
                    page.set_season(season);
                    // Override the follow state: set_season keeps user_status
                    // when login=1, but the pgc view often reports login=0 for
                    // valid sessions, so the network layer resolved the real
                    // state from the follow list and we apply it here.
                    page.apply_follow_status(followed);
                }
            }
            network::NetworkEvent::RequestFailed {
                req_id,
                target,
                error,
            } => {
                if !self.is_latest_request(target, req_id) {
                    return;
                }
                match (&mut self.current_page, target) {
                    (Page::Home(page), "home") => {
                        page.apply_recommendations_error(format!("加载推荐视频失败: {}", error))
                    }
                    (Page::Home(page), "home_more") => page.apply_load_more_error(),
                    (Page::Home(page), "hotwords") => page
                        .search_mut()
                        .set_hotword_error(format!("加载热搜失败: {}", error)),
                    (Page::Search(page), "hotwords") => {
                        page.set_hotword_error(format!("加载热搜失败: {}", error))
                    }
                    (Page::Home(page), "search") => {
                        page.search_mut().set_error(format!("搜索失败: {}", error))
                    }
                    (Page::Search(page), "search") => {
                        page.set_error(format!("搜索失败: {}", error))
                    }
                    (Page::Sections(page), "sections") => {
                        page.apply_error(format!("加载分区失败: {}", error))
                    }
                    (Page::Dynamic(page), "dynamic_init")
                    | (Page::Dynamic(page), "dynamic_refresh")
                    | (Page::Dynamic(page), "dynamic_more") => {
                        page.loading_more = false;
                        page.set_error(format!("加载动态失败: {}", error));
                    }
                    (Page::History(page), "history_init") => {
                        page.apply_load_more_error(format!("加载历史记录失败: {}", error));
                    }
                    (Page::History(page), "history_more") => {
                        page.apply_load_more_error(format!("加载更多失败: {}", error));
                    }
                    (Page::Live(page), "live_init") => {
                        page.apply_live_init_error(format!("加载直播推荐失败: {}", error));
                    }
                    (Page::Live(page), "live_more") => page.apply_live_more_error(),
                    (Page::VideoDetail(page), "video_detail") => {
                        page.error_message = Some(format!("加载视频信息失败: {}", error));
                        page.loading = false;
                    }
                    (Page::Up(page), "up_page") | (Page::Up(page), "up_videos") => {
                        page.set_error(format!("加载UP主空间失败: {error}"));
                    }
                    (Page::Up(page), "favorite_resources") => {
                        page.set_error(format!("加载收藏夹失败: {error}"));
                    }
                    (_, "playlist_build") => {
                        self.playback.status = crate::domain::playback::PlaybackStatus::Failed;
                        self.playback.last_error = Some(format!("加载播放列表失败: {error}"));
                    }
                    (Page::Favorites(page), "favorites_init")
                    | (Page::Favorites(page), "favorites_content") => {
                        page.set_error(format!("加载收藏失败: {error}"));
                    }
                    (Page::DynamicDetail(page), "dynamic_detail") => {
                        page.error_message = Some(format!("加载动态详情失败: {}", error));
                        page.loading = false;
                    }
                    (Page::ArticleDetail(page), "article_detail") => {
                        page.set_error(format!("加载专栏失败: {error}"));
                    }
                    (Page::Bangumi(page), "bangumi_timeline") => {
                        page.set_error(format!("加载番剧时间表失败: {}", error));
                    }
                    (Page::Bangumi(page), "bangumi_index") => {
                        page.set_error(format!("加载番剧索引失败: {}", error));
                    }
                    (Page::Bangumi(page), "bangumi_follow_list") => {
                        page.set_follow_error(format!("加载追番列表失败: {}", error));
                    }
                    (Page::BangumiDetail(page), "bangumi_detail") => {
                        page.set_error(format!("加载番剧详情失败: {}", error));
                    }
                    (Page::Notifications(page), "chat_sessions") => {
                        page.apply_sessions_error(format!("加载会话失败: {error}"));
                    }
                    (Page::Notifications(page), "chat_detail") => {
                        page.apply_sessions_error(format!("加载聊天失败: {error}"));
                    }
                    (Page::Notifications(page), "notifications_init") => {
                        page.apply_items_error(format!("加载消息失败: {error}"));
                    }
                    (Page::Mall(page), "mall_orders") => {
                        page.apply_orders_error(format!("加载订单失败: {error}"));
                    }
                    _ => {}
                }
            }
        }
    }
}
