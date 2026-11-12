//! Polling of the asynchronous key state.
//!
//! Push-to-talk needs both edges: when the key goes down we transmit, when it is
//! released we stop. Windows' `RegisterHotKey` only reports presses, so the state
//! is polled instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Source of "is this virtual key currently down?".
///
/// Behind a trait so the poll loop is testable without touching real hardware.
pub trait KeyState: Send + 'static {
    fn is_down(&self, vk: u16) -> bool;
}

/// Reads the physical key state from Windows, independent of window focus.
#[cfg(windows)]
pub struct SystemKeys;

#[cfg(windows)]
impl KeyState for SystemKeys {
    fn is_down(&self, vk: u16) -> bool {
        // GetAsyncKeyState sets the high bit while the key is down. The low bit
        // ("pressed since last call") is intentionally ignored.
        let state = unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) as u16
        };
        state & 0x8000 != 0
    }
}

/// A push-to-talk key being watched in the background.
pub struct PttMonitor {
    pressed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PttMonitor {
    /// Watch `vk`, updating the flag at most every `poll` interval.
    pub fn start<K: KeyState>(keys: K, vk: u16, poll: Duration) -> Self {
        let pressed = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let pressed = pressed.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("ts6-ptt".into())
                .spawn(move || {
                    poll_loop(keys, vk, poll, pressed, stop, |down| {
                        tracing::debug!(down, "push-to-talk state changed");
                    });
                })
                .ok()
        };

        Self {
            pressed,
            stop,
            thread,
        }
    }

    /// Shared flag: `true` while the key is held.
    pub fn pressed(&self) -> Arc<AtomicBool> {
        self.pressed.clone()
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The poll loop, factored out so tests can drive it with a fake key source.
pub fn poll_loop<K: KeyState>(
    keys: K,
    vk: u16,
    poll: Duration,
    pressed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    mut on_change: impl FnMut(bool),
) {
    // Publish the initial state immediately instead of waiting one interval.
    let mut last = keys.is_down(vk);
    pressed.store(last, Ordering::Relaxed);
    on_change(last);

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(poll);
        let down = keys.is_down(vk);
        if down != last {
            pressed.store(down, Ordering::Relaxed);
            last = down;
            on_change(down);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct FakeKeys {
        down: Arc<AtomicBool>,
    }

    impl KeyState for FakeKeys {
        fn is_down(&self, _vk: u16) -> bool {
            self.down.load(Ordering::Relaxed)
        }
    }

    #[test]
    fn poll_loop_reports_both_edges() {
        let down = Arc::new(AtomicBool::new(false));
        let pressed = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<bool>();

        let handle = {
            let keys = FakeKeys { down: down.clone() };
            let pressed = pressed.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                poll_loop(
                    keys,
                    0x7C,
                    Duration::from_millis(5),
                    pressed,
                    stop,
                    move |state| {
                        let _ = tx.send(state);
                    },
                )
            })
        };

        // Initial state is published right away.
        assert!(!rx.recv_timeout(Duration::from_secs(2)).unwrap());

        down.store(true, Ordering::Relaxed);
        assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(pressed.load(Ordering::Relaxed));

        down.store(false, Ordering::Relaxed);
        assert!(!rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(!pressed.load(Ordering::Relaxed));

        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }
}
