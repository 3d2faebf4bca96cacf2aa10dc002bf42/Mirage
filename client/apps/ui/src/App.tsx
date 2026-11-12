import { AudioPanel } from "./components/AudioPanel";
import { ChannelTree } from "./components/ChannelTree";
import { ChannelView } from "./components/ChannelView";
import { ConnectOverlay } from "./components/ConnectOverlay";
import { EventConsole } from "./components/EventConsole";
import { TitleBar } from "./components/TitleBar";
import { useBridge } from "./useBridge";

export default function App() {
  const bridge = useBridge();
  const speaking = bridge.audio?.speaking ?? [];

  return (
    <div className="app">
      <TitleBar
        status={bridge.status}
        snapshot={bridge.snapshot}
        socket={bridge.socket}
        onDisconnect={() => bridge.send({ op: "disconnect" })}
        onReconnect={() =>
          bridge.send({
            op: "connect",
            address: bridge.status.address ?? "127.0.0.1:9987",
          })
        }
      />

      <main className="body">
        {bridge.snapshot ? (
          <>
            <ChannelTree
              snapshot={bridge.snapshot}
              speaking={speaking}
              onJoin={(channelId) => bridge.send({ op: "join", channel: String(channelId) })}
            />
            <ChannelView
              snapshot={bridge.snapshot}
              speaking={speaking}
              pttBinding={bridge.audio?.ptt_binding ?? null}
            />
            <AudioPanel
              devices={bridge.devices}
              audio={bridge.audio}
              status={bridge.status}
              send={bridge.send}
            />
          </>
        ) : (
          <>
            <aside className="panel panel--channels">
              <div className="panel__head">
                <h2 className="panel__title">Channels</h2>
              </div>
              <p className="placeholder">Connect to a server to view channels.</p>
            </aside>
            <section className="panel panel--center">
              <p className="placeholder">
                No active session. The client uses the TeamSpeak 6 protocol over UDP (tested with beta13.1).
              </p>
            </section>
            <AudioPanel
              devices={bridge.devices}
              audio={bridge.audio}
              status={bridge.status}
              send={bridge.send}
            />
          </>
        )}
      </main>

      <EventConsole logs={bridge.logs} />

      {!bridge.status.connected && (
        <ConnectOverlay socket={bridge.socket} status={bridge.status} send={bridge.send} />
      )}
    </div>
  );
}
