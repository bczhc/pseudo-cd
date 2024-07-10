use std::io;
use std::io::stdout;
use std::process::exit;

use ratatui::backend::Backend;
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{event, ExecutableCommand};
use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, Padding, Paragraph};
use ratatui::{Frame, Terminal};

const TUI_APP_TITLE: &str = "Pseudo-CD Player";

#[derive(Clone, Debug)]
enum AppUiState {
    /// Shows a starting centered text, indicating initialization
    Starting(StartingUiData),
    Player(PlayerUiData),
}

pub struct Tui<B: Backend> {
    terminal: Terminal<B>,
    should_quit: bool,
    ui_data: UiData,
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

pub struct UiData {
    ui_state: AppUiState,
}

impl Default for UiData {
    fn default() -> Self {
        Self {
            ui_state: AppUiState::Starting(StartingUiData {
                info_text: "Checking cdrskin...".into(),
            }),
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

        match &self.ui_state {
            AppUiState::Starting(d) => {
                d.draw_to(frame, app_block_inner_rect);
            }
            AppUiState::Player(d) => {
                d.draw_to(frame, app_block_inner_rect);
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
            ui_data: UiData::new(),
            should_quit: false,
        })
    }

    pub fn tick(&mut self) -> io::Result<()> {
        self.terminal.draw(|frame| {
            self.ui_data.draw_to(frame);
        })?;
        self.handle_events()?;
        if self.should_quit {
            clean_up_and_exit();
        }

        Ok(())
    }

    pub fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    // Ctrl-C pressed
                    self.should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    self.should_quit = true;
                }
            }
        }
        Ok(())
    }
}
