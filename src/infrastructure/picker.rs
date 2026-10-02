//! Process-wide terminal graphics protocol detection.
//!
//! `Picker::from_query_stdio()` writes capability queries to the terminal and
//! blocks on stdin (up to a 2s timeout) to read the responses. Doing that in
//! every page constructor meant each navigation paid a terminal round-trip,
//! and — worse — the query raced with the main event loop for stdin, so
//! keypresses typed during a page switch could be swallowed as query
//! responses. Query once at first use and share the result everywhere.

use ratatui_image::picker::Picker;
use std::sync::{Arc, OnceLock};

static PICKER: OnceLock<Arc<Picker>> = OnceLock::new();

/// Returns the shared [`Picker`], detecting the terminal's graphics protocol
/// (Kitty/Sixel/iTerm2) only on the first call and falling back to halfblocks
/// when detection fails or times out.
pub fn shared() -> Arc<Picker> {
    PICKER
        .get_or_init(|| {
            Arc::new(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()))
        })
        .clone()
}
