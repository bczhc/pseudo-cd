use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;

#[derive(clap::Parser, Debug, Default)]
pub struct Args {
    /// Path of the disc drive (like /dev/sr0 on Linux)
    /// TODO: on platforms other than *nix?
    #[arg(default_value = "/dev/sr0")]
    pub drive: PathBuf,
    /// Number (starts from one) of the track that stores meta info of this "Pseudo-CD" authoring
    ///
    /// By default, the first track is picked.
    #[arg(default_value = "1", short, long, alias = "mit")]
    pub meta_info_track: usize,
    /// On true, assume all tracks are PCM data.
    #[arg(long, default_value = "false")]
    pub no_meta: bool,
    /// Program to fetch optical medium info
    #[arg(value_enum, long, default_value = "cdrskin")]
    pub minfo_program: MinfoCli,
    /// Program log will output to this if present
    #[arg(short, long)]
    pub log_file: Option<PathBuf>,
}

#[derive(clap::ValueEnum, Debug, Eq, PartialEq, Copy, Clone)]
pub enum MinfoCli {
    Cdrskin,
    Cdrecord,
    Wodim,
}

impl Default for MinfoCli {
    fn default() -> Self {
        Self::Cdrskin
    }
}

impl MinfoCli {
    pub fn name(&self) -> &'static str {
        match self {
            MinfoCli::Cdrskin => "cdrskin",
            MinfoCli::Cdrecord => "cdrecord",
            MinfoCli::Wodim => "wodim",
        }
    }
}

pub static ARGS: Lazy<Mutex<Args>> = Lazy::new(|| {
    Mutex::new(
        Default::default(), /* this is just a placeholder dummy value */
    )
});
