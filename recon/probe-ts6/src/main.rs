//! Interop probe: can an independent, open-source TS3-protocol client log into an
//! official TeamSpeak 6 server?
//!
//! Usage: probe-ts6 [address] [nickname]
//! Prints the full bookkeeping state the server pushed during the handshake.

use std::time::Duration;

use futures::prelude::*;
use tsclientlib::{Connection, DisconnectOptions, StreamItem};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9987".to_string());
    let nickname = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "ts6-probe".to_string());

    println!("== probe: connecting to {address} as {nickname} ==");

    let mut con = Connection::build(address.clone())
        .name(nickname)
        .log_commands(true)
        .connect()?;

    // Wait for the first bookkeeping event: that means the server accepted us and
    // pushed the initial state (initserver + channels + clients).
    let first = con
        .events()
        .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
        .next()
        .await;

    match first {
        Some(Ok(item)) => println!("== OK: handshake completed, bookkeeping is live ({item:?}) =="),
        Some(Err(e)) => {
            println!("== FAIL: error during handshake: {e:?} ==");
            return Err(e.into());
        }
        None => {
            println!("== FAIL: stream ended before any bookkeeping event ==");
            anyhow::bail!("no bookkeeping event");
        }
    }

    {
        let state = con.get_state()?;
        println!("\n--- server ---");
        println!("{:#?}", state.server);
        println!("\n--- own_client ---");
        println!("{:?}", state.own_client);
        println!("\n--- clients ({}) ---", state.clients.len());
        for (id, c) in &state.clients {
            println!("{id:?}");
            println!("{c:#?}\n");
        }
        println!("\n--- channels ({}) ---", state.channels.len());
        for (id, ch) in &state.channels {
            println!("{id:?}");
            println!("{ch:#?}\n");
        }
    }

    // Stay connected briefly so the server-side logs show a complete session.
    tokio::time::sleep(Duration::from_secs(2)).await;

    con.disconnect(DisconnectOptions::new())?;
    con.events().for_each(|_| future::ready(())).await;
    println!("== disconnected cleanly ==");

    Ok(())
}
