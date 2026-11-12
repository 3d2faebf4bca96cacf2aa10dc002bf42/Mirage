//! Opus encode/decode for the TS6 voice channel.

use anyhow::{Context, Result};
use audiopus::coder::{Decoder, Encoder};
use audiopus::{Application, Channels, SampleRate};

/// TeamSpeak voice is 48 kHz mono.
pub const SAMPLE_RATE: u32 = 48_000;
/// One packet carries 20 ms, i.e. 960 samples at 48 kHz.
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize) / 50;
/// Maximum size of an Opus frame (RFC 6716).
pub const MAX_OPUS_FRAME: usize = 1275;

/// Encodes PCM f32 frames into Opus packets.
pub struct VoiceEncoder {
    encoder: Encoder,
    buf: [u8; MAX_OPUS_FRAME],
}

impl VoiceEncoder {
    /// `bitrate_bps` is the target bitrate; our callers normally request 64 kbit/s.
    pub fn new(bitrate_bps: i32) -> Result<Self> {
        let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)
            .context("creating opus encoder")?;
        let mut this = Self {
            encoder,
            buf: [0; MAX_OPUS_FRAME],
        };
        // Set the target bitrate, retaining the encoder's default VBR behavior.
        this.encoder
            .set_bitrate(audiopus::Bitrate::BitsPerSecond(bitrate_bps))
            .context("setting opus bitrate")?;
        Ok(this)
    }

    /// Encode exactly one frame. `samples.len()` should be [`FRAME_SAMPLES`];
    /// returns the encoded packet bytes.
    pub fn encode(&mut self, samples: &[f32]) -> Result<&[u8]> {
        let len = self
            .encoder
            .encode_float(samples, &mut self.buf[..])
            .context("encoding opus frame")?;
        Ok(&self.buf[..len])
    }
}

/// Decodes Opus packets back to PCM f32.
pub struct VoiceDecoder {
    decoder: Decoder,
    pcm: Vec<f32>,
}

impl VoiceDecoder {
    pub fn new() -> Result<Self> {
        let decoder =
            Decoder::new(SampleRate::Hz48000, Channels::Mono).context("creating opus decoder")?;
        Ok(Self {
            decoder,
            pcm: vec![0.0; FRAME_SAMPLES * 6],
        })
    }

    /// Decode one packet. `None` triggers packet-loss concealment.
    pub fn decode(&mut self, packet: Option<&[u8]>) -> Result<&[f32]> {
        // audiopus wants its own borrowed `Packet` wrapper, not a raw slice.
        let input = match packet {
            Some(bytes) => {
                Some(audiopus::packet::Packet::try_from(bytes).context("wrapping an opus packet")?)
            }
            None => None,
        };
        // For a real packet Opus returns its own frame length, so hand it the whole
        // buffer. For concealment (no packet) libopus fills exactly what it is
        // asked for, so ask for one 20 ms frame or it would emit its maximum.
        let request = if packet.is_none() {
            FRAME_SAMPLES
        } else {
            self.pcm.len()
        };
        let output = audiopus::MutSignals::try_from(&mut self.pcm[..request])
            .context("wrapping the pcm output buffer")?;
        let len = self
            .decoder
            .decode_float(input, output, false)
            .context("decoding opus frame")?;
        Ok(&self.pcm[..len])
    }
}

/// Independent Opus decoder state for each sender during a diagnostic session.
#[derive(Default)]
pub struct PeerDecoders {
    decoders: std::collections::HashMap<u16, VoiceDecoder>,
}

impl PeerDecoders {
    pub fn decode(&mut self, peer: u16, packet: &[u8]) -> Result<&[f32]> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.decoders.entry(peer) {
            entry.insert(VoiceDecoder::new()?);
        }
        self.decoders
            .get_mut(&peer)
            .expect("peer decoder initialized")
            .decode(Some(packet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An offline codec round-trip must preserve the signal. Server/peer
    /// interoperability needs a separate live experiment.
    #[test]
    fn opus_roundtrip_preserves_a_sine() {
        let mut encoder = VoiceEncoder::new(64_000).expect("encoder");
        let mut decoder = VoiceDecoder::new().expect("decoder");

        let mut total_energy = 0.0f64;
        let mut frames = 0;

        for i in 0..10 {
            let phase = i as f32 * 0.1;
            let frame: Vec<f32> = (0..FRAME_SAMPLES)
                .map(|n| {
                    let t = (n as f32) / SAMPLE_RATE as f32 + phase;
                    0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                })
                .collect();

            let encoded = encoder.encode(&frame).expect("encode").to_vec();
            assert!(!encoded.is_empty());
            assert!(encoded.len() <= MAX_OPUS_FRAME);

            let decoded = decoder.decode(Some(&encoded)).expect("decode");
            assert_eq!(decoded.len(), FRAME_SAMPLES);

            total_energy += decoded
                .iter()
                .map(|s| (*s as f64) * (*s as f64))
                .sum::<f64>();
            frames += decoded.len();
        }

        let rms = (total_energy / frames as f64).sqrt();
        // A 0.5 amplitude sine has RMS ~0.354; Opus at 64k keeps it well above 0.2.
        assert!(rms > 0.2, "decoded RMS too low: {rms}");
        assert!(rms < 0.6, "decoded RMS unexpectedly high: {rms}");
    }

    /// Packet-loss concealment must keep the stream alive with the right frame
    /// size after decoding a real frame.
    #[test]
    fn decoder_conceals_a_lost_packet_with_the_stream_frame_size() {
        let mut encoder = VoiceEncoder::new(64_000).expect("encoder");
        let mut decoder = VoiceDecoder::new().expect("decoder");

        let frame = vec![0.1f32; FRAME_SAMPLES];
        let encoded = encoder.encode(&frame).expect("encode").to_vec();
        let first = decoder.decode(Some(&encoded)).expect("decode real frame");
        assert_eq!(first.len(), FRAME_SAMPLES);

        // Now the decoder knows the 20 ms frame size and must conceal in kind.
        let concealed = decoder.decode(None).expect("concealment");
        assert_eq!(concealed.len(), FRAME_SAMPLES);
    }
}

#[cfg(test)]
mod peer_tests {
    use super::*;

    #[test]
    fn interleaved_senders_match_independent_reference_decoders() {
        let mut bank = PeerDecoders::default();
        let mut encoders = [
            VoiceEncoder::new(64_000).unwrap(),
            VoiceEncoder::new(64_000).unwrap(),
        ];
        let mut references = [VoiceDecoder::new().unwrap(), VoiceDecoder::new().unwrap()];
        let mut tones = [
            crate::ToneSource::new(440.0, 0.1),
            crate::ToneSource::new(880.0, 0.3),
        ];
        for _ in 0..12 {
            for peer in 0..2 {
                let frame = tones[peer].next_frame();
                let packet = encoders[peer].encode(&frame).unwrap().to_vec();
                let expected = references[peer].decode(Some(&packet)).unwrap();
                let actual = bank.decode(peer as u16, &packet).unwrap();
                assert_eq!(actual.len(), expected.len());
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (a - b).abs() < 1e-6),
                    "decoder history leaked between independent speakers"
                );
            }
        }
    }
}
