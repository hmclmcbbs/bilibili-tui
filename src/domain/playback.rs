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

#[cfg(test)]
mod tests {
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
            bvid: None
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
}
