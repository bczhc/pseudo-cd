use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::{channel, Receiver, sync_channel, SyncSender};
use std::thread::spawn;

use anyhow::anyhow;
use byteorder::{LE, ReadBytesExt};
use cpal::{Sample, SampleFormat, SampleRate, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use once_cell::sync::Lazy;

use crate::{mutex_lock, SECTOR_SIZE, Track};

/// We place [`Stream`] here just to prevent it from dropping
pub static AUDIO_STREAM: Lazy<Mutex<Option<StreamSendWrapper>>> = Lazy::new(|| Mutex::new(None));
pub static PLAYBACK_HANDLE: Lazy<Mutex<Option<PlaybackHandle>>> = Lazy::new(|| Mutex::new(None));
pub const AUDIO_SAMPLE_RATE: u32 = 44100;
pub const AUDIO_BIT_DEPTH: u32 = 16;
pub const AUDIO_CHANNELS: u32 = 2;

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
    /// Go to a track
    ///
    /// The second parameter indicates autoplay
    Goto(Track, bool),
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

pub enum PlayerCallbackEvent {
    Finished,
    Paused(bool),
    /// (current, total), in seconds
    Progress(u32, u32),
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

pub struct PlaybackHandle {
    command_tx: SyncSender<PlayerCommand>,
    result_rx: Arc<Mutex<Receiver<PlayerResult>>>,
}

impl PlaybackHandle {
    pub fn send(&self, cmd: PlayerCommand) {
        self.command_tx.send(cmd).unwrap()
    }

    pub fn send_commands(&self, cmds: impl IntoIterator<Item = PlayerCommand>) {
        for c in cmds {
            self.send(c);
        }
    }

    pub fn player_result(&self) -> PlayerResult {
        mutex_lock!(self.result_rx).recv().unwrap()
    }

    pub fn send_recv(&self, cmd: PlayerCommand) -> PlayerResult {
        self.send(cmd);
        self.player_result()
    }
}

pub fn set_global_playback_handle(playback_handle: PlaybackHandle) {
    mutex_lock!(PLAYBACK_HANDLE).replace(playback_handle);
}

pub fn start_global_playback_thread<D, F>(
    drive: PathBuf,
    callback_data: D,
    event_callback: Option<F>
) -> anyhow::Result<PlaybackHandle> where
    D: Send + 'static,
    F: Fn(PlayerCallbackEvent, &D) + Send + 'static {
    let (cmd_tx, cmd_rx) = sync_channel::<PlayerCommand>(1);
    let (result_tx, result_rx) = sync_channel::<PlayerResult>(1);
    let result_rx = Arc::new(Mutex::new(result_rx));
    
    let (stream, sample_tx) = create_audio_stream()?;
    mutex_lock!(AUDIO_STREAM).replace(StreamSendWrapper(stream));
    spawn(move || {
        let mut paused = true;
        let mut reader: Option<BufReader<File>> = None;
        let mut start_pos = 0_u64;
        let mut end_pos = 0_u64;
        let mut song_seconds = 0_u32;
        let event_callback = event_callback;
        let callback_data = callback_data;
        const BYTES_ONE_SEC: u64 = AUDIO_SAMPLE_RATE as u64 * AUDIO_BIT_DEPTH as u64 * AUDIO_CHANNELS as u64 / 8;
        macro event_callback($($arg:tt)*) {
            if let Some(x) = event_callback.as_ref() { x($($arg)*, &callback_data) }
        }
        // TODO: avoid the endless loop
        loop {
            // TODO: error handling (unwrap) inside-thread
            match cmd_rx.try_recv() {
                Ok(PlayerCommand::Start) => {
                    reader = Some(BufReader::new(File::open(&drive).unwrap()));
                }
                Ok(PlayerCommand::Goto(track, play)) => {
                    if let Some(ref mut r) = reader {
                        r.seek(SeekFrom::Start(track.start_offset())).unwrap();
                        if play {
                            paused = false;
                            event_callback!(PlayerCallbackEvent::Paused(false))
                        }
                    }
                    start_pos = track.start_offset();
                    end_pos = track.end_offset();
                    song_seconds = ((end_pos - start_pos) / BYTES_ONE_SEC) as u32;
                    event_callback!(PlayerCallbackEvent::Progress(0, song_seconds))
                }
                Ok(PlayerCommand::Pause) => {
                    paused = true;
                    event_callback!(PlayerCallbackEvent::Paused(paused))
                }
                Ok(PlayerCommand::Play) => {
                    paused = false;
                    event_callback!(PlayerCallbackEvent::Paused(paused))
                }
                Ok(PlayerCommand::SetPaused(p)) => {
                    paused = p;
                    event_callback!(PlayerCallbackEvent::Paused(paused));
                }
                Ok(PlayerCommand::GetIsPaused) => {
                    result_tx.send(PlayerResult::IsPaused(paused)).unwrap();
                }
                Ok(_) => {
                    unimplemented!();
                }
                Err(_) => {}
            }
            if !paused && let Some(ref mut r) = reader {
                for _ in 0..1024 {
                    let sample = r.read_i16::<LE>().unwrap();
                    // TODO: this panics on `clean_up_and_exit`
                    sample_tx.send(sample).unwrap();

                    let pos = r.stream_position().unwrap();
                    if (pos - start_pos) % (BYTES_ONE_SEC) == 0 {
                        event_callback!(PlayerCallbackEvent::Progress(((pos - start_pos) / BYTES_ONE_SEC) as u32,song_seconds));
                    }
                }
            }
        }
        // block the thread
        let mutex = Mutex::new(());
        let _a = Condvar::new().wait(mutex.lock().unwrap()).unwrap();
    });
    Ok(PlaybackHandle {
        command_tx: cmd_tx,
        result_rx
    })
}
