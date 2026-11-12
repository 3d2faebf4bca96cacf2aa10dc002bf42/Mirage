//! The session: connection lifecycle, event surfacing and the command layer.
//!
//! Every command — including the TS6 stream commands we add later — goes through
//! [`Session::send_command`], which sends a raw TS command and waits for the
//! server's `MessageResult` for *that* command (correlated by `return_code`, not
//! by arrival order).

use std::collections::VecDeque;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::prelude::*;
use tokio::time::{timeout_at, Instant};
use tsclientlib::prelude::*;
use tsclientlib::{Connection, DisconnectOptions, InMessage, MessageHandle, StreamItem};
use tsproto_packets::packets::{Direction, Flags, InAudioBuf, OutCommand, OutPacket, PacketType};

use crate::config::ClientConfig;
use crate::state::Snapshot;

/// Where a text message goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTarget {
    Server,
    Channel,
    Client(u16),
}

/// What the core surfaces upwards.
///
/// Not `Clone`: `InAudioBuf` owns a decoded buffer that is moved, never copied.
#[derive(Debug)]
pub enum Item {
    /// Client/channel/server bookkeeping changed: re-read the snapshot.
    StateChanged,
    /// A chat or poke message.
    TextMessage(TextMessage),
    /// A notify the vendored library does not model.
    ///
    /// This is how all TS6 stream events arrive (`notifystreamstarted`,
    /// `notifyjoinstreamrequest`, `notifystreamsignaling`, ...). `content` is the
    /// raw, still TS-escaped command line.
    UnknownCommand { name: String, content: String },
    /// Identity security-level grind progress.
    IdentityLevel(u8),
    /// The connection dropped and is being rebuilt.
    Reconnecting,
    /// One received voice packet (server to client).
    ///
    /// Decoding/mixing is `ts6-audio`'s job; the core only hands the packet over.
    Audio(InAudioBuf),
}

#[derive(Debug, Clone)]
pub struct TextMessage {
    pub invoker_id: Option<u16>,
    pub invoker_name: Option<String>,
    pub message: String,
    pub from_self: bool,
}

/// A wake-up from [`Session::poll_with`].
#[derive(Debug)]
pub enum Wake<T = ()> {
    /// Something happened on the server connection.
    Item(Item),
    /// One batch of captured frames was transmitted; poll again to keep draining.
    MicSent,
    /// A command arrived from the caller (UI, bridge, …).
    Command(T),
}

pub struct Session {
    con: Connection,
    config: ClientConfig,
    /// Items consumed while waiting for a command result, replayed to the caller.
    pending: VecDeque<Item>,
    /// Voice frames handed to the transport so far (surfaced to the UI).
    audio_frames_sent: u64,
}

impl Session {
    /// Open the connection and return only once the server pushed its initial
    /// state (the library's `connect()` returns before the handshake completes).
    pub async fn connect(config: ClientConfig) -> Result<Self> {
        let identity = crate::identity::load_or_create(&config.identity_file)?;
        let mut builder = Connection::build(config.address.clone())
            .name(config.nickname.clone())
            .identity(identity)
            .log_commands(config.log_commands);

        if let Some(channel) = &config.channel {
            builder = builder.channel(channel.clone());
        }
        if let Some(pwd) = &config.channel_password {
            builder = builder.channel_password(pwd.clone());
        }
        if let Some(pwd) = &config.server_password {
            builder = builder.password(pwd.clone());
        }
        if let Some(ip) = config.local_ip {
            builder = builder.local_address(std::net::SocketAddr::new(ip, 0));
        }

        let con = builder.connect()?;
        let mut session = Self {
            con,
            config,
            pending: VecDeque::new(),
            audio_frames_sent: 0,
        };
        session.wait_synced().await?;
        tracing::info!(addr = %session.config.address, "connected and synced");
        Ok(session)
    }

