import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AudioEvent,
  BridgeEvent,
  Command,
  DeviceLists,
  LogLine,
  Snapshot,
  StatusEvent,
} from "./types";

const MAX_LOGS = 200;

export type Bridge = {
  socket: "connecting" | "open" | "closed";
  status: StatusEvent;
  snapshot: Snapshot | null;
  audio: AudioEvent | null;
  devices: DeviceLists;
  logs: LogLine[];
  version: string | null;
  send: (command: Command) => void;
};

/// Owns the WebSocket to `ts6-bridge` and exposes the latest projection of it.
/// Reconnects automatically: the bridge may restart while the UI stays open.
export function useBridge(): Bridge {
  const [socket, setSocket] = useState<"connecting" | "open" | "closed">("connecting");
  const [status, setStatus] = useState<StatusEvent>({
    connected: false,
    address: null,
    nickname: null,
    error: null,
  });
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [audio, setAudio] = useState<AudioEvent | null>(null);
  const [devices, setDevices] = useState<DeviceLists>({ inputs: [], outputs: [] });
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [version, setVersion] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef<number | null>(null);

  const pushLog = useCallback((line: LogLine) => {
    setLogs((previous) => {
      const next = [...previous, line];
      return next.length > MAX_LOGS ? next.slice(next.length - MAX_LOGS) : next;
    });
  }, []);

  useEffect(() => {
    let disposed = false;

    const open = () => {
      const protocol = window.location.protocol === "https:" ? "wss" : "ws";
      const ws = new WebSocket(`${protocol}://${window.location.host}/ws`);
      wsRef.current = ws;
      setSocket("connecting");

      ws.onopen = () => {
        setSocket("open");
        pushLog({ level: "info", text: "bridge connected", ts: stamp() });
      };

      ws.onclose = () => {
        setSocket("closed");
        if (!disposed) {
          retryRef.current = window.setTimeout(open, 1200);
        }
      };

      ws.onmessage = (message) => {
        let event: BridgeEvent;
        try {
          event = JSON.parse(message.data as string) as BridgeEvent;
        } catch {
          return;
        }
        switch (event.ev) {
          case "hello":
            setVersion(event.version);
            setDevices(event.devices);
            break;
          case "status":
            setStatus({
              connected: event.connected,
              address: event.address,
              nickname: event.nickname,
              error: event.error,
            });
            if (!event.connected) {
              setSnapshot(null);
            }
            break;
          case "state":
            setSnapshot(event.snapshot);
            break;
          case "audio":
            setAudio({
              rx_packets: event.rx_packets,
              tx_frames: event.tx_frames,
              ptt_down: event.ptt_down,
              speaking: event.speaking,
              capture: event.capture,
              playback: event.playback,
              tone: event.tone,
              ptt_binding: event.ptt_binding,
            });
            break;
          case "devices":
            setDevices(event.devices);
            break;
          case "log":
            // Stamp with the browser's local clock: the bridge reports UTC, and the
            // console is read by a human in local time.
            pushLog({ level: event.level, text: event.text, ts: stamp() });
            break;
        }
      };
    };

    open();
    return () => {
      disposed = true;
      if (retryRef.current !== null) {
        window.clearTimeout(retryRef.current);
      }
      wsRef.current?.close();
    };
  }, [pushLog]);

  const send = useCallback((command: Command) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(command));
    }
  }, []);

  return { socket, status, snapshot, audio, devices, logs, version, send };
}

function stamp(): string {
  const now = new Date();
  return [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map((part) => String(part).padStart(2, "0"))
    .join(":");
}
