# WebSpeak3 — architecture recon

Read-only investigation of `vendor/webspeak3` for a desktop TeamSpeak 6 client.
All paths below are relative to `vendor/webspeak3/`.

**Snapshot inspected:** commit `0d8421692a2401fb6e6d649107c26bcdda5e9f48`, branch `main`,
2026-11-11 ("Mobile menu: pin Feedback to the bottom of the ☰ drawer"). Licence: MIT
(`LICENSE`). Status per `README.md:13`: public beta.

**Caveat that shapes every protocol answer:** `tsclientlib/` is a git submodule that is
**not checked out in this clone**. `git submodule status` returns
`-2e7794928f18d5aef2da5b48f420f9c801b1112c tsclientlib` (leading `-` = not initialised) and
the directory contains 0 files. `.gitmodules:1-4` points it at
`https://github.com/Moepchi/tsclientlib.git`, branch `webspeak3`. So the TS3/TS6 wire
protocol — UDP framing, handshake, crypto, packet layouts — **does not exist in this tree**;
only its callers do. Anything below tagged *(submodule)* is unverifiable from this checkout.

---

## 1. Topology

**Short answer: WebSpeak3 does not implement the TeamSpeak client protocol. It is a wrapper
around `tsclientlib` — a real Rust TS3 client library — run as one OS process per browser
session, plus a Node WebSocket relay and a React UI.**

The repo states this itself. `README.md:123-126`: "Browsers can't send raw UDP, which is what
TeamSpeak's native protocol runs over, so a pure client-side implementation isn't possible — a
server-side gateway is required that speaks the real TS protocol on one side and WebSocket to
the browser on the other." `gateway/package.json:5` describes the gateway as "bridges browser
(WebSocket) clients to a native TeamSpeak server". `gateway/src/ts3/connection.ts:6-11`:
"Bridges to the real TeamSpeak client protocol via the `ts-connector` Rust binary (built on top
of tsclientlib) … This process is spawned per connection."

### 1.1 Runtimes

| Part | Language / runtime | Build artefact | Role |
|---|---|---|---|
| `web/` | TypeScript + React 19 + Vite 8 (`web/package.json:1-30`) | static bundle | UI, mic capture, playback, WebRTC for TS6 streams |
| `gateway/` | Node.js ≥ 18, TypeScript (`gateway/package.json:1-21`), only runtime dep `ws ^8.18.0` | `gateway/dist/` | HTTP + `/ws` relay, spawns/supervises one connector per socket |
| `connector/` | Rust, edition 2021 (`connector/Cargo.toml:2-3`), binary name `ts-connector` | `connector/target/<profile>/ts-connector` | the actual TS client: connect, state, chat, audio, files, streams |
| `tsclientlib/` | Rust (submodule, **absent**) | crate | TS3/TS6/TeaSpeak protocol + audio jitter buffer + crypto |
| `connector/src/ts2.rs` | Rust, in-tree | part of the connector | hand-written **TeamSpeak 2** UDP client (the one real protocol implementation in this repo) |

The connector binary path is resolved as `CONNECTOR_BIN` env or
`../../../connector/target/debug/ts-connector[.exe]` (`connection.ts:233-240`); in the Docker
image it is baked to `/app/connector-bin/ts-connector` (`Dockerfile:89,98`).

### 1.2 Data flow

```
Browser  (web/, React 19 + Vite)
  │  WebSocket  /ws   — JSON text frames, both directions
  │  uplink   : {type:"sendAudio", pcm:"<b64 mono i16 48 kHz>"}   20 ms per frame
  │  downlink : {type:"audioOut",  pcm:"<b64 stereo i16 48 kHz>"} pre-mixed, all speakers
  ▼
Gateway  (gateway/, Node)
  │  one `Ts3Connection` per /ws socket; each owns exactly ONE child process
  │  stdin  : one command per line     ("chat hi", "switch 7 <b64pw>", "audio <b64>")
  │  stdout : newline-delimited JSON   ({"type":"connected"|"channels"|"audioOut"|…})
  │  stderr : connector tracing, server-side only
  ▼
Connector  (connector/, Rust `ts-connector`)
  │  tsclientlib::Connection (command c2s / event s2c) + AudioHandler + audiopus
  │  + speex-sys if --server-type=ts2
  ▼
TeamSpeak 3 / 6 / TeaSpeak / VoySpeak server
   UDP 9987 for control + voice;  separate TCP port for file transfer
```

Evidence for each hop:

- browser → gateway: `web/src/App.tsx:10620` / `:10698` (`new WebSocket(GATEWAY_URL)`);
  URL derivation `App.tsx:255-267` (dev → `:8080/ws`, prod → same origin or `VITE_GATEWAY_URL`).
- gateway framing in: `gateway/src/index.ts:712-737` (`JSON.parse`, then a `switch (msg.type)` of
  ~60 cases, `index.ts:738-1068`).
- gateway → connector, one command per line: `connection.ts:261-269`
  (`writeLine` strips `[\r\n]+` so a newline in any field cannot smuggle a second command).
- spawn: `connection.ts:271-283` — `--address=`, `--nickname=` as argv; server/channel
  passwords, identity and privilege key go via **environment variables** (`:278-282`) because
  argv is world-readable via `ps`.
- connector framing out: `connector/src/main.rs:968-969` — "stdout is a strict
  newline-delimited-JSON channel … all diagnostic logging must go to stderr".
- gateway parses lines: `connection.ts:306` (`createInterface({input: child.stdout}).on("line")`),
  snake_case→camelCase mapping at `:462-604`; unparseable lines become an `error` event
  (`:605-607`), never a crash.
- connector → server: `main.rs:1012` (`con_config.connect()`), command packets from
  `tsclientlib::messages::c2s` (`main.rs:18-26`), audio via `con.send_audio(packet)`
  (`main.rs:2562`).
