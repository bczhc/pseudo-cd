use std::cell::RefCell;
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom, stdin};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::thread::{sleep, spawn};
use std::time::Duration;
use byteorder::{LE, ReadBytesExt};
use cpal::{ChannelCount, Sample, SampleFormat, SampleRate, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use once_cell::sync::Lazy;
use rand::RngCore;
use rand::rngs::OsRng;

pub fn create_audio_stream() -> anyhow::Result<(Stream, SyncSender<i16>)> {
    let (tx, rx) = sync_channel(44100);

    // TODO: do a bunch of strict output config checks
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let output_configs = device.supported_output_configs()?;
    let output_configs = output_configs.filter(|x| {
        x.channels() == 2 && x.sample_format() == SampleFormat::I16
    }).collect::<Vec<_>>();
    let output_config = output_configs[0].with_sample_rate(SampleRate(44100));

    // Why here there's no multiple-move encountering?? this `play_fn` should be called
    // multiple times, and `rx` will be "moved" many times?
    let play_fn = move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
        for x in data.iter_mut() {
            *x = rx.try_recv().unwrap_or(i16::EQUILIBRIUM);
        };
    };
    let stream = device.build_output_stream(&output_config.config(), play_fn, move |err| {
        println!("{}", err);
    }, None /* blocking */)?;
    stream.play()?;
    Ok((stream, tx))
}

enum Order {
    Goto(u64),
}

fn play() -> anyhow::Result<SyncSender<Order>> {
    let file = File::open("/dev/sr0").unwrap();
    let mut reader = BufReader::new(file);

    let (tx, rx) = sync_channel::<Order>(1);
    let (stream, sample_tx) = create_audio_stream()?;
    Box::leak(Box::new(stream));
    spawn(move || {
        loop {
            for _ in 0..1024 {
                let sample = reader.read_i16::<LE>().unwrap();
                sample_tx.send(sample).unwrap()
            }
            if let Ok(Order::Goto(offset)) = rx.try_recv() {
                reader.seek(SeekFrom::Start(offset)).unwrap();
            }
        }
        // block the thread
        let mutex = Mutex::new(());
        let _a = Condvar::new().wait(mutex.lock().unwrap()).unwrap();
    });
    Ok(tx)
}

pub fn demo() -> anyhow::Result<()> {
    let offset1 = 154112_u64 * 2048;
    let offset2 = 175056_u64 * 2048;

    let order_sender = play()?;
    sleep(Duration::from_secs(1));
    order_sender.send(Order::Goto(offset1)).unwrap();

    sleep(Duration::from_secs(5));
    order_sender.send(Order::Goto(offset2)).unwrap();

    sleep(Duration::from_secs(5));

    Ok(())
}