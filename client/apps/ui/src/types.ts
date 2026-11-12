/// Wire types shared with `crates/ts6-bridge/src/protocol.rs`.
/// Keep the field names identical: the bridge serializes with serde defaults.

export type Device = {
  name: string;
  is_default: boolean;
  channels: number;
  sample_rate: number;
  sample_format: string;
};

export type DeviceLists = { inputs: Device[]; outputs: Device[] };

export type ChannelInfo = {
  id: number;
  parent: number;
  name: string;
  topic: string | null;
  codec: string;
  clients: number;
};

export type ClientInfo = {
  id: number;
  channel_id: number;
  name: string;
  uid: string | null;
  input_muted: boolean;
  output_muted: boolean;
  is_self: boolean;
};

export type ServerInfo = {
  name: string;
  version: string;
  platform: string;
  max_clients: number;
  welcome_message: string;
};

export type Snapshot = {
  own_client_id: number;
  own_uid: string | null;
  server: ServerInfo;
  channels: ChannelInfo[];
  clients: ClientInfo[];
};

export type StatusEvent = {
  connected: boolean;
  address: string | null;
  nickname: string | null;
  error: string | null;
};

export type AudioEvent = {
  rx_packets: number;
  tx_frames: number;
  ptt_down: boolean;
  speaking: number[];
  capture: string | null;
  playback: string | null;
  tone: boolean;
  ptt_binding: string | null;
};

export type LogLine = { level: string; text: string; ts: string };

export type BridgeEvent =
  | { ev: "hello"; version: string; devices: DeviceLists }
  | ({ ev: "status" } & StatusEvent)
  | { ev: "state"; snapshot: Snapshot }
  | ({ ev: "log" } & LogLine)
  | ({ ev: "audio" } & AudioEvent)
  | { ev: "devices"; devices: DeviceLists };

export type Command =
  | { op: "connect"; address: string; nickname?: string; channel?: string; server_password?: string }
  | { op: "disconnect" }
  | { op: "join"; channel: string }
  | { op: "nick"; name: string }
  | { op: "mute"; input?: boolean; output?: boolean }
  | {
      op: "audio";
      enabled?: boolean;
      tone?: boolean;
      ptt?: string;
      input_device?: string;
      output_device?: string;
    }
  | { op: "devices" }
  | { op: "refresh" };
