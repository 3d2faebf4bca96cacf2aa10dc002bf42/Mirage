import { useEffect, useState } from "react";
import type { AudioEvent, Command, DeviceLists, StatusEvent } from "../types";

type Props = {
  devices: DeviceLists;
  audio: AudioEvent | null;
  status: StatusEvent;
  send: (command: Command) => void;
};

export function AudioPanel({ devices, audio, status, send }: Props) {
  const [enabled, setEnabled] = useState(false);
  const [tone, setTone] = useState(false);
  const [ptt, setPtt] = useState("");
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [nickname, setNickname] = useState("");

  // Adopt whatever the bridge reports, so a restart of the UI shows real state.
  useEffect(() => {
    if (audio) {
      setPtt(audio.ptt_binding ?? "");
      setInput(audio.capture ?? "");
      setOutput(audio.playback ?? "");
    }
  }, [audio?.capture, audio?.playback, audio?.ptt_binding]);

  useEffect(() => {
    if (status.nickname) {
      setNickname(status.nickname);
    }
  }, [status.nickname]);

  const apply = () => {
    send({
      op: "audio",
      enabled,
      tone,
      ptt,
      input_device: input,
      output_device: output,
    });
  };

  return (
    <aside className="panel panel--audio">
      <div className="panel__head">
        <h2 className="panel__title">Audio</h2>
        <span className={`dot ${audio?.ptt_down ? "dot--on" : "dot--idle"}`} aria-hidden />
      </div>

      <div className="field field--row">
        <label htmlFor="audio-enabled">Enable audio</label>
        <input
          id="audio-enabled"
          type="checkbox"
          checked={enabled}
          onChange={(event) => setEnabled(event.target.checked)}
        />
      </div>

      <div className="field">
        <label htmlFor="audio-input">Input</label>
        <select
          id="audio-input"
          value={input}
          onChange={(event) => setInput(event.target.value)}
          disabled={!enabled}
        >
          <option value="">
            {devices.inputs.length === 0 ? "No microphone" : "System default"}
          </option>
          {devices.inputs.map((device) => (
            <option key={device.name} value={device.name}>
              {device.name}
            </option>
          ))}
        </select>
      </div>

      <div className="field">
        <label htmlFor="audio-output">Output</label>
        <select
          id="audio-output"
          value={output}
          onChange={(event) => setOutput(event.target.value)}
          disabled={!enabled}
        >
          <option value="">System default</option>
          {devices.outputs.map((device) => (
            <option key={device.name} value={device.name}>
              {device.name}
              {device.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>
      </div>

      <div className="field field--row">
        <label htmlFor="audio-ptt">Push-to-talk</label>
        <input
          id="audio-ptt"
          className="keycap"
          type="text"
          value={ptt}
          placeholder="F13"
          onChange={(event) => setPtt(event.target.value)}
          disabled={!enabled}
        />
      </div>

      <div className="field field--row">
        <label htmlFor="audio-tone">Test tone</label>
        <input
          id="audio-tone"
          type="checkbox"
          checked={tone}
          onChange={(event) => setTone(event.target.checked)}
          disabled={!enabled}
          title="Send a 440 Hz tone instead of microphone audio"
        />
      </div>

      <button
        type="button"
        className="btn btn--primary"
        onClick={apply}
        disabled={!status.connected}
      >
        Apply audio
      </button>
      <button
        type="button"
        className="btn btn--ghost"
        onClick={() => send({ op: "devices" })}
      >
        Refresh devices
      </button>

      <dl className="stats">
        <div>
          <dt>Received</dt>
          <dd>{audio?.rx_packets ?? 0} packets</dd>
        </div>
        <div>
          <dt>Sent</dt>
          <dd>{audio?.tx_frames ?? 0} frames</dd>
        </div>
        <div>
          <dt>PTT</dt>
          <dd>{audio?.ptt_down ? "pressed" : "released"}</dd>
        </div>
        <div>
          <dt>Source</dt>
          <dd>{audio?.tone ? "test tone" : audio?.capture ?? "—"}</dd>
        </div>
      </dl>

      <div className="panel__head panel__head--sub">
        <h2 className="panel__title">You</h2>
      </div>
      <div className="field field--row">
        <input
          className="input"
          type="text"
          value={nickname}
          onChange={(event) => setNickname(event.target.value)}
          placeholder="nickname"
        />
        <button
          type="button"
          className="btn btn--ghost"
          onClick={() => send({ op: "nick", name: nickname })}
          disabled={!status.connected || nickname.trim() === ""}
        >
          Save
        </button>
      </div>
    </aside>
  );
}
