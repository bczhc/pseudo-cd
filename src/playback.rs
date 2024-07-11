use crate::mutex_lock;
use byteorder::{ReadBytesExt, LE};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SampleRate, Stream};
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{sleep, spawn};
use std::time::Duration;

/// We place [`Stream`] here just to prevent it from dropping
pub static AUDIO_STREAM: Lazy<Mutex<Option<StreamSendWrapper>>> = Lazy::new(|| Mutex::new(None));

pub fn create_audio_stream() -> anyhow::Result<(Stream, SyncSender<i16>)> {
    let (tx, rx) = sync_channel(44100);

    // TODO: need more strict output config checks
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let output_configs = device.supported_output_configs()?;
    let output_configs = output_configs
        .filter(|x| x.channels() == 2 && x.sample_format() == SampleFormat::I16)
        .collect::<Vec<_>>();
    let output_config = output_configs[0].with_sample_rate(SampleRate(44100));

    // Why here there's no multiple-move encountering?? this `play_fn` should be called
    // multiple times, and `rx` will be "moved" many times?
    let play_fn = move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
        for x in data.iter_mut() {
            *x = rx.try_recv().unwrap_or(i16::EQUILIBRIUM);
        }
    };
    let stream = device.build_output_stream(
        &output_config.config(),
        play_fn,
        move |err| {
            println!("{}", err);
        },
        None, /* blocking */
    )?;
    stream.play()?;
    Ok((stream, tx))
}

pub enum PlayerCommand {
    /// Go to a position with [offset] in bytes
    Goto(u64),
    /// Seek to a position with duration in seconds
    Seek(f64),
    /// Open the file and start playing
    Start,
    Pause,
    Play,
    /// Volume level is in 0..1
    ChangeVolume(f64),
    /// Get the current position in seconds
    GetPosition(RefCell<f64>),
}

pub struct StreamSendWrapper(Stream);

impl From<Stream> for StreamSendWrapper {
    fn from(value: Stream) -> Self {
        Self(value)
    }
}

unsafe impl Send for StreamSendWrapper {}

pub fn start_global_playback_thread(drive: PathBuf) -> anyhow::Result<SyncSender<PlayerCommand>> {
    let (tx, rx) = sync_channel::<PlayerCommand>(1);
    let (stream, sample_tx) = create_audio_stream()?;
    mutex_lock!(AUDIO_STREAM).replace(StreamSendWrapper(stream));
    spawn(move || {
        let mut paused = true;
        let mut reader: Option<BufReader<File>> = None;
        // TODO: avoid the endless loop
        loop {
            // TODO: error handling (unwrap) inside-thread
            match rx.try_recv() {
                Ok(PlayerCommand::Start) => {
                    reader = Some(BufReader::new(File::open(&drive).unwrap()));
                }
                Ok(PlayerCommand::Goto(offset)) => {
                    if let Some(ref mut r) = reader {
                        r.seek(SeekFrom::Start(offset)).unwrap();
                    }
                }
                Ok(PlayerCommand::Pause) => {
                    paused = true;
                }
                Ok(PlayerCommand::Play) => {
                    paused = false;
                }
                Ok(_) => {
                    unimplemented!();
                }
                Err(_) => {
                }
            }
            if !paused && let Some(ref mut r) = reader {
                for _ in 0..1024 {
                    let sample = r.read_i16::<LE>().unwrap();
                    sample_tx.send(sample).unwrap()
                }
            }
        }
        // block the thread
        let mutex = Mutex::new(());
        let _a = Condvar::new().wait(mutex.lock().unwrap()).unwrap();
    });
    Ok(tx)
}
