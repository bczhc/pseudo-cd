#![feature(yeet_expr)]

use anyhow::anyhow;
use clap::Parser;
use yeet_ops::yeet;

use pseudo_cd::{cdrskin_medium_track_info, check_cdrskin_version, extract_meta_info, mutex_lock};
use pseudo_cd::cli::{ARGS, Args};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    *mutex_lock!(ARGS) = args;

    let cdrskin_version = check_cdrskin_version();
    if cdrskin_version.is_err() || cdrskin_version.unwrap().is_none() {
        yeet!(anyhow!("cdrskin is needed"));
    }

    let tracks = cdrskin_medium_track_info()?;
    let meta_info_track =
        &tracks[mutex_lock!(ARGS).meta_info_track - 1 /* track numbers start from 1 */];
    println!("{:?}", extract_meta_info(meta_info_track));

    Ok(())
}
