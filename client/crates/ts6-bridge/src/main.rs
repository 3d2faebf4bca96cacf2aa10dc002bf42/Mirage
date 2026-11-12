//! `ts6-bridge` — owns one TS6 session and exposes it to the desktop UI over a
//! local WebSocket.
//!
//! Why a bridge instead of driving the session from the UI process: the session is
//! single-threaded (the protocol library is `!Send`), the audio pipeline lives on
//! its own OS threads, and keeping them out of the UI process makes the same React
//! app usable inside Tauri later without rewriting the transport.
//!
//! Run:  cargo run -p ts6-bridge -- [--port 8787] [--ui <dir>]

mod audio;
mod probe;
mod protocol;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tower_http::services::ServeDir;

use protocol::{Command, Event};
use ts6_core::{Item, Session, Wake};

#[derive(Clone)]
struct App {
    port: u16,
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<String>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,ts6_bridge=debug")),
        )
        .init();

    let port: u16 = std::env::var("TS6_BRIDGE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .or_else(|| arg_value("--port").and_then(|value| value.parse().ok()))
        .unwrap_or(8787);
    let ui_dir = std::env::var("TS6_UI_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| arg_value("--ui").map(PathBuf::from))
        .unwrap_or_else(default_ui_dir);

    // Session-only diagnostic mode (docs/verification.md V-23).
    if let Some(address) = arg_value("--probe") {
        let options = probe::Options {
            address,
            seconds: arg_value("--seconds")
                .and_then(|value| value.parse().ok())
                .unwrap_or(20),
            tone: !has_flag("--no-tone"),
            commands: !has_flag("--no-commands"),
        };
        let runtime = if has_flag("--current-thread") {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
        } else {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?
        };
        return runtime.block_on(probe::run(options));
    }

    // The protocol session is not `Send`, so it stays on the thread that runs this
    // future (`Runtime::block_on`). Worker threads are still needed: the library
    // drives socket I/O from the poll of its event stream. Historical receive
    // delays had several confounding factors (docs/verification.md V-23/V-25);
    // they do not establish that a single-thread runtime caused the symptom.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move { serve(port, ui_dir).await })
}

fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next();
        }
        if let Some(rest) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Whether a bare flag is present (flags that take no value).
fn has_flag(flag: &str) -> bool {
    std::env::args().skip(1).any(|arg| arg == flag)
}
fn default_ui_dir() -> PathBuf {
    // <repo>/client/crates/ts6-bridge -> <repo>/client/apps/ui/dist
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .map(|client| client.join("apps").join("ui").join("dist"))
        .unwrap_or_else(|| PathBuf::from("apps/ui/dist"))
}

