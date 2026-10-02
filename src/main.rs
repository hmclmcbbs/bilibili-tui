use bilibili_tui::app::App;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
};
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize terminal
    let mut terminal = ratatui::init();
    // clear() reads the cursor position, which some environments (tmux under
    // CI, dumb terminals, script(1)) never answer. The query times out after
    // ~2s and — previously — aborted the whole app before it drew anything.
    // The alternate screen starts out empty anyway, so a failed clear is
    // harmless: ignore it instead of bailing out.
    let _ = terminal.clear();

    // Enable mouse capture
    execute!(std::io::stdout(), EnableMouseCapture)?;

    // Run the application
    let app = App::new();
    let result = app.run(&mut terminal).await;

    // Disable mouse capture before restoring
    let _ = execute!(std::io::stdout(), DisableMouseCapture);

    // Restore terminal
    ratatui::restore();

    result
}
