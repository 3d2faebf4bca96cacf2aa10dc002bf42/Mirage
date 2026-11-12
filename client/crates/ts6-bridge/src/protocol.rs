//! Wire protocol between the bridge and the desktop UI.
//!
//! Both directions are JSON objects with a discriminant: `op` for commands the UI
//! sends, `ev` for events the bridge pushes. Keeping it boring makes the UI simple
//! and the protocol easy to extend (chat, streams) without breaking old clients.

use serde::{Deserialize, Serialize};

/// Commands from the UI.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    Connect {
        address: String,
        #[serde(default)]
        nickname: Option<String>,
        #[serde(default)]
        channel: Option<String>,
        #[serde(default)]
        server_password: Option<String>,
    },
    Disconnect,
    Join {
        channel: String,
    },
    Nick {
        name: String,
    },
    Mute {
        #[serde(default)]
        input: Option<bool>,
        #[serde(default)]
        output: Option<bool>,
    },
    /// (Re)configure the local audio pipeline.
    Audio {
        #[serde(default)]
        enabled: Option<bool>,
        /// Transmit a synthetic tone instead of the microphone (no-mic machines).
        #[serde(default)]
        tone: Option<bool>,
        /// Push-to-talk key binding, e.g. "F13"; empty string disables it.
        #[serde(default)]
        ptt: Option<String>,
        #[serde(default)]
        input_device: Option<String>,
        #[serde(default)]
        output_device: Option<String>,
    },
    /// Ask for the device lists again (after plugging a headset, for example).
    Devices,
    /// Ask for the current snapshot again.
    Refresh,
}

// Log only the operation, never credential-bearing fields or user content.
impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Connect { .. } => "Connect",
            Self::Disconnect => "Disconnect",
            Self::Join { .. } => "Join",
            Self::Nick { .. } => "Nick",
            Self::Mute { .. } => "Mute",
            Self::Audio { .. } => "Audio",
            Self::Devices => "Devices",
            Self::Refresh => "Refresh",
        })
    }
}

/// Device description sent to the UI.
#[derive(Debug, Serialize, Clone)]
pub struct Device {
    pub name: String,
    pub is_default: bool,
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_format: String,
}

impl From<ts6_audio::DeviceInfo> for Device {
    fn from(info: ts6_audio::DeviceInfo) -> Self {
        Self {
            name: info.name,
            is_default: info.is_default,
            channels: info.channels,
            sample_rate: info.sample_rate,
            sample_format: info.sample_format,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeviceLists {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
}

/// Events pushed to the UI.
#[derive(Debug, Serialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Event<'a> {
    Hello {
        version: &'a str,
        devices: DeviceLists,
    },
    Status {
        connected: bool,
        address: Option<&'a str>,
        nickname: Option<&'a str>,
        error: Option<&'a str>,
    },
    State {
        snapshot: &'a ts6_core::Snapshot,
    },
    Log {
        level: &'a str,
        text: String,
        ts: String,
    },
    Audio {
        rx_packets: u64,
        tx_frames: u64,
        ptt_down: bool,
        speaking: Vec<u16>,
        capture: Option<String>,
        playback: Option<String>,
        tone: bool,
        ptt_binding: Option<String>,
    },
    Devices {
        devices: DeviceLists,
    },
}

/// Send an event to every connected UI.
pub fn publish(sender: &tokio::sync::broadcast::Sender<String>, event: &Event<'_>) {
    if let Ok(json) = serde_json::to_string(event) {
        let _ = sender.send(json);
    }
}

/// Current wall-clock time as `HH:MM:SS`, for the event console.
pub fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_of_day = now % 86_400;
    // The UI shows local time; the bridge only needs a stable, readable stamp.
    let offset = local_utc_offset_seconds();
    let local = (secs_of_day as i64 + offset).rem_euclid(86_400) as u64;
    format!(
        "{:02}:{:02}:{:02}",
        local / 3600,
        (local % 3600) / 60,
        local % 60
    )
}

#[cfg(windows)]
fn local_utc_offset_seconds() -> i64 {
    // Reading the Windows time-zone requires a syscall; the UI mostly needs a
    // monotonic-looking timestamp, so fall back to UTC when it is not available.
    // (Kept as a seam for a later `time`-crate based implementation.)
    0
}

#[cfg(not(windows))]
fn local_utc_offset_seconds() -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_diagnostics_never_include_passwords() {
        let command: Command = serde_json::from_str(
            r#"{"op":"connect","address":"example.org","server_password":"fixture-sensitive-value"}"#,
        ).unwrap();
        assert!(!format!("{command:?}").contains("fixture-sensitive-value"));
    }

    #[test]
    fn commands_parse_from_ui_json() {
        let connect: Command =
            serde_json::from_str(r#"{"op":"connect","address":"127.0.0.1:9987"}"#).unwrap();
        assert!(matches!(connect, Command::Connect { .. }));

        let join: Command = serde_json::from_str(r#"{"op":"join","channel":"3"}"#).unwrap();
        assert!(matches!(join, Command::Join { channel } if channel == "3"));

        let audio: Command =
            serde_json::from_str(r#"{"op":"audio","tone":true,"ptt":"F13"}"#).unwrap();
        match audio {
            Command::Audio { tone, ptt, .. } => {
                assert_eq!(tone, Some(true));
                assert_eq!(ptt.as_deref(), Some("F13"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn events_serialize_with_a_type_tag() {
        let json = serde_json::to_string(&Event::Status {
            connected: true,
            address: Some("127.0.0.1:9987"),
            nickname: Some("tester"),
            error: None,
        })
        .unwrap();
        assert!(json.contains(r#""ev":"status""#), "{json}");
        assert!(json.contains(r#""connected":true"#));
    }

    #[test]
    fn audio_event_serializes_speaking_clients() {
        let json = serde_json::to_string(&Event::Audio {
            rx_packets: 10,
            tx_frames: 0,
            ptt_down: false,
            speaking: vec![3, 5],
            capture: None,
            playback: Some("Speakers".into()),
            tone: false,
            ptt_binding: Some("F13".into()),
        })
        .unwrap();
        assert!(json.contains(r#""speaking":[3,5]"#), "{json}");
        assert!(json.contains(r#""ev":"audio""#));
    }
}