    /// Wait for the initial bookkeeping, then keep draining until the server goes
    /// quiet (channel/client lists arrive as a burst right after `initserver`).
    async fn wait_synced(&mut self) -> Result<()> {
        let hard_deadline = Instant::now() + Duration::from_secs(20);

        // 1. The handshake is done when the first book event arrives.
        loop {
            let item = match timeout_at(hard_deadline, self.con.events().next()).await {
                Err(_) => return Err(anyhow!("timed out during the handshake")),
                Ok(None) => return Err(anyhow!("connection closed during the handshake")),
                Ok(Some(Err(err))) => return Err(anyhow!("handshake failed: {err}")),
                Ok(Some(Ok(item))) => item,
            };
            match item {
                StreamItem::BookEvents(_) => break,
                other => self.buffer(other),
            }
        }

        // 2. Drain the burst of channel/client book events until it goes quiet.
        //
        // The quiet window is not enough on its own: a peer already streaming voice
        // keeps the stream busy forever, so the hard deadline must be enforced here
        // too (measured: a receiver joining a channel with a tone never finished
        // syncing).
        loop {
            let now = Instant::now();
            if now >= hard_deadline {
                tracing::debug!("sync drain reached the hard deadline; continuing");
                return Ok(());
            }
            let quiet = std::cmp::min(now + Duration::from_millis(350), hard_deadline);

            // Hoisted into a `let` so the borrow of `self.con` ends before we call
            // `self.buffer(..)`.
            let next = timeout_at(quiet, self.con.events().next()).await;
            match next {
                Err(_) => return Ok(()),
                Ok(None) => return Ok(()),
                Ok(Some(Err(_))) => return Ok(()),
                Ok(Some(Ok(item))) => self.buffer(item),
            }
        }
    }

    /// Translate a library stream item into our own queue (used while waiting).
    fn buffer(&mut self, item: StreamItem) {
        // A channel with active voice traffic can produce items faster than the
        // caller drains them; keep the queue bounded so a busy server cannot grow
        // it without limit.
        const MAX_PENDING: usize = 4096;
        if self.pending.len() >= MAX_PENDING {
            self.pending.pop_front();
        }
        match item {
            StreamItem::BookEvents(_) => self.pending.push_back(Item::StateChanged),
            StreamItem::UnknownCommand { name, content } => self
                .pending
                .push_back(Item::UnknownCommand { name, content }),
            StreamItem::MessageEvent(InMessage::TextMessage(msg)) => {
                if let Some(first) = msg.iter().next() {
                    let invoker_id = Some(first.invoker_id.0);
                    self.pending.push_back(Item::TextMessage(TextMessage {
                        invoker_id,
                        invoker_name: Some(first.invoker_name.clone()),
                        message: first.message.clone(),
                        from_self: invoker_id == self.own_id(),
                    }));
                }
            }
            StreamItem::IdentityLevelIncreasing(level) => {
                self.pending.push_back(Item::IdentityLevel(level))
            }
            StreamItem::DisconnectedTemporarily(_) => self.pending.push_back(Item::Reconnecting),
            StreamItem::Audio(packet) => self.pending.push_back(Item::Audio(packet)),
            _ => {}
        }
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        self.con
            .get_state()
            .map(Snapshot::from_state)
            .map_err(Into::into)
    }

