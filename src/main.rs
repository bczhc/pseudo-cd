#![feature(yeet_expr)]

use std::io::stdout;
use std::thread::spawn;

use ratatui::prelude::*;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

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
    spawn(register_signal_hooks);
    run_tui()?;

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
