use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::thread::spawn;

use anyhow::anyhow;
use byteorder::{LE, ReadBytesExt};
use cpal::{Sample, SampleFormat, SampleRate, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use once_cell::sync::Lazy;

use crate::mutex_lock;

/// We place [`Stream`] here just to prevent it from dropping
pub static AUDIO_STREAM: Lazy<Mutex<Option<StreamSendWrapper>>> = Lazy::new(|| Mutex::new(None));
pub const AUDIO_SAMPLE_RATE: u32 = 44100;

pub fn create_audio_stream() -> anyhow::Result<(Stream, SyncSender<i16>)> {
    let (tx, rx) = sync_channel(AUDIO_SAMPLE_RATE as usize);

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No audio output device found"))?;
    let configs = device.supported_output_configs()?;
    let mut configs =
        configs.filter(|x| x.channels() == 2 && x.sample_format() == SampleFormat::I16);
    let first = configs
        .next()
        .ok_or_else(|| anyhow!("No audio output profile found"))?;

    let output_config = first
        .try_with_sample_rate(SampleRate(AUDIO_SAMPLE_RATE))
        .ok_or_else(|| {
            anyhow!(
                "No audio output profile with sample rate {} found",
                AUDIO_SAMPLE_RATE
            )
        })?;

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
    ///
    /// The second parameter indicates autoplay
    Goto(u64, bool),
    /// Seek to a position with duration in seconds
    Seek(f64),
    /// Open the file and start playing
    Start,
    Pause,
    Play,
    /// equivalent to [`PlayerCommand::Start`] on `false`
    /// and [`PlayerCommand::Pause`] on `true`
    SetPaused(bool),
    /// Volume level is in 0..1
    ChangeVolume(f64),
    /// Get the current position in seconds
    GetPosition,
    /// Get if in paused state
    GetIsPaused,
}

pub enum PlayerResult {
    None,
    IsPaused(bool),
    /// Current position in seconds
    Position(f64),
}

pub struct StreamSendWrapper(Stream);

impl From<Stream> for StreamSendWrapper {
    fn from(value: Stream) -> Self {
        Self(value)
    }
}

// TODO: safety is not investigated for multiple platforms
unsafe impl Send for StreamSendWrapper {}

// TODO: create a helper wrapper
pub fn start_global_playback_thread(
    drive: PathBuf,
    result_arc: Arc<Mutex<PlayerResult>>,
) -> anyhow::Result<SyncSender<PlayerCommand>> {
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
                Ok(PlayerCommand::Goto(offset, play)) => {
                    if let Some(ref mut r) = reader {
                        r.seek(SeekFrom::Start(offset)).unwrap();
                        if play {
                            paused = false;
                        }
                    }
                }
                Ok(PlayerCommand::Pause) => {
                    paused = true;
                }
                Ok(PlayerCommand::Play) => {
                    paused = false;
                }
                Ok(PlayerCommand::SetPaused(p)) => {
                    paused = p;
                }
                Ok(PlayerCommand::GetIsPaused) => {
                    *result_arc.lock().unwrap() = PlayerResult::IsPaused(paused);
                }
                Ok(_) => {
                    unimplemented!();
                }
                Err(_) => {}
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