- one socket ↔ one connector, explicitly: `index.ts:685-688` and `README.md:134-137`
  ("Spawns the Rust connector **per browser WebSocket**"). Multi-join = several `/ws` sockets =
  several connector processes (`README.md:100`, `index.ts:686-687`).

The JSON event surface is declared once in the gateway at `connection.ts:137-214` and mirrored
from Rust at `main.rs:276-456` (`enum Event`), which is the best single map of what the client
can do.

---

## 2. Protocol implementation — what is actually here

### 2.1 In-tree vs vendored

| Concern | Implemented in this repo? | Where |
|---|---|---|
| UDP transport, packet framing | **No** *(submodule)* | `tsclientlib`/`tsproto` |
| Handshake (`clientinit`, `clientinitiv`, `initivexpand`) | **No** *(submodule)*; only described in prose | `docs/teaspeak.md:8-14` |
| ECC identity, crypto | **No** *(submodule)*; one browser-side UID helper exists | `web/src/identityUid.ts:1-112` |
| Command escaping/parsing | **No** for the TS3 command protocol; a parser is only used for *undeclared* commands | `main.rs:34`, `main.rs:2833-2850` |
| Server-state bookkeeping | **No** *(submodule: `ts-bookkeeping`)* | called as `con.get_state()`, e.g. `main.rs:1035`, `:2593` |
| Voice packet build/parse | **Partly**: Opus encode + packet assembly are in-tree; RTP-ish framing on the wire is *(submodule)* | `main.rs:2534-2571`, `:2717-2743` |
| Jitter buffer / Opus decode | **No** *(submodule: `tsclientlib::audio::AudioHandler`)* | `main.rs:16`, `:1081`, `:2728`, `:3241` |
| File transfer (TCP) | **No** for the wire; **yes** for pacing, chunking, size caps, browser relay | `main.rs:42-68`, `:3140-3208` |
| TeamSpeak **2** protocol | **Yes — hand-written** | `connector/src/ts2.rs:1-898` |

### 2.2 Handshake

In-tree there is only configuration:

- identity: `main.rs:986-990` — accepts either tsclientlib's JSON blob or a native TS3
  `"<counter>V<obfuscated>"` string (`Identity::new_from_str`), else `Identity::create`.
- `Connection::build(address).name(nick).identity(id)` then `.server_type(Auto|Teamspeak|Teaspeak)`,
  `.default_token(privilege_key)`, `.password()`, `.channel_password()`, `.channel()`
  (`main.rs:992-1011`).
- login is considered done when the first `BookEvents` batch arrives; hard 15 s timeout because
  "tsclientlib only reports failures through this event stream rather than from `connect()`
  itself" (`main.rs:1014-1033`).
- after login: `channelsubscribeall` (`main.rs:1055`), `servervariable` poll every 20 s
  (`main.rs:1057-1059`, `:1177-1180`), 800 ms settle then first snapshot (`main.rs:1061-1070`).

The actual handshake bytes live in the submodule. `docs/teaspeak.md:8-14` documents what the
fork adds for TeaSpeak/VoySpeak: `clientinitiv` with the **`-teaspeak` switch**,
`ip=unknown`, alpha bit0, classic TeaSpeak crypto via `initivexpand` (**P-256 ECDH, 20-byte
IV**), a second TEAMSPEAK identity handshake after crypto, and DNS SRV `_gts._udp` in addition
to `_ts3._udp`. `docs/teaspeak.md:18-23` lists what is still missing (TEAFORO/forum identity,
POW parity, badges).

### 2.3 Crypto

