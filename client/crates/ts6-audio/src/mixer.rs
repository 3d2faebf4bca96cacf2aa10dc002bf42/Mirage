//! Receiving side: jitter buffers per peer (from the vendored library) mixed into
//! one playback buffer.

use anyhow::Result;
use tsclientlib::audio::AudioHandler;

pub use tsproto_packets::packets::AudioData;
use tsproto_packets::packets::InAudioBuf;

/// A peer is identified by its client id on the server.
pub type PeerId = u16;

/// Mixes all incoming voice streams.
pub struct Mixer {
    handler: AudioHandler<PeerId>,
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            handler: AudioHandler::new(),
        }
    }

    /// Feed one received voice packet. The handler keeps a per-peer queue,
    /// reorders it and decodes with Opus (loss concealment included).
    pub fn push(&mut self, from: PeerId, packet: InAudioBuf) -> Result<()> {
        self.handler.handle_packet(from, packet)?;
        Ok(())
    }

    /// Peers that currently have queued audio.
    pub fn active_peers(&self) -> Vec<PeerId> {
        let mut peers: Vec<PeerId> = self.handler.get_queues().keys().copied().collect();
        peers.sort_unstable();
        peers
    }

    /// Render `frames` interleaved stereo frames (the shape `cpal` wants).
    pub fn render_stereo(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        self.handler.fill_buffer(&mut out);
        out
    }

    /// Render `frames` mono frames, for metering and for tests.
    pub fn render_mono(&mut self, frames: usize) -> Vec<f32> {
        let stereo = self.render_stereo(frames);
        stereo
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| (pair[0] + pair[1]) * 0.5)
            .collect()
    }

    pub fn clear(&mut self) {
        self.handler.reset();
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract who sent a voice packet, if it is a server-to-client voice packet.
pub fn sender_of(packet: &InAudioBuf) -> Option<PeerId> {
    match packet.data().data() {
        AudioData::S2C { from, .. } => Some(*from),
        AudioData::S2CWhisper { from, .. } => Some(*from),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_mixer_renders_silence_of_the_requested_length() {
        let mut mixer = Mixer::new();
        assert!(mixer.active_peers().is_empty());

        let mono = mixer.render_mono(480);
        assert_eq!(mono.len(), 480);
        assert!(mono.iter().all(|s| *s == 0.0), "idle mixer must be silent");

        let stereo = mixer.render_stereo(240);
        assert_eq!(stereo.len(), 480);
    }
}