async fn serve(port: u16, ui_dir: PathBuf) -> anyhow::Result<()> {
    let (commands, command_rx) = mpsc::channel::<Command>(32);
    let (events, _) = broadcast::channel::<String>(256);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(App {
            port,
            commands,
            events: events.clone(),
        })
        .fallback_service(ServeDir::new(&ui_dir).append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    println!(
        "ts6-bridge: http://127.0.0.1:{port}  (ui: {})",
        ui_dir.display()
    );

    // The HTTP server runs as its own task; the protocol session stays on the thread
    // that runs this future (the connection is `!Send`). Keep the existing ownership
    // structure unless a continuously polled alternative is regression tested.
    // The earlier V-21 runtime diagnosis was superseded; see V-23 and V-25.
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    session_task(command_rx, events).await;
    match server.await {
        Ok(result) => result?,
        Err(error) => tracing::warn!(%error, "http server task failed"),
    }
    Ok(())
}

fn browser_allowed(headers: &HeaderMap, port: u16) -> bool {
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|host| hosts.iter().any(|allowed| host == allowed))
    {
        return false;
    }
    // Native diagnostic clients have no Origin. Local processes remain trusted.
    match headers.get("origin") {
        None => true,
        Some(origin) => origin.to_str().is_ok_and(|origin| {
            hosts.iter().any(|host| origin == format!("http://{host}"))
                || origin == "http://127.0.0.1:5174"
                || origin == "http://localhost:5174"
        }),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(app): State<App>, headers: HeaderMap) -> Response {
    if !browser_allowed(&headers, app.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| ws_session(socket, app))
}

async fn ws_session(socket: WebSocket, app: App) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = app.events.subscribe();

    // Greeting: version plus the device lists the UI needs for its settings panel.
    let hello = Event::Hello {
        version: env!("CARGO_PKG_VERSION"),
        devices: audio::device_lists(),
    };
    if let Ok(json) = serde_json::to_string(&hello) {
        if sender.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "ui fell behind on events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            message = receiver.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<Command>(text.as_str()) {
                        Ok(command) => {
                            tracing::debug!(?command, "ui command");
                            let _ = app.commands.send(command).await;
                        }
                        Err(error) => {
                            let log = Event::Log {
                                level: "error",
                                text: format!("invalid command: {error}"),
                                ts: protocol::timestamp(),
                            };
                            if let Ok(json) = serde_json::to_string(&log) {
                                let _ = sender.send(Message::Text(json.into())).await;
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    tracing::debug!(%error, "websocket error");
                    break;
                }
            },
        }
    }
}

fn log(events: &broadcast::Sender<String>, level: &str, text: impl Into<String>) {
    protocol::publish(
        events,
        &Event::Log {
            level,
            text: text.into(),
            ts: protocol::timestamp(),
        },
    );
}

fn publish_state(session: &mut Session, events: &broadcast::Sender<String>) {
    match session.snapshot() {
        Ok(snapshot) => protocol::publish(
            events,
            &Event::State {
                snapshot: &snapshot,
            },
        ),
        Err(error) => log(events, "error", format!("state unavailable: {error:#}")),
    }
}

fn publish_audio(
    engine: &mut audio::AudioEngine,
    events: &broadcast::Sender<String>,
    tx_frames: u64,
) {
    let counters = engine.counters();
    protocol::publish(
        events,
        &Event::Audio {
            rx_packets: counters.rx_packets,
            tx_frames,
            ptt_down: counters.ptt_down,
            speaking: counters.speaking,
            capture: counters.capture,
            playback: counters.playback,
            tone: counters.tone,
            ptt_binding: counters.ptt_binding,
        },
    );
}