No TS3 crypto code in-tree. Available crypto crates resolved by `connector/Cargo.lock`:
`ring 0.17.14` (`Cargo.lock:1970`), `curve25519-dalek-ng 4.1.1` (`:521`), `aes 0.8.4`
(`:16`), `tokio-rustls 0.26.4` (`:2588`). (Upstream tsproto uses ring for its ECC/UID
primitives; the fork's actual choices are unverifiable here.)

The one place where protocol crypto is reimplemented in TypeScript is identity derivation:
`web/src/identityUid.ts:1-5` — "UID = base64(sha1(base64(tomcrypt public key DER))), same as
tsproto's `EccKeyPubP256::get_uid`", with TS3's native identity obfuscation table
(`:7-9`) and de-obfuscation (`:91-99`), DER building (`:54-63`) and PKCS#8 scalar import
(`:66-78`). This is real, portable protocol knowledge, but it is only used to *display* a UID
without connecting (`identityUid.ts:1-2`).

### 2.4 Command layer

The gateway's `switch` (`index.ts:738-1068`) is the authoritative command surface. Everything is
a thin mapping to a one-line stdin command:

- chat: `chat`, `serverchat`, `pm <clid>`, `poke <clid>` (`connection.ts:840-858`,
  handlers `index.ts:807-822`) — each sanitises `[\r\n]+` before writing.
- channels: `switch <id> [b64pw]`, `moveclient <clid> <cid> [b64pw]` (`connection.ts:624-639`).
- self state: `away`/`unaway`, `muteinput`, `muteoutput`, `nickname`
  (`connection.ts:937-962`), `whisper`/`unwhisper` (`:976-987`).
- admin/query: `serveredit <json>`, `kickchannel`, `kickserver`, `banclient`, `bandel`,
  `bandelall`, `complaindel`, `messageadd`, `setchannelgroup`, `addservergroup`,
  `delservergroup`, `serverquerylogin`, `permlist`, `permadd`, `permdel`
  (`connection.ts:649-795`).
- files: `ftlist`, `ftmkdir`, `ftdelete`, `ftrename`, `ftdownload`, `ftupload`,
  `ftuploadchunk` (`connection.ts:797-838`), `ftlimit` (`:970-974`).
- TS6 streams: `streaminfo`, `streamjoin`, `streamleave`, `streamsignal`, `streamsetup`,
  `streamrespond`, `streamstop` (`connection.ts:864-935`).

On the connector side each line is prefix-matched in a long `if/else` chain (`main.rs:1203-2573`).
Commands that need a server acknowledgement are sent with `send_with_result()`, keyed by a
`MessageHandle` in a `HashMap` so a `MessageResult` can be turned into a visible error
(`main.rs:1088-1096`, `:2744-2760`) — including mapping `ChannelInvalidPassword` to a
`channelPasswordRequired` event (`main.rs:2748-2753`).

For commands tsclientlib does **not** declare, the connector parses the raw command stream with
`tsproto_packets::commands::CommandParser` (`main.rs:34`) and forwards only TS6 stream
commands (`main.rs:2833-2850`): `name.starts_with("notifystream") || name.ends_with("joinstreamrequest")`.
Everything else undeclared is deliberately dropped, and merely named on stderr.

Self-admission in `main.rs:448-449`: "`tsclientlib` has no declarations for these, so they
arrive as `StreamItem::UnknownCommand`". That single line captures the project's posture — it
is a consumer of an existing library, extending it only where TS6 outran the library.

### 2.5 Voice packets

Uplink (`main.rs:2534-2572`):

1. line `audio <b64>` → base64 decode;
2. `chunks_exact(2)` → `i16::from_le_bytes → f32 / 32768.0` (`:2537-2540`);
3. **hard requirement** `samples.len() == FRAME_SAMPLES` (960) (`:2541`) — i.e. exactly one
   20 ms 48 kHz mono frame, or the packet is silently dropped;
4. `opus_encoder.encode_float(&samples, &mut opus_output)` into a 1275-byte buffer
   (`:2542-2543`) — 1275 is Opus's max packet size;
5. wrapped as `AudioData::C2S { id: 0, codec: CodecType::OpusVoice, data }`, or
   `AudioData::C2SWhisper { channels, clients, .. }` when whisper targets are set
   (`:2547-2561`), then `con.send_audio(packet)` (`:2562`).

Encoder construction (`main.rs:1076-1080`): `audiopus::SampleRate::Hz48000`,
`Channels::Mono`, `Application::Voip`. **No bitrate, no VBR/DTX, no frame-size, no
`OpusMusic` variant is ever configured** — the only `bitrate` in the file is the TS6 stream
setup field (`main.rs:170`, `:1704`), and both audio paths hardcode `CodecType::OpusVoice`
(`:2550`, `:2558`). A channel set to *Opus Music* still receives OpusVoice frames at the
encoder's default rate.

Downlink `StreamItem::Audio(packet)` (`main.rs:2717-2743`): the packet is handed to
`audio_handler.handle_packet(from, packet)`; the connector applies a per-client volume to the
queue the handler just created (`:2729-2734`) and emits `talkers` when the set changes
(`:2735-2740`). Whisper packets (`AudioData::S2CWhisper`) are filtered against a
block/allow policy before decoding (`:2718-2726`) — "blocked whispers are dropped undecoded",
so a blocked whisper costs no CPU.

### 2.6 File transfer

The wire protocol is tsclientlib's; the in-tree work is relaying and bounding it:

- size caps: downloads 25 MiB, uploads 24 MiB (`main.rs:42-44`); avatars/icons 512 KiB
  (`main.rs:69-72`, applied at `:3123-3127`); the browser also caps at 24 MiB
  (`web/src/App.tsx:4644`).
- relay chunk size 192 KiB raw, "a multiple of 3 so the base64 pieces can simply be
  concatenated", introduced after "one huge line per file made the Node gateway hold several
  copies of it at once and run out of heap (2026-10-11)" (`main.rs:45-49`).
- uploads arrive as `ftuploadchunk <cid>\t<offset>\t<total>\t<path>\t<data>`, are validated
  (`/^[A-Za-z0-9+/]*={0,2}$/`, safe integers — `connection.ts:833-838`) and reassembled with at
  most `MAX_PARTIAL_UPLOADS = 4` concurrent half-received files (`main.rs:68`, `:533-577`).
- actual TCP transfer is `StreamItem::FileDownload` / `FileUpload`, which "tsclientlib resolves
  … into a ready TCP stream on its own" (`main.rs:1143-1149`; download loop `:3140-3180`,
  upload loop `:3183-3199`), with bandwidth pacing via `ft_pace` against atomic limits
  (`main.rs:50-65`, set by `connection.ts:970-974`).

### 2.7 The exception: TS2 is a genuine in-repo protocol implementation

`connector/src/ts2.rs` (898 lines) is a from-scratch UDP client for TeamSpeak 2, written from
the Wireshark dissector and checked against a real 2.0.24.1 server (`ts2.rs:3-5`). It is the
best protocol reference in this repo:

- packet classes and layout, little-endian: `0xbef4` connection, `0xbef0` reliable,
  `0xbef1` ACK, `0xbef2`/`0xbef3` voice; CRC32 over the packet with the CRC field zeroed
  (`ts2.rs:7-13`), CRC implementation `:48-64`;
- **stop-and-wait reliable layer** with resend counter and `MAX_RESENDS = 10`
  (`ts2.rs:285-288`, `:306-333`), keepalive every 3 s (`:30`, `:335-347`), out-of-order
  buffering in `early: BTreeMap<u32, Vec<u8>>` plus fragment reassembly (`:289-291`,
  `:637-654`);
- Speex narrowband: 20 ms / 160 samples / 8 kHz, 5 frames per voice packet (`ts2.rs:34-36`);
  codec id → Speex submode mapping derived from payload sizes (`:120-138`), FFI encode/decode
  through `speex-sys` (`:139-231`).

### 2.8 Dependencies that provide the protocol

