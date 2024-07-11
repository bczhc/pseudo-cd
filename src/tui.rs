/// ## Key bindings
///
/// Space: Play/Pause
/// n: Next
/// p: Previous
/// j: Selection move up
/// k: Selection move down
/// Enter: Play the selection

use std::io;
use std::io::stdout;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Duration;

use anyhow::anyhow;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{event, ExecutableCommand};
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph};
use ratatui::{Frame, Terminal};
use ratatui::prelude::{Color, Layout, Style, Stylize};
use yeet_ops::yeet;

use crate::cli::ARGS;
use crate::{cdrskin_medium_track_info, check_cdrskin_version, extract_meta_info, mutex_lock};

const TUI_APP_TITLE: &str = "Pseudo-CD Player";

#[derive(Clone, Debug, Eq, PartialEq)]
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
                    Block::bordered().padding(padding),
                )
                .alignment(Alignment::Center),
            frame.size(),
        );
    }
}

#[derive(Clone, Debug)]
enum PlayerState {
    Playing,
    Paused,
}

#[derive(Clone, Debug)]
struct PlayerUiData {
    song_list: Vec<String>,
    player_state: PlayerState,
    song_name: String,
    highlighted_track: usize,
}

impl PlayerUiData {
    fn draw_to(&self, frame: &mut Frame, rect: Rect) {
        let layout = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
            .split(rect);



        let list_items = self.song_list.iter().enumerate().map(|(i, x)| {
            let item_text = format!("{}: {}", i + 1, x);
            let mut item = ListItem::new(item_text);
            if self.highlighted_track - 1 == i {
                // TODO: not consider terminal themes like white-background-black-text?
                let style = Style {
                    bg: Some(Color::White),
                    fg: Some(Color::Black),
                    ..Default::default()
                };
                item = item.style(style);
            }
            item
        });
        let list = List::new(list_items);
        frame.render_widget(list, layout[0]);

        let state_str = match self.player_state {
            PlayerState::Playing => "Playing: ",
            PlayerState::Paused => "Paused: "
        };
        let bottom_title = format!("{state_str}{}", self.song_name);

        frame.render_widget(
            Block::new().borders(Borders::BOTTOM).title(bottom_title.as_str()).title_alignment(Alignment::Center),
            layout[1],
        )
    }
}

#[derive(Clone, Debug)]
struct ErrorUiData {
    title: &'static str,
    content: String,
}

impl ErrorUiData {
    fn draw_to(&self, frame: &mut Frame, rect: Rect) {
        frame.render_widget(
            Paragraph::new(self.title).alignment(Alignment::Center),
            rect,
        );
        frame.render_widget(
            Paragraph::new(self.content.as_str()),
            Rect::new(rect.x, rect.y + 1, rect.width, rect.height - 1),
        )
    }
}

pub struct UiData {
    ui_state: AppUiState,
    starting_ui_data: StartingUiData,
    player_ui_data: PlayerUiData,
    error_ui_data: ErrorUiData,
    any_key_to_exit: bool,
}

impl Default for UiData {
    fn default() -> Self {
        Self {
            ui_state: AppUiState::Starting,
            starting_ui_data: StartingUiData {
                info_text: "Initializing...".into(),
            },
            player_ui_data: PlayerUiData {
                song_list: Default::default(),
                highlighted_track: 1,
                song_name: Default::default(),
                player_state: PlayerState::Playing,
            },
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

        frame.render_widget(Block::bordered().title(TUI_APP_TITLE).title_alignment(Alignment::Center), frame_rect);
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

    fn background_thread(ui_data: &Arc<Mutex<UiData>>) -> anyhow::Result<()> {
        macro starting_info_text($($arg:tt)*) {
            mutex_lock!(ui_data).starting_ui_data.info_text = format!($($arg)*)
        }

        starting_info_text!("Checking cdrskin...");

        let version = check_cdrskin_version();
        let version = match version {
            Err(_) | Ok(None) => {
                yeet!(anyhow!("Command `cdrskin` not found"))
            }
            Ok(Some(version)) => version,
        };

        starting_info_text!("cdrskin version: {version}; Fetching tracks info...");
        let tracks = match cdrskin_medium_track_info() {
            Ok(t) => t,
            Err(e) => {
                // TODO: errors from `cdrskin`
                yeet!(anyhow!("{}", e))
            }
        };

        starting_info_text!("Tracks fetched. Extracting meta info...");

        let meta_info_track = tracks
            .get(
                mutex_lock!(ARGS).meta_info_track - 1, /* track number starts from one */
            )
            .ok_or_else(|| {
                anyhow!(
                    "Meta info track is out-of-index; Number of tracks: {}",
                    tracks.len()
                )
            })?;
        let meta_info = extract_meta_info(meta_info_track)?;

        starting_info_text!("Done.");
        sleep(Duration::from_secs_f64(0.1));

        mutex_lock!(ui_data).ui_state = AppUiState::Player;
        let coerced_song_list = meta_info.list.into_iter().take(tracks.len() - 1).collect::<Vec<_>>();
        mutex_lock!(ui_data).player_ui_data.song_list = coerced_song_list;

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
                    guard.error_ui_data.title = "Error occurred. Press any key to exit.";
                    guard.error_ui_data.content = format!("{:?}", e);
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
                let mut ui_data_guard = mutex_lock!(self.ui_data);
                let track_length = ui_data_guard.player_ui_data.song_list.len();

                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    // Ctrl-C pressed
                    self.should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    self.should_quit = true;
                }
                if ui_data_guard.any_key_to_exit {
                    self.should_quit = true;
                }

                if ui_data_guard.ui_state == AppUiState::Player {
                    match key.code {
                        KeyCode::Char('n') => {
                            // next

                        }
                        KeyCode::Char('p') => {
                            // previous
                        }
                        KeyCode::Char('j') => {
                            // move down
                            let track_no = &mut ui_data_guard.player_ui_data.highlighted_track;
                            *track_no += 1;
                            if *track_no > track_length {
                                *track_no = 1;
                            }
                        }
                        KeyCode::Char('k') => {
                            let track_no = &mut ui_data_guard.player_ui_data.highlighted_track;
                            if *track_no - 1 == 0 {
                                *track_no = track_length;
                            } else {
                                *track_no -= 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
