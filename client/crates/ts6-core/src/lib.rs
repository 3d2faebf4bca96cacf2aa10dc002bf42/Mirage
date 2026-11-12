//! `ts6-core` — protocol/state layer of our TeamSpeak 6 client.
//!
//! Design rule: this crate knows nothing about UI or audio devices. It owns the
//! connection, the bookkeeping snapshot and the command layer, and it surfaces
//! everything the server sends, including TS6 notifies we do not model yet.
//!
//! Protocol base: vendored `tsclientlib`/`tsproto` (MIT OR Apache-2.0).
//! TS6-specific commands (`setupstream`, `joinstreamrequest`, `streamsignaling`)
//! are sent through [`Session::send_command`], the same raw path the type-safe
//! commands use, so the TS6 layer can live entirely in this crate.

pub mod config;
pub mod identity;
pub mod session;
pub mod state;

pub use config::ClientConfig;
pub use session::{ChatTarget, Item, Session, TextMessage, Wake};
pub use state::{ChannelInfo, ClientInfo, ServerInfo, Snapshot};

// Packet-level types needed to build/send voice and TS6 stream commands, so that
// downstream crates do not have to depend on `tsproto-packets` directly.
pub use tsproto_packets::packets::{AudioData, CodecType, InAudioBuf, OutAudio, OutPacket};