Versions from `connector/Cargo.lock`, licences from crates.io metadata (external, not in tree):

| Crate | Version | Licence | Role |
|---|---|---|---|
| `tsclientlib` (fork, path dep) | 0.2.0 (`Cargo.lock:2813`) | MIT OR Apache-2.0 | the TS3 client: connection, state, audio handler |
| `tsproto` | 0.2.0 (`:2840`) | MIT OR Apache-2.0 | "implementation of the TeamSpeak3 protocol as a library" |
| `tsproto-packets` | 0.1.0 (`:2869`) | (not stated in tree) | packet/command encode-decode, used directly by the connector |
| `ts-bookkeeping` | 0.1.0 (`:2774`) | (not stated in tree) | server data model / book events |
| `audiopus` | 0.3.0-rc.0 (`:109`), pinned `Cargo.toml:9` | ISC | Opus encode (decode is inside tsclientlib) |
| `audiopus_sys` → bundled libopus | 0.2.2 (`:118`) | libopus is BSD-3-Clause upstream (**not verified in tree**) | C Opus build, needs cmake |
| `speex-sys` | 0.4.0 (`:2333`), `Cargo.toml:20` | (not stated in tree) | TS2 voice |
| `ring`, `curve25519-dalek-ng`, `aes`, `tokio-rustls` | see §2.3 | — | crypto/TLS for the TS handshake |
| `ws` (gateway) | ^8.18.0 (`gateway/package.json:13`) | MIT | WebSocket only — the gateway has **no** TS protocol code |
| `@ennuicastr/webrtcaec3.js` (web) | ^0.3.0 (`web/package.json:15`) | — | WASM AEC3 |

Nothing on the browser side decodes or encodes Opus: `web/package.json:14-19` lists only
aec3, `fflate`, `qrcode-generator`, `react`, `react-dom`, and a grep for `opus|AudioWorklet`
across `web/src` finds only a comment and demo strings.

---

## 3. Audio: browser ⇄ Opus

### 3.1 Constants and the split of work

`web/src/voice.ts:3-10` states the contract: "The connector always speaks 48 kHz PCM in 20 ms
frames: mono for the microphone → server direction (which it Opus-encodes), stereo mixed for
the server → speakers direction (already decoded/mixed for us)."

- `SAMPLE_RATE = 48000`, `FRAME_SAMPLES = 960` — `voice.ts:9-10`, mirrored in Rust at
  `main.rs:39-40` and `main.rs:73` (`OUT_CHANNELS = 2`).

So the browser is a **PCM endpoint**, not a codec endpoint. Encoder, decoder and mixer all live
in the Rust connector.

### 3.2 Uplink chain

1. `getUserMedia({audio:{channelCount:1, sampleRate:{ideal:48000}, echoCancellation,
   noiseSuppression, autoGainControl, deviceId?}})` — `voice.ts:115-129`. Requesting the
   pipeline rate is deliberate: "avoids an extra internal resampling step ahead of the browser's
   own echo canceller" (`voice.ts:118-123`).
2. The `AudioContext` is created with a forced rate: `new AudioContext({ sampleRate: SAMPLE_RATE })`
   — `web/src/App.tsx:9601`. Any device/OS rate conversion is therefore the browser's job; there
   is no hand-written resampler in `web/` (the only SRC in the repo is for TS2, §3.5).
3. Capture node is a **`ScriptProcessorNode`**, not an AudioWorklet:
   `this.context.createScriptProcessor(2048, 2, 1)` (`voice.ts:137`). Deprecated but universal;
   input channel 0 = mic, channel 1 = optional mixed-in "extra" source (`voice.ts:135-137`,
   `:221-230`), used for shared-tab audio.
4. Optional WASM AEC3 runs **before** VAD/encoding, in place of relying only on the browser's
   native cancellation: `voice.ts:46-49`, `:141-147`; module loader
   `web/src/aec3.ts:38-64`; the render reference is tapped post-volume from the player
   (`voice.ts:489-502`).
5. Voice activation: RMS over the block vs `threshold` (default 0.02) with a `hangoverSeconds`
   (default 0.3) so word tails are not clipped (`voice.ts:102-103`, `:158-165`). Modes:
   `pushToTalk` (send only while `pttPressed`), `continuous`, or the default VAD
   (`voice.ts:87-91`, `:165`). While an extra source is attached it sends continuously
   (`voice.ts:166-167`). Typing attenuation damps the mic to 10 % for 200 ms after a keystroke
   (`voice.ts:92-95`, `:148-151`, `:215-217`).
6. Framing: samples are accumulated into `pending[]`, and every full 960-sample frame is
   converted to `Int16Array`, converted to base64 and emitted (`voice.ts:176-202`);
   `int16ToBase64` at `voice.ts:12-20`. The "extra" source is mixed at 0.5 gain to leave
   headroom (`voice.ts:196-199`) and its queue lead is capped to bound latency
   (`voice.ts:187-190`).
7. `onFrame → socket.send({type:"sendAudio", pcm})` — `App.tsx:11359-11366`;
   gateway case `index.ts:823-825`; `connection.sendAudio` writes `audio <b64>`
   (`connection.ts:860-862`); Rust decodes+encodes (§2.5).

Because VAD gates the frames, the uplink is silent (zero bandwidth) when nobody talks —
which is why a 128 KB/s PCM stream is survivable in a browser deployment.

### 3.3 Downlink chain

1. Rust mixes **all** speakers: 20 ms `tokio::time::interval` (`main.rs:1087`) →
   `audio_handler.fill_buffer(&mut buf)` on a `960 × 2` f32 buffer (`main.rs:3238-3241`);
   clamp, convert to i16 LE (`:3252-3257`), base64, emit `AudioOut` (`:3258`).
2. Gateway forwards verbatim (`connection.ts:154`, `:602-604`).
3. Browser: `case "audioOut"` → `audioPlayer.playFrame(data.pcm)` when output not muted
   (`App.tsx:10168-10170`).
