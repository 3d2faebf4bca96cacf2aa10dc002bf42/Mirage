import type { Snapshot, StatusEvent } from "../types";

type Props = {
  status: StatusEvent;
  snapshot: Snapshot | null;
  socket: "connecting" | "open" | "closed";
  onDisconnect: () => void;
  onReconnect: () => void;
};

export function TitleBar({ status, snapshot, socket, onDisconnect, onReconnect }: Props) {
  const bridgeLabel =
    socket === "open" ? "bridge online" : socket === "connecting" ? "connecting to bridge…" : "bridge offline";

  return (
    <header className="titlebar">
      <div className="titlebar__app">
        <span className="mark" aria-hidden />
        <span className="titlebar__name">ts6client</span>
        <span className={`bridge bridge--${socket}`} title={bridgeLabel} />
      </div>

      <div className="titlebar__center">
        {status.connected ? (
          <>
            <span className="titlebar__server">{snapshot?.server.name ?? status.address}</span>
            <span className="sep">·</span>
            <span className="mono muted">{status.address}</span>
          </>
        ) : (
          <span className="muted">disconnected</span>
        )}
      </div>

      <div className="titlebar__right">
        <span className={`dot ${status.connected ? "dot--on" : "dot--off"}`} aria-hidden />
        <span className={status.connected ? "status-on" : "muted"}>
          {status.connected ? "Connected" : "Disconnected"}
        </span>
        {status.connected ? (
          <button type="button" className="btn btn--ghost" onClick={onDisconnect}>
            Disconnect
          </button>
        ) : (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onReconnect}
            disabled={socket !== "open"}
          >
            Reconnect
          </button>
        )}
      </div>
    </header>
  );
}
