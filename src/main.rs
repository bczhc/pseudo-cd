#![feature(yeet_expr)]

use std::io::{stdin, stdout};
use std::thread::spawn;
use clap::Parser;

use ratatui::prelude::*;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use pseudo_cd::cli::{Args, ARGS};
use pseudo_cd::mutex_lock;

use pseudo_cd::tui::{clean_up_and_exit, Tui};

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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    *mutex_lock!(ARGS) = args;

    spawn(register_signal_hooks);
    run_tui()?;
    Ok(())
}