4. `AudioPlayer.playFrame` decodes base64 → `Int16Array`, builds a **48 kHz stereo**
   `AudioBuffer` and schedules a `BufferSource` at `nextTime`; if it fell behind, it restarts
   20 ms ahead instead of letting a backlog build (`voice.ts:417-442`).
5. Output device routing: `MediaStreamAudioDestinationNode` → hidden `<audio>` element so
   `HTMLMediaElement.setSinkId` (Chrome, Firefox 130+) can be used, else
   `AudioContext.setSinkId` (Chrome 110+), else system default (`voice.ts:355-409`, `:514-525`).
6. "Automatic volume leveling" is a single `DynamicsCompressorNode` for the whole mix
   (threshold −30 dB, ratio 4, 5 ms attack / 300 ms release) — with the author's own note
   "one compressor for the whole mix, not per speaker like TS3" (`voice.ts:387-396`).
7. Per-speaker volume is **not** done in the browser: it is applied in the connector on the
   decoder queue (`main.rs:2729-2734`, command `clientvolume` at `connection.ts:954-957`).

Consequence worth internalising: the browser receives **one pre-mixed stereo stream**. It cannot
do per-speaker DSP, positional audio, per-speaker AGC, or selective streams — the mixing point is
inside the connector process.

### 3.4 Jitter buffer and loss handling

Two different things, in two different places:

- **Real jitter buffer: in Rust, inside tsclientlib.** Per-speaker queues live in
  `tsclientlib::audio::AudioHandler` (`main.rs:16`, `:1081`); the connector only fills its 20 ms
  tick from it (`main.rs:3241`) and reads `get_queues()` / `get_mut_queues()` for volume and
  "who is talking" (`main.rs:2728-2734`, `:3239`). Its internals are **not in this checkout**.
  What the in-tree comment does reveal is loss handling: `main.rs:1185-1188` — "After a stall
  (e.g. CPU steal) the missed audio ticks fire in a burst. Polling in order drains the queued
  packets first, otherwise a tick can find an empty queue and **play Opus loss concealment**
  instead", implemented as `biased;` in the `tokio::select!` (`main.rs:1188`).
- **Browser-side: no jitter buffer at all**, only `nextTime` scheduling with a 20 ms floor
  (`voice.ts:434-441`).

The hand-rolled equivalent (for reference) is the TS2 path: per-speaker `VecDeque<i16>`, start
playing only once `PREBUFFER = 5 × 160 + 1` samples are queued because TS2 clients "send in
bursts and anything less underruns on ordinary jitter", and underflow drops the speaker back to
non-playing rather than emitting silence (`ts2.rs:37-40`, `:273-277`, `:547-566`).

### 3.5 Sample-rate conversion

- TS3/TS6 path: **none in app code**. Capture requests 48 kHz (`voice.ts:123`), the
  `AudioContext` is pinned to 48 kHz (`App.tsx:9601`), playback buffers are built at
  `SAMPLE_RATE` (`voice.ts:422`), and the connector only ever produces 48 kHz
  (`main.rs:1076-1078`, `OUT_CHANNELS` stereo at `main.rs:73`). Everything else is browser/OS
  internal resampling.
