#![feature(try_blocks)]
#![feature(decl_macro)]
#![feature(yeet_expr)]
#![feature(byte_slice_trim_ascii)]
#![feature(let_chains)]

extern crate core;

use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Command, ExitStatus, Stdio};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cli::ARGS;

pub mod cli;
pub mod playback;
pub mod tui;

/// The sector size optical discs use is 2048 bytes.
const SECTOR_SIZE: u64 = 2048;

macro lazy_regex($name:tt ,$regex:expr) {
    static $name: Lazy<Regex> = Lazy::new(|| Regex::new($regex).unwrap());
}

pub macro mutex_lock($m:expr) {
    $m.lock().unwrap()
}

lazy_regex!(CDRSKIN_VERSION_REGEX, r"cdrskin version +: +(\d.*)");
lazy_regex!(
    CDRSKIN_TRACKS_HEADER_REGEX,
    r"Track +Sess +Type +Start Addr +End Addr +Size"
);
lazy_regex!(
    CDRSKIN_TRACK_CAPTURING_REGEX,
    r"^ *(\d+) +(\d+) +Data +(\d+) +(\d+) +(\d+) *$"
);

/// [start_addr], [end_addr] and [size] are in sectors (see [SECTOR_SIZE])
#[derive(Debug, Clone, Copy)]
pub struct Track {
    pub track_no: u32,
    pub session_no: u32,
    pub start_addr: u64,
    pub end_addr: u64,
    pub size: u64,
}

#[derive(Debug)]
struct ProgramError {
    stdout: String,
    stderr: String,
    exit_status: ExitStatus,
}

impl ProgramError {
    fn new(exit_status: ExitStatus, stderr: String, stdout: String) -> ProgramError {
        Self {
            exit_status,
            stderr,
            stdout,
        }
    }
}

impl Display for ProgramError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.exit_status.success() {
            writeln!(f, "Non-zero exit status: {:?}", self.exit_status.code())?;
        }
        writeln!(f)?;
        writeln!(f, "Stderr:")?;
        writeln!(f, "{}\n\n", self.stderr)?;
        writeln!(f, "Stdout:")?;
        writeln!(f, "{}\n", self.stdout)?;
        Ok(())
    }
}

impl std::error::Error for ProgramError {}

fn execute_command_with_output(cmd: &[&str]) -> io::Result<String> {
    assert!(!cmd.is_empty());
    let output = Command::new(cmd[0])
        .args(cmd.iter().skip(1))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    if !output.status.success() {
        return Err(io::Error::other(ProgramError::new(
            output.status,
            format!("{}", String::from_utf8_lossy(&output.stderr)),
            format!("{}", String::from_utf8_lossy(&output.stdout)),
        )));
    }
    Ok(String::from_utf8(output.stdout).expect("Invalid UTF-8 met"))
}

fn cdrskin_medium_info_string() -> io::Result<String> {
    execute_command_with_output(&[
        "cdrskin",
        &format!("dev={}", mutex_lock!(ARGS).drive.display()),
        "-minfo",
    ])
}

pub fn cdrskin_medium_track_info() -> io::Result<Vec<Track>> {
    let output = cdrskin_medium_info_string()?;
    let filtered = output
        .lines()
        .skip_while(|&x| !CDRSKIN_TRACKS_HEADER_REGEX.is_match(x))
        .skip(2)
        .take_while(|&x| !x.is_empty())
        .collect::<Vec<_>>();
    let mut tracks = Vec::new();
    for x in filtered {
        let _: Option<_> = try {
            let captures = CDRSKIN_TRACK_CAPTURING_REGEX.captures_iter(x).next()?;
            let track = Track {
                track_no: captures.get(1)?.as_str().parse().unwrap(), /* the RegExp asserts it's a `\d` */
                session_no: captures.get(2)?.as_str().parse().unwrap(),
                start_addr: captures.get(3)?.as_str().parse().unwrap(),
                end_addr: captures.get(4)?.as_str().parse().unwrap(),
                size: captures.get(5)?.as_str().parse().unwrap(),
            };
            tracks.push(track);
        };
    }
    Ok(tracks)
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SongInfo {
    name: String,
    /// Session numbers start from one
    session_no: usize,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetaInfo {
    title: String,
    creation_time: u64,
    list: Vec<SongInfo>,
}

/// Extracts the meta info from [track]
///
/// The meta info is a JSON.
/// Just read out all the text until a NUL ('\0').
pub fn extract_meta_info(track: Track) -> io::Result<MetaInfo> {
    let mut disc_file = File::open(&mutex_lock!(ARGS).drive)?;
    disc_file.seek(SeekFrom::Start(track.start_addr * SECTOR_SIZE))?;
    let bytes = disc_file
        .bytes()
        .take_while(|x| x.is_ok() && *x.as_ref().unwrap() != b'\0')
        .collect::<io::Result<Vec<_>>>()?;
    let bytes = bytes.trim_ascii_end();
    serde_json::from_slice(bytes).map_err(io::Error::other)
}

pub fn check_cdrskin_version() -> io::Result<Option<String>> {
    let output = execute_command_with_output(&["cdrskin", "--version"])?;
    let version: Option<&str> = try {
        CDRSKIN_VERSION_REGEX
            .captures_iter(&output)
            .next()?
            .get(1)?
            .as_str()
    };
    Ok(version.map(Into::into))
}
