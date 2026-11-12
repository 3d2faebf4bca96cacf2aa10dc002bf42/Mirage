//! Microphone capture: device -> mono 48 kHz -> 20 ms frames -> Opus packets.
//!
//! Runs on its own OS thread because `cpal::Stream` is not `Send` on Windows
//! (WASAPI requires the stream to live on the thread that created it). Encoded
//! packets are handed to the network loop over a bounded `tokio::mpsc` channel, so
//! a slow network never blocks the audio callback: excess frames are dropped and
//! counted.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SampleFormat;

use crate::codec::{VoiceEncoder, FRAME_SAMPLES, SAMPLE_RATE};
use crate::devices::pick_input;
use crate::resample::{downmix_to_mono, LinearResampler};

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Exact device name, or `None` for the system default.
    pub device: Option<String>,
    /// Opus target bitrate in bits per second.
    pub bitrate: i32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device: None,
            bitrate: 64_000,
        }
    }
}

pub struct Capture {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    /// Frames encoded and handed to the network loop.
    pub fn frames_encoded(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// Frames dropped because the network loop did not keep up.
    pub fn frames_dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Stop capturing and wait for the thread to finish.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Everything the audio callback needs, moved into the callback closure.
struct Processor {
    encoder: VoiceEncoder,
    resampler: LinearResampler,
    mono: Vec<f32>,
    resampled: Vec<f32>,
    pending: Vec<f32>,
    sink: tokio::sync::mpsc::Sender<Vec<u8>>,
    frames: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}

impl Processor {
    fn new(
        sample_rate: u32,
        bitrate: i32,
        sink: tokio::sync::mpsc::Sender<Vec<u8>>,
        frames: Arc<AtomicU64>,
        dropped: Arc<AtomicU64>,
    ) -> Result<Self> {
        Ok(Self {
            encoder: VoiceEncoder::new(bitrate)?,
            resampler: LinearResampler::new(sample_rate, SAMPLE_RATE),
            mono: Vec::with_capacity(4096),
            resampled: Vec::with_capacity(4096),
            pending: Vec::with_capacity(FRAME_SAMPLES * 2),
            sink,
            frames,
            dropped,
        })
    }

    /// `interleaved` is a device buffer with `channels` interleaved channels.
    fn feed(&mut self, interleaved: &[f32], channels: usize) {
        if interleaved.is_empty() {
            return;
        }

        self.mono.clear();
        downmix_to_mono(interleaved, channels, &mut self.mono);

        self.resampled.clear();
        self.resampler.process(&self.mono, &mut self.resampled);
        self.pending.extend_from_slice(&self.resampled);

        while self.pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = self.pending.drain(..FRAME_SAMPLES).collect();
            match self.encoder.encode(&frame) {
                Ok(encoded) => match self.sink.try_send(encoded.to_vec()) {
                    Ok(()) => {
                        self.frames.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                },
                Err(error) => tracing::warn!(%error, "failed to encode a captured frame"),
            }
        }
    }
}

/// Open the microphone and start feeding encoded frames into `sink`.
pub fn start_capture(
    config: CaptureConfig,
    sink: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<Capture> {
    let device = pick_input(config.device.clone()).ok_or_else(|| anyhow!("no input device"))?;
    let device_name = crate::devices::device_name(&device);
    let supported = device.default_input_config()?;
    let channels = supported.channels() as usize;
    let sample_rate = supported.sample_rate();
    let sample_format = supported.sample_format();
    let stream_config = supported.config();

    let stop = Arc::new(AtomicBool::new(false));
    let frames = Arc::new(AtomicU64::new(0));
    let dropped = Arc::new(AtomicU64::new(0));

    let (stop_thread, frames_thread, dropped_thread) =
        (stop.clone(), frames.clone(), dropped.clone());
    let bitrate = config.bitrate;
    let device_for_thread = device_name.clone();

    let thread = std::thread::Builder::new()
        .name("ts6-capture".into())
        .spawn(move || {
            let mut processor = match Processor::new(
                sample_rate,
                bitrate,
                sink,
                frames_thread,
                dropped_thread,
            ) {
                Ok(processor) => processor,
                Err(error) => {
                    tracing::error!(%error, device = %device_for_thread, "cannot create the Opus encoder");
                    return;
                }
            };

            let on_error = |error| tracing::error!(%error, "capture stream error");
            let stream = match sample_format {
                SampleFormat::F32 => device.build_input_stream(
                    stream_config,
                    move |data: &[f32], _| processor.feed(data, channels),
                    on_error,
                    None,
                ),
                SampleFormat::I16 => device.build_input_stream(
                    stream_config,
                    move |data: &[i16], _| {
                        let converted: Vec<f32> =
                            data.iter().map(|s| *s as f32 / 32768.0).collect();
                        processor.feed(&converted, channels);
                    },
                    on_error,
                    None,
                ),
                SampleFormat::U16 => device.build_input_stream(
                    stream_config,
                    move |data: &[u16], _| {
                        let converted: Vec<f32> = data
                            .iter()
                            .map(|s| (*s as f32 - 32768.0) / 32768.0)
                            .collect();
                        processor.feed(&converted, channels);
                    },
                    on_error,
                    None,
                ),
                other => {
                    tracing::error!(?other, "unsupported capture sample format");
                    return;
                }
            };

            let stream = match stream {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::error!(%error, device = %device_for_thread, "cannot open the capture stream");
                    return;
                }
            };

            if let Err(error) = stream.play() {
                tracing::error!(%error, "cannot start the capture stream");
                return;
            }
            tracing::info!(device = %device_for_thread, sample_rate, channels, ?sample_format, "capture started");

            while !stop_thread.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            drop(stream);
            tracing::info!(device = %device_for_thread, "capture stopped");
        })?;

    Ok(Capture {
        device_name,
        sample_rate,
        channels: channels as u16,
        stop,
        frames,
        dropped,
        thread: Some(thread),
    })
}