- TS2 path: real SRC, both directions, hand-written —
  8 → 48 kHz by 6× linear interpolation (`ts2.rs:547-584`, with the author's "ponytail: linear
  interpolation, swap for a proper resampler if it sounds dull"), and
  48 → 8 kHz by averaging each 6 samples as "a cheap low-pass" (`ts2.rs:608-613`).

---

## 4. Feature coverage vs the official client

Claims below are code-level, not marketing. "NOT IMPLEMENTED" means a grep for the concept
found nothing.

| Area | Status | Evidence |
|---|---|---|
| Channel + client tree | ✅ live snapshots on every book event, correct sibling ordering resolved explicitly | `main.rs:291-303`, `:689-722`, `:2715` |
| Server/channel/private text chat, BBCode | ✅ plus poke, server messages, offline messages | `index.ts:807-822`, `main.rs:2595-2635`, `web/src/lib/bbcode.tsx` |
| Channel switch / move others, channel passwords | ✅ with `channelPasswordRequired` round-trip | `connection.ts:624-639`, `main.rs:2748-2753` |
| Permissions (view + edit) | ✅ effective overview, per-scope lists, add/remove, full catalog, group lists | `connection.ts:737-795`, `main.rs:18-26`, `:1121-1137` |
| Avatars + group/server icons | ✅ fetched as ordinary file transfers from channel 0 (`/avatar_<uid>`, `/icon_*`) with 512 KiB caps, "fetched for everyone in the user's channel without asking" | `main.rs:69-72`, `:154-156`, `:3123-3127`; path builder `App.tsx:4591-4599` |
| File transfer (browse/upload/download/mkdir/rename/delete) | ✅ chunked both ways, bandwidth caps, 24/25 MiB relay limits | `connection.ts:797-838`, `main.rs:42-49`, `:3140-3208` |
| Whisper (channel/client targeted) | ✅ send + receive policy + per-target exceptions | `main.rs:2545-2560`, `:2718-2726`, `connection.ts:964-987` |
| Push-to-talk | ✅ (`pttPressed`, plus `continuous` and VAD) | `voice.ts:87-91`, `:165` |
| Away status, nicknames, input/output mute | ✅ | `connection.ts:886-961` |
| Moderation: kick, ban, ban list, complaints | ✅ | `connection.ts:649-714` |
| Server log feed (join/leave/switch/group/server edits) | ✅ synthesised from book events, plus raw protocol log | `connection.ts:126-135`, `main.rs:262-275`, `:591+`, `:688-690` |
| Connection stats (ping, loss, bandwidth) | ✅ per-client and per-server | `connection.ts:641-647`, `main.rs:355-389` |
| Server query login | ✅ (`serverQueryLogin`) but no interactive ServerQuery console | `connection.ts:757-761` |
| Local recording | ✅ WAV via `encodeWavStereo` | `voice.ts:283-315`, `App.tsx:11616` |
| Echo cancellation | ✅ browser-native, opt-in WASM AEC3 | `voice.ts:141-147`, `aec3.ts:38-64` |
| **Channel commander toggle** | ❌ **NOT IMPLEMENTED** — read-only (`is_channel_commander` is populated, never set) | `main.rs:134`, `:765`; no setter anywhere |
| **TS6 screen stream — watch** | ⚠️ ALPHA. Receives TS6 SDP offer over the command channel, answers, trickles ICE; receive-only peer connection | `web/src/stream.ts:1-15`, `:143-168`, `:236-264` |
| **TS6 screen stream — publish** | ⚠️ ALPHA. Publisher offers; P2P mesh, one peer connection per viewer | `web/src/publish.ts:1-30`, `:15-16` |
| **TS6 "Server" mode (SFU)** | ❌ **NOT IMPLEMENTED** — "Only P2P is implemented; SFU needs the mediasoup path" | `publish.ts:41-45`, `:55`; `ROADMAP.md:30-31` |
| **TS6 1:1 Call** | ❌ **NOT IMPLEMENTED** — no `setupcall`/`notifycall`/`calljoin` anywhere; only Stream commands are forwarded | `main.rs:2847` (stream-only filter) |
| Video for TS3 | N/A — the TS3 protocol has no video; not attempted | — |
| Multi-server tabs | ✅ one `/ws` + one connector process per tab | `README.md:100`, `index.ts:686-687` |
| Auto-reconnect / seamless switch | ✅ browser-side `scheduleReconnect`, plus `reconnecting`/`reconnected`/resubscribe from tsclientlib | `App.tsx:10731-10751`, `main.rs:2586-2592`, `:3209-3212` |
| Identity persistence / import | ✅ JSON or native `<counter>V<...>` blob round-tripped through the gateway | `main.rs:986-991`, `:1043-1052` |
| Account sync (E2E) | ✅ separate server process, browser-side PBKDF2→AES-GCM, random 16-digit account | `README.md:205-222`, `gateway/src/sync-server.ts`, `web/src/sync.ts` |
| i18n | ✅ 6 locale files (`de, en, fa, cs-CZ, ru, zh-CN`) | `web/src/locales/*.json`, README claims 4 |
| TS2 | ⚠️ EXPERIMENTAL, opt-in `ENABLE_TS2=1`; voice/channels/PMs only, no files/avatars/streams/admin | `index.ts:17-18`, `README.md:252`, `ts2.rs:1-14` |

Known-limits list the maintainers give themselves: `ROADMAP.md:34-38` (Safari audio, uncommon
permission combinations and large permission sets, very large servers, long-running sessions,
other reverse-proxy setups) and `README.md:149-156` (browser matrix).

---

## 5. Scale and deployment model

### 5.1 Process model

- **One connector OS process per live TeamSpeak session.** Not configurable, not pooled:
  `spawn(CONNECTOR_BIN, args, {env})` per `Ts3Connection` (`connection.ts:283`), one
  `Ts3Connection` per `/ws` socket (`index.ts:685-688`), and the connector itself is one-shot
  (`connector/src/main.rs:76`: "One-shot bridge: connects to a real TeamSpeak server and reports
  events as JSON lines on stdout").
- **No shared UDP socket, no shared session, no multiplexing.** Each process holds its own
  identity, its own UDP socket, its own book state, its own `AudioHandler`. Two browser tabs
  joining the same server = two TS clients as far as the server is concerned (hence the
  duplicate-identity problems described at `index.ts:652-658`).
- Lifecycle: `connect` reuses the socket but tears the previous child down first
  (`index.ts:757-767`); `disconnect` writes `disconnect`, ends stdin and **waits up to 3 s for
  the child to actually exit** before a reconnect may spawn a replacement, "otherwise two
  connector processes would briefly hold the same audio/socket resources"
  (`connection.ts:989-1013`).
- Zombie protection: 20 s WS ping/heartbeat that terminates dead sockets so the child does not
  outlive the browser (`index.ts:592-632`); SIGTERM handler disconnects every live session
  before exit so the TS server does not keep a ghost client (`index.ts:636-669`).

### 5.2 Hard limits (defaults)

| Limit | Default | Source |
|---|---|---|
| concurrent `/ws` per client IP | 10 (`0` = off) | `index.ts:97`, enforced `:585-587` |
| concurrent `/ws` total | 60 (`0` = off) | `index.ts:100`, enforced `:691-695` (sends `gateway_full`, close 1013) |
| server connects per IP per minute | 30 | `index.ts:106-107`, enforced `:750-756` |
| single WS message size | 34 MiB | `index.ts:109-114` |
| upload / download relay | 24 MiB / 25 MiB | `main.rs:42-44` |
| partial uploads in flight | 4 | `main.rs:68` |
| connect/login timeout | 15 s | `main.rs:1018` |
| file-transfer bandwidth | unlimited unless `ftlimit` set | `main.rs:50-53` |

### 5.3 The cost figure that matters

`index.ts:98-100`, measured 2026-10-25 by the maintainers:

> "~8-9 % of a core per client listening to constant audio (connector + node + cloudflared),
> ~10-15 MB RAM. 60 suits a 2-core VPS."

That is the project's own answer to "how many users": **~60 concurrent sessions per 2 vCPU
box, at ~9 % CPU and ~15 MB RAM each**, and that is *while audio is flowing* — i.e. the pipeline
is CPU-bound on PCM↔base64 + Opus + process/IPC overhead, not on bandwidth.

### 5.4 What actually breaks first with many users

In the order the code itself flags them:

1. **File descriptors / process count.** `connection.ts:294-300` spells out the failure mode:
   "spawn() itself can fail asynchronously … or (under load, with many connections already open)
   **EMFILE/ENFILE from running out of file descriptors**. That failure surfaces as an unhandled
   'error' event … far more likely to hit at the worst possible time". N processes ×
   (stdin, stdout, stderr, 2×UDP, N×TCP for transfers) per session.
2. **One Node event loop for every session's JSON.** All audio frames for all users are JSON
   strings parsed and re-serialised in a single thread (`index.ts:712-737`, `:780-788`). Nothing
   is offloaded.
3. **Base64 PCM amplification.** Uplink is 960 samples × 2 B = 1920 B / 20 ms = 96 KB/s raw,
   **128 KB/s base64**; downlink is 960 × 2 ch × 2 B = 3840 B / 20 ms = 192 KB/s raw,
   **256 KB/s base64** (constants: `voice.ts:9-10`, `main.rs:73`). For comparison, the Opus
   payload the connector actually puts on the wire is a single ≤1275-byte packet per 20 ms
   (`main.rs:2542`) — orders of magnitude smaller. The gateway is an amplifier: it carries
   ~100× the bytes the TS server sees.
4. **Heap pressure from one-file-at-a-time relays.** Already hit once: `main.rs:45-49`
   documents an OOM on 2026-10-11 caused by a single huge upload line, fixed by 192 KiB
   chunking; `MAX_PARTIAL_UPLOADS` and the 34 MiB WS cap are the residual guards.
5. **Per-session login storms.** Each session is a fresh login with its own identity, which is
   why a connect rate limit exists at all (`index.ts:102-107`: 10 sockets were enough to
   join-flood a server at several joins per second).

### 5.5 Deployment shape

Single container, single Node process, built as one image with the Rust binary and the static
bundle inside (`Dockerfile:68-110`): `node:22-bookworm-slim`, `USER node` (`:105`), healthcheck
on `/healthz` (`:107-108`), `CONNECTOR_BIN=/app/connector-bin/ts-connector` (`:98`). Compose adds
a cloudflared tunnel and drops all capabilities (`docker-compose.yml`). Horizontal scaling is
"run more containers"; there is no session affinity or shared state between gateways — each
session is self-contained.

Security posture relevant to us: the gateway is an **SSRF surface by design**, guarded by an
allowlist (`ALLOWED_SERVERS`, `index.ts:33-64`), an origin allowlist for `/ws`
(`index.ts:85-93`, `:580-589`), and a private-address block implemented *inside tsclientlib*
(`tsclientlib::resolver::is_allowed_target`, called explicitly on the TS2 path at
`ts2.rs:727-733`; the TS3 path surfaces it as `BlockedTarget`, `main.rs:947-951`). The trust
model is stated openly at `README.md:259`: whoever runs the gateway handles the user's identity
key, server/channel passwords and privilege keys.

---

## 6. Reuse verdict for a desktop Electron/Tauri TS6 client

### 6.1 Directly portable (real value)

| Module | Why portable | Work |
|---|---|---|
| `connector/src/main.rs` + `ts2.rs` (~4.2 k lines Rust) | Already a headless client with a clean command/event boundary; no browser, no Node, no HTTP assumptions. Depends only on tsclientlib + audiopus + tokio | Replace the stdin/stdout loop with a Rust API or keep the exact same NDJSON contract over a local socket; Tauri gains nothing from Electron here |
| `gateway/src/ts3/connection.ts` (1017 lines) | A complete, ordered command map plus event schema — the best documentation of the library API that exists in the repo | Keep as the interface spec; drop `spawn`, keep the sanitisation rules |
| Command-line hygiene patterns | newline-stripping on every write (`connection.ts:261-269`), secrets via env not argv (`:275-282`), tolerant spawn/exit handling so one session cannot kill the supervisor (`:286-304`), `send_with_result` → user-visible errors (`main.rs:1088-1096`), disconnect-before-respawn (`:989-1013`) | copy as-is |
| `web/src/voice.ts` capture/playback (`MicCapture`, `AudioPlayer`) | Framework-agnostic Web Audio, no React, no gateway types | swap `ScriptProcessorNode` → `AudioWorklet` (`voice.ts:69`, `:137`) |
| `web/src/aec3.ts` + `voice.ts:489-502` | WASM AEC3 with the render-tap pattern is genuinely hard to get right | reuse verbatim |
| `web/src/stream.ts` / `publish.ts` (814 lines) | Pure WebRTC + a JSON signaling shim; TS6 stream protocol knowledge (offers, `accessibility`/`mode`/`type` enums, `notifyrespondjoinstreamrequest`) is documented inline (`publish.ts:20-45`, `stream.ts:24-36`) | keep the protocol knowledge, replace the transport with Tauri/Electron IPC |
| `web/src/identityUid.ts` | UID derivation + TS3 identity de-obfuscation, pure WebCrypto | reuse |
| `voice.ts:283-315` `encodeWavStereo` | trivial, useful for local recording | reuse |
| Node/gateway findings as negative knowledge | See §5.4 — these are the bugs we do not need to rediscover | read, don't port |

### 6.2 Assumptions that must change

1. **A server-side gateway is required.** The entire Node layer exists only because "browsers
   can't send raw UDP" (`README.md:123-126`). A desktop app can speak UDP directly, so the
   gateway, `/ws`, `hello`/feature negotiation (`index.ts:129`, `:704`), origin allowlists,
   per-IP limits and the static-file server all disappear. Only keep the *process boundary* if
   you want crash isolation; even then it should be one process for all sessions, not one per
   session (`connection.ts:283`).
2. **Audio crosses a JSON/WebSocket/base64 boundary.** 128 KB/s up and 256 KB/s down of base64
   PCM (§5.4) becomes a memcpy, or — better — in-process direct buffers. Do not carry PCM
   through JSON anywhere.
3. **The browser is a PCM endpoint.** In a desktop client you can move Opus in-process: encode
   in Rust and hand the app the Opus frames, or decode per speaker and mix in the app so you keep
   per-speaker volume, panning and DSP. Today mixing is centralised in the connector
   (`main.rs:2729-2734`, `:3241`) and the browser gets one stereo mix.
4. **Identity and secrets are handed to the gateway.** `README.md:259` is explicit that the
   operator handles the identity key and privilege keys. On desktop the key should never leave
   the process (OS keystore), which removes the whole class of "trust your gateway operator".
5. **Storage is `localStorage`.** `README.md:261` — identities including private keys live
   unencrypted in `localStorage` unless a master password is set. Desktop should use OS-level
   secret storage.
6. **The library is a submodule and is a 2021-era fork.** `tsclientlib 0.2.0` / `tsproto 0.2.0`
   were published 2021-05-12; the branch used here is `Moepchi/tsclientlib` branch `webspeak3`
   (`.gitmodules:1-4`), which is where TeaSpeak/VoySpeak/TL6 extensions live
   (`docs/teaspeak.md:8-14`). Its absence from this checkout means **you must decide early
   whether to depend on the fork, re-vendor it, or bypass it**. Practical consequence:
   TS6 stream commands had to be handled through `StreamItem::UnknownCommand` because the
   library does not declare them (`main.rs:446-455`, `:2833-2850`) — expect the same for any
   newer TS6 feature.
7. **HTTP/Cloudflare/TURN plumbing is deployment-specific.** `index.ts:136-145` mints TURN
   credentials and ships them in `hello` (`:704`); `cookie`-free static serving, CSP
   (`index.ts:560`), proxy IP handling (`TRUST_PROXY`) are all browser-hosting artefacts.

### 6.3 Gaps to fill for a real desktop TS6 client

Verified as absent: channel-commander toggle (§4), TS6 SFU / mediasoup streams
(`publish.ts:55`), TS6 1:1 calls (no `setupcall`/`notifycall`), an interactive ServerQuery
console, Opus bitrate/mode selection and *Opus Music* channels (`main.rs:2550`, `:2558`),
per-speaker audio objects on the client side. Also note the connector only ever sends
`CodecType::OpusVoice`, so a music-oriented client must extend that path before claiming parity.

---

## WHAT WE SHOULD COPY / WHAT WE SHOULD NOT

**COPY — architecture and code**

- **The insight that the protocol is not worth rewriting.** Use `tsclientlib`/`tsproto`
  (MIT OR Apache-2.0) instead of re-deriving TS3 UDP + crypto. WebSpeak3 chose this and it is
  why the project has feature coverage at all.
- **`connector/src/main.rs` + `ts2.rs` as the starting skeleton** — a decoupled command/event
  client core, plus a fully hand-written reference UDP client (reliable layer, jitter buffer,
  Speex, SRC) to learn from.
- **`gateway/src/ts3/connection.ts` as the API contract**: the ordered command list and the
  complete event schema, with snake_case→camelCase mapping.
- **The robustness patterns**: newline-strip every line-protocol write; secrets via env, not
  argv; never let one session's exception kill the supervisor; `send_with_result` so a rejected
  command produces a visible error instead of silence; wait for the old connection to die before
  reconnecting; heartbeat to reap zombie sessions; graceful SIGTERM disconnect.
- **`voice.ts` mic/playback plumbing** (device picker fallback chain, AEC3 render tap,
  VAD + hangover + PTT + typing attenuation) and `stream.ts`/`publish.ts` (TS6 WebRTC protocol
  knowledge, ICE-before-remoteDescription buffering, TURN-offered-to-peer trick).
- **`identityUid.ts`** for UID derivation and TS3 identity de-obfuscation.
- **Chunking discipline** for file transfers (192 KiB, multiple of 3, aligned to base64) and the
  24/25 MiB relay caps.
- **The feature negotiation idea** (`hello.features`, `index.ts:129`, `:704`): the UI must not
  assume the backend supports a capability.

**DO NOT COPY**

- **The three-tier browser topology** (web ⇄ WS gateway ⇄ spawned Rust process). It exists
  solely because a browser cannot open a UDP socket (`README.md:123-126`). One process, direct
  sockets, in-process calls.
- **One OS process per session** (`connection.ts:283`). N processes × ~9 % CPU and N × 15 MB,
  with EMFILE as the first hard wall (`connection.ts:294-300`).
- **PCM-in-base64-over-JSON as the audio transport** — ~128 KB/s up / ~256 KB/s down versus
  ≤1275 B per 20 ms of Opus (`main.rs:2542`), all of it parsed on one Node event loop.
- **Centralised mixing + a single pre-mixed downlink** — it caps you at "TS3-like", forbids
  per-speaker DSP/panning and blocks any future spatial or multi-stream work.
- **`ScriptProcessorNode`** (`voice.ts:137`) — use AudioWorklet.
- **Trusting the gateway operator with identities and privilege keys** (`README.md:259`), and
  `localStorage` for private keys (`README.md:261`).
- **The `App.tsx` monolith** — 14 843 lines / 602 KB in one component file. It is the single
  clearest thing not to reproduce.
- **`ALLOWED_SERVERS`-style SSRF guards are not needed** on a client that connects where the
  user says; do not port the gateway's policy layer, but do port `is_allowed_target` thinking to
  anything that auto-fetches URLs (banners, avatars, TURN).
- **Unconfigurable Opus** (always `OpusVoice`, no bitrate/VBR/DTX, `main.rs:2550`, `:2558`) —
  a desktop client should expose bitrate and support Opus Music channels.
