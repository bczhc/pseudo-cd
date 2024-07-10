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
}

pub static ARGS: Lazy<Mutex<Args>> = Lazy::new(|| {
    Mutex::new(
        Default::default(), /* this is just a placeholder dummy value */
    )
});
