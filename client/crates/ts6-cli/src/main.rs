//! `ts6-cli` — diagnostic harness for controlled interoperability and audio tests.
//! A successful offline build does not verify behavior against a real server.
//!
//! ```text
//! ts6-cli probe  <addr> [--seconds 3]
//! ts6-cli chat   <addr> <server|channel|@<clid>> <text...>
//! ts6-cli join   <addr> <channel-name-or-id>
//! ts6-cli nick   <addr> <nickname>
//! ts6-cli raw    <addr> <command> [key=value ...]
//! ts6-cli stream <addr> [--type 2] [--bitrate 2500] [--seconds 6]
//! ```
//!
//! Options (before or after the positional arguments):
//! `--name <nick>` `--identity <path>` `--channel <name>` `--channel-password <pwd>`
//! `--server-password <pwd>` `--verbose`

use std::time::Duration;

use anyhow::{bail, Context, Result};
use ts6_core::{ChatTarget, ClientConfig, Item, Session};

const HELP: &str = "\
ts6-cli — diagnostic client for the TeamSpeak 6 core

USAGE:
  ts6-cli <command> [args] [options]

COMMANDS:
  probe  <addr> [secs]                   connect, print state, stream events
  chat   <addr> <server|channel|@clid> <text...>
  join   <addr> <channel-name-or-id>
  nick   <addr> <nickname>
  raw    <addr> <command> [key=value ...]  send any TS command, print result
  stream <addr> [--type N] [--bitrate N] [--seconds N]
                                          TS6: setupstream + watch the notifies
  audio-send <addr> [seconds]             send a 440 Hz Opus tone to the channel
  audio-recv <addr> [seconds]             decode incoming voice and report RMS/peak
  audio-call <addr> [seconds]             full duplex with real mic/speakers
  devices                                 list input/output audio devices
  ptt-probe <key> [seconds]               watch a key and print its press/release state

OPTIONS:
  --name <nickname>         nickname to use (default: ts6client)
  --identity <path>         identity file (default: %APPDATA%\\ts6client\\identity.json)
  --channel <name>          channel to join on connect
  --channel-password <pwd>  channel password
  --server-password <pwd>   server password
  --verbose                 log every command and packet
  --local-ip <ip>           bind the UDP socket to this local IP (multi-client tests)
  --input <device>          microphone device name (default: system default)
  --output <device>         playback device name (default: system default)
  --tone                    replace the microphone with a quiet 440 Hz tone
  --ptt <key>               push-to-talk key (F13, ctrl, mouse4, 0x77); transmit only while held
  -h, --help                this help
";

#[derive(Debug, Default)]
struct Opts {
    name: Option<String>,
    identity: Option<String>,
    channel: Option<String>,
    channel_password: Option<String>,
    server_password: Option<String>,
    verbose: bool,
    seconds: Option<u64>,
    stream_type: Option<String>,
    bitrate: Option<String>,
    local_ip: Option<String>,
    input_device: Option<String>,
    output_device: Option<String>,
    tone: bool,
    ptt: Option<String>,
}

