//! `ts6-audio` — the audio plane.
//!
//! Layering:
//! * [`codec`], [`tone`], [`meter`], [`mixer`], [`resample`] are hardware-free and
//!   unit-tested (they are what the automated verification exercises).
//! * [`devices`], [`capture`], [`playback`] touch real hardware through `cpal` and
//!   live on their own OS threads (a `cpal::Stream` is not `Send` on Windows).
//!
//! Wire format facts (see `docs/protocol-ts3j.md` §5):
//! * 48 kHz, mono, 20 ms frames = 960 samples per packet, 50 packets/s.
//! * Frame body = `id(2) [+from(2) when S2C] + codec(1) + opus payload`.
//! * An empty payload ends a voice session; the vendored library handles the
//!   session flags.

pub mod capture;
pub mod codec;
pub mod devices;
pub mod meter;
pub mod mixer;
pub mod playback;
pub mod resample;
pub mod tone;

pub use capture::{start_capture, Capture, CaptureConfig};
pub use codec::{VoiceDecoder, VoiceEncoder, FRAME_SAMPLES, MAX_OPUS_FRAME, SAMPLE_RATE};
pub use devices::DeviceInfo;
pub use meter::RmsMeter;
pub use mixer::Mixer;
pub use playback::{start_playback, Playback};
pub use tone::ToneSource;
