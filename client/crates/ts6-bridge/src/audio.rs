//! Local audio pipeline for the bridge: microphone (or synthetic tone) into the
//! session, received voice into the speakers, optional push-to-talk gate.
//!
//! Everything hardware-facing lives on its own thread inside `ts6-audio`; this
//! module only wires the pieces together and keeps the counters the UI shows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ts6_core::{AudioData, InAudioBuf};

/// How long a client keeps its "speaking" ring after the last voice packet.
const SPEAKING_HOLD: Duration = Duration::from_millis(450);

#[derive(Debug, Clone, Default)]
pub struct AudioConfig {
    pub enabled: bool,
    pub tone: bool,
    pub ptt: Option<String>,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}

pub struct AudioEngine {
    playback: Option<ts6_audio::Playback>,
    capture: Option<ts6_audio::Capture>,
    gate: Option<ts6_input::Gate>,
    ptt: Option<ts6_input::PttMonitor>,
    mic_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    mic_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    tone_stop: Option<Arc<AtomicBool>>,
    tone_thread: Option<std::thread::JoinHandle<()>>,
    pub tone: bool,
    pub ptt_binding: Option<String>,
    rx_packets: u64,
    speaking: std::collections::HashMap<u16, Instant>,
    last_publish: Instant,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (mic_tx, mic_rx) = tokio::sync::mpsc::channel(64);
        Self {
            playback: None,
            capture: None,
            gate: None,
            ptt: None,
            mic_tx,
            mic_rx,
            tone_stop: None,
            tone_thread: None,
            tone: false,
            ptt_binding: None,
            rx_packets: 0,
            speaking: Default::default(),
            last_publish: Instant::now(),
        }
    }

    /// The session polls this for microphone frames.
    pub fn mic_rx_mut(&mut self) -> &mut tokio::sync::mpsc::Receiver<Vec<u8>> {
        &mut self.mic_rx
    }

    /// A received voice packet: note who is speaking and queue it for playback.
    pub fn on_audio(&mut self, packet: InAudioBuf) {
        let from = match packet.data().data() {
            AudioData::S2C { from, .. } => Some(*from),
            AudioData::S2CWhisper { from, .. } => Some(*from),
            _ => None,
        };
        if let Some(from) = from {
            self.rx_packets += 1;
            self.speaking.insert(from, Instant::now());
            if let Some(playback) = &self.playback {
                playback.enqueue(from, packet);
            }
        }
    }

    pub fn configure(&mut self, config: &AudioConfig) {
        self.teardown_input();

        if !config.enabled {
            if let Some(playback) = self.playback.take() {
                playback.stop();
            }
            self.tone = false;
            self.ptt_binding = None;
            return;
        }

        // Push-to-talk, when bound: the gate sits between the source and the session.
        let binding = config.ptt.clone().filter(|key| !key.trim().is_empty());
        let mut monitor = None;
        let target = match &binding {
            Some(key) => match ts6_input::bind_ptt(key) {
                Ok(started) => {
                    let (source_tx, source_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
                    let gate =
                        ts6_input::Gate::start(source_rx, self.mic_tx.clone(), started.pressed());
                    self.gate = Some(gate);
                    monitor = Some(started);
                    source_tx
                }
                Err(error) => {
                    tracing::warn!(%error, "PTT unavailable; transmission disabled");
                    return;
                }
            },
            None => self.mic_tx.clone(),
        };
        self.ptt = monitor;
        self.ptt_binding = self.ptt.as_ref().and_then(|_| binding.clone());

        // Playback (receiving voice).
        if self.playback.is_none() {
            match ts6_audio::start_playback(config.output_device.clone()) {
                Ok(playback) => self.playback = Some(playback),
                Err(error) => tracing::warn!(%error, "playback unavailable"),
            }
        }

        // Source: synthetic tone when asked, otherwise the real microphone.
        // An unavailable microphone is reported, never silently replaced by a tone.
        if config.tone {
            if let Some((stop, thread)) = spawn_tone(target) {
                self.tone_stop = stop;
                self.tone_thread = Some(thread);
                self.tone = true;
            }
        } else {
            match ts6_audio::start_capture(
                ts6_audio::CaptureConfig {
                    device: config.input_device.clone(),
                    bitrate: 64_000,
                },
                target,
            ) {
                Ok(capture) => self.capture = Some(capture),
                Err(error) => tracing::warn!(%error, "capture unavailable"),
            }
        }
    }

    fn teardown_input(&mut self) {
        if let Some(gate) = self.gate.take() {
            gate.stop();
        }
        if let Some(ptt) = self.ptt.take() {
            ptt.stop();
        }
        if let Some(capture) = self.capture.take() {
            capture.stop();
        }
        if let Some(stop) = self.tone_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(thread) = self.tone_thread.take() {
            let _ = thread.join();
        }
        self.tone = false;
        self.ptt_binding = None;
        // Producers are stopped; stale frames must not survive reconfiguration.
        while self.mic_rx.try_recv().is_ok() {}
    }

    /// Reset the per-session counters when a new session starts, so the UI never
    /// shows counters accumulated across connections.
    pub fn reset_counters(&mut self) {
        self.rx_packets = 0;
        self.speaking.clear();
        self.last_publish = Instant::now();
    }
    pub fn stop(&mut self) {
        self.teardown_input();
        if let Some(playback) = self.playback.take() {
            playback.stop();
        }
        self.speaking.clear();
    }

    /// Snapshot of the counters for the UI, refreshed at most every 400 ms.
    pub fn stats_due(&mut self) -> bool {
        self.last_publish.elapsed() >= Duration::from_millis(400)
    }

    pub fn mark_published(&mut self) {
        self.last_publish = Instant::now();
    }

    pub fn counters(&mut self) -> Counters {
        let now = Instant::now();
        self.speaking
            .retain(|_, at| now.duration_since(*at) < SPEAKING_HOLD);
        let mut speaking: Vec<u16> = self.speaking.keys().copied().collect();
        speaking.sort_unstable();
        Counters {
            rx_packets: self.rx_packets,
            ptt_down: self
                .ptt
                .as_ref()
                .map(|monitor| monitor.pressed().load(Ordering::Relaxed))
                .unwrap_or(false),
            speaking,
            capture: self.capture.as_ref().map(|c| c.device_name.clone()),
            playback: self.playback.as_ref().map(|p| p.device_name.clone()),
            tone: self.tone,
            ptt_binding: self.ptt_binding.clone(),
        }
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Counters {
    pub rx_packets: u64,
    pub ptt_down: bool,
    pub speaking: Vec<u16>,
    pub capture: Option<String>,
    pub playback: Option<String>,
    pub tone: bool,
    pub ptt_binding: Option<String>,
}

/// Spawn the synthetic tone generator; returns its stop flag and thread handle.
pub(crate) fn spawn_tone(
    sink: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Option<(Option<Arc<AtomicBool>>, std::thread::JoinHandle<()>)> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let thread = std::thread::Builder::new()
        .name("ts6-bridge-tone".into())
        .spawn(move || {
            let mut encoder = match ts6_audio::VoiceEncoder::new(64_000) {
                Ok(encoder) => encoder,
                Err(error) => {
                    tracing::error!(%error, "cannot create the tone encoder");
                    return;
                }
            };
            let mut tone = ts6_audio::ToneSource::new(440.0, 0.1);
            // An absolute cadence reduces cumulative scheduling drift. Historical
            // measurements showed unexplained excess packets; this scheduling
            // choice alone does not establish their cause or resolution.
            let frame_duration = Duration::from_millis(20);
            let mut next_frame_at = std::time::Instant::now();
            while !stop_thread.load(Ordering::Relaxed) {
                let frame = tone.next_frame();
                if let Ok(encoded) = encoder.encode(&frame) {
                    if sink.try_send(encoded.to_vec()).is_err() {
                        // Session is not draining; skip this frame.
                    }
                }
                next_frame_at += frame_duration;
                let now = std::time::Instant::now();
                if next_frame_at > now {
                    std::thread::sleep(next_frame_at - now);
                } else {
                    // Fell behind (or the system clock jumped): resynchronise.
                    next_frame_at = now;
                }
            }
        })
        .ok()?;
    Some((Some(stop), thread))
}

/// Refresh the device lists.
pub fn device_lists() -> crate::protocol::DeviceLists {
    crate::protocol::DeviceLists {
        inputs: ts6_audio::devices::inputs()
            .into_iter()
            .map(Into::into)
            .collect(),
        outputs: ts6_audio::devices::outputs()
            .into_iter()
            .map(Into::into)
            .collect(),
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;

    #[test]
    fn invalid_ptt_never_starts_a_transmitting_source() {
        let mut engine = AudioEngine::new();
        engine.mic_tx.try_send(vec![1, 2, 3]).unwrap();
        engine.configure(&AudioConfig {
            enabled: true,
            tone: true,
            ptt: Some("not-a-key".into()),
            ..AudioConfig::default()
        });
        let transmitting = engine.tone || engine.capture.is_some();
        let queued = engine.mic_rx.try_recv().is_ok();
        engine.stop();
        assert!(!transmitting, "invalid PTT must fail closed");
        assert!(!queued, "queued audio leaked after disabling PTT");
    }
}