impl Opts {
    fn config(&self, addr: &str) -> ClientConfig {
        let mut cfg = ClientConfig::new(
            addr,
            self.name.clone().unwrap_or_else(|| "ts6client".into()),
        );
        if let Some(identity) = &self.identity {
            cfg = cfg.identity_file(identity);
        }
        if let Some(channel) = &self.channel {
            cfg = cfg.channel(channel);
        }
        if let Some(pwd) = &self.channel_password {
            cfg = cfg.channel_password(pwd);
        }
        if let Some(pwd) = &self.server_password {
            cfg = cfg.server_password(pwd);
        }
        if let Some(ip) = &self.local_ip {
            match ip.parse::<std::net::IpAddr>() {
                Ok(ip) => cfg = cfg.local_ip(ip),
                Err(err) => tracing::warn!(%ip, %err, "invalid --local-ip, ignoring"),
            }
        }
        cfg.log_commands(self.verbose)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_max_level(if argv.iter().any(|a| a == "--trace") {
            tracing::Level::TRACE
        } else if argv.iter().any(|a| a == "--verbose") {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_target(false)
        .init();

    let (positional, opts) = parse_args(&argv)?;
    let mut positional = positional.into_iter();
    let command = positional.next().context("missing command")?;

    if command == "devices" {
        print_devices();
        return Ok(());
    }
    if command == "ptt-probe" {
        let binding = positional.next().context("missing key (for example F13)")?;
        let seconds = positional
            .next()
            .and_then(|s| s.parse().ok())
            .or(opts.seconds)
            .unwrap_or(10);
        return ptt_probe(&binding, seconds);
    }

    let addr = positional.next().context("missing server address")?;
    let cfg = opts.config(&addr);
    let mut session = Session::connect(cfg).await?;

    // Run the command inside an async block so that an early `?`/`bail!` still
    // lets us close the connection cleanly: the server rejects a second connection
    // from the same identity ("ClientTooManyClonesConnected") until the previous
    // one is really gone.
    let outcome: Result<()> = async {
        match command.as_str() {
            "probe" => probe(&mut session, &mut positional, &opts).await?,
            "chat" => {
                let target = positional.next().context("missing chat target")?;
                let text: Vec<String> = positional.collect();
                if text.is_empty() {
                    bail!("missing message text");
                }
                let target = match target.as_str() {
                    "server" => ChatTarget::Server,
                    "channel" => ChatTarget::Channel,
                    other => match other.strip_prefix('@').map(str::parse::<u16>) {
                        Some(Ok(id)) => ChatTarget::Client(id),
                        _ => bail!("target must be `server`, `channel` or `@<client-id>`"),
                    },
                };
                session.send_chat(target, &text.join(" ")).await?;
                // The server echoes channel/server messages back to us: use that as
                // proof of delivery instead of trusting "no error".
                let needle = text.join(" ");
                let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
                let mut echoed = false;
                while let Ok(Some(item)) =
                    tokio::time::timeout_at(deadline, session.next_item()).await
                {
                    if let Item::TextMessage(msg) = item {
                        if msg.from_self && msg.message == needle {
                            echoed = true;
                            break;
                        }
                    }
                }
                if echoed {
                    println!("delivered (echo received)");
                } else {
                    println!("sent (no echo within 3s)");
                }
            }
            "join" => {
                let needle = positional.next().context("missing channel")?;
                let snapshot = session.snapshot()?;
                let channel = snapshot
                    .find_channel(&needle)
                    .with_context(|| format!("channel `{needle}` not found"))?;
                let (id, name) = (channel.id, channel.name.clone());
                println!("joining {name} (#{id})");
                session.join_channel(id).await?;

                // Verify we really moved: the book is authoritative.
                let mut moved = false;
                for _ in 0..25 {
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    if let Ok(snapshot) = session.snapshot() {
                        if snapshot
                            .clients
                            .iter()
                            .any(|c| c.is_self && c.channel_id == id)
                        {
                            moved = true;
                            break;
                        }
                    }
                }
                if !moved {
                    bail!("server accepted `clientmove` but we are still in another channel");
                }
                println!("now in {name} (#{id})");
            }
            "nick" => {
                let nick = positional.next().context("missing nickname")?;
                session.set_nickname(&nick).await?;
                println!("nickname set to {nick}");
            }
            "raw" => {
                let name = positional.next().context("missing command name")?;
                let mut owned: Vec<(String, String)> = Vec::new();
                for pair in positional {
                    let (key, value) = pair
                        .split_once('=')
                        .with_context(|| format!("argument `{pair}` is not key=value"))?;
                    owned.push((key.to_string(), value.to_string()));
                }
                let args: Vec<(&str, String)> =
                    owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                println!("sending `{name}` …");
                session.send_command(&name, &args).await?;
                println!("ok: server accepted `{name}`");
            }
            "stream" => stream(&mut session, &opts).await?,
            "audio-send" => {
                let seconds = positional
                    .next()
                    .and_then(|s| s.parse().ok())
                    .or(opts.seconds)
                    .unwrap_or(10);
                send_tone(&mut session, seconds).await?
            }
            "late-join-probe" => {
                let seconds = positional
                    .next()
                    .and_then(|s| s.parse().ok())
                    .or(opts.seconds)
                    .unwrap_or(10);
                let raw: Vec<String> = positional.collect();
                late_join_probe(&mut session, seconds, &raw).await?
            }
            "duplex-probe" => {
                let seconds = positional
                    .next()
                    .and_then(|s| s.parse().ok())
                    .or(opts.seconds)
                    .unwrap_or(30);
                duplex_probe(&mut session, &opts, seconds).await?
            }
            "audio-call" => {
                let seconds = positional
                    .next()
                    .and_then(|s| s.parse().ok())
                    .or(opts.seconds)
                    .unwrap_or(30);
                audio_call(&mut session, &opts, seconds).await?
            }
            "audio-recv" => {
                let seconds = positional
                    .next()
                    .and_then(|s| s.parse().ok())
                    .or(opts.seconds)
                    .unwrap_or(10);
                recv_meter(&mut session, seconds).await?
            }
            other => bail!("unknown command `{other}` (try --help)"),
        }
        Ok(())
    }
    .await;

    let closed = session.disconnect().await;
    outcome?;
    closed?;
    Ok(())
}

async fn probe(
    session: &mut Session,
    rest: &mut impl Iterator<Item = String>,
    opts: &Opts,
) -> Result<()> {
    let seconds = rest
        .next()
        .and_then(|s| s.parse().ok())
        .or(opts.seconds)
        .unwrap_or(3);
    let snapshot = session.snapshot()?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    println!("\n--- events for {seconds}s ---");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        let item = tokio::time::timeout_at(deadline, session.next_item()).await;
        match item {
            Err(_) => break,
            Ok(None) => {
                println!("stream closed");
                break;
            }
            Ok(Some(item)) => print_item(&item),
        }
    }
    Ok(())
}

async fn stream(session: &mut Session, opts: &Opts) -> Result<()> {
    let stream_type = opts.stream_type.clone().unwrap_or_else(|| "2".into());
    let bitrate = opts.bitrate.clone().unwrap_or_else(|| "2500".into());
    let seconds = opts.seconds.unwrap_or(6);

    println!("setupstream type={stream_type} bitrate={bitrate} mode=1 (P2P) …");
    session
        .send_command(
            "setupstream",
            &[
                ("name", "ts6-cli test stream".to_string()),
                ("type", stream_type),
                ("bitrate", bitrate),
                ("accessibility", "1".to_string()),
                ("mode", "1".to_string()),
                ("viewer_limit", "0".to_string()),
                ("audio", "0".to_string()),
            ],
        )
        .await?;
    println!("ok: server accepted `setupstream`; watching notifies for {seconds}s");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        match tokio::time::timeout_at(deadline, session.next_item()).await {
            Err(_) => break,
            Ok(None) => break,
            Ok(Some(item)) => print_item(&item),
        }
    }
    Ok(())
}

fn print_item(item: &Item) {
    match item {
        Item::StateChanged => println!("[state] bookkeeping changed"),
        Item::TextMessage(msg) => println!(
            "[chat] {} ({}): {}",
            msg.invoker_name.as_deref().unwrap_or("?"),
            msg.invoker_id
                .map(|i| i.to_string())
                .unwrap_or_else(|| "-".into()),
            msg.message
        ),
        Item::UnknownCommand { name, content } => {
            println!("[notify] {name}: {content}")
        }
        Item::IdentityLevel(level) => println!("[identity] level {level}"),
        Item::Reconnecting => println!("[link] reconnecting"),
        Item::Audio(_) => {}
    }
}

async fn send_tone(session: &mut Session, seconds: u64) -> Result<()> {
    use ts6_core::{AudioData, CodecType, OutAudio};

    let mut encoder = ts6_audio::VoiceEncoder::new(64_000)?;
    let mut tone = ts6_audio::ToneSource::default_tone();

    // Talk power/permissions settle a moment after joining.
    let mut allowed = false;
    for _ in 0..50 {
        if session.can_send_audio() {
            allowed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !allowed {
        tracing::warn!("server did not report send permission yet, sending anyway");
    }

    let sender_client_id = session.snapshot()?.own_client_id;
    println!("sending 440 Hz tone for {seconds}s (20 ms frames, Opus voip, 64 kbit/s; sender_client_id={sender_client_id})");
    let mut ticker = tokio::time::interval(Duration::from_millis(20));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let start = tokio::time::Instant::now();
    let (mut frames, mut bytes) = (0u64, 0u64);

    let deadline = start + Duration::from_secs(seconds);
    while tokio::time::Instant::now() < deadline {
        let next = futures::future::poll_fn(|cx| {
            use std::task::Poll;
            // Poll the connection on every wake, including timer wakes. Sending
            // alone only queues packets in the underlying protocol connection.
            let network_ready = match session.poll_with::<()>(None, None, cx) {
                Poll::Ready(None) => {
                    return Poll::Ready(Err(anyhow::anyhow!(
                        "connection closed during tone transmission"
                    )))
                }
                Poll::Ready(Some(_)) => true,
                Poll::Pending => false,
            };
            // A ready network event must not starve the frame timer.
            if ticker.poll_tick(cx).is_pending() {
                return if network_ready {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                };
            }
            let frame = tone.next_frame();
            let encoded = match encoder.encode(&frame) {
                Ok(encoded) => encoded,
                Err(error) => return Poll::Ready(Err(error)),
            };
            bytes += encoded.len() as u64;
            let packet = OutAudio::new(&AudioData::C2S {
                id: 0,
                codec: CodecType::OpusVoice,
                data: encoded,
            });
            if let Err(error) = session.send_audio(packet) {
                return Poll::Ready(Err(error));
            }
            frames += 1;
            // Register a wake after enqueueing so network progress is not
            // postponed until the next frame's timer.
            cx.waker().wake_by_ref();
            Poll::Ready(Ok(()))
        });
        match tokio::time::timeout_at(deadline, next).await {
            Ok(result) => result?,
            Err(_) => break,
        }
    }

    let kbps = if seconds > 0 {
        bytes as f64 * 8.0 / seconds as f64 / 1000.0
    } else {
        0.0
    };
    println!("sent {frames} frames / {bytes} bytes (~{kbps:.1} kbit/s)");
    Ok(())
}

/// Receive voice, decode it and report the measured level. This is the automated
/// proof that the voice path works end to end (transport + Opus payload).
///
/// Decoding is done per packet instead of through the jitter buffer: the buffer is
/// paced by a real playback clock, which this diagnostic does not have.
async fn recv_meter(session: &mut Session, seconds: u64) -> Result<()> {
    use ts6_audio::{codec::PeerDecoders, RmsMeter};
    use ts6_core::AudioData;

    let mut decoders = PeerDecoders::default();
    let mut meter = RmsMeter::new();
    let mut packets = 0u64;
    let mut senders: std::collections::BTreeSet<u16> = Default::default();

    println!(
        "listening for voice for {seconds}s (can_send_audio={}, can_receive_audio={})",
        session.can_send_audio(),
        session.can_receive_audio()
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut next_report = tokio::time::Instant::now() + Duration::from_millis(500);

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }

        // Wait for the next event with a LONG timeout on purpose. Polling the
        // events stream with short timeouts starves the audio path: the library
        // hands voice packets over only while the stream is being polled, so
        // dropping/recreating the future every few hundred ms defers them.
        // Measured: 200 ms polling delivered all 260 packets only at the end;
        // long polling delivered them live.
        let wait = deadline - now;
        match tokio::time::timeout(wait, session.next_item()).await {
            Ok(Some(Item::Audio(packet))) => {
                let (from, payload) = match packet.data().data() {
                    AudioData::S2C { from, data, .. } => (Some(*from), *data),
                    AudioData::S2CWhisper { from, data, .. } => (Some(*from), *data),
                    _ => (None, &[][..]),
                };
                if let Some(from) = from {
                    senders.insert(from);
                    match decoders.decode(from, payload) {
                        Ok(pcm) => {
                            packets += 1;
                            meter.push(pcm);
                        }
                        Err(error) => tracing::debug!(%error, "failed to decode a voice packet"),
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }

        if tokio::time::Instant::now() >= next_report {
            let (rms, peak, samples) = meter.take();
            println!(
                "[audio] packets={packets} rms={rms:.4} peak={peak:.4} samples={samples} senders={senders:?}"
            );
            next_report = tokio::time::Instant::now() + Duration::from_millis(500);
        }
    }

    let (rms, peak, samples) = meter.take();
    println!(
        "received {packets} voice packets from {senders:?}; last-window rms={rms:.4} peak={peak:.4} samples={samples}"
    );

    Ok(())
}

/// Diagnostic: watch a push-to-talk key and print every state change.
fn ptt_probe(binding: &str, seconds: u64) -> Result<()> {
    use std::sync::atomic::Ordering;

    let vk = ts6_input::parse_key(binding)?;
    let monitor =
        ts6_input::PttMonitor::start(ts6_input::SystemKeys, vk, Duration::from_millis(10));
    let pressed = monitor.pressed();

    println!("watching `{binding}` (VK 0x{vk:02X}) for {seconds}s — press and release it");
    let started = std::time::Instant::now();
    let mut last = pressed.load(Ordering::Relaxed);
    println!("pressed={last}");

    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(10));
        let now = pressed.load(Ordering::Relaxed);
        if now != last {
            println!("pressed={now} (t+{:.2}s)", started.elapsed().as_secs_f32());
            last = now;
        }
    }

    monitor.stop();
    println!("done");
    Ok(())
}

fn print_devices() {
    println!("Input devices:");
    let inputs = ts6_audio::devices::inputs();
    if inputs.is_empty() {
        println!("  (none)");
    }
    for device in &inputs {
        println!("{device}");
    }

    println!("\nOutput devices:");
    let outputs = ts6_audio::devices::outputs();
    if outputs.is_empty() {
        println!("  (none)");
    }
    for device in &outputs {
        println!("{device}");
    }
}

/// Full duplex: microphone -> Opus -> server, and server -> jitter buffer ->
/// speakers, in one coherent poll loop.
async fn audio_call(session: &mut Session, opts: &Opts, seconds: u64) -> Result<()> {
    use futures::future::poll_fn;
    use ts6_audio::mixer::sender_of;
    use ts6_core::Wake;

    // Playback is optional: a machine may have no output device.
    let playback = match ts6_audio::start_playback(opts.output_device.clone()) {
        Ok(playback) => {
            println!(
                "playback: {} ({} ch @ {} Hz)",
                playback.device_name, playback.channels, playback.sample_rate
            );
            Some(playback)
        }
        Err(error) => {
            println!("playback unavailable: {error}");
            None
        }
    };

    let (mic_tx, mut mic_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    // Optional push-to-talk: capture feeds a gate, and only the gate feeds the
    // session, so frames produced with the key up never reach the encoder queue.
    let mut ptt_monitor = None;
    let mut gate = None;
    let feed_tx = match &opts.ptt {
        Some(binding) => {
            let (capture_tx, capture_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
            let monitor = ts6_input::bind_ptt(binding)?;
            let vk = ts6_input::parse_key(binding)?;
            let gated = ts6_input::Gate::start(capture_rx, mic_tx, monitor.pressed());
            println!("push-to-talk: `{binding}` (VK 0x{vk:02X}) — transmitting only while held");
            ptt_monitor = Some(monitor);
            gate = Some(gated);
            capture_tx
        }
        None => mic_tx,
    };

    // Transmission source: a synthetic tone, or the real microphone.
    let mut capture = None;
    let mut tone_producer = None;
    if opts.tone {
        tone_producer = Some(spawn_tone_producer(feed_tx, seconds)?);
        println!("capture: synthetic 440 Hz tone at amplitude 0.1 (--tone)");
    } else {
        match ts6_audio::start_capture(
            ts6_audio::CaptureConfig {
                device: opts.input_device.clone(),
                bitrate: 64_000,
            },
            feed_tx,
        ) {
            Ok(started) => {
                println!(
                    "capture: {} ({} ch @ {} Hz)",
                    started.device_name, started.channels, started.sample_rate
                );
                capture = Some(started);
            }
            Err(error) => println!("capture unavailable: {error}"),
        }
    }
    println!("full duplex for {seconds}s");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut next_report = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut received = 0u64;

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }

        // `timeout_at` only fires at the very end, so it cannot disturb the audio
        // path mid-call; it exists so the loop cannot hang when both the microphone
        // channel is closed and the network is idle (no waker would ever fire).
        let wake = match tokio::time::timeout_at(
            deadline,
            poll_fn(|cx| session.poll_with_mic(&mut mic_rx, cx)),
        )
        .await
        {
            Ok(wake) => wake,
            Err(_) => break,
        };
        match wake {
            None => break,
            Some(Wake::Item(Item::Audio(packet))) => {
                if let Some(from) = sender_of(&packet) {
                    received += 1;
                    if let Some(playback) = &playback {
                        playback.enqueue(from, packet);
                    }
                }
            }
            Some(Wake::Item(_)) => {}
            Some(Wake::MicSent) => {}
            Some(Wake::Command(())) => {}
        }

        if tokio::time::Instant::now() >= next_report {
            let sent = session.audio_frames_sent();
            let (encoded, capture_dropped) = capture
                .as_ref()
                .map(|c| (c.frames_encoded(), c.frames_dropped()))
                .unwrap_or((0, 0));
            let (played, playback_dropped) = playback
                .as_ref()
                .map(|p| (p.frames_played(), p.packets_dropped()))
                .unwrap_or((0, 0));
            let (gated, gated_suppressed) = gate
                .as_ref()
                .map(|g| (g.forwarded(), g.suppressed()))
                .unwrap_or((0, 0));
            let ptt_state = ptt_monitor
                .as_ref()
                .map(|monitor| {
                    if monitor.pressed().load(std::sync::atomic::Ordering::Relaxed) {
                        "PTT=down"
                    } else {
                        "PTT=up"
                    }
                })
                .unwrap_or("PTT=off");
            println!(
                "[call] {ptt_state} tx_frames={sent} gate={gated} (suppressed={gated_suppressed}) capture_encoded={encoded} (dropped={capture_dropped}) rx_packets={received} played_frames={played} (dropped={playback_dropped})"
            );
            next_report = tokio::time::Instant::now() + Duration::from_secs(1);
        }
    }

    let sent = session.audio_frames_sent();
    let (encoded, capture_dropped) = capture
        .as_ref()
        .map(|c| (c.frames_encoded(), c.frames_dropped()))
        .unwrap_or((0, 0));
    let played = playback.as_ref().map(|p| p.frames_played()).unwrap_or(0);
    println!(
        "call finished: tx_frames={sent} capture_encoded={encoded} capture_dropped={capture_dropped} rx_packets={received} played_frames={played}"
    );

    if let Some(gate) = gate {
        println!(
            "push-to-talk gate: forwarded={} suppressed={}",
            gate.forwarded(),
            gate.suppressed()
        );
        gate.stop();
    }
    if let Some(monitor) = ptt_monitor {
        monitor.stop();
    }
    if let Some(capture) = capture {
        capture.stop();
    }
    if let Some(playback) = playback {
        playback.stop();
    }
    if let Some(producer) = tone_producer {
        let _ = producer.join();
    }
    Ok(())
}

/// Feed encoded tone frames into the microphone channel, at real-time pace.
/// Reproduces the bridge's poll shape: `poll_with` with an idle command channel.
/// Isolates defects that depend on the caller's event loop (docs/verification.md V-21).
/// Joins a channel where someone is ALREADY speaking and measures whether voice
/// arrives. Optionally sends extra commands after joining, to discover what a
/// late joiner must do to start receiving (server beta13.1 does not forward voice
/// to late joiners by default: docs/verification.md V-25).
async fn late_join_probe(session: &mut Session, seconds: u64, extra: &[String]) -> Result<()> {
    use ts6_core::{AudioData, Item};

    println!(
        "late-join probe: listening for {seconds}s (extra commands: {})",
        if extra.is_empty() {
            "none".to_string()
        } else {
            extra.join(", ")
        }
    );

    let started = std::time::Instant::now();
    let mut next_report = Duration::from_millis(500);
    let mut received = 0u64;
    let mut next_extra = std::time::Duration::from_millis(1500);

    loop {
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_secs(seconds) {
            break;
        }

        // Send the candidate commands once, a moment after joining.
        while !extra.is_empty() && elapsed >= next_extra {
            for raw in extra {
                let mut parts = raw.split_whitespace();
                let Some(name) = parts.next() else { continue };
                let owned: Vec<(String, String)> = parts
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let args: Vec<(&str, String)> =
                    owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                match session.send_command(name, &args).await {
                    Ok(()) => println!("  sent `{raw}`"),
                    Err(error) => println!("  failed `{raw}`: {error:#}"),
                }
            }
            next_extra = Duration::from_secs(3600); // only once
        }

        let wait = Duration::from_millis(500).min(Duration::from_secs(seconds) - elapsed);
        match tokio::time::timeout(wait, session.next_item()).await {
            Ok(Some(Item::Audio(packet))) => {
                if let AudioData::S2C { .. } | AudioData::S2CWhisper { .. } = packet.data().data() {
                    received += 1;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }

        if started.elapsed() >= next_report {
            println!(
                "  t={:>4.1}s rx={received}",
                started.elapsed().as_secs_f32()
            );
            next_report += Duration::from_millis(1000);
        }
    }

    println!("late-join probe finished: rx={received}");
    Ok(())
}
async fn duplex_probe(session: &mut Session, _opts: &Opts, seconds: u64) -> Result<()> {
    use futures::future::poll_fn;
    use ts6_core::Wake;

    let (mic_tx, mut mic_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let _tone = spawn_tone_producer(mic_tx, seconds)?;
    // Exactly like the bridge: a command channel that stays open but silent.
    let (_command_tx, mut command_rx) = tokio::sync::mpsc::channel::<()>(8);

    let started = std::time::Instant::now();
    let mut received = 0u64;
    let mut next_report = Duration::from_millis(500);
    while started.elapsed() < Duration::from_secs(seconds) {
        let wake =
            poll_fn(|cx| session.poll_with(Some(&mut mic_rx), Some(&mut command_rx), cx)).await;
        match wake {
            None => break,
            Some(Wake::Item(Item::Audio(_))) => received += 1,
            Some(Wake::Item(_)) => {}
            Some(Wake::MicSent) => {}
            Some(Wake::Command(())) => {}
        }
        if started.elapsed() >= next_report {
            println!(
                "[probe] t={:>4.1}s rx={} tx={}",
                started.elapsed().as_secs_f32(),
                received,
                session.audio_frames_sent()
            );
            next_report += Duration::from_millis(500);
        }
    }
    println!(
        "probe finished: rx={} tx={} in {:.1}s",
        received,
        session.audio_frames_sent(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}
fn spawn_tone_producer(
    sink: tokio::sync::mpsc::Sender<Vec<u8>>,
    seconds: u64,
) -> Result<std::thread::JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("ts6-tone".into())
        .spawn(move || {
            let mut encoder = match ts6_audio::VoiceEncoder::new(64_000) {
                Ok(encoder) => encoder,
                Err(error) => {
                    tracing::error!(%error, "cannot create the tone encoder");
                    return;
                }
            };
            // Quiet on purpose: this is a test signal, not a siren.
            let mut tone = ts6_audio::ToneSource::new(440.0, 0.1);
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(seconds) {
                let frame = tone.next_frame();
                match encoder.encode(&frame) {
                    Ok(encoded) => {
                        let _ = sink.try_send(encoded.to_vec());
                    }
                    Err(error) => tracing::warn!(%error, "tone encode failed"),
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        })?;
    Ok(handle)
}

fn parse_args(argv: &[String]) -> Result<(Vec<String>, Opts)> {
    let mut positional = Vec::new();
    let mut opts = Opts::default();
    let mut i = 0;

    // Reads the value of an option and advances past it.
    fn value(argv: &[String], i: &mut usize, flag: &str) -> Result<String> {
        *i += 1;
        argv.get(*i)
            .cloned()
            .with_context(|| format!("{flag} requires a value"))
    }

    while i < argv.len() {
        match argv[i].as_str() {
            "--name" => opts.name = Some(value(argv, &mut i, "--name")?),
            "--identity" => opts.identity = Some(value(argv, &mut i, "--identity")?),
            "--channel" => opts.channel = Some(value(argv, &mut i, "--channel")?),
            "--channel-password" => {
                opts.channel_password = Some(value(argv, &mut i, "--channel-password")?)
            }
            "--server-password" => {
                opts.server_password = Some(value(argv, &mut i, "--server-password")?)
            }
            "--seconds" => {
                opts.seconds = Some(
                    value(argv, &mut i, "--seconds")?
                        .parse()
                        .context("--seconds must be a number")?,
                )
            }
            "--type" => opts.stream_type = Some(value(argv, &mut i, "--type")?),
            "--bitrate" => opts.bitrate = Some(value(argv, &mut i, "--bitrate")?),
            "--local-ip" => opts.local_ip = Some(value(argv, &mut i, "--local-ip")?),
            "--input" => opts.input_device = Some(value(argv, &mut i, "--input")?),
            "--output" => opts.output_device = Some(value(argv, &mut i, "--output")?),
            "--tone" => opts.tone = true,
            "--ptt" => opts.ptt = Some(value(argv, &mut i, "--ptt")?),
            "--verbose" => opts.verbose = true,
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    Ok((positional, opts))
}
