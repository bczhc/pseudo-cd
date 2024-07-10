#![feature(yeet_expr)]

use anyhow::anyhow;
use yeet_ops::yeet;

use pseudo_cd::{cdrskin_medium_track_info, check_cdrskin_version};

fn main() -> anyhow::Result<()> {
    let cdrskin_version = check_cdrskin_version();
    if cdrskin_version.is_err() || cdrskin_version.unwrap().is_none() {
        yeet!(anyhow!("cdrskin is needed"));
    }

    let tracks = cdrskin_medium_track_info()?;
    for x in tracks {
        println!("{:?}", x);
    }

    Ok(())
}
