use std::io::stdout;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Duration;
use std::io;

use anyhow::anyhow;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{event, ExecutableCommand};
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::prelude::{Color, Layout, Modifier, Style};
use ratatui::widgets::{Block, Borders, LineGauge, List, ListItem, Padding, Paragraph};
use ratatui::{Frame, symbols, Terminal};
use yeet_ops::yeet;

use crate::cli::ARGS;
use crate::playback::{start_global_playback_thread, PlayerCommand, PlayerResult, AUDIO_STREAM, set_global_playback_handle, PLAYBACK_HANDLE, PlayerCallbackEvent};
use crate::{cdrskin_medium_track_info, check_cdrskin_version, extract_meta_info, mutex_lock, Track, SECTOR_SIZE, SongInfo, MetaInfo};

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
                .block(Block::bordered().padding(padding))
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

impl PlayerState {
    fn from_paused(paused: bool) -> Self {
        match paused {
            true => Self::Paused,
            false => Self::Playing
        }
    }
}

#[derive(Clone, Debug)]
struct PlayerUiData {
    player_state: PlayerState,
    selected_song_idx: usize,
    playing_song_idx: usize,
    meta_info: Arc<MetaInfo>,
    current_position: u32,
    total_duration: u32,
}

impl PlayerUiData {
    fn song_name_by_song_idx(&self, idx: usize) -> &str {
        &self.meta_info.list[idx].name
    }

    fn draw_to(&self, frame: &mut Frame, rect: Rect) {
        let layout = Layout::vertical([Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)]).split(rect);

