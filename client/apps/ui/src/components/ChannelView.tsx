import type { Snapshot } from "../types";

type Props = {
  snapshot: Snapshot;
  speaking: number[];
  pttBinding: string | null;
};

export function ChannelView({ snapshot, speaking, pttBinding }: Props) {
  const own = snapshot.clients.find((client) => client.is_self);
  const channel = snapshot.channels.find((item) => item.id === own?.channel_id);
  const clients = snapshot.clients.filter((client) => client.channel_id === channel?.id);

  return (
    <section className="panel panel--center">
      <header className="channel-head">
        <div>
          <h1 className="channel-head__name">{channel?.name ?? "No channel"}</h1>
          <p className="channel-head__sub">
            {channel ? `${channel.codec} · 48 kHz` : "waiting for server state"}
            {channel?.topic ? ` · ${channel.topic}` : ""}
          </p>
        </div>
        <div className="chips">
          <span className="chip">
            {clients.length} {clients.length === 1 ? "client" : "clients"}
          </span>
          <span className="chip">{snapshot.server.max_clients} slots</span>
          <span className="chip">PTT {pttBinding || "—"}</span>
        </div>
      </header>

      <div className="grid">
        {clients.map((client) => {
          const isSpeaking = speaking.includes(client.id);
          return (
            <article key={client.id} className={`client ${isSpeaking ? "client--speaking" : ""}`}>
              <span className={`avatar ${isSpeaking ? "avatar--speaking" : ""}`} aria-hidden>
                {initials(client.name)}
              </span>
              <div className="client__meta">
                <span className="client__name">{client.name}</span>
                <span className="client__state">
                  {isSpeaking ? (
                    <span className="speaking">speaking</span>
                  ) : client.is_self ? (
                    "you"
                  ) : (
                    "connected"
                  )}
                </span>
              </div>
              {(client.input_muted || client.output_muted) && (
                <span className="tag tag--muted">{client.input_muted ? "muted" : "deafened"}</span>
              )}
            </article>
          );
        })}

        {clients.length === 0 && (
          <p className="empty">No one is in this channel. Select a channel on the left to join.</p>
        )}
      </div>

      <div className="composer" title="Text chat is not implemented in this client">
        <input
          className="composer__input"
          type="text"
          placeholder="Text chat is not implemented yet"
          disabled
          readOnly
        />
        <button type="button" className="btn btn--ghost" disabled>
          Send
        </button>
      </div>
    </section>
  );
}

function initials(name: string): string {
  const trimmed = name.trim();
  return trimmed ? trimmed.slice(0, 2).toUpperCase() : "?";
}
