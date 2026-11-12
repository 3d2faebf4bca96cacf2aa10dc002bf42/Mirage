import { useState } from "react";
import type { Command, StatusEvent } from "../types";

type Props = {
  socket: "connecting" | "open" | "closed";
  status: StatusEvent;
  send: (command: Command) => void;
};

export function ConnectOverlay({ socket, status, send }: Props) {
  const [address, setAddress] = useState("127.0.0.1:9987");
  const [nickname, setNickname] = useState("ts6client");
  const [channel, setChannel] = useState("");
  const [password, setPassword] = useState("");

  const connect = () => {
    send({
      op: "connect",
      address: address.trim(),
      nickname: nickname.trim() || undefined,
      channel: channel.trim() || undefined,
      server_password: password || undefined,
    });
  };

  return (
    <div className="overlay">
      <form
        className="dialog"
        onSubmit={(event) => {
          event.preventDefault();
          connect();
        }}
      >
        <h1 className="dialog__title">Connect to a server</h1>
        <p className="dialog__sub">
          Open-source TeamSpeak 6 client. Compatibility was previously tested with server beta13.1.
        </p>

        <div className="field">
          <label htmlFor="connect-address">Address</label>
          <input
            id="connect-address"
            className="input"
            type="text"
            value={address}
            onChange={(event) => setAddress(event.target.value)}
            placeholder="127.0.0.1:9987"
            autoFocus
          />
        </div>

        <div className="field">
          <label htmlFor="connect-nick">Nickname</label>
          <input
            id="connect-nick"
            className="input"
            type="text"
            value={nickname}
            onChange={(event) => setNickname(event.target.value)}
            placeholder="your nickname"
          />
        </div>

        <div className="field">
          <label htmlFor="connect-channel">Channel (optional)</label>
          <input
            id="connect-channel"
            className="input"
            type="text"
            value={channel}
            onChange={(event) => setChannel(event.target.value)}
            placeholder="Default Channel or 1"
          />
        </div>

        <div className="field">
          <label htmlFor="connect-password">Server password (optional)</label>
          <input
            id="connect-password"
            className="input"
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            placeholder="—"
          />
        </div>

        {status.error && <p className="dialog__error">{status.error}</p>}
        {socket !== "open" && (
          <p className="dialog__hint">
            {socket === "connecting"
              ? "connecting to the local bridge…"
              : "bridge offline: run `cargo run -p ts6-bridge`"}
          </p>
        )}

        <button type="submit" className="btn btn--primary" disabled={socket !== "open"}>
          Connect
        </button>
      </form>
    </div>
  );
}