async fn session_task(mut commands: mpsc::Receiver<Command>, events: broadcast::Sender<String>) {
    let mut session: Option<Session> = None;
    let mut engine = audio::AudioEngine::new();
    let mut config = audio::AudioConfig::default();
    // Diagnostic counter (docs/verification.md V-21): tells "packets never arrive"
    // apart from "packets arrive but are not surfaced".
    let mut audio_items: u64 = 0;
    // Heartbeat diagnostics: proves whether the loop is running while voice is
    // expected (docs/verification.md V-21).
    let mut polls: u64 = 0;
    let mut last_beat = std::time::Instant::now();

    loop {
        let mut pending: Option<Command> = None;
        let mut lost = false;

        match session.as_mut() {
            Some(active) => {
                let wake = futures::future::poll_fn(|cx| {
                    active.poll_with(Some(engine.mic_rx_mut()), Some(&mut commands), cx)
                })
                .await;
                polls += 1;
                if last_beat.elapsed() >= std::time::Duration::from_secs(2) {
                    tracing::debug!(
                        polls,
                        audio_items,
                        rx = engine.counters().rx_packets,
                        tx = active.audio_frames_sent(),
                        can_send = active.can_send_audio(),
                        can_recv = active.can_receive_audio(),
                        "session heartbeat"
                    );
                    last_beat = std::time::Instant::now();
                }

                match wake {
                    None => {
                        tracing::warn!(polls, "poll_with returned None: session over");
                        lost = true
                    }
                    Some(Wake::MicSent) => {} // frames already counted by the session
                    Some(Wake::Command(command)) => pending = Some(command),
                    Some(Wake::Item(item)) => match item {
                        Item::StateChanged => publish_state(active, &events),
                        Item::Audio(packet) => {
                            audio_items += 1;
                            if audio_items <= 3 || audio_items.is_multiple_of(100) {
                                tracing::info!(
                                    audio_items,
                                    "voice packet reached the bridge session task"
                                );
                            }
                            engine.on_audio(packet)
                        }
                        Item::TextMessage(message) => log(
                            &events,
                            "text",
                            format!(
                                "{}: {}",
                                message.invoker_name.as_deref().unwrap_or("?"),
                                message.message
                            ),
                        ),
                        Item::UnknownCommand { name, content } => {
                            tracing::debug!(%name, %content, "unknown server command");
                        }
                        Item::Reconnecting => log(&events, "warn", "reconnecting…"),
                        Item::IdentityLevel(level) => {
                            tracing::debug!(level, "identity level increasing")
                        }
                    },
                }
            }
            None => match commands.recv().await {
                Some(command) => pending = Some(command),
                None => break,
            },
        }

        if lost {
            protocol::publish(
                &events,
                &Event::Status {
                    connected: false,
                    address: None,
                    nickname: None,
                    error: Some("connection closed"),
                },
            );
            log(&events, "warn", "connection closed");
            session = None;
            engine.stop();
        }

        if let Some(command) = pending {
            apply_command(command, &mut session, &mut engine, &mut config, &events).await;
        }

        if engine.stats_due() {
            let tx_frames = session
                .as_ref()
                .map(|active| active.audio_frames_sent())
                .unwrap_or(0);
            publish_audio(&mut engine, &events, tx_frames);
            engine.mark_published();
        }
    }
}

