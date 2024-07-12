#![feature(yeet_expr)]

use clap::Parser;
use log::info;
use std::io::stdout;
use std::panic;
use std::panic::take_hook;

use std::thread::spawn;

use pseudo_cd_player::cli::{Args, ARGS};
use pseudo_cd_player::{mutex_lock, set_up_logging};
use ratatui::prelude::*;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use pseudo_cd_player::tui::{clean_up_and_exit, clean_up_tui, Tui};

fn register_signal_hooks() {
    let mut signals = Signals::new([SIGINT, SIGTERM]).unwrap();
    #[allow(clippy::never_loop)]
    for _signal in &mut signals {
        clean_up_and_exit();
    }
}

fn run_tui() -> anyhow::Result<()> {
    let backend = CrosstermBackend::new(stdout());
    let mut tui = Tui::new(backend)?;
    loop {
        tui.tick()?;
    }
}

fn set_up_panic_hook() {
    let default_hook = take_hook();
    panic::set_hook(Box::new(move |x| {
        let _ = clean_up_tui();
        default_hook(x);
    }));
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(ref f) = args.log_file {
        set_up_logging(f)?;
    }

    info!("Args: {:?}", args);
    *mutex_lock!(ARGS) = args;

    set_up_panic_hook();
    spawn(register_signal_hooks);
    run_tui()?;
    Ok(())
}
