//! `ts6-input` — push-to-talk input handling.
//!
//! * [`keys`] parses bindings (`F13`, `ctrl`, `mouse4`, `0x77`).
//! * [`poller`] watches the physical key state (`GetAsyncKeyState`, so it works
//!   regardless of window focus and reports releases, which `RegisterHotKey`
//!   does not).
//! * [`gate`] lets captured audio through only while the key is held.
//!
//! The Windows-specific part is a single syscall behind [`poller::KeyState`]; the
//! poll loop and the gate are platform independent and unit-tested.

pub mod gate;
pub mod keys;
pub mod poller;

pub use gate::Gate;
pub use keys::parse_key;
pub use poller::{KeyState, PttMonitor};

#[cfg(windows)]
pub use poller::SystemKeys;

/// Convenience: bind a key name and start watching it.
#[cfg(windows)]
pub fn bind_ptt(binding: &str) -> anyhow::Result<PttMonitor> {
    let vk = keys::parse_key(binding)?;
    Ok(PttMonitor::start(
        SystemKeys,
        vk,
        std::time::Duration::from_millis(10),
    ))
}