    /// Next event from the connection (pending replayed first).
    pub async fn next_item(&mut self) -> Option<Item> {
        if let Some(item) = self.pending.pop_front() {
            return Some(item);
        }
        loop {
            let raw = self.con.events().next().await?;
            match raw {
                Ok(StreamItem::BookEvents(_)) => return Some(Item::StateChanged),
                Ok(StreamItem::UnknownCommand { name, content }) => {
                    return Some(Item::UnknownCommand { name, content })
                }
                Ok(StreamItem::MessageEvent(InMessage::TextMessage(msg))) => {
                    let first = match msg.iter().next() {
                        Some(m) => m,
                        None => continue,
                    };
                    let invoker_id = Some(first.invoker_id.0);
                    let from_self = invoker_id == self.own_id();
                    return Some(Item::TextMessage(TextMessage {
                        invoker_id,
                        invoker_name: Some(first.invoker_name.clone()),
                        message: first.message.clone(),
                        from_self,
                    }));
                }
                // TextMessage is the only message we surface for now; other
                // in-messages are either handled by the library or not needed yet.
                Ok(StreamItem::MessageEvent(_)) => continue,
                Ok(StreamItem::IdentityLevelIncreasing(level)) => {
                    return Some(Item::IdentityLevel(level))
                }
                Ok(StreamItem::IdentityLevelIncreased) => continue,
                Ok(StreamItem::DisconnectedTemporarily(reason)) => {
                    tracing::warn!(?reason, "temporarily disconnected");
                    return Some(Item::Reconnecting);
                }
                Ok(StreamItem::Audio(packet)) => return Some(Item::Audio(packet)),
                Ok(StreamItem::MessageResult(..)) => continue,
                Ok(other) => {
                    tracing::debug!(?other, "unhandled stream item");
                    continue;
                }
                Err(err) => {
                    tracing::error!(%err, "connection error");
                    return None;
                }
            }
        }
    }

    fn own_id(&self) -> Option<u16> {
        self.con.get_state().ok().map(|s| s.own_client.0)
    }

    /// Map a raw library event into our own event type.
    ///
    /// `None` means "internal, keep polling" (message results, identity progress,
    /// messages we do not surface yet).
    fn map_raw(&mut self, raw: StreamItem) -> Option<Item> {
        match raw {
            StreamItem::BookEvents(_) => Some(Item::StateChanged),
            StreamItem::UnknownCommand { name, content } => {
                Some(Item::UnknownCommand { name, content })
            }
            StreamItem::MessageEvent(InMessage::TextMessage(msg)) => {
                let first = msg.iter().next()?;
                let invoker_id = Some(first.invoker_id.0);
                let from_self = invoker_id == self.own_id();
                Some(Item::TextMessage(TextMessage {
                    invoker_id,
                    invoker_name: Some(first.invoker_name.clone()),
                    message: first.message.clone(),
                    from_self,
                }))
            }
            StreamItem::MessageEvent(_) => None,
            StreamItem::IdentityLevelIncreasing(level) => Some(Item::IdentityLevel(level)),
            StreamItem::IdentityLevelIncreased => None,
            StreamItem::DisconnectedTemporarily(reason) => {
                tracing::warn!(?reason, "temporarily disconnected");
                Some(Item::Reconnecting)
            }
            StreamItem::Audio(packet) => Some(Item::Audio(packet)),
            StreamItem::MessageResult(..) => None,
            other => {
                tracing::debug!(?other, "unhandled stream item");
                None
            }
        }
    }

