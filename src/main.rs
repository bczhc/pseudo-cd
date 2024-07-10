#![feature(yeet_expr)]

use std::io::{self, stdout};
use std::process::exit;
use std::thread::spawn;

use ratatui::{
    crossterm::{
        event::{self, Event, KeyCode},
        ExecutableCommand,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    prelude::*,
    widgets::*,
};
use ratatui::crossterm::event::KeyModifiers;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

const TUI_APP_TITLE: &str = "Pseudo-CD Player";

fn register_signal_hooks() {
    let mut signals = Signals::new([SIGINT, SIGTERM]).unwrap();
    #[allow(clippy::never_loop)]
    for _signal in &mut signals {
        clean_up_and_exit();
    }
}

fn stop_tui() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn clean_up_and_exit() {
    let _ = stop_tui();
    exit(0);
}

fn start_tui() -> anyhow::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut should_quit = false;
    while !should_quit {
        terminal.draw(ui)?;
        should_quit = handle_events()?;
    }
    stop_tui()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    spawn(register_signal_hooks);
    start_tui()?;

    // let args = Args::parse();
    // *mutex_lock!(ARGS) = args;
    //
    // let cdrskin_version = check_cdrskin_version();
    // if cdrskin_version.is_err() || cdrskin_version.unwrap().is_none() {
    //     yeet!(anyhow!("cdrskin is needed"));
    // }

    // let tracks = cdrskin_medium_track_info()?;
    // let meta_info_track =
    //     &tracks[mutex_lock!(ARGS).meta_info_track - 1 /* track numbers start from 1 */];
    // println!("{:?}", extract_meta_info(meta_info_track));

    Ok(())
}

/// Returns `true` if the TUI app is time to quit
fn handle_events() -> io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                // Ctrl-C pressed
                clean_up_and_exit();
            }
            if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn ui(frame: &mut Frame) {
    let frame_rect = frame.size();
    let app_block_inner_rect = Rect::new(1, 1, frame_rect.width - 2, frame_rect.height - 2);
    app_block_ui(frame, app_block_inner_rect);
}

/// Renders content inside the app's outside-most border
fn app_block_ui(frame: &mut Frame, rect: Rect) {
    let padding = Padding::new(
        0,
        0,
        (rect.height - 1/* the center text takes up one line */) / 2,
        0,
    );

    frame.render_widget(
        Paragraph::new("Test")
            .block(
                Block::bordered()
                    .title(TUI_APP_TITLE)
                    .title_alignment(Alignment::Center)
                    .padding(padding),
            )
            .alignment(Alignment::Center),
        frame.size(),
    );
}
