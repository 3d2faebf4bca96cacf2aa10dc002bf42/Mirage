import type { Snapshot } from "../types";

type Props = {
  snapshot: Snapshot;
  speaking: number[];
  onJoin: (channelId: number) => void;
};

export function ChannelTree({ snapshot, speaking, onJoin }: Props) {
  const own = snapshot.clients.find((client) => client.is_self);
  const activeChannel = own?.channel_id ?? 0;

  return (
    <aside className="panel panel--channels">
      <div className="panel__head">
        <h2 className="panel__title">Channels</h2>
        <span className="count">{snapshot.channels.length}</span>
      </div>

      <nav className="tree" aria-label="Server channels">
        {snapshot.channels.map((channel) => {
          const clients = snapshot.clients.filter((client) => client.channel_id === channel.id);
          const selected = channel.id === activeChannel;

          return (
            <div key={channel.id} className="tree__group">
              <button
                type="button"
                className={`tree__row ${selected ? "is-active" : ""}`}
                onClick={() => onJoin(channel.id)}
                aria-current={selected ? "true" : undefined}
              >
                <svg className="icon" viewBox="0 0 16 16" aria-hidden>
                  <path
                    d="M6 3.5v9M10 3.5v9M2.5 6.5h3M2.5 9.5h3M10.5 6.5h3M10.5 9.5h3"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.2"
                    strokeLinecap="round"
                  />
                </svg>
                <span className="tree__label">{channel.name}</span>
                <span className="count count--inline">{clients.length}</span>
              </button>

              {clients.map((client) => (
                <div key={client.id} className="tree__client">
                  <span
                    className={`avatar avatar--sm ${
                      speaking.includes(client.id) ? "avatar--speaking" : ""
                    }`}
                    aria-hidden
                  >
                    {initials(client.name)}
                  </span>
                  <span className="tree__client-name">{client.name}</span>
                  {client.input_muted && <span className="tag tag--muted">muted</span>}
                  {client.is_self && <span className="tag">you</span>}
                </div>
              ))}
            </div>
          );
        })}
      </nav>
    </aside>
  );
}

function initials(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) {
    return "?";
  }
  return trimmed.slice(0, 2).toUpperCase();
}