    /// One coherent poll that advances the voice link and the caller's commands.
    ///
    /// Why a hand-written poll instead of `tokio::select!`: the events stream must
    /// not be cancelled between polls (measured in `docs/verification.md` V-13), so
    /// this polls the network first, then the microphone, then the command queue.
    /// Commands are returned to the caller *after* the borrow of the connection
    /// ends, so the caller can run async session methods for them.
    pub fn poll_with<T>(
        &mut self,
        mut mic: Option<&mut tokio::sync::mpsc::Receiver<Vec<u8>>>,
        mut commands: Option<&mut tokio::sync::mpsc::Receiver<T>>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Wake<T>>> {
        use std::task::Poll;

        loop {
            if let Some(item) = self.pending.pop_front() {
                return Poll::Ready(Some(Wake::Item(item)));
            }

            // Network first, and polled on every wake-up: never cancelled.
            let polled = {
                let mut stream = self.con.events();
                std::pin::Pin::new(&mut stream).poll_next(cx)
            };
            match polled {
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(Err(err))) => {
                    tracing::error!(%err, "connection error");
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Ok(raw))) => {
                    if let Some(item) = self.map_raw(raw) {
                        return Poll::Ready(Some(Wake::Item(item)));
                    }
                    continue; // internal event: keep going
                }
                Poll::Pending => {}
            }

            // Nothing from the network: transmit captured frames.
            //
            // Sending is done inline and the loop CONTINUES, deliberately not
            // returning a "frame sent" wake-up. Returning early on every captured
            // frame starved voice reception in the bridge (measured: rx dropped to
            // zero while a tone was being transmitted). Draining a small batch and
            // immediately re-polling the network keeps both directions alive.
            if let Some(mic) = mic.as_mut() {
                let mut sent_any = false;
                for _ in 0..8 {
                    match mic.poll_recv(cx) {
                        Poll::Ready(Some(payload)) => {
                            let packet = tsproto_packets::packets::OutAudio::new(
                                &tsproto_packets::packets::AudioData::C2S {
                                    id: 0,
                                    codec: tsproto_packets::packets::CodecType::OpusVoice,
                                    data: &payload,
                                },
                            );
                            match self.con.send_audio(packet) {
                                Ok(()) => {
                                    self.audio_frames_sent += 1;
                                    sent_any = true;
                                }
                                Err(err) => {
                                    tracing::error!(%err, "failed to send captured audio");
                                    return Poll::Ready(None);
                                }
                            }
                        }
                        Poll::Ready(None) => break, // microphone side closed
                        Poll::Pending => break,
                    }
                }
                if sent_any {
                    // Hand control back to the caller after a bounded batch.
                    //
                    // A continue here looks harmless but is not: with a steady
                    // microphone rate this poll can keep finding exactly one new
                    // frame per iteration and never return (measured: a single call
                    // ran for 27 s while the bridge received nothing). Returning a
                    // wake keeps the caller's loop responsive, and the caller polls
                    // again immediately, so nothing is lost.
                    return Poll::Ready(Some(Wake::MicSent));
                }
            }

            // Still idle: hand a queued command to the caller.
            if let Some(commands) = commands.as_mut() {
                match commands.poll_recv(cx) {
                    Poll::Ready(Some(command)) => {
                        return Poll::Ready(Some(Wake::Command(command)));
                    }
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Pending => {}
                }
            }

            return Poll::Pending;
        }
    }

    /// Convenience wrapper used by the CLI: network + microphone only.
    pub fn poll_with_mic(
        &mut self,
        mic: &mut tokio::sync::mpsc::Receiver<Vec<u8>>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Wake<()>>> {
        self.poll_with(Some(mic), None, cx)
    }

    /// Send any TS command with arguments and wait for the correlated result.
    ///
    /// This is the same path the typed helpers use, and the one the TS6 stream
    /// commands will use.
    pub async fn send_command(&mut self, name: &str, args: &[(&str, String)]) -> Result<()> {
        let handle = self.send_command_no_wait(name, args)?;
        self.wait_result(name, handle).await
    }

    /// Fire-and-forget variant: returns the handle so the caller can await it later.
    pub fn send_command_no_wait(
        &mut self,
        name: &str,
        args: &[(&str, String)],
    ) -> Result<MessageHandle> {
        let mut cmd = OutCommand::new(Direction::C2S, Flags::empty(), PacketType::Command, name);
        for (key, value) in args {
            cmd.write_arg(key, value);
        }
        let handle = cmd.send_with_result(&mut self.con)?;
        tracing::debug!(command = name, ?handle, "sent command");
        Ok(handle)
    }

    /// Await the result of a command sent earlier, replaying other events.
    pub async fn wait_result(&mut self, name: &str, handle: MessageHandle) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let stream_item = match timeout_at(deadline, self.con.events().next()).await {
                Err(_) => return Err(anyhow!("timed out waiting for the result of `{name}`")),
                Ok(None) => return Err(anyhow!("connection closed while awaiting `{name}`")),
                Ok(Some(Err(err))) => return Err(anyhow!("connection error: {err}")),
                Ok(Some(Ok(item))) => item,
            };

            match stream_item {
                StreamItem::MessageResult(got, result) if got == handle => {
                    return result.map_err(|err| anyhow!("server rejected `{name}`: {err}"));
                }
                StreamItem::BookEvents(_) => self.pending.push_back(Item::StateChanged),
                StreamItem::UnknownCommand { name, content } => self
                    .pending
                    .push_back(Item::UnknownCommand { name, content }),
                StreamItem::MessageEvent(InMessage::TextMessage(msg)) => {
                    if let Some(first) = msg.iter().next() {
                        let invoker_id = Some(first.invoker_id.0);
                        self.pending.push_back(Item::TextMessage(TextMessage {
                            invoker_id,
                            invoker_name: Some(first.invoker_name.clone()),
                            message: first.message.clone(),
                            from_self: invoker_id == self.own_id(),
                        }));
                    }
                }
                StreamItem::Audio(packet) => self.pending.push_back(Item::Audio(packet)),
                _ => {}
            }
        }
    }

    /// Send a text message.
    pub async fn send_chat(&mut self, target: ChatTarget, text: &str) -> Result<()> {
        let (target_mode, target_id) = match target {
            ChatTarget::Server => ("3", "0".to_string()),
            ChatTarget::Channel => ("2", "0".to_string()),
            ChatTarget::Client(id) => ("1", id.to_string()),
        };
        self.send_command(
            "sendtextmessage",
            &[
                ("targetmode", target_mode.to_string()),
                ("target", target_id),
                ("msg", text.to_string()),
            ],
        )
        .await
    }

    /// Move ourselves to another channel.
    pub async fn join_channel(&mut self, channel_id: u64) -> Result<()> {
        let own = self.own_id().ok_or_else(|| anyhow!("not connected"))?;
        self.send_command(
            "clientmove",
            &[("clid", own.to_string()), ("cid", channel_id.to_string())],
        )
        .await
    }

    /// Join a channel by numeric id or by (case-insensitive) name.
    ///
    /// Returns the id that was actually joined, so callers can confirm it.
    pub async fn join_channel_by_name(&mut self, needle: &str) -> Result<u64> {
        let snapshot = self.snapshot()?;
        let channel_id = snapshot
            .find_channel(needle)
            .ok_or_else(|| anyhow!("channel `{needle}` not found"))?
            .id;
        self.join_channel(channel_id).await?;
        Ok(channel_id)
    }

    pub async fn set_nickname(&mut self, nickname: &str) -> Result<()> {
        self.send_command("clientupdate", &[("client_nickname", nickname.to_string())])
            .await
    }

    pub async fn set_muted(&mut self, input: bool, output: bool) -> Result<()> {
        self.send_command(
            "clientupdate",
            &[
                ("client_input_muted", bool_flag(input)),
                ("client_output_muted", bool_flag(output)),
            ],
        )
        .await
    }

    /// Voice frames handed to the transport so far (for the UI counters).
    pub fn audio_frames_sent(&self) -> u64 {
        self.audio_frames_sent
    }
    /// Send one encoded voice packet (see `ts6-audio` for the encoder).
    pub fn send_audio(&mut self, packet: OutPacket) -> Result<()> {
        self.con.send_audio(packet)?;
        Ok(())
    }

    /// Whether the server currently lets us transmit voice (not muted, talk power).
    pub fn can_send_audio(&self) -> bool {
        self.con.can_send_audio()
    }

    /// Whether we should play back incoming voice (output not muted).
    pub fn can_receive_audio(&self) -> bool {
        self.con.can_receive_audio()
    }

    /// Leave cleanly.
    pub async fn disconnect(mut self) -> Result<()> {
        self.con.disconnect(DisconnectOptions::new())?;

        // Drain until the server closes the stream, but NEVER wait forever: a peer
        // streaming audio into our channel keeps producing items, so an unbounded
        // drain hangs the process (measured: the CLI never exited while the UI was
        // transmitting a tone). Three seconds is enough for a clean shutdown.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match timeout_at(deadline, self.con.events().next()).await {
                Ok(None) => break,
                Ok(Some(Err(_))) => break,
                Ok(Some(Ok(_))) => {}
                Err(_) => {
                    tracing::debug!("disconnect drain timed out; closing anyway");
                    break;
                }
            }
        }
        Ok(())
    }
}

fn bool_flag(value: bool) -> String {
    if value {
        "1".to_string()
    } else {
        "0".to_string()
    }
}
