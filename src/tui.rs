use std::io;
use std::io::stdout;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread::spawn;
use std::time::Duration;

use ratatui::{Frame, Terminal};
use ratatui::backend::Backend;
use ratatui::crossterm::{event, ExecutableCommand};
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, Padding, Paragraph};
use yeet_ops::yeet;

use crate::{cdrskin_medium_track_info, check_cdrskin_version, mutex_lock};

const TUI_APP_TITLE: &str = "Pseudo-CD Player";

#[derive(Clone, Debug)]
enum AppUiState {
    /// Shows a starting centered text, indicating initialization
    Starting,
    Player,
    Error,
}

pub struct Tui<B: Backend> {
    terminal: Terminal<B>,
    should_quit: bool,
    ui_data: Arc<Mutex<UiData>>,
    bg_thread_started: bool,
}

#[derive(Clone, Debug)]
struct StartingUiData {
    info_text: String,
}

impl StartingUiData {
    fn draw_to(&self, frame: &mut Frame, rect: Rect) {
        let padding = Padding::new(
            0,
            0,
            (rect.height - 1/* the center text takes up one line */) / 2,
            0,
        );

        frame.render_widget(
            Paragraph::new(&*self.info_text)
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
}

#[derive(Clone, Debug)]
struct PlayerUiData {}

impl PlayerUiData {
    fn draw_to(&self, frame: &mut Frame, rect: Rect) {}
}

#[derive(Clone, Debug)]
struct ErrorUiData {
    title: &'static str,
    content: String,
}

impl ErrorUiData {
    fn draw_to(&self, frame: &mut Frame, rect: Rect) {
        frame.render_widget(Paragraph::new(self.title).alignment(Alignment::Center), rect);
        frame.render_widget(Paragraph::new(self.content.as_str()), Rect::new(
            rect.x, rect.y + 1, rect.width, rect.height - 1,
        ))
    }
}

pub struct UiData {
    ui_state: AppUiState,
    starting_ui_data: StartingUiData,
    player_ui_data: PlayerUiData,
    any_key_to_exit: bool,
    error_ui_data: ErrorUiData,
}

impl Default for UiData {
    fn default() -> Self {
        Self {
            ui_state: AppUiState::Starting,
            starting_ui_data: StartingUiData {
                info_text: "Initializing...".into(),
            },
            player_ui_data: PlayerUiData {},
            any_key_to_exit: false,
            error_ui_data: ErrorUiData {
                title: "",
                content: "".into(),
            },
        }
    }
}

impl UiData {
    pub fn new() -> Self {
        Default::default()
    }
}

impl UiData {
    pub fn draw_to(&self, frame: &mut Frame) {
        let frame_rect = frame.size();
        let app_block_inner_rect = Rect::new(1, 1, frame_rect.width - 2, frame_rect.height - 2);

        match self.ui_state {
            AppUiState::Starting => {
                self.starting_ui_data.draw_to(frame, app_block_inner_rect);
            }
            AppUiState::Player => {
                self.player_ui_data.draw_to(frame, app_block_inner_rect);
            }
            AppUiState::Error => {
                self.error_ui_data.draw_to(frame, app_block_inner_rect);
            }
        }
    }
}

pub fn set_up_tui() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    Ok(())
}

pub fn clean_up_tui() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

pub fn clean_up_and_exit() {
    let _ = clean_up_tui();
    exit(0);
}

impl<B: Backend> Tui<B> {
    pub fn new(backend: B) -> io::Result<Self> {
        set_up_tui()?;
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            ui_data: Arc::new(Mutex::new(UiData::new())),
            should_quit: false,
            bg_thread_started: false,
        })
    }

    fn background_thread(ui_data: &Arc<Mutex<UiData>>) -> Result<(), String> {
        mutex_lock!(ui_data).starting_ui_data.info_text = "Checking cdrskin...".into();

        let version = check_cdrskin_version();
        let version = match version {
            Err(_) | Ok(None) => {
                mutex_lock!(ui_data).error_ui_data.title = "cdrskin not found! Press any key to exit";
                yeet!(String::default())
            }
            Ok(Some(version)) => {
                version
            }
        };

        mutex_lock!(ui_data).starting_ui_data.info_text = format!("cdrskin version: {version}; Fetching tracks info...");
        let tracks = match cdrskin_medium_track_info() {
            Ok(t) => {
                t
            }
            Err(e) => {
                mutex_lock!(ui_data).error_ui_data.title = "Error occurred. Press any key to exit";
                yeet!(format!("{}", e))
            }
        };

        mutex_lock!(ui_data).starting_ui_data.info_text = format!("Tracks number: {}", tracks.len());
        Ok(())
    }

    pub fn tick(&mut self) -> io::Result<()> {
        if !self.bg_thread_started {
            self.bg_thread_started = true;
            let arc = Arc::clone(&self.ui_data);
            spawn(move || {
                let result = Self::background_thread(&arc);
                if let Err(e) = result {
                    let mut guard = mutex_lock!(arc);
                    guard.any_key_to_exit = true;
                    guard.ui_state = AppUiState::Error;
                    guard.error_ui_data.content =  e;
                }
            });
        }

        self.terminal.draw(|frame| {
            mutex_lock!(self.ui_data).draw_to(frame);
        })?;
        self.handle_events()?;
        if self.should_quit {
            clean_up_and_exit();
        }

        Ok(())
    }

    pub fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    // Ctrl-C pressed
                    self.should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    self.should_quit = true;
                }
                if mutex_lock!(self.ui_data).any_key_to_exit {
                    self.should_quit = true;
                }
            }
        }
        Ok(())
    }
}
