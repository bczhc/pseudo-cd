#![feature(try_blocks)]
#![feature(decl_macro)]
#![feature(yeet_expr)]
#![feature(let_chains)]

extern crate core;

use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cli::ARGS;

pub mod cli;
pub mod playback;
pub mod tui;
pub mod minfo;

/// The sector size optical discs use is 2048 bytes.
const SECTOR_SIZE: u64 = 2048;

macro lazy_regex($name:tt ,$regex:expr) {
    static $name: Lazy<Regex> = Lazy::new(|| Regex::new($regex).unwrap());
}

pub macro mutex_lock($m:expr) {
    $m.lock().unwrap()
}

/// [start_addr], [end_addr] and [size] are in sectors (see [SECTOR_SIZE])
#[derive(Debug, Clone, Copy)]
pub struct Track {
    pub track_no: u32,
    pub session_no: u32,
    pub start_addr: u64,
    pub end_addr: u64,
    pub size: u64,
}

impl Track {
    /// Starting offset in bytes
    pub fn start_offset(&self) -> u64 {
        self.start_addr * SECTOR_SIZE
    }

    /// Ending offset in bytes
    pub fn end_offset(&self) -> u64 {
        self.end_addr * SECTOR_SIZE
    }

    /// Track length in bytes
    pub fn size_bytes(&self) -> u64 {
        self.size * SECTOR_SIZE
    }
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

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SongInfo {
    name: String,
    /// Session numbers start from one
    session_no: usize,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetaInfo {
    title: Option<String>,
    creation_time: Option<u64>,
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

pub fn set_up_logging<P: AsRef<Path>>(file_path: P) -> anyhow::Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339(std::time::SystemTime::now()),
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(fern::log_file(file_path)?)
        .apply()?;
    Ok(())
}
