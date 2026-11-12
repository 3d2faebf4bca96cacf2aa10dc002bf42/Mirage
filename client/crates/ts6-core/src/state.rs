//! Snapshot of the server state we keep locally.
//!
//! `tsclientlib` already maintains the authoritative book; this module projects it
//! into owned DTOs so the UI layer never borrows the connection.

use serde::Serialize;
use tsclientlib::data;

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub own_client_id: u16,
    pub own_uid: Option<String>,
    pub server: ServerInfo,
    pub channels: Vec<ChannelInfo>,
    pub clients: Vec<ClientInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub max_clients: u16,
    pub welcome_message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelInfo {
    pub id: u64,
    pub parent: u64,
    pub name: String,
    pub topic: Option<String>,
    pub codec: String,
    pub clients: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientInfo {
    pub id: u16,
    pub channel_id: u64,
    pub name: String,
    pub uid: Option<String>,
    pub input_muted: bool,
    pub output_muted: bool,
    pub is_self: bool,
}

impl Snapshot {
    pub fn from_state(state: &data::Connection) -> Self {
        let own_client_id = state.own_client.0;

        let clients: Vec<ClientInfo> = state
            .clients
            .values()
            .map(|c| ClientInfo {
                id: c.id.0,
                channel_id: c.channel.0,
                name: c.name.clone(),
                uid: c.uid.as_ref().map(|u| format!("{}", &**u)),
                input_muted: c.input_muted,
                output_muted: c.output_muted,
                is_self: c.id.0 == own_client_id,
            })
            .collect();

        let mut channels: Vec<ChannelInfo> = state
            .channels
            .values()
            .map(|ch| ChannelInfo {
                id: ch.id.0,
                parent: ch.parent.0,
                name: ch.name.clone(),
                topic: ch.topic.clone(),
                codec: format!("{:?}", ch.codec),
                clients: clients.iter().filter(|c| c.channel_id == ch.id.0).count(),
            })
            .collect();
        channels.sort_by_key(|c| c.id);
        let mut clients = clients;
        clients.sort_by_key(|c| c.id);

        Snapshot {
            own_client_id,
            own_uid: clients
                .iter()
                .find(|c| c.is_self)
                .and_then(|c| c.uid.clone()),
            server: ServerInfo {
                name: state.server.name.clone(),
                version: state.server.version.clone(),
                platform: state.server.platform.clone(),
                max_clients: state.server.max_clients,
                welcome_message: state.server.welcome_message.clone(),
            },
            channels,
            clients,
        }
    }

    /// Find a channel by numeric id or by (case-insensitive) name.
    pub fn find_channel(&self, needle: &str) -> Option<&ChannelInfo> {
        if let Ok(id) = needle.parse::<u64>() {
            if let Some(ch) = self.channels.iter().find(|c| c.id == id) {
                return Some(ch);
            }
        }
        self.channels
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(needle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(id: u64, name: &str) -> ChannelInfo {
        ChannelInfo {
            id,
            parent: 0,
            name: name.to_string(),
            topic: None,
            codec: "OpusVoice".to_string(),
            clients: 0,
        }
    }

    fn snapshot(channels: Vec<ChannelInfo>) -> Snapshot {
        Snapshot {
            own_client_id: 1,
            own_uid: None,
            server: ServerInfo {
                name: "server".into(),
                version: "6.0.0-beta13.1".into(),
                platform: "Windows".into(),
                max_clients: 32,
                welcome_message: String::new(),
            },
            channels,
            clients: Vec::new(),
        }
    }

    #[test]
    fn finds_channel_by_id_and_by_name_case_insensitively() {
        let s = snapshot(vec![
            channel(1, "Default Channel"),
            channel(3, "Permanent Channel"),
        ]);

        assert_eq!(
            s.find_channel("3").map(|c| c.name.as_str()),
            Some("Permanent Channel")
        );
        assert_eq!(s.find_channel("permanent channel").map(|c| c.id), Some(3));
        assert_eq!(s.find_channel("DEFAULT CHANNEL").map(|c| c.id), Some(1));
        assert!(s.find_channel("does-not-exist").is_none());
    }

    #[test]
    fn numeric_id_wins_over_a_channel_named_like_a_number() {
        let s = snapshot(vec![channel(1, "Default Channel"), channel(7, "7")]);

        // "7" parses as an id, so the channel with id 7 is returned even though
        // another channel is literally named "7".
        assert_eq!(s.find_channel("7").map(|c| c.id), Some(7));
    }
}
