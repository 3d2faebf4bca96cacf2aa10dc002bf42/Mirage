//! The transmit gate: captured frames reach the network only while push-to-talk
//! is engaged.
//!
//! A dedicated thread sits between capture and the session's microphone channel so
//! the network loop does not need to know about key state, and so frames produced
//! while the key is up are dropped *before* they can fill the queue.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Gate between capture and the session.
pub struct Gate {
    forwarded: Arc<AtomicU64>,
    suppressed: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Gate {
    /// Forward frames from `source` to `sink` while `pressed` is true.
    pub fn start(
        source: tokio::sync::mpsc::Receiver<Vec<u8>>,
        sink: tokio::sync::mpsc::Sender<Vec<u8>>,
        pressed: Arc<AtomicBool>,
    ) -> Self {
        let forwarded = Arc::new(AtomicU64::new(0));
        let suppressed = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let forwarded = forwarded.clone();
            let suppressed = suppressed.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("ts6-ptt-gate".into())
                .spawn(move || gate_loop(source, sink, pressed, stop, forwarded, suppressed))
                .ok()
        };

        Self {
            forwarded,
            suppressed,
            stop,
            thread,
        }
    }

    pub fn forwarded(&self) -> u64 {
        self.forwarded.load(Ordering::Relaxed)
    }

    pub fn suppressed(&self) -> u64 {
        self.suppressed.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn gate_loop(
    mut source: tokio::sync::mpsc::Receiver<Vec<u8>>,
    sink: tokio::sync::mpsc::Sender<Vec<u8>>,
    pressed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    forwarded: Arc<AtomicU64>,
    suppressed: Arc<AtomicU64>,
) {
    while !stop.load(Ordering::Relaxed) {
        // Do not block on a live but idle sender: stop() must be able to join.
        let frame = match source.try_recv() {
            Ok(frame) => frame,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
        };

        if pressed.load(Ordering::Relaxed) {
            match sink.try_send(frame) {
                Ok(()) => {
                    forwarded.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    suppressed.fetch_add(1, Ordering::Relaxed);
                }
            }
        } else {
            suppressed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wait_for(condition: impl Fn() -> bool, label: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for {label}");
    }

    #[test]
    fn stopping_an_idle_gate_does_not_require_closing_the_source() {
        let (source_tx, source_rx) = tokio::sync::mpsc::channel(8);
        let (sink_tx, _sink_rx) = tokio::sync::mpsc::channel(8);
        let gate = Gate::start(source_rx, sink_tx, Arc::new(AtomicBool::new(false)));
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            gate.stop();
            let _ = done_tx.send(());
        });
        let stopped = done_rx.recv_timeout(Duration::from_millis(500)).is_ok();
        drop(source_tx);
        assert!(stopped, "idle gate shutdown blocked on an open source");
    }

    #[test]
    fn gate_drops_frames_while_the_key_is_up_and_forwards_while_down() {
        let (source_tx, source_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
        let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
        let pressed = Arc::new(AtomicBool::new(false));

        let gate = Gate::start(source_rx, sink_tx, pressed.clone());

        // Released: the frame must never reach the network.
        source_tx.try_send(vec![1, 2, 3]).unwrap();
        wait_for(|| gate.suppressed() == 1, "the suppressed frame");
        assert!(sink_rx.try_recv().is_err(), "frame leaked while released");
        assert_eq!(gate.forwarded(), 0);

        // Pressed: frames flow through unchanged.
        pressed.store(true, Ordering::Relaxed);
        source_tx.try_send(vec![4, 5, 6]).unwrap();
        wait_for(|| gate.forwarded() == 1, "the forwarded frame");
        assert_eq!(sink_rx.try_recv().unwrap(), vec![4, 5, 6]);

        // Releasing again stops transmission.
        pressed.store(false, Ordering::Relaxed);
        source_tx.try_send(vec![7]).unwrap();
        wait_for(|| gate.suppressed() == 2, "the second suppressed frame");

        drop(source_tx);
        gate.stop();
    }
}