        let list_items = self.meta_info.list.iter().enumerate().map(|(i, x)| {
            let item_text = format!("{}: {}", i + 1, x.name);
            let mut item = ListItem::new(item_text);
            // TODO: not consider terminal themes like white-background-black-text?
            if self.selected_song_idx == i {
                let style = Style {
                    bg: Some(Color::Gray),
                    fg: Some(Color::White),
                    ..Default::default()
                };
                item = item.style(style);
            }
            if self.playing_song_idx == i {
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
            PlayerState::Paused => "Paused: ",
        };
        let bottom_title = format!("{state_str}{}", self.song_name_by_song_idx(self.playing_song_idx));

        frame.render_widget(
            Block::new()
                .title(bottom_title.as_str())
                .title_alignment(Alignment::Center),
            layout[1],
        );

        fn non_nan(ratio: f64) -> f64 {
            if ratio.is_nan() {
                return 0.0;
            }
            ratio
        }

        frame.render_widget(LineGauge::default()
                                .filled_style(Style::default().fg(Color::Blue))
                                .unfilled_style(Style::default().fg(Color::Gray))
                                .label(duration_string((self.current_position, self.total_duration)))
                                .ratio(non_nan(self.current_position as f64 / self.total_duration as f64)), layout[2]);
    }
}

fn duration_string((position, total): (u32, u32)) -> String {
    let pad_zero = |num: u32| {
        if num < 10 {
            format!("0{num}")
        } else {
            format!("{num}")
        }
    };
    let make_string = |num: u32| {
        format!("{}:{}", pad_zero(num / 60), pad_zero(num % 60))
    };
    format!("{}/{}", make_string(position), make_string(total))
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
    /// tracks info (precisely for DVDs it's "sessions") from `cdrskin -minfo`
    disc_tracks: Arc<Vec<Track>>,
    meta_info: Arc<MetaInfo>,
}

impl Default for UiData {
    fn default() -> Self {
        Self {
            ui_state: AppUiState::Starting,
            starting_ui_data: StartingUiData {
                info_text: "Initializing...".into(),
            },
            player_ui_data: PlayerUiData {
                playing_song_idx: 0,
                selected_song_idx: 0,
                player_state: PlayerState::Playing,
                meta_info: Default::default(),
                current_position: 0,
                total_duration: 0,
            },
            any_key_to_exit: false,
            disc_tracks: Default::default(),
            error_ui_data: ErrorUiData {
                title: "",
                content: "".into(),
            },
            meta_info: Arc::new(Default::default()),
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

        frame.render_widget(
            Block::bordered()
                .title(TUI_APP_TITLE)
                .title_alignment(Alignment::Center),
            frame_rect,
        );
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
    drop(mutex_lock!(AUDIO_STREAM).take());
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
        let tracks = cdrskin_medium_track_info()?;
        let tracks = Arc::new(tracks);
        mutex_lock!(ui_data).disc_tracks = Arc::clone(&tracks);

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
        let meta_info = Arc::new(extract_meta_info(*meta_info_track)?);
        mutex_lock!(ui_data).meta_info = Arc::clone(&meta_info);
        mutex_lock!(ui_data).player_ui_data.meta_info = Arc::clone(&meta_info);

        starting_info_text!("Initializing audio sink...");
        let ui_data_for_player_callback = Arc::clone(ui_data);
        let playback_handle = start_global_playback_thread(
            mutex_lock!(ARGS).drive.clone(),
            ui_data_for_player_callback,
            Some(|event, ui_data: &Arc<Mutex<UiData>>| {
                match event {
                    PlayerCallbackEvent::Finished => {
                        // TODO
                    }
                    PlayerCallbackEvent::Paused(paused) => {
                        mutex_lock!(ui_data).player_ui_data.player_state = PlayerState::from_paused(paused);
                    }
                    PlayerCallbackEvent::Progress(current, total) => {
                        let mut guard = mutex_lock!(ui_data);
                        guard.player_ui_data.current_position = current;
                        guard.player_ui_data.total_duration = total;
                    }
                }
            })
        )?;
        set_global_playback_handle(playback_handle);

        starting_info_text!("Done.");
        sleep(Duration::from_secs_f64(0.1));

        mutex_lock!(ui_data).ui_state = AppUiState::Player;

        // play the first track initially
        if let Some(first_song) = meta_info.list.first() {
            mutex_lock!(PLAYBACK_HANDLE).as_ref().unwrap().send_commands([
                PlayerCommand::Start,
                PlayerCommand::Goto(tracks[first_song.session_no - 1], true)
            ]);
        }

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

    /// ## Key bindings
    ///
    /// Space: Play/Pause
    /// n: Next
    /// p: Previous
    /// j, ArrowDown: Selection move up
    /// k, ArrowUp: Selection move down
    /// Enter: Play the selection
    pub fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                let mut ui_data_guard = mutex_lock!(self.ui_data);
                let song_number = ui_data_guard.meta_info.list.len();

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

                let wrapping_next = |song_idx: usize| {
                    if song_idx == song_number - 1 {
                        0
                    } else {
                        song_idx + 1
                    }
                };
                let wrapping_prev = |song_idx: usize| {
                    if song_idx == 0 {
                        song_number - 1
                    } else {
                        song_idx - 1
                    }
                };
                macro player_send($cmd:expr) {
                    mutex_lock!(PLAYBACK_HANDLE).as_ref().unwrap().send($cmd);
                }
                macro index_inc($tt:tt) {{
                    let idx = &mut ui_data_guard.player_ui_data.$tt;
                    *idx = wrapping_next(*idx);
                }}
                macro index_dec($tt:tt) {{
                    let idx = &mut ui_data_guard.player_ui_data.$tt;
                    *idx = wrapping_prev(*idx);
                }}
                macro player_goto_playing_one() {
                    let playing_song_idx = ui_data_guard.player_ui_data.playing_song_idx;
                    let song_track = ui_data_guard.disc_tracks[ui_data_guard.meta_info.list[playing_song_idx].session_no - 1];
                    player_send!(PlayerCommand::Goto(song_track, true))
                }

                if ui_data_guard.ui_state == AppUiState::Player {
                    match key.code {
                        KeyCode::Char('n') => {
                            // next
                            index_inc!(playing_song_idx);
                            player_goto_playing_one!();
                        }
                        KeyCode::Char('p') => {
                            // previous
                            index_dec!(playing_song_idx);
                            player_goto_playing_one!();
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            // move down
                            index_inc!(selected_song_idx);
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            // move up
                            index_dec!(selected_song_idx);
                        }
                        KeyCode::Enter => {
                            ui_data_guard.player_ui_data.playing_song_idx = ui_data_guard.player_ui_data.selected_song_idx;
                            player_goto_playing_one!();
                        }
                        KeyCode::Char(' ') => {
                            let PlayerResult::IsPaused(paused) = mutex_lock!(PLAYBACK_HANDLE).as_ref().unwrap().send_recv(PlayerCommand::GetIsPaused) else {
                                panic!("Unexpected player result")
                            };
                            let toggle = !paused;
                            player_send!(PlayerCommand::SetPaused(toggle));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