async fn apply_command(
    command: Command,
    session: &mut Option<Session>,
    engine: &mut audio::AudioEngine,
    config: &mut audio::AudioConfig,
    events: &broadcast::Sender<String>,
) {
    match command {
        Command::Connect {
            address,
            nickname,
            channel,
            server_password,
        } => {
            if let Some(old) = session.take() {
                let _ = old.disconnect().await;
            }
            engine.stop();
            engine.reset_counters();

            let nickname = nickname.unwrap_or_else(|| "ts6client".to_string());
            let mut client_config = ts6_core::ClientConfig::new(&address, &nickname);
            if let Some(channel) = channel.filter(|value| !value.trim().is_empty()) {
                client_config = client_config.channel(channel);
            }
            if let Some(password) = server_password.filter(|value| !value.is_empty()) {
                client_config = client_config.server_password(password);
            }

            log(events, "info", format!("connecting to {address}…"));
            match Session::connect(client_config).await {
                Ok(connected) => {
                    protocol::publish(
                        events,
                        &Event::Status {
                            connected: true,
                            address: Some(&address),
                            nickname: Some(&nickname),
                            error: None,
                        },
                    );
                    log(
                        events,
                        "ok",
                        format!("connected to {address} as {nickname}"),
                    );
                    let mut connected = connected;
                    publish_state(&mut connected, events);
                    *session = Some(connected);
                }
                Err(error) => {
                    protocol::publish(
                        events,
                        &Event::Status {
                            connected: false,
                            address: None,
                            nickname: None,
                            error: Some(&format!("{error:#}")),
                        },
                    );
                    log(events, "error", format!("connection failed: {error:#}"));
                }
            }
        }

        Command::Disconnect => {
            if let Some(active) = session.take() {
                let _ = active.disconnect().await;
                log(events, "info", "disconnected");
            }
            engine.stop();
            protocol::publish(
                events,
                &Event::Status {
                    connected: false,
                    address: None,
                    nickname: None,
                    error: None,
                },
            );
        }

        Command::Join { channel } => match session.as_mut() {
            Some(active) => match active.join_channel_by_name(&channel).await {
                Ok(id) => {
                    log(events, "ok", format!("joined channel {channel} (#{id})"));
                    publish_state(active, events);
                }
                Err(error) => log(events, "error", format!("join failed: {error:#}")),
            },
            None => log(events, "warn", "not connected"),
        },

        Command::Nick { name } => match session.as_mut() {
            Some(active) => match active.set_nickname(&name).await {
                Ok(()) => {
                    log(events, "ok", format!("nickname changed to {name}"));
                    publish_state(active, events);
                }
                Err(error) => log(
                    events,
                    "error",
                    format!("nickname change failed: {error:#}"),
                ),
            },
            None => log(events, "warn", "not connected"),
        },

        Command::Mute { input, output } => match session.as_mut() {
            Some(active) => {
                let muted_input = input.unwrap_or(false);
                let muted_output = output.unwrap_or(false);
                match active.set_muted(muted_input, muted_output).await {
                    Ok(()) => log(
                        events,
                        "info",
                        format!("muted: input={muted_input} output={muted_output}"),
                    ),
                    Err(error) => log(events, "error", format!("mute change failed: {error:#}")),
                }
            }
            None => log(events, "warn", "not connected"),
        },

        Command::Audio {
            enabled,
            tone,
            ptt,
            input_device,
            output_device,
        } => {
            if let Some(enabled) = enabled {
                config.enabled = enabled;
            }
            if let Some(tone) = tone {
                config.tone = tone;
            }
            if let Some(ptt) = ptt {
                config.ptt = Some(ptt).filter(|key| !key.trim().is_empty());
            }
            if let Some(device) = input_device {
                config.input_device = Some(device).filter(|name| !name.is_empty());
            }
            if let Some(device) = output_device {
                config.output_device = Some(device).filter(|name| !name.is_empty());
            }
            engine.configure(config);
            log(
                events,
                "info",
                format!(
                    "audio: enabled={} tone={} ptt={}",
                    config.enabled,
                    engine.tone,
                    engine.ptt_binding.as_deref().unwrap_or("—")
                ),
            );
            let tx_frames = session
                .as_ref()
                .map(|active| active.audio_frames_sent())
                .unwrap_or(0);
            publish_audio(engine, events, tx_frames);
        }

        Command::Devices => protocol::publish(
            events,
            &Event::Devices {
                devices: audio::device_lists(),
            },
        ),

        Command::Refresh => match session.as_mut() {
            Some(active) => publish_state(active, events),
            None => log(events, "warn", "not connected"),
        },
    }
}

#[cfg(test)]
mod access_tests {
    use super::*;
    use axum::http::HeaderMap;

    fn headers(host: &str, origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", host.parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
        headers
    }

    #[test]
    fn websocket_access_rejects_cross_site_and_rebound_hosts() {
        for origin in [
            "https://evil.example",
            "null",
            "http://127.0.0.1:9999",
            "http://localhost:8787.evil.example",
        ] {
            assert!(!browser_allowed(
                &headers("127.0.0.1:8787", Some(origin)),
                8787
            ));
        }
        assert!(!browser_allowed(
            &headers("evil.example:8787", Some("http://evil.example:8787")),
            8787
        ));
        assert!(!browser_allowed(&HeaderMap::new(), 8787));
    }

    #[test]
    fn websocket_access_allows_local_ui_vite_and_native_probes() {
        for origin in [
            None,
            Some("http://127.0.0.1:8787"),
            Some("http://localhost:8787"),
            Some("http://localhost:5174"),
            Some("http://127.0.0.1:5174"),
        ] {
            assert!(browser_allowed(&headers("127.0.0.1:8787", origin), 8787));
        }
        assert!(browser_allowed(
            &headers("localhost:9000", Some("http://localhost:9000")),
            9000
        ));
    }
}
