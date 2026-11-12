//! Session-only probe: runs the exact loop the bridge uses, with none of the
//! bridge's extra machinery (HTTP server, WebSocket, audio engine, playback).
//!
//! Purpose: bisect bridge-only defects (docs/verification.md V-23). If this mode
//! receives voice but the full bridge does not, the cause is the HTTP/audio side;
//! if this mode also fails, the cause is in the session loop or the runtime.
//!
//! Usage: ts6-bridge --probe 127.0.0.1:9987 [--seconds 20] [--no-tone]
//!                     [--no-commands] [--current-thread]

use std::time::{Duration, Instant};

use futures::future::poll_fn;
use ts6_core::{ClientConfig, Item, Session, Wake};

pub struct Options {
    pub address: String,
    pub seconds: u64,
    pub tone: bool,
    pub commands: bool,
}

pub async fn run(options: Options) -> anyhow::Result<()> {
    let started = Instant::now();
    println!(
        "probe: connecting to {} (tone={}, commands={})",
        options.address, options.tone, options.commands
    );

    let mut session = Session::connect(ClientConfig::new(&options.address, "bridge-probe")).await?;
    println!(
        "probe: connected in {:.1}s (can_send={}, can_receive={})",
        started.elapsed().as_secs_f32(),
        session.can_send_audio(),
        session.can_receive_audio()
    );

    // Microphone channel: a synthetic tone unless disabled. The sender is kept
    // alive for the whole run so the channel is never seen as closed.
    let (mic_tx, mut mic_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (tone_stop, tone_thread) = if options.tone {
        match crate::audio::spawn_tone(mic_tx.clone()) {
            Some((stop, thread)) => (stop, Some(thread)),
            None => {
                println!("probe: could not start the tone producer");
                (None, None)
            }
        }
    } else {
        (None, None)
    };
    let _keep_sender_alive = mic_tx;

    // Idle command channel (the bridge always polls one).
    let (_command_tx, mut command_rx) = tokio::sync::mpsc::channel::<()>(8);

    let deadline = started + Duration::from_secs(options.seconds);
    let mut next_report = started + Duration::from_millis(500);
    let mut received = 0u64;
    let mut first_item: Option<Instant> = None;

    loop {
        if Instant::now() >= deadline {
            break;
        }

        let wake = if options.commands {
            poll_fn(|cx| session.poll_with(Some(&mut mic_rx), Some(&mut command_rx), cx)).await
        } else {
            poll_fn(|cx| session.poll_with(Some(&mut mic_rx), None, cx)).await
        };

        match wake {
            None => {
                println!(
                    "probe: session ended at t+{:.1}s",
                    started.elapsed().as_secs_f32()
                );
                break;
            }
            Some(Wake::Item(Item::Audio(_))) => {
                received += 1;
                first_item.get_or_insert_with(Instant::now);
            }
            Some(Wake::Item(_)) | Some(Wake::MicSent) | Some(Wake::Command(())) => {}
        }

        if Instant::now() >= next_report {
            let first = first_item
                .map(|at| format!("{:.1}s", at.duration_since(started).as_secs_f32()))
                .unwrap_or_else(|| "never".to_string());
            println!(
                "probe: t={:>5.1}s rx={received} tx={} first_rx={first}",
                started.elapsed().as_secs_f32(),
                session.audio_frames_sent()
            );
            next_report += Duration::from_millis(500);
        }
    }

    println!(
        "probe: finished rx={received} tx={} in {:.1}s",
        session.audio_frames_sent(),
        started.elapsed().as_secs_f32()
    );

    if let Some(stop) = tone_stop {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some(thread) = tone_thread {
        let _ = thread.join();
    }
    let _ = session.disconnect().await;
    Ok(())
}
