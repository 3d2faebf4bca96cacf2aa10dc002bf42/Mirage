# Astrum TS6 — how an open-source client talks to a TeamSpeak 6 server

Read-only investigation of `vendor/astrum-ts6` (with `vendor/ts3-mobile` and `vendor/ts3j` as
upstream context) for the desktop TS6 client we intend to build.

All paths are relative to the repo named in the section's **Prefix** line. Every claim carries a
`file:LINE` citation. Where a feature does not exist, the text says **NOT IMPLEMENTED / not found**
and names the search that proves it.

**Prefix legend**

| Prefix | Repo | Licence |
|---|---|---|
| `astrum:` | `vendor/astrum-ts6` | AGPL-3.0 (`astrum:LICENSE:1-5`) |
| `ts3mobile:` | `vendor/ts3-mobile` | Apache-2.0 (`astrum:THIRD_PARTY_NOTICES.md:12`) |
| `ts3j:` | `vendor/ts3j` | Apache-2.0 as declared — **but see §5.5, it is contaminated** |
| `webspeak3:` | `vendor/webspeak3` — **4th repo, corroboration only** | MIT; see §7 |

**Snapshot inspected:** `astrum-ts6` git `650c022` (shallow clone, 1 commit); version
`1.0.0`/`versionCode 10` (`astrum:app/build.gradle.kts:15-16`); README milestone M8
(`astrum:README.md:33`). Status per its own README: pre-release
(`astrum:README.md:59-62`).

**The one-paragraph answer.** Astrum TS6 is a **TS3 client that happens to work against a TS6
server**, plus a **WebRTC layer bolted on through reverse-engineered TS6 commands**. Login is
byte-for-byte the ts3j TS3 handshake with `client_version = "3.?.? [Build: 5680278000]"` and
`client_platform = "Windows"` — no TS6 version string, no capability flags, no "enable video" call
(§1.4). Video is **not** carried by the TS server: the server is a **signalling relay**, and media
flows **peer-to-peer in a mesh, one peer connection per viewer**, with `mode=1` (P2P) hardcoded and
the SFU mode (`mode=2`) unimplemented (§2.1, §2.7). The signalling rides the ordinary TS command
channel as new commands (`setupstream`, `joinstreamrequest`, `respondjoinstreamrequest`,
`streamsignaling`) and new `notify*` events that ts3j surfaces as `UnknownTeamspeakEvent` (§2.3).

---

## 1. Connection and handshake

### 1.1 Transport — one UDP socket, one port

**Prefix:** `astrum:`, `ts3j:`

| Fact | Evidence |
|---|---|
| Single `DatagramSocket` for the whole session | `ts3j:src/main/java/com/github/manevolent/ts3j/protocol/socket/client/LocalTeamspeakClientSocket.java:44`, created at `:371` |
| Single remote endpoint (no second socket for voice) | `ts3j:.../LocalTeamspeakClientSocket.java:374` (`this.remote = remote`) |
| Default port 9987 | `astrum:ts6-protocol/src/main/kotlin/br/app/astrum/ts6/protocol/Model.kt:5`; ts3j fallback `ts3j:.../LocalTeamspeakClientSocket.java:336`; named `TS3_VOICE_DEFAULT_PORT` in `ts3j:src/main/java/com/github/manevolent/ts3j/protocol/TS3DNS.java:13` |

**Commands, voice and the handshake all share that one UDP socket.** Voice is a different *packet
kind*, not a different port: the sender task builds `PacketBody0Voice` and calls the same
`writePacket` used by commands (`ts3j:.../LocalTeamspeakClientSocket.java:615-622`), and the
library's `PacketKind` enum is `KEEPALIVE, SPEECH, CONTROL`
(`ts3j:src/main/java/com/github/manevolent/ts3j/protocol/PacketKind.java:3-6`).

**Things a plain TS3 client also has, but which this client never opens:**

| Port | Purpose | Status in these repos |
|---|---|---|
| 10011 | ServerQuery (TCP) | **NOT IMPLEMENTED / not found** — declared as a constant only, `ts3j:.../TS3DNS.java:14`; no connection code |
| 30033 | File transfer (TCP) | **NOT IMPLEMENTED / not found** — declared only, `ts3j:.../TS3DNS.java:15` |
| 41144 | TSDNS | **NOT IMPLEMENTED / not found** — declared only, `ts3j:.../TS3DNS.java:16` |

**SRV lookup is bypassed by Astrum.** ts3j *can* resolve `_ts3._udp.<host>` SRV records
(`ts3j:.../TS3DNS.java:19`, `:40-41`), but Astrum calls the `InetSocketAddress` overload, which
skips that path entirely: `client.connect(address, password, CONNECT_TIMEOUT_MS)` with an explicit
`InetSocketAddress(host, port)` (`astrum:ts6-protocol/.../Ts3jSessionClient.kt:114-119`). A desktop
client that wants `server.example.com` to resolve via SRV must call the `String` overload instead
(`ts3j:.../LocalTeamspeakClientSocket.java:326-340`).

Connect timeout is 10 s (`astrum:ts6-protocol/.../Ts3jSessionClient.kt:711`). On timeout the client
reports whether the server answered at all by summing per-kind packet statistics
(`astrum:ts6-protocol/.../Ts3jSessionClient.kt:688-699`).

### 1.2 Handshake chain as implemented by ts3j

**Prefix:** `ts3j:` (all paths `src/main/java/com/github/manevolent/ts3j/`)

The Astrum layer adds nothing here; it calls `LocalTeamspeakClientSocket.connect` and lets ts3j run
the TS3 handshake. The chain, in order:

| # | Step | Evidence |
|---|---|---|
| 1 | `setTransformation(new InitPacketTransformation())` — the pre-crypto packet layout | `protocol/socket/client/LocalTeamspeakClientSocket.java:393` |
| 2 | `Init1` **Step 0** — client sends 4 random bytes + unix timestamp, with version bytes `0x0C 0xFF 0xD2 0xFE` | `protocol/packet/handler/client/LocalClientHandlerConnecting.java:28` (constant), `:43-51` (step 0), `:56-60` (version applied) |
| 3 | Server replies **Step 1** (`a0reversed` nonce + `serverStuff`); client echoes it back in **Step 2** | `.../LocalClientHandlerConnecting.java:71-88` |
| 4 | Server sends **Step 3** (`level`, `x`, `n`) — a modular-exponentiation proof-of-work challenge | `.../LocalClientHandlerConnecting.java:89-113` |
| 5 | Client solves `y = x^(2^level) mod n`, pads to 64 bytes, and embeds the **`clientinitiv` command bytes inside Step 4** | `.../LocalClientHandlerConnecting.java:104-125` |
| 6 | Server sends **Step 127** → client sleeps 1 s, restarts at Step 0 with the post-init layout | `.../LocalClientHandlerConnecting.java:127-134` |
| 7 | `clientinitiv` body: `alpha` (10 random bytes, base64), `omega` (identity public key), `ot=1`, `ip` | `.../LocalClientHandlerConnecting.java:215-229` |
| 8 | Server replies `initivexpand2` with `ot=1`, `l` (licence), `beta`, `omega`, `proof`; client **verifies the licence signature** and throws `SecurityException` if it fails | `.../LocalClientHandlerConnecting.java:156-180` |
| 9 | Client sends `clientek ek=<b64 pubkey> proof=<b64 sig>` | `.../LocalClientHandlerConnecting.java:182-195` |
| 10 | Both sides derive the AES-EAX session keys; transport switches to `PacketTransformation` | `.../LocalClientHandlerConnecting.java:197`; `protocol/socket/client/AbstractTeamspeakClientSocket.java:167` |
| 11 | `clientinit` — the actual login (§1.3) | `.../LocalClientHandlerConnecting.java:201`, `:231-285` |
| 12 | State → `RETRIEVING_DATA` | `.../LocalClientHandlerConnecting.java:204` |

A legacy `initivexpand` (no `clientek`) path also exists for old servers
(`.../LocalClientHandlerConnecting.java:145-155`). Astrum does not select between them; ts3j follows
whichever the server sends.

**Two clarifications that matter for a reimplementation:**

- **Step 3–4 is a modular-exponentiation proof-of-work, not the key exchange.** The client computes
  `y = x^(2^level) mod n` and pads it to 64 bytes; the actual session key comes later from
  ECDH/Ed25519 during `initivexpand2` (`.../LocalClientHandlerConnecting.java:106-107`, level bound
  check at `:98-99`).
- **The pre-handshake transport is "encrypted" with a hardcoded key.** `InitPacketTransformation`
  uses the key bytes for ASCII `c:\windows\syste` and the nonce for ASCII `m\firewall32.cpl`
  (`ts3j:src/main/java/com/github/manevolent/ts3j/protocol/packet/transformation/InitPacketTransformation.java:8-16`).
  After `clientek` the real transport is **AES-128-EAX**, with a per-packet key/nonce derived as
  `SHA-256(roleByte ‖ packetTypeIndex ‖ generation ‖ ivStruct)` and an 8-byte MAC
  (`ts3j:.../transformation/PacketTransformation.java:27`, `:35-63`, `:19`, `:76-84`).
- Outbound packets are fragmented at **500 bytes**
  (`ts3j:src/main/java/com/github/manevolent/ts3j/protocol/packet/fragment/Fragments.java:21`) and
  large commands are QuickLZ-compressed — see the licence problem in §5.4.
- Known defect in ts3j, inherited by every client that uses it: `clientinitiv`'s `ip` parameter is
  built with `address.getHostAddress().substring(1)`, which **drops the first character** of the IP
  (`.../LocalClientHandlerConnecting.java:226`). Reported as observed; harmless in practice.

**What the server sends back before `CONNECTED`** (worth knowing, since a reimplementation must
handle it): `initserver` (`aclid` = our client id, `acn` = accepted nickname,
`virtualserver_id`) → `channellist` rows → `channellistfinished`, **which is the transition to
`CONNECTED`** (`ts3j:.../LocalTeamspeakClientSocket.java:71`, `:73-80`, `:82`, `:752-762`). The
client id is therefore server-assigned and only known after `initserver`
(`ts3j:.../LocalTeamspeakClientSocket.java:772-787`).

### 1.3 What `clientinit` actually advertises

**Prefix:** `ts3j:` — `protocol/packet/handler/client/LocalClientHandlerConnecting.java`

```
clientinit client_nickname=<nick> client_version=<version> client_platform=<platform>
           client_version_sign=<sign> client_input_hardware=1 client_output_hardware=1
           client_default_channel= client_default_channel_password= client_server_password=
           client_nickname_phonetic= client_meta_data= client_default_token=
           client_key_offset=<int> hwid=<b64>
```

Built at `.../LocalClientHandlerConnecting.java:231-285`. The values that matter:

| Parameter | Value sent | Evidence |
|---|---|---|
| `client_version` | **`"3.?.? [Build: 5680278000]"`** (default; overridable) | `.../LocalClientHandlerConnecting.java:236-241` |
| `client_platform` | **`"Windows"`** (default) | `.../LocalClientHandlerConnecting.java:242-246` |
| `client_version_sign` | hardcoded base64 blob `DX5NIYLvfJEUjuIbCidnoeozxIDRRkpq3I9vVMBmE9L2qnekOoBzSenkzsg2lC9CMv8K5hkEzhr2TYUYSwUXCg==` | `.../LocalClientHandlerConnecting.java:247-252` |
| `client_input_hardware` | `1` | `.../LocalClientHandlerConnecting.java:253` |
| `client_output_hardware` | `1` | `.../LocalClientHandlerConnecting.java:254` |
| `client_key_offset` | identity key offset | `.../LocalClientHandlerConnecting.java:271-273` |
| `hwid` | overridden by Astrum to the **base64 of the identity UID** | `astrum:ts6-protocol/.../Ts3jSessionClient.kt:81` |

**Parameters that silently vanish:** a parameter whose option is `null` is rendered as `""` and
dropped from the wire by `CommandSingleParameter.toString()`
(`ts3j:src/main/java/com/github/manevolent/ts3j/command/parameter/CommandSingleParameter.java:19-24`).
Because neither Astrum nor ts3-mobile ever sets `client_default_channel`,
`client_default_channel_password`, `client_default_token` or `client_nickname_phonetic`, those four
never appear — **and the client therefore joins the server's default channel**. `client_meta_data`
is a literal `""` and *is* emitted as `client_meta_data=`
(`.../LocalClientHandlerConnecting.java:267`).

**Nickname travels only in `clientinit`.** `setNickname` only emits a `clientupdate` if the socket is
already `RETRIEVING_DATA`/`CONNECTED`; both apps call it before `connect`, so no `clientupdate` is
sent (`ts3j:.../LocalTeamspeakClientSocket.java:789-807`; call at
`astrum:ts6-protocol/.../Ts3jSessionClient.kt:80`). After login the server's `acn` is authoritative
and overwrites the option (`ts3j:.../LocalTeamspeakClientSocket.java:780`).

**Channel switching** uses `clientmove cid= clid= [cpw=<base64 SHA-1 of password>]`
(`ts3j:.../LocalTeamspeakClientSocket.java:1117-1131`), reached from
`astrum:ts6-protocol/.../Ts3jSessionClient.kt:183`.

### 1.4 Does TS6 require anything extra at login? — **No.**

**Prefix:** `astrum:`, `ts3j:`

This is a strong negative result, and it is the single most useful fact for building our client.

- **No client-version override.** Astrum never calls `setClientVersion`, and never sets
  `client.version_string` / `client.version_platform` / `client.version_sign`. A repo-wide grep of
  `astrum-ts6` for `setClientVersion|version_string|version_platform|version_sign|client_version|client_platform`
  returns only four unrelated hits — `setIdentity`, `setNickname`, `setHWID`, `subscribeAll`
  (`astrum:ts6-protocol/.../Ts3jSessionClient.kt:79-81`, `:126`). **The client logs into a TS6
  server as a period-accurate TS3 client.**
- **No capability flags / no video enable.** No hit for `capab*`, `is_video`, `client_badges`,
  `video` in the connect path. The only `callable` for video is not a login flag: it is the
  `setupstream` command sent later (§2.2).
- **No `client_type`, no plugin token, no HTTP registration.** The only HTTP-ish constant anywhere
  in the video layer is a *STUN server hostname* (§2.6). There is no WebSocket, no REST call, no
  OAuth, no myTeamSpeak login anywhere in `astrum-ts6`.
- The identity is a plain ts3j `LocalIdentity` exported as text, generated at security level 10
  (`astrum:ts6-protocol/.../Ts3IdentityCodec.kt:8-12`), stored AES-GCM-encrypted under the Android
  Keystore (`astrum:app/.../identity/IdentityVault.kt:20-54`).

**Implication:** TS6 server-side admission for a video-capable client is not negotiated at login.
It is negotiated entirely through the stream commands. A desktop client can reuse the ts3j
handshake verbatim and still get full TS6 stream interoperability.

### 1.5 Event subscription — and why it quietly fails

**Prefix:** `astrum:`, `ts3j:`

Astrum calls `client.subscribeAll()` immediately after connect and **swallows the resulting
`CommandException`** as "event subscriptions unavailable"
(`astrum:ts6-protocol/.../Ts3jSessionClient.kt:125-129`). `subscribeAll()` sends exactly one
command — `channelsubscribeall`, with no explicit parameters; ts3j injects
`return_code=<n>` as the sequence number (`ts3j:.../LocalTeamspeakClientSocket.java:1215-1218`,
`:489`). Because it blocks on `.get()`, a server that refuses it stalls the connect path until that
call unwinds — which is presumably why the exception is swallowed.

Consequently Astrum does **not** rely on subscriptions for anything. It works because ts3j routes
every `notify*` command it does not recognise into the listener as a raw command/map pair
(`ts3j:.../LocalTeamspeakClientSocket.java:435-437`), and `TS3Event.createEvent` falls through to
`UnknownTeamspeakEvent` for TS6 stream events (`ts3j:src/main/java/com/github/manevolent/ts3j/event/TS3Event.java:90-91`;
class at `ts3j:src/main/java/com/github/manevolent/ts3j/event/UnknownTeamspeakEvent.java:5-20`).
That is the whole mechanism by which TS6 video reaches the app: **`UnknownTeamspeakEvent`, hand-parsed.**

Channel state comes from the classic TS3 events Astrum does implement — `onChannelList`,
`onClientJoin`, `onClientLeave`, `onClientMoved`, `onClientChanged`, `onChannelCreate/Deleted/Edit/Moved`
(`astrum:ts6-protocol/.../Ts3jSessionClient.kt:401-455`).

---

## 2. TS6 video / screen share

### 2.1 Answer up front: **P2P mesh. The server is a signalling relay, not an SFU.**

**Prefix:** `astrum:`, confirmed by `webspeak3:`

Astrum's implementation is unambiguously peer-to-peer. The evidence, in increasing order of
strength:

1. **One `PeerConnection` per (viewer, stream) pair**, keyed by the composite string
   `"clientId:streamId"` — `astrum:app/src/main/java/br/app/astrum/ts6/app/video/WebRtcManager.kt:89-90`,
   key builder at `:757`.
2. **The broadcaster creates the offer**, not the server, and sends it *to a specific client id* via
   a per-viewer command: `respondJoinStreamCallback?.invoke(requesterClientId, streamId, offerSdp)`
   (`astrum:.../WebRtcManager.kt:475`).
3. **The stream commands address a peer by `clid`**, not a server-side media endpoint:
   `joinstreamrequest … id=<stream> clid=<broadcasterClid>` (`astrum:ts6-protocol/.../Ts3jSessionClient.kt:254-262`).
4. **ICE candidates go client-to-client** through `streamsignaling` addressed to the peer's clid
   (`astrum:.../WebRtcManager.kt:702-713`), and are queued per peer until the remote description
   exists (`astrum:.../WebRtcManager.kt:91`, `:652-658`). An SFU would not need viewer-side ICE.
5. **The code carries a reverse-engineering note describing the official client doing the same:**
   *"The captured TS6 broadcaster sends a video SEND_ONLY transceiver followed by an INACTIVE audio
   transceiver, even with audio capture disabled. The initial offer is carried by
   respondjoinstreamrequest; the viewer supplies the answer."*
   (`astrum:.../WebRtcManager.kt:412-418`)

Independently confirmed by a **second, unrelated implementation** in this workspace —
`webspeak3:web/src/publish.ts:15-16`:

> `// TS6's P2P mode is a mesh: there is no SFU in the path, so every viewer gets`
> `// its own peer connection and its own copy of the encoded media.`

**However — TS6 has an SFU mode that this client does not use.** `webspeak3` recovered TS6's own
enums from the official UI bundle (`webspeak3:web/src/publish.ts:20-21`) and they read:

```
StreamMode = { NONE: 0, P2P: 1, SFU: 2 }     // webspeak3:web/src/publish.ts:41-45
```

Astrum hardcodes **`mode=1` (P2P)** (`astrum:ts6-protocol/.../Ts3jSessionClient.kt:210`). Serving
`mode=2` requires the server's mediasoup SFU path, which webspeak3 documents as unimplemented
(`webspeak3:web/src/publish.ts:55` — *"Only P2P is implemented; SFU needs the mediasoup path"*).
**Both open-source clients therefore only ever do P2P.** What the official server actually does in
mode 2 is **not verified by any source in this workspace** (§9).

**Cost of P2P we must plan for:** uplink = viewers × bitrate (no simulcast, no SVC in either
implementation). Astrum caps each viewer's encoding separately at the sender
(`astrum:.../WebRtcManager.kt:308-320`, called per peer at `:460`), so the encoder is re-run per
viewer rather than shared.

### 2.2 The commands (client → server)

**Prefix:** `astrum:` — all in `ts6-protocol/src/main/kotlin/br/app/astrum/ts6/protocol/Ts3jSessionClient.kt`

| Command | Parameters as sent | Purpose | Evidence |
|---|---|---|---|
| `setupstream` | `name`, `type`, `bitrate` (kbps), `accessibility=1`, `mode=1`, `viewer_limit=0`, `audio=0|1` | Announce a stream; server replies with the stream `id` | `:204-213` |
| `stopstream` | `id`, `reason=1` | End a stream | `:244-249` |
| `joinstreamrequest` | `id`, `clid`, `msg=""`, `is_remove=0` | Viewer asks the broadcaster for access | `:254-262` |
| `joinstreamrequest` | same with `is_remove=1` | Viewer leaves | `:264-272` |
| `respondjoinstreamrequest` | `id`, `clid`, `msg=""`, `offer=<SDP>`, `decision=1|0` | Broadcaster's accept-with-offer / reject | `:274-286` |
| `streamsignaling` | `id`, `clid`, `json=<payload>` | Answer + trickle ICE | `:288-295` |
| `requeststreaminfo` | `clid` | Ask what a participant is streaming | `:297-302` |

**Semantics of the numeric parameters** (recovered from TS6's own UI bundle by webspeak3, not
guessed — `webspeak3:web/src/publish.ts:20-21`):

| Param | Values | Evidence |
|---|---|---|
| `type` | `0` NONE, `1` CAMERA, `2` SCREEN, `3` WINDOW, `4` EXISTING_SESSION | `webspeak3:web/src/publish.ts:24-30`; Astrum's matching enum `astrum:ts6-protocol/.../Model.kt:47-51` |
| `accessibility` | `0` NONE, `1` PUBLIC, `2` CONTACTS_ONLY, `3` PRIVATE | `webspeak3:web/src/publish.ts:33-38` |
| `mode` | `0` NONE, `1` **P2P**, `2` **SFU** | `webspeak3:web/src/publish.ts:41-45` |
| `viewer_limit` | `0` = unlimited | `webspeak3:web/src/publish.ts:57`; `webspeak3:gateway/src/ts3/connection.ts:37-38` |

Note a discrepancy worth knowing: `webspeak3` says *"a real TS6 screen share sends 3"*
(`webspeak3:gateway/src/ts3/connection.ts:30`), i.e. the official client reports screen shares as
WINDOW, while Astrum sends `type=2` for SCREEN (`astrum:.../Ts3jSessionClient.kt:207`,
`astrum:app/.../TeamSpeakService.kt:1284`). Both are accepted; the value is a hint, not a contract.

**Why a custom parameter class exists.** `respondjoinstreamrequest` carries a full SDP, and SDP needs
a trailing CRLF. ts3j's stock `CommandSingleParameter.toString()` trims whitespace and would strip
it, so Astrum substitutes its own `Ts3Parameter` that escapes without trimming
(`astrum:ts6-protocol/.../Ts3jSessionClient.kt:335-356`, used at `:315`). The SDP is also
CRLF-terminated explicitly before being sent (`:281-282`). **Any reimplementation must reproduce
this exact escaping or the desktop client will reject the offer.**

### 2.3 The events (server → client)

**Prefix:** `astrum:` — `ts6-protocol/.../Ts3jSessionClient.kt:462-556`

| Event | Fields used | Effect | Evidence |
|---|---|---|---|
| `notifystreamstarted`, `notifystreaminfo` | `id`/`streamid`, `clid`, `type`, `name`, `width`, `height`, `fps`, `bitrate` | Register stream, notify UI; also resolves the broadcaster's own `setupstream` | `:470-497` |
| `notifystreamstopped` | `id`, `clid` | Drop stream | `:498-505` |
| `notifystreamupdated` | `id`, `width`, `height`, `fps`, `bitrate` | Update params in place | `:506-519` |
| `notifystreamsignaling` | `id`, `clid`, `json` (falls back to `msg`, `signaling`, `data`) | Hand payload to WebRTC | `:520-526` |
| `notifyjoinstreamrequest` | `id`, `clid` | A viewer asked to watch us | `:527-532` |
| `notifyrespondjoinstreamrequest` | `id`, `clid`, `decision`, **`offer`** | The offer arrives here — not in `streamsignaling` | `:533-542` |
| `notifystreamclientjoined` | `id`, `clid` | Viewer attached | `:543-548` |
| `notifystreamclientleft` | `id`, `clid` | Viewer detached → tear down its PC | `:549-554` |

`notifystreamstarted` carries the server-assigned stream `id`; Astrum waits up to 5 s for it and
falls back to a random UUID if it never arrives (`astrum:ts6-protocol/.../Ts3jSessionClient.kt:234-241`).
The event name-to-field mapping is defensive: `map["id"] ?: map["streamid"]`, and `clid` falls back
to `event.invokerId` (`:471-472`, `:499-500`, `:521-522`).

Field-naming quirk in the two notifies, recovered by webspeak3: `notifystreaminfo` uses
`accessibility` while `notifystreamstarted` abbreviates it to `access`, and only
`notifystreaminfo` carries `viewer` (`webspeak3:web/src/stream.ts:72-77`).

### 2.4 The full signalling flow

**Prefix:** `astrum:`

```
BROADCASTER                                        SERVER                                   VIEWER
-----------                                        ------                                   ------
setupstream name,type,bitrate,
  accessibility=1, mode=1, viewer_limit=0, audio         -> (registered)  -> notifystreamstarted(id, clid=us)

                                                     <- joinstreamrequest(id, clid=us)
       <- notifyjoinstreamrequest(id, clid=viewer)
                                                                                     requestJoinStream(clid=us)
  [user accepts, or auto-accept]                                                     watchStream()  Req 1439
  handleJoinRequest(viewer, id)        Req 419
    PC per viewer (key viewer:id)
    addTransceiver(video, SEND_ONLY "outgoing_video")
    addTransceiver(audio, INACTIVE)                                                addTransceiver(video, RECV_ONLY)
    createOffer / setLocalDescription                                                    |              Req 372-379
  respondjoinstreamrequest(id, clid=viewer,
      offer=<SDP>, decision=1)                                                   -> notifyrespondjoinstreamrequest
                                                                                    (carries offer) -> setRemoteDescription(OFFER)
                                                                                                    -> createAnswer
                                                                                                    -> answers with
                                                                                  <- streamsignaling(id, clid=us,
                                                                                        json={"cmd":"answer",
                                                                                              "args":{"answer":"<sdp>"}})
  <- notifystreamsignaling(json)  -> setRemoteDescription(ANSWER)
  ICE  -> streamsignaling(json={"cmd":"iceCandidate",...})   [both directions, trickle]
  <- notifystreamclientjoined(id, clid=viewer)

  stop: stopstream(id, reason=1)     |    viewer leave: joinstreamrequest(..., is_remove=1)
  viewer gone: notifystreamclientleft -> stopViewer()
```

Call-site chain for the broadcaster: `TeamSpeakService.startCameraBroadcast`
(`astrum:app/.../TeamSpeakService.kt:1180-1240`) or `startScreenBroadcast` (`:1267-1351`) →
`session.startStream(...)` → `webRtcManager.startCameraBroadcast/startScreenBroadcast`; then
`onStreamJoinRequested` (`:492-524`) → `acceptViewerRequest` (`:1397-1409`) →
`webRtcManager.handleJoinRequest` (`astrum:.../WebRtcManager.kt:419-488`).

Call-site chain for the viewer: `watchStream` (`astrum:app/.../TeamSpeakService.kt:1439-1484`) →
`session.requestJoinStream` **then** `webRtcManager.watchStream` (`astrum:.../WebRtcManager.kt:360-380`).
Note the viewer creates its `PeerConnection` and adds a **RECV_ONLY** video transceiver *before* any
SDP arrives (`:369-379`), then passively waits for the offer.

Rejection path: `respondjoinstreamrequest(..., decision=0)` with no `offer`
(`astrum:app/.../TeamSpeakService.kt:1411-1424`), also sent automatically when a join request arrives
while not broadcasting (`:498-510`).

### 2.5 Signalling payload formats

**Prefix:** `astrum:`

All signalling payloads are JSON **except the initial offer**, which is raw SDP because it travels
as an escaped command parameter. Two `cmd` dialects are accepted for robustness
(`astrum:.../WebRtcManager.kt:499-517`).

| Message | Exact wire JSON | Evidence |
|---|---|---|
| Offer (in `respondjoinstreamrequest`) | raw `v=0\r\n…` SDP, CRLF-terminated | `astrum:.../WebRtcManager.kt:473-475`, `astrum:.../Ts3jSessionClient.kt:280-283` |
| Offer (alternative) | `{"cmd":"offer","args":{"offer":"<sdp>"}}` | `astrum:.../WebRtcManager.kt:513-526`; test at `astrum:app/src/test/java/br/app/astrum/ts6/app/video/WebRtcSignalingTest.kt:59-74` |
| Answer | `{"cmd":"answer","args":{"answer":"<sdp>"}}` | `astrum:.../WebRtcManager.kt:633-639`; test `WebRtcSignalingTest.kt:76-91` |
| ICE candidate | `{"cmd":"iceCandidate","args":{"mLine":0,"mid":"0","sdp":"candidate:…"}}` | `astrum:.../WebRtcManager.kt:704-711`; test `WebRtcSignalingTest.kt:93-115` |
| Legacy/alt answers | `{"type":"answer","answer":"<sdp>"}`, `{"sdp":…}`, `{"candidate":…,"sdpMid":…,"sdpMLineIndex":…}` | `astrum:.../WebRtcManager.kt:505-576` |

Recognised `cmd` values for an offer: `joinResponse`, `offer`, `reconnectOffer`, and `type:"offer"`
(`astrum:.../WebRtcManager.kt:513-514`) — the presence of `joinResponse` and `reconnectOffer` shows
the authors were matching the official desktop client's actual strings.

**Answers are sanitised before sending:** every `a=candidate:` line is stripped because candidates
are trickled separately, and the SDP is re-joined with CRLF
(`astrum:.../WebRtcManager.kt:592-604`, called at `:631`). Inbound SDP is normalised
(CRLF/CR→LF, trim, re-join) before `setRemoteDescription` (`:582-590`).

**Note a stale test:** `astrum:ts6-protocol/src/test/kotlin/br/app/astrum/ts6/protocol/Ts6StreamProtocolTest.kt:98-123`
asserts `streamid=` and `msg=` parameter names for `streamsignaling`/`joinstreamrequest`, but the
shipping code uses `id=` and `json=` / `id=` and `clid=`
(`astrum:.../Ts3jSessionClient.kt:254-262`, `:288-295`). Trust the implementation, not that test.

### 2.6 SDP / ICE handling details

**Prefix:** `astrum:`

| Setting | Value | Evidence |
|---|---|---|
| ICE servers | `stun:turn.teamspeak.com:3478`, `stun:turn2.teamspeak.com:3478`, `stun:stun.l.google.com:19302` | `app/.../WebRtcManager.kt:96-100` |
| TURN credentials | **none** — no `.setUsername()`/`.setPassword()`, so the `turn.` hostnames are used as plain STUN | `app/.../WebRtcManager.kt:96-100` |
| SDP semantics | `UNIFIED_PLAN` | `app/.../WebRtcManager.kt:662` |
| ICE gathering | `GATHER_CONTINUALLY` (trickle ICE) | `:663` |
| ICE transports | `ALL` | `:664` |
| Bundle / mux | `BALANCED` / `REQUIRE` | `:665-666` |
| TCP candidates | `ENABLED` | `:667` |
| Encoder factory | `DefaultVideoEncoderFactory(egl, enableIntelVp8Encoder=true, enableH264HighProfile=true)` | `:109-113` |
| Decoder factory | `DefaultVideoDecoderFactory(egl)` | `:114` |
| Capture source | `Camera2Enumerator` (camera) / `ScreenCapturerAndroid` + `MediaProjection` (screen) | `:130-171` / `:194-208` |
| Codec negotiation | left entirely to libwebrtc's default offer ordering; **no** explicit codec preference, no `setCodecPreferences` | **NOT IMPLEMENTED / not found** — no `setCodecPreferences` anywhere in `astrum-ts6` |

**Transceiver layout is contractual.** The broadcaster sends exactly what the official TS6 client
sends, because the desktop renderer depends on it: one video `SEND_ONLY` transceiver with mid group
`"outgoing_video"`, followed by one audio transceiver set **`INACTIVE`** (even when not sending
audio) — `astrum:.../WebRtcManager.kt:449-459`, and the comment explaining where this was captured
is at `:412-418`. The viewer mirrors it with a single video `RECV_ONLY` transceiver
(`:372-379`). The answer is constrained to `OfferToReceiveVideo=true`,
`OfferToReceiveAudio=false` (`:622-625`).

**Two hard-won compatibility quirks, both worth carrying into a desktop client:**

1. **CVO is not honoured by the TS6 desktop renderer.** `urn:3gpp:video-orientation` is ignored, so
   a rotated camera frame arrives sideways. Astrum works around it by **physically rotating the
   I420 buffer** with libyuv inside a `VideoProcessor` before the encoder
   (`astrum:.../WebRtcManager.kt:801-871`, note at `:801-807`). A desktop client capturing from a
   non-rotated source does not need this, but a mobile→desktop port must not reintroduce rotation
   metadata and expect it to work.
2. **Bitrate must be imposed per sender, not via SDP.** `encoding.maxBitrateBps = N` and
   `minBitrateBps = N/4` are set on every video sender of every peer connection
   (`astrum:.../WebRtcManager.kt:308-320`), driven by the preset the user picked
   (`astrum:app/.../TeamSpeakService.kt:1187`, `:1278`; presets at
   `astrum:ts6-protocol/.../Model.kt:96-109`: 720p30/2000 kbps, 1080p30/3500, 720p60/3500,
   480p30/800).

### 2.7 What is missing in the video layer

**Prefix:** `astrum:`

| Missing | Evidence |
|---|---|
| **SFU mode (`mode=2`)** | hardcoded `"mode" to "1"` at `ts6-protocol/.../Ts3jSessionClient.kt:210`; no other value referenced |
| **Simulcast / SVC** | no hit for `simulcast`, `RtpParameters.Encoding.scalabilityMode`, or multiple encodings — only a single `addTransceiver` per stream (`app/.../WebRtcManager.kt:449-455`) |
| **Renegotiation** | empty handler: `override fun onRenegotiationNeeded() {}` (`app/.../WebRtcManager.kt:740`) |
| **Data channels** | `override fun onDataChannel(channel: DataChannel?) {}` (`:739`) — unused |
| **Server-side stream auth (`accessibility=2/3`)** | always sends `accessibility=1` (PUBLIC) (`ts6-protocol/.../Ts3jSessionClient.kt:209`) |
| **Viewer limit** | always `viewer_limit=0` (unlimited) (`:211`) |
| **Audio inside WebRTC** | the audio transceiver is always `INACTIVE` (`app/.../WebRtcManager.kt:456-459`); stream audio is instead mixed into the **Opus voice** channel (§3.1b) |
| **`type=4` EXISTING_SESSION** | Astrum's enum stops at 3 (`ts6-protocol/.../Model.kt:47-51`) |
| **`notifystreaminfo` `viewer` count** | not parsed; only `id/clid/type/name/width/height/fps/bitrate` (`ts6-protocol/.../Ts3jSessionClient.kt:476-485`) |
| **TURN relay** | STUN-only ICE config (§2.6); webspeak3 notes TS6 itself also only configures `stun:` URLs (`webspeak3:web/src/stream.ts:57-61`) |
| **DTLS/SRTP pinning** | none — default libwebrtc fingerprints (`app/.../WebRtcManager.kt:660-668`) |

---

## 3. Audio

**Prefix:** `astrum:`. Paths under `audio-opus/src/main/` unless stated.

### 3.1 Capture

| Fact | Value | Evidence |
|---|---|---|
| API | `android.media.AudioRecord` (**not** OpenSL/AAudio/MediaRecorder) | `java/.../OpusMicrophoneCapture.kt:87-91`, `:119-137` |
| Sample rate | 48 000 Hz | `:476` |
| Channels | mono, `CHANNEL_IN_MONO` | `:89`, `:126` |
| Format | `ENCODING_PCM_16BIT` | `:90`, `:124` |
| AudioRecord buffer | `max(minBufferSize, 3840)` bytes = **40 ms** | `:129`, `:479-480` |
| Read granularity | 480 samples = **10 ms** per `READ_BLOCKING` call | `:477`, `:250-255` |
| Opus frame | 960 samples = **20 ms** (two 10 ms sub-frames accumulated) | `:478`, `:334-337`; `java/.../NativeOpusEncoder.kt:23` |
| Audio source | `VOICE_COMMUNICATION` only in `NOISE_SUPPRESSOR` mode, else `VOICE_RECOGNITION` | `:114-118` |
| Resampling | **NOT IMPLEMENTED / not found** — 48 kHz end to end | grep `resample\|SampleRateConverter\|sonic` → prose only |
| AEC | `AcousticEchoCanceler`, enabled **unconditionally** | `:398-404`, `:452-473` |
| Hardware NS | `NoiseSuppressor`, only in `NOISE_SUPPRESSOR`/`BOTH` modes | `:408-415`, `:418-433` |
| AGC | **NOT IMPLEMENTED / not found** — deliberately disabled, see `README.md:50` |
| Thread | one `"ts3-opus-capture"` thread at `THREAD_PRIORITY_AUDIO` | `:179-182`, `:248` |
| Encoded hand-off | `ArrayBlockingQueue<ByteArray>(3)`, **drops oldest** on overflow | `:28`, `:481`, `:340-343` |
| Pull interface | `isReady()` / `pollEncodedFrame()` → ts3j `Microphone.provide()` | `:214-221`; `ts6-protocol/.../Ts3jSessionClient.kt:626-635` |

**3.1b System audio for screen share** (Android 10+): `AudioPlaybackCaptureConfiguration` +
`AudioRecord` at 48 kHz stereo, downmixed to mono into a 48 000-sample (1 s) ring buffer
(`java/.../SystemAudioCapture.kt:22`, `:33-82`), attached to the mic pipeline
(`app/.../TeamSpeakService.kt:1315-1327`) and mixed **before** denoising
(`java/.../OpusMicrophoneCapture.kt:262-271`). On desktop the equivalent is a WASAPI/loopback
capture injected at the same point.

### 3.2 Opus encode parameters — exact

**Prefix:** `astrum:` — `src/main/cpp/opus_jni.cpp`

```c
opus_encoder_create(48000, 1, OPUS_APPLICATION_VOIP, &error);   // :124
OPUS_SET_BITRATE(64000)            // :134
OPUS_SET_VBR(1)                    // :135
OPUS_SET_VBR_CONSTRAINT(1)         // :136   constrained VBR
OPUS_SET_COMPLEXITY(10)            // :137   maximum
OPUS_SET_SIGNAL(OPUS_SIGNAL_VOICE) // :138
OPUS_SET_MAX_BANDWIDTH(OPUS_BANDWIDTH_FULLBAND)  // :139
OPUS_SET_LSB_DEPTH(16)             // :140
```

Kotlin side fixes `SAMPLE_RATE=48_000`, `CHANNEL_COUNT=1`, `BITRATE=64_000`, `FRAME_SAMPLES=960`
(`java/.../NativeOpusEncoder.kt:20-23`); output buffer 1275 bytes
(`src/main/cpp/opus_jni.cpp:169`).

**Deliberately absent (NOT IMPLEMENTED / not found):** `OPUS_SET_DTX`, `OPUS_SET_INBAND_FEC`,
`OPUS_SET_PACKET_LOSS_PERC`, `OPUS_SET_EXPERT_FRAME_DURATION`, `OPUS_SET_FORCE_CHANNELS`. Exactly
seven `opus_encoder_ctl` calls exist (`opus_jni.cpp:134-140`). The codec type sent to the server is
always `OPUS_VOICE` (`ts6-protocol/.../Ts3jSessionClient.kt:630`) — `OPUS_MUSIC` is received but
never produced by this client.

### 3.3 Opus decode

**Prefix:** `astrum:`

- `opus_decoder_create(48000, 1, &error)` (`src/main/cpp/opus_jni.cpp:37`), Kotlin constants at
  `java/.../NativeOpusDecoder.kt:25-27` (`MAX_FRAME_SIZE = 5760` = 120 ms).
- `opus_decode(decoder, data, len, pcm, frame_size, 0)` — **`decode_fec` is hardcoded 0**
  (`opus_jni.cpp:70-76`). In-band FEC is **NOT IMPLEMENTED / not found**.
- PLC works by passing a **null packet**: `decoder.decode(null, lastFrameSamples)`
  (`java/.../OpusAudioPlayer.kt:510`, `:492`) → JNI null/0-length path (`opus_jni.cpp:61-67`).
- One decoder **per participant**: `TalkerState.decoder` (`java/.../OpusAudioPlayer.kt:436`), created
  lazily per `clientId` (`:297`, `:434`).
- **`packetId` is a 16-bit counter.** It is masked with `0xffff` and unwrapped to a monotonic `Long`
  via signed 16-bit distance: `((cur - prev + 0x8000) and 0xffff) - 0x8000`
  (`java/.../OpusAudioPlayer.kt:633`, `:645-646`, applied at `:534-549`). Packets are decoded in
  strictly ascending unwrapped order (`:487-497`). This is a reusable, protocol-level detail.
- Decoder reset via `OPUS_RESET_STATE` (`opus_jni.cpp:102`) on gap overflow, talkspurt end and
  pipeline reset (`java/.../OpusAudioPlayer.kt:503`, `:521`, `:552`).

### 3.4 Jitter buffer — fixed-depth reorder buffer, **not adaptive**

**Prefix:** `astrum:` — `java/.../OpusAudioPlayer.kt`

There is **no class named `JitterBuffer`**; a repo-wide grep for `jitter` matches only README prose
(`README.md:45`, `:79`). The mechanism is a per-talker sorted reorder map plus a PCM FIFO:

| Element | Value | Evidence |
|---|---|---|
| Reorder map | `TreeMap<Long, QueuedFrame> pending` keyed by unwrapped sequence | `:437` |
| PCM FIFO | `PcmFifo(5_760 samples)` = **120 ms** capacity | `:435`, `:627` |
| Fill target | `PCM_TARGET_SAMPLES = 2_880` = **60 ms**, hardcoded | `:626`, `:481` |
| Initial hold | `INITIAL_HOLD_NANOS = 60 ms` before output starts | `:640`, `:467-472` |
| Max pending | `MAX_PENDING_PACKETS = 64`, exceeded → pipeline reset | `:630`, `:458-460` |
| Forward-gap reset | `MAX_FORWARD_PACKET_GAP = 64` | `:632`, `:453-456` |
| Concealment | max `3` consecutive PLC frames, then hard resync | `:631`, `:502-513` |
| Late/duplicate | silently dropped | `:457` |
| Talker idle eviction | 2 s (`TALKER_IDLE_NANOS`) | `:643`, `:346-355` |
| Ingress queue | `ArrayBlockingQueue(256)` shared by all talkers; overflow drops oldest **and** flags that client for a full pipeline reset | `:23`, `:628`, `:93-98`, `:300-307` |

**Latency is never measured and never adapts** — the two numbers above are the only latency
controls. Effective target ≈ 60 ms FIFO + up to 120 ms backlog + AudioTrack buffer. **A desktop port
should replace this with a real adaptive jitter buffer**; this is the weakest part of the audio
stack and it is *not* worth copying.

### 3.5 Playback

**Prefix:** `astrum:`

- `AudioTrack` in `MODE_STREAM`, 48 kHz mono S16, `PERFORMANCE_MODE_LOW_LATENCY`
  (`java/.../OpusAudioPlayer.kt:417`, `:418`, `:402-413`, `:623`).
- Attributes `USAGE_MEDIA` + `CONTENT_TYPE_SPEECH` (`:651-654`) — deliberately **not**
  `USAGE_VOICE_COMMUNICATION`.
- Buffer `max(minBufferSize, 3840)` bytes = 40 ms (`:416`, `:636`).
- **One** `AudioTrack` and **one** playback thread for the entire app (`:79-82`); per-participant
  state is decoder + FIFO only, mixed in Kotlin (`:309-344`).
- Mix tick 480 samples = 10 ms, absolute scheduling, resync if >50 ms late (`:624`, `:641`, `:642`,
  `:238`, `:223-225`).
- Mixing: `Σ sample×gain`, scaled by `1/sqrt(N)` when N > 1, clamped
  (`java/.../ParticipantGainMixer.kt:17-33`).
- Audio focus: `AUDIOFOCUS_GAIN`, requested only while output is live, loss ⇒ flush + volume 0
  (`java/.../OpusAudioPlayer.kt:54-58`, `:363-383`, `:47-52`).

### 3.6 Mic mute and deafen

**Prefix:** `astrum:`

- **Mic mute = tear the capture down**, not a silent flag. `shouldCaptureMicrophone()`
  (`app/.../TeamSpeakService.kt:922-931`) gates `stopMicrophoneLocked()` (`:938-942`), which calls
  `microphone.stop()` → `record.stop()`, release encoder/denoisers, join the thread with a 1.5 s
  timeout (`java/.../OpusMicrophoneCapture.kt:186-212`, `:378-385`). Every PTT press re-initialises
  the whole chain including the DeepFilterNet model (`:94-137`). **Bad pattern; do not copy.**
- A secondary soft-mute exists for the system-audio case: samples overwritten with zeros and Opus
  still transmits (`java/.../OpusMicrophoneCapture.kt:266-274`, `:52-53`).
- **No mute is signalled to the server.** `client_input_muted` / `client_output_muted` are only
  *parsed* from server events (`ts6-protocol/.../Ts3jSessionClient.kt:671-672`); nothing in
  `astrum-ts6` ever sends `clientupdate`. **NOT IMPLEMENTED / not found.**
  **But the Apache-2.0 ancestor does it, so this is a cheap win for us:** ts3-mobile sends
  `clientupdate client_away= client_away_message= client_input_muted= client_output_muted=`
  (`ts3mobile:ts3-protocol/src/main/kotlin/io/github/ts3mobile/protocol/Ts3jControlCommands.kt:10-26`).
  Copying that gives us correct mute icons on everyone else's roster — module licensing in §5.4.
- **Deafen: NOT IMPLEMENTED / not found.** Repo-wide grep `deafen|Deafen|ensurdec` → zero hits.
  There is only a global playback mute (`playbackMuted`), in memory only
  (`app/.../TeamSpeakService.kt:1515-1518`, `:249`, `:258`, `:305`).

### 3.7 Per-client volume

**Prefix:** `astrum:`

- Per-participant **gain inside the PCM mixer**, not per-track volume
  (`java/.../ParticipantGainMixer.kt:11-35`; gain read per client at `OpusAudioPlayer.kt:320-323`).
- Range `0f..2f` (0–200 %), clamp at `OpusAudioPlayer.kt:637-639`; service model
  `ParticipantAudioSettings(volumePercent, muted)` with
  `gain = if (muted) 0f else volumePercent.coerceIn(0,200)/100f`
  (`app/.../service/TeamSpeakServiceState.kt:38-44`).
- **Keyed by TS3 unique identity, not clid** — `uniqueIdentifier.ifBlank { "session:$id" }`
  (`TeamSpeakServiceState.kt:46-47`, `:61`), so it survives reconnects.
- **Persistence: NOT IMPLEMENTED / not found** — the map is rebuilt empty on every connection
  (`app/.../TeamSpeakService.kt:252-261`). Only the noise-suppression mode is persisted
  (`app/.../service/AudioPreferences.kt:21-23`, `:120-136`).
- UI: slider `0f..200f`, 5 % steps (`app/.../ui/MainScreen.kt:3144-3153`).

### 3.8 Noise suppression — components and licences

**Prefix:** `astrum:`

Order inside the 480-sample loop: *(system audio add / zero)* → transient suppressor → **DeepFilterNet
or RNNoise** → VoiceGate → 960-sample Opus frame (`java/.../OpusMicrophoneCapture.kt:225-352`).

`SuppressionMode` values (`java/.../SuppressionMode.kt:3-10`): `OFF`, `RNNOISE`, `DEEPFILTER`,
`NOISE_SUPPRESSOR`, `BOTH`, `ASTRUM_CLARITY` (default, `app/.../AudioPreferences.kt:63`). Selection is
evaluated **per 10 ms frame**, so it can change mid-stream (`OpusMicrophoneCapture.kt:285-327`).

| Component | Nature | Params / frame size | Licence | Evidence |
|---|---|---|---|---|
| **RNNoise v0.2** | vendored C, static lib, built in-tree | **480 samples @48 kHz = 10 ms**, mono; JNI asserts frame size | **BSD-3-Clause**, © Jean-Marc Valin / Amazon / Mozilla / Xiph | `src/main/cpp/CMakeLists.txt:6-31`; `src/main/cpp/rnnoise_jni.cpp:12`, `:41-44`; `src/main/cpp/third_party/rnnoise/COPYING:1-5`, clause 3 at `:18-20`; provenance `.../rnnoise/VERSION.md:3-8` (upstream `904a876d`) |
| **DeepFilterNet** | external AAR `io.github.kaleyravideo:android-deepfilternet:0.0.8` | 480-sample frame; `attenuationLimit=100.0f`, `postFilterBeta=0.05f` | declared MIT/Apache-2.0 **only** in `THIRD_PARTY_NOTICES.pt-BR.md:35-39` — see gap below | `gradle/libs.versions.toml:15`, `:31`; `audio-opus/build.gradle.kts:47`; `java/.../NativeDeepFilterProcessor.kt:12`, `:92-93` |
| **Transient suppressor** | first-party C++ (no third-party header) | 16-sample sub-blocks (0.33 ms), pre-emph α=0.92, ratio 3.8, min gain 0.015, hold 16 sub-blocks; 48 kHz hardcoded | project code → **AGPL-3.0** | `src/main/cpp/transient_suppressor.cpp:10-15`, `:41-84`; `transient_suppressor.h:29` (`[[maybe_unused]] sample_rate_`) |
| **VoiceGate** | first-party Kotlin expander | open 0.45 / close 0.25, attack 2 frames, hold 16, release 24, min gain 0.02 | project code → **AGPL-3.0** | `java/.../VoiceGate.kt:107-121` |
| Hardware AEC/NS | Android platform | — | platform | `java/.../OpusMicrophoneCapture.kt:398-433` |

**Licence gap worth flagging:** the **English** `THIRD_PARTY_NOTICES.md` lists only ts3-mobile, ts3j,
libopus, RNNoise and AndroidX (`astrum:THIRD_PARTY_NOTICES.md:8-42`). It does **not** mention
DeepFilterNet (present only in the pt-BR file) and does **not** mention the WebRTC library that is a
hard runtime dependency (`astrum:app/build.gradle.kts:87`). `app/src/main/assets/licenses/` contains
only `libopus.txt` and `rnnoise.txt`. So the in-tree licence record is incomplete relative to the
dependency set.

### 3.9 Push-to-talk

**Prefix:** `astrum:`

- Three modes `OFF / PUSH_TO_TALK / CONTINUOUS`, default PTT
  (`app/.../service/TeamSpeakServiceState.kt:12-16`, `:62`).
- Implemented as an **on-screen press-and-hold button** via
  `pointerInput { awaitEachGesture { awaitFirstDown(); …; waitForUpOrCancellation() } }`
  (`app/.../ui/MainScreen.kt:2184-2196`), shown only in PTT mode (`:2117-2121`).
- **No hardware keys, no global hotkeys** — grep for `onKeyEvent|KeyEvent|dispatchKeyEvent|
  adjustStreamVolume` → zero hits. **NOT IMPLEMENTED / not found.**
- On the wire, PTT-up means capture stopped (§3.6), not silence.

---

## 4. Feature coverage vs the official TS6 client

**Prefix:** `astrum:` (with `ts3mobile:` for what the upstream fork already had)

| Area | Astrum TS6 | Evidence |
|---|---|---|
| Connect / reconnect (1–30 s backoff, network-aware) | **Yes** | `app/.../service/ReconnectPolicy.kt`; `app/.../TeamSpeakService.kt:625-683` |
| Channel tree, join, passwords | **Yes** | `ts6-protocol/.../ChannelTree.kt`; join at `Ts3jSessionClient.kt:179-189`; UI `app/.../ui/MainScreen.kt` |
| Channel/member roster, talking+mute indicators | **Yes** | `astrum:CHANGELOG.md:12-22` |
| Voice (Opus) send/receive | **Yes** (§3) | — |
| Whisper **receive** | Parsed, then treated as normal voice | `ts6-protocol/.../Ts3jSessionClient.kt:583-592`; player ignores `isWhisper` (`audio-opus/.../OpusAudioPlayer.kt:87-91`) |
| Whisper **send** | **NO** | `astrum:README.md:59-60` |
| Camera stream (publish + watch) | **Yes** (§2) | `app/.../WebRtcManager.kt` |
| Screen share | **Yes** — `MediaProjection`, system audio on API 29+ | `app/.../TeamSpeakService.kt:1267-1351`, `:1315-1327` |
| Multi-viewer watching (up to 4) | **Yes** | `app/.../TeamSpeakService.kt:1473` (`takeLast(4)`) |
| SFU stream mode | **NO** (§2.7) | hardcoded `mode=1` |
| **Text chat** | **NO** | grep `sendtextmessage\|notifytextmessage\|textmessage` → zero hits in `astrum-ts6`. `astrum:README.md:59-62` states it explicitly. **But the upstream fork has it** — see below |
| **Poke** | **NO** | same |
| **File transfer** | **NO** | grep `filetransfer\|ftinit\|ftinitupload` → zero hits |
| **Permissions admin** | **NO** | grep `permm\|permoverview\|clientgetpermission` → zero hits |
| **Server/channel groups** | **NO** | grep `servergrouplist\|channelgrouplist` → zero hits |
| **Bans** | **NO** | grep `banadd` → zero hits |
| **Privilege keys** | **NO** | grep `privilegekey` → zero hits |
| **Contacts / myTeamSpeak** | **NO** | grep `contact\|buddy\|myteamspeak` → zero hits |
| **Homebase** | **NO** | grep `homebase` → zero hits |
| **E2EE** | **NO** | grep `e2ee` → zero hits. Transport encryption only (§1.2 AES-EAX) |
| Deafen | **NO** (§3.6) | — |
| PTT hotkeys | **NO** (§3.9) | — |
| Per-client volume | **Yes**, but not persisted (§3.7) | — |
| Bookmarks / multi-server | **NO** — one saved-server list, one session | `app/.../data/SavedServer.kt`, `ServerStore.kt` |

**Important for the reuse plan:** the *foundation* repo already implements the TS3 features Astrum
dropped. `vendor/ts3-mobile` has channel chat, private chat, poke, bookmarks, presence, identity
export/restore and an audio diagnostics pipeline:

| Feature | ts3-mobile implementation |
|---|---|
| Text chat (channel + private) | `ts3mobile:app/src/main/java/io/github/ts3mobile/app/chat/SessionChatController.kt:141`; `.../app/chat/ChatStore.kt` |
| `sendtextmessage` command | `ts3mobile:ts3-protocol/src/main/kotlin/io/github/ts3mobile/protocol/Ts3jClientSocket.kt:72-77`; `Ts3jTextCommands.kt`; tests `.../protocol/Ts3jTextCommandsTest.kt` |
| Poke | `ts3mobile:.../protocol/Ts3jClientSocket.kt:79-84`; `app/.../chat/PokeController.kt` |
| Bookmarks, presence, channel info | `ts3mobile:.../protocol/Ts3jClientSocket.kt:86-102`, `:136-154` |
| Jitter/audio pipeline | `ts3mobile:audio-opus/.../TalkerJitterPipeline.kt` |
| Identity backup/restore | `ts3mobile:app/.../identity/IdentityExportController.kt`; `docs/IDENTITY_BACKUP_FORMAT_V1.md` |

`ts3mobile:docs/COMPATIBILITY.md:45` states the boundary plainly: *"TeamSpeak 3 full client
protocol, via a pinned ts3j adapter; not ServerQuery, and does not claim TS5/TS6 full feature
compatibility."* Astrum's delta on top of that is exactly §2 plus §3.8.

---

## 5. Dependency and licence inventory

**Prefix:** `astrum:` unless stated.

### 5.1 Modules

| Module | Role | Evidence |
|---|---|---|
| `app` | Compose UI, Android lifecycle, foreground service, identity vault, WebRTC manager | `README.md:77`; `settings.gradle.kts` |
| `ts6-protocol` | JVM-only ts3j facade, models, session, channel ordering | `README.md:78`; no Android imports |
| `audio-opus` | JNI libopus codec, capture, denoisers, jitter/reorder, Android audio I/O | `README.md:79-80` |

### 5.2 Runtime dependencies

| Component | Coordinate | Version | Purpose | Licence (as declared in-tree) |
|---|---|---|---|---|
| ts3j | `com.github.Manevolent:ts3j` (JitPack) | commit `db57d60c989e399626aa16d921390f5033e6cdeb` | TS3/TS6 wire protocol | **Apache-2.0** declared — `gradle/libs.versions.toml:14`, `:39`; `THIRD_PARTY_NOTICES.md:15-19`. **Contaminated, see §5.4** |
| **WebRTC** | `io.getstream:stream-webrtc-android` | `1.3.10` | video capture/encode/decode/PC | **Apache-2.0** wrapper around **BSD-3-Clause** libwebrtc. `gradle/libs.versions.toml:16`, `:40`; `app/build.gradle.kts:87`. In-tree claim says "BSD 3-Clause" (`THIRD_PARTY_NOTICES.pt-BR.md:41-45`) — **mismatch, and absent from the English notices** |
| **libopus** | `com.fpliu.ndk.pkg.prefab.android.21:opus` (**prebuilt Prefab AAR**) | `1.3.1` | Opus codec | **BSD-3-Clause** — `libs.versions.toml:13`, `:38`; `audio-opus/build.gradle.kts:46`; `THIRD_PARTY_NOTICES.md:21-26`; text in `app/src/main/assets/licenses/libopus.txt`. **Neither vendored nor downloaded**: resolved by `find_package(opus REQUIRED CONFIG)` (`audio-opus/src/main/cpp/CMakeLists.txt:4`) against the AAR, `prefab = true` (`build.gradle.kts:24`); zero `FetchContent`/`ExternalProject`/`curl` hits repo-wide |
| RNNoise | vendored C source in-tree | v0.2 @ `904a876d` | neural denoise | **BSD-3-Clause** — `CMakeLists.txt:8-21`; `third_party/rnnoise/COPYING:1-32`; `THIRD_PARTY_NOTICES.md:28-34` |
| DeepFilterNet | `io.github.kaleyravideo:android-deepfilternet` (AAR, bundles model) | `0.0.8` | neural denoise (**default-on**) | **Apache-2.0** per its POM; upstream is MIT OR Apache-2.0. Declared **only** in `THIRD_PARTY_NOTICES.pt-BR.md:35-39`; **absent from the English notices**, no vendored licence text |
| AndroidX / Compose | `androidx.*`, `compose-bom` | BOM `2024.10.01` | UI | Apache-2.0 — `libs.versions.toml:4-8`, `:19-30`; `THIRD_PARTY_NOTICES.md:36-42` |
| Kotlin / coroutines | `org.jetbrains.kotlinx:kotlinx-coroutines-*` | `1.9.0` | concurrency | Apache-2.0 — `libs.versions.toml:9`, `:32-34` |
| ts3j transitives | BouncyCastle `bcprov-jdk15on:1.67`, `commons-lang:2.6`, `dnsjava:2.1.8`, `ini4j:0.5.1` | — | crypto, SRV/DNS, INI | BC Licence (MIT-style), Apache-2.0, **BSD-2-Clause**, Apache-2.0 — `ts3j:pom.xml:37-59`; kept in the APK by `app/proguard-rules.pro:2-8` |

### 5.3 Native build

`externalNativeBuild` via CMake 3.22.1, NDK `27.0.12077973`, `ANDROID_STL=c++_shared`,
`prefab = true` (`astrum:audio-opus/build.gradle.kts:9`, `:15-32`). Two CMake targets:
static `rnnoise` from 10 vendored `.c` files, and shared `ts3opus_jni` from four `.cpp` files
(`astrum:audio-opus/src/main/cpp/CMakeLists.txt:8-44`). libopus is linked as prefab
`opus::libopus.a` — **not** vendored source, **not** downloaded at build time (URL/version
resolution happens in the prefab package, outside the tree). ABIs: **NOT DETERMINED / not found**
in the tree — there is no `abiFilters`, so Gradle defaults apply.

RNNoise is fully vendored — 29 files under `third_party/rnnoise/`, with the model dominating
(`src/rnnoise_data.c`, ~29.5 MB) — and provenance is pinned in
`astrum:audio-opus/src/main/cpp/third_party/rnnoise/VERSION.md:1-13` (upstream `904a876d`, model
`0b50c45`, archive SHA-256 recorded). Note the ancestor does this differently: `ts3-mobile`
`FetchContent`s **Opus 1.6.1** with a pinned `URL_HASH`
(`ts3mobile:audio-opus/src/main/cpp/opus-source.cmake:1-24`) instead of using a prebuilt AAR — a
strictly better supply-chain posture, and the one to copy.

DeepFilterNet is **not** built here: it arrives prebuilt inside its AAR, with no CMake reference
anywhere.

### 5.4 AGPL contamination analysis

`astrum-ts6` is **AGPL-3.0** (`astrum:LICENSE:1-5`, `astrum:README.md:124-126`). Three distinct
categories matter for reuse:

1. **Pure third-party files, permissively licensed, safe to lift individually.**
   `audio-opus/src/main/cpp/third_party/rnnoise/**` is upstream Xiph RNNoise under BSD-3-Clause
   (`.../rnnoise/COPYING:1-20`) — use it directly from Xiph, not from here.
2. **Permissively-licensed dependencies consumed as binaries.** ts3j (Apache-2.0), libopus
   (BSD-3), stream-webrtc-android (BSD-3), AndroidX (Apache-2.0). Taking the *dependency* is
   unencumbered; the Apache-2.0 of ts3j also means **ts3j itself is a legitimate alternative
   integration path** for a non-AGPL client — but note ts3j alone gives you no TS6 streams.
3. **The project's own code is AGPL-3.0, and that is where all the TS6 value lives.**
   `WebRtcManager.kt`, `Ts3jSessionClient.kt`, `TeamSpeakService.kt`, the transient suppressor,
   VoiceGate, the session/snapshot store — all first-party, all AGPL
   (`astrum:LICENSE:1-5`). There is no separate per-file licence header granting anything else
   (e.g. `audio-opus/src/main/cpp/transient_suppressor.h:1-2` opens directly with the include
   guard). **Copying these into a proprietary desktop client is an AGPL violation.**

**The correct use of this repo for a proprietary product is documentary, not verbatim:** read
`Ts3jSessionClient.kt` + `WebRtcManager.kt` to learn the *wire contract* (command names, parameter
names, JSON shapes, transceiver layout, ICE config), then implement it independently. Wire-protocol
facts are not copyrightable; this file and `docs/webspeak.md` are the distilled version of exactly
that knowledge. If we *do* want to lift code, `webspeak3` (MIT) is the better source — see §7.

### 5.5 The licence hazard that is **not** Astrum's fault — `ts3j` itself

This one affects us whichever client we write, so it deserves its own subsection. `ts3j` is declared
Apache-2.0 (`ts3j:LICENSE:1-3`), but it is not clean:

| Problem | Evidence (verified directly) |
|---|---|
| **QuickLZ 1.5.0 is GPL-or-commercial, not Apache** | header in `ts3j:src/main/java/com/github/manevolent/ts3j/util/QuickLZ.java:1-13` — *"QuickLZ can be used for free under the GPL 1, 2 or 3 license … or under a commercial license"*, © Lasse Mikkel Reinhold |
| **QuickLZ is on the live packet path, not vestigial** | compress on send `ts3j:.../protocol/packet/fragment/Fragments.java:39`; decompress on receive `ts3j:.../packet/fragment/PacketReassembly.java:95` and `ts3j:.../socket/client/AbstractTeamspeakClientSocket.java:587` |
| **Provenance is a port of OSL-3.0 code** | `ts3j:README.md:3` — *"an adaptation of Splamy's C# TS3Client source code"*; Splamy/TS3AudioBot is OSL-3.0 |
| **A bundled NaCl/Ed25519 port has no licence header at all** | `ts3j:src/main/java/Punisher/NaCl/**` (no header blocks) |

**Consequences for our project, stated conservatively:**

1. **We must not assume "ts3j is Apache-2.0, therefore linking it is safe."** That claim rests on a
   file that contains GPL-or-commercial code reachable from every fragmented or compressed packet.
   Whether a linked combined work must be GPL is a legal question, not an engineering one — but the
   risk is real and cheap to avoid.
2. **Astrum-ts6 discloses none of this.** `astrum:THIRD_PARTY_NOTICES.md:15-19` says only
   "ts3j — Apache-2.0". So the "just use ts3j" path that both mobile clients took is exactly the
   path we should not take without a decision.
3. **Mitigations, in order of preference:** (a) use a client library with a clean, single-licence
   pedigree — `tsclientlib` (Rust, `Moepchi/tsclientlib`) is the obvious candidate: an independent
   team selected it for exactly this role in `webspeak3:.gitmodules:1-4`, and its licence can be
   checked in its own tree rather than inferred from a wrapper's notice file; (b) keep ts3j but
   **replace `QuickLZ` with a clean-room implementation of the compression** (the algorithm is only
   used for packet compression, and the call sites are narrow — `Fragments.java:39`,
   `PacketReassembly.java:95`, `AbstractTeamspeakClientSocket.java:587`) and get the Punisher.NaCl
   provenance resolved; (c) buy a QuickLZ commercial licence.

   Note this doc cannot resolve which is right: `tsclientlib` is **not checked out** in this
   workspace (submodule present in `webspeak3:.gitmodules:1-4`, empty on disk), so its licence and
   TS6-stream maturity are unverified here. That check is the first task of Phase 0.5.

### 5.6 Other compliance gaps in `astrum-ts6` (do not inherit these)

| Gap | Evidence |
|---|---|
| `CONTRIBUTING.md` says contributions are under **Apache-2.0** while `LICENSE` is AGPL-3.0 | `astrum:CONTRIBUTING.md:33-36` vs `astrum:LICENSE:1-5` |
| The APK **strips** `META-INF/LICENSE` and `META-INF/NOTICE` from every dependency | `astrum:app/build.gradle.kts:73-81` |
| No Apache-2.0 licence **text** anywhere in-tree; notices only link to it | `astrum:THIRD_PARTY_NOTICES.md:12`, `:19` |
| `app/src/main/assets/licenses/` has only `libopus.txt` + `rnnoise.txt`, and **no code reads them** | `astrum:app/src/main/assets/licenses/` |
| Zero per-file SPDX/copyright headers on any Kotlin or C++ file | grep `SPDX-License-Identifier\|Copyright` over `astrum-ts6/**/*.kt` → no matches |
| No dependency locking; ts3j's four transitives float | no `gradle.lockfile` in the repo (the ancestor `ts3-mobile` does use `LockMode.STRICT`) |

---

## 6. Reusability verdict

**Prefix:** `astrum:`

### 6.1 Portable nearly verbatim to a desktop / KMP client

These have no Android imports and encode protocol knowledge that is expensive to rediscover:

| File | Why it ports | Android coupling |
|---|---|---|
| `ts6-protocol/src/main/kotlin/br/app/astrum/ts6/protocol/Ts3SessionClient.kt` (35 lines) | The whole session interface + listener contract, incl. all TS6 stream methods | none |
| `ts6-protocol/.../Ts3jSessionClient.kt` (716 lines) | The single most valuable file: every TS6 command/notify name, param spelling, fallback chains, `Ts3Parameter` CRLF escaping | none — JVM only (needs ts3j) |
| `ts6-protocol/.../Model.kt` | `ServerConfig` (port 9987), `StreamType`, `StreamPreset` bitrate table, `Ts6StreamInfo`, `Ts6StreamSignaling`, `SessionSnapshot` | none |
| `ts6-protocol/.../Voice.kt` | `VoiceCodec`, `VoiceFrame`, `EncodedVoiceSource` | none |
| `ts6-protocol/.../SessionSnapshotStore.kt` | Thread-safe channel/participant/stream registry | none |
| `ts6-protocol/.../ChannelTree.kt` | Channel ordering | none |
| `ts6-protocol/.../ConnectionFailure.kt` | Retryable-vs-terminal classifier (`IOException`/`Timeout` retry, `CommandException` terminal, "too many clones" special case) | none |
| `ts6-protocol/.../Ts3IdentityCodec.kt` | Identity export/import via ts3j | none |
| `audio-opus/.../ParticipantGainMixer.kt` | Sample mixing + `1/sqrt(N)` normalisation | none |
| `audio-opus/.../VoiceGate.kt` | Expander state machine | none |
| `audio-opus/.../SuppressionMode.kt` | Mode enum | none |
| `audio-opus/.../AudioRouting.kt` | Route model types | none |
| `app/.../service/TeamSpeakServiceState.kt` | State model incl. `ParticipantAudioSettings.gain` | none |
| `audio-opus/src/main/cpp/transient_suppressor.{h,cpp}` | First-party DSP; plain C++, no JNI | none (only the `_jni.cpp` glue is Android) |
| `app/.../WebRtcManager.kt` **logic** | The signalling state machine is the crown jewel — but see caveat | Android types throughout (see 6.2) |

### 6.2 Irreducibly Android — must be rewritten, not ported

| File | Blocking APIs |
|---|---|
| `app/.../WebRtcManager.kt` (781 lines) | `org.webrtc.android.*` (`Camera2Enumerator`, `ScreenCapturerAndroid`, `SurfaceTextureHelper`, `EglBase`, `GlSurfaceView`); `Context`; `MediaProjection`; `android.util.Log` |
| `app/.../WebRtcVideoView.kt` | `SurfaceViewRenderer`, Compose |
| `audio-opus/.../OpusMicrophoneCapture.kt` | `AudioRecord`, `AudioEffect`, `AcousticEchoCanceler`, `NoiseSuppressor`, `AudioDeviceInfo`, `Context`, `RECORD_AUDIO` permission |
| `audio-opus/.../OpusAudioPlayer.kt` | `AudioTrack`, `AudioManager`, `AudioFocusRequest`, `AudioAttributes` |
| `audio-opus/.../AudioDeviceRouter.kt` | `AudioManager.setCommunicationDevice`, Bluetooth SCO, `Handler(Looper)` |
| `audio-opus/.../SystemAudioCapture.kt` | `MediaProjection`, `AudioPlaybackCaptureConfiguration` |
| `audio-opus/.../NativeDeepFilterProcessor.kt` | Android-only AAR (`io.github.kaleyravideo:android-deepfilternet`) |
| `audio-opus/.../Native*` (Encoder/Decoder/RnNoise/TransientSuppressor) | `System.loadLibrary("ts3opus_jni")` — the **C++ underneath is portable**, only the JNI glue + Gradle prefab packaging are Android |
| `app/.../TeamSpeakService.kt` (1666 lines) | Android `Service`, foreground-service types, notification, `ConnectivityManager`, `OrientationEventListener` — but ~60 % of it is pure orchestration logic worth reading |
| `app/.../IdentityVault.kt` | `AndroidKeyStore`, DataStore |
| `app/.../ui/**` (`MainScreen.kt` 3132 lines, `FullscreenStreamOverlay.kt`, `StreamConfigDialog.kt`) | Compose |

**Practically:** the split is clean. `ts6-protocol` (≈1000 lines) is already a KMP-ready protocol
module; `audio-opus` is ~40 % portable Kotlin/C++ (mixer, gate, DSP) and ~60 % Android I/O;
`app` is Android-only except `WebRtcManager`'s signalling logic, which must be re-expressed against
a desktop WebRTC binding (libwebrtc JNI, `webrtc-rs`, or browser `RTCPeerConnection`).

---

## 7. Independent corroboration — `webspeak3` (4th repo, MIT)

Not part of the original scope, but it is a **second, independent implementation of the same TS6
stream protocol** in this workspace, and it settles questions Astrum leaves ambiguous. Prefix
`webspeak3:` = `vendor/webspeak3`, MIT licensed. Treat as corroboration, not primary evidence.

| Finding | Evidence |
|---|---|
| **"TS6's P2P mode is a mesh: there is no SFU in the path, so every viewer gets its own peer connection and its own copy of the encoded media."** | `web/src/publish.ts:15-16` |
| `setupstream` enums decoded from TS6's own UI bundle: `StreamSource{0,1,2,3,4}`, `StreamAccess{0..3}`, **`StreamMode{0,1=P2P,2=SFU}`** | `web/src/publish.ts:20-45` |
| "Only P2P is implemented; SFU needs the mediasoup path" | `web/src/publish.ts:55` |
| Same five-step exchange Astrum uses: `setupStream` → `notifyjoinstreamrequest` (one per viewer) → offer in `respondJoinStream` → `{cmd:"answer",args:{answer}}` → trickle `{cmd:"iceCandidate"}` | `web/src/publish.ts:7-13` |
| Same three ICE servers, and the observation that **TS6 itself only ever configures `stun:` URLs so it never gathers a relay candidate** — but accepts a relay candidate offered by a peer | `web/src/stream.ts:57-69` |
| Field-name quirk: `notifystreaminfo` sends `accessibility` but `notifystreamstarted` shortens it to `access`; only `notifystreaminfo` carries `viewer` | `web/src/stream.ts:71-89` |
| "a real TS6 screen share sends 3" (type = WINDOW) | `gateway/src/ts3/connection.ts:30` |
| One PC per viewer on the publisher, with pending-candidate queue per viewer | `web/src/publish.ts:86-99`, `:279` |

**Takeaway:** two independent teams converged on the identical wire format, which raises confidence
that the §2 contract is complete and correct. `webspeak3` is also the only one of the two whose
licence (MIT) permits lifting code.

---

## 8. REUSE PLAN for a desktop TS6 client

**Target architecture** — reuse `ts6-protocol` as the protocol core, keep the signalling contract
identical, replace every Android I/O edge.

**Phase 0 — pin the contract (no code).** Lift the wire contract from `§2.2 + §2.3 + §2.5` into a
spec: 7 commands, 8 notifies, 3 JSON shapes, 2 numeric enums. This is the whole TS6 delta and it is
~2 pages.

**Phase 0.5 — settle the protocol-library licence before writing code.** This gates everything else.
Decision required between: (a) `tsclientlib` (Rust, clean pedigree, what webspeak3 uses),
(b) `ts3j` with QuickLZ replaced by a clean-room implementation plus NaCl provenance resolved, or
(c) vendor QuickLZ commercially. **Do not "just link ts3j" on the assumption it is Apache-2.0** —
§5.5.

**Phase 1 — protocol core (KMP, days not weeks).**
Port `ts6-protocol/*` almost verbatim: `Ts3SessionClient`, `Ts3jSessionClient`, `Model`, `Voice`,
`SessionSnapshotStore`, `ChannelTree`, `ConnectionFailure`, `Ts3IdentityCodec`. **Keep
`Ts3Parameter`'s no-trim escaping; it is load-bearing for SDP.** Keep the `channelsubscribeall`
failure tolerance (§1.5) and the `initserver`→`channellistfinished` connect barrier (§1.2).
→ Deliverable: headless CLI that connects to a TS6 server, joins a channel, lists streams, and
sends/receives a stream offer.

**Phase 2 — signalling engine (the one thing to rewrite carefully).**
Re-implement `WebRtcManager`'s state machine on a desktop WebRTC binding, preserving exactly:
per-viewer PC keyed `clid:streamId`; viewer adds RECV_ONLY video transceiver **before** the offer;
publisher adds video SEND_ONLY (`outgoing_video`) + **INACTIVE audio** transceiver; offer goes into
`respondjoinstreamrequest`, answer + ICE go into `streamsignaling`; pending-candidate queue until
remote description; strip `a=candidate:` from answers; per-sender bitrate caps.
Do **not** port the Android camera orientation hack (CVO workaround) unless the desktop capture
source can rotate.

**Phase 3 — audio.**
- Opus: reuse the exact encoder parameters from §3.2 (48 kHz mono, VOIP, 64 kbps, constrained VBR,
  complexity 10, fullband) through a native libopus binding; reuse the JNI-free parts of the C++
  (transient suppressor) as-is, and take RNNoise from Xiph directly (BSD-3).
- Replace the fixed 60 ms reorder buffer with a real adaptive jitter buffer — the current design is
  the weakest link (§3.4).
- Reuse `ParticipantGainMixer` + `VoiceGate` verbatim (pure Kotlin). Key per-user volume by
  **unique identity, not clid** (§3.7), and **persist it** (Astrum does not).
- Capture/playback: desktop APIs (WASAPI/CoreAudio/ALSA or miniaudio). Send the `clientupdate`
  presence command from §3.6 so mute/deafen show correctly on other clients' rosters — Astrum never
  does, and the ts3-mobile implementation is 15 lines and Apache-2.0.
- PTT: global hotkey (Astrum has none, §3.9). Also fix the stop/start-per-press model: keep the
  capture device open and gate at the encoder.

**Phase 4 — close the feature gaps cheaply by mining `ts3-mobile`, not Astrum.**
Text chat, private chat, poke, bookmarks, presence, identity backup are **already implemented** in
`vendor/ts3-mobile` (Apache-2.0 — §4 table). Port `Ts3jTextCommands`, `Ts3jControlCommands`,
`SessionChatController`, `ChatStore`, `PokeController` from there. That is the fastest route to
"doesn't feel crippled vs the official client", and it is the only route that keeps us clear of
Astrum's AGPL.

**Explicitly do not copy:** `OpusMicrophoneCapture` stop/start-per-PTT, the non-adaptive jitter
buffer, the Android rotation processor, `TeamSpeakService`'s Android lifecycle, and Compose UI.

**Licence posture.** AGPL-3.0 permits reading and reimplementing. Two safe paths for *Astrum's own*
code: (a) clean-room re-implementation from the §2 contract (recommended — the contract is small),
or (b) derive from `webspeak3` (MIT). Any verbatim copy of `astrum-ts6` first-party code makes our
desktop client AGPL-3.0. Separately, and independently of Astrum, the **ts3j/QuickLZ problem in §5.5
must be decided before Phase 1**, because it constrains the protocol library we build on rather
than any individual file we copy.

---

## 9. Open questions / unverified

| Question | Status |
|---|---|
| Does the official TS6 **server** act as SFU when `mode=2`? | **Not verified.** Both open-source clients only implement `mode=1`. `webspeak3:web/src/publish.ts:55` says SFU "needs the mediasoup path", implying a server-side SFU exists, but no source here proves the server-side behaviour or the media transport it uses |
| Which codec does the official client actually negotiate (H.264 vs VP8/VP9)? | **Not determined.** Astrum sets no codec preference (`WebRtcManager.kt:109-113`) and never inspects the negotiated codec |
| Does the TS6 server accept `client_version = "3.?.?"` indefinitely, or is there a minimum-version gate? | **Not verified.** Astrum works in practice (it is a shipping client), but no source states the server's `virtualserver_min_client_version` policy for stream commands |
| `setupstream`'s `reason=1` on `stopstream` — semantics of other values | **Not found.** Only `reason=1` is ever sent (`Ts3jSessionClient.kt:248`) |
| `type=4` `EXISTING_SESSION` meaning | **Not found.** Appears only in webspeak3's enum (`web/src/publish.ts:29`) |
| Whether `notifystreaminfo` must be requested or is pushed | Both: Astrum proactively sends `requeststreaminfo` per participant (`Ts3jSessionClient.kt:358-369`, `:410-414`) and also handles unsolicited `notifystreamstarted` |
| ABIs built | **Not found in tree** (no `abiFilters` in `audio-opus/build.gradle.kts`) |
| Does linking `ts3j` (GPL-or-commercial QuickLZ) force our client to be GPL? | **Not determinable here — legal question.** We only establish the facts (§5.5): the GPL/commercial header, the three live call sites, the OSL-3.0 provenance claim, and the unlicensed NaCl port |
| Licence of `Punisher.NaCl` inside ts3j | **NOT DETERMINED / not found** — 20 files, no header, no notice |
| Exact licence of `io.getstream:stream-webrtc-android` (Apache-2.0 per POM vs "BSD 3-Clause" in-tree) | **Conflicting sources**; POM and repo LICENSE say Apache-2.0, `THIRD_PARTY_NOTICES.pt-BR.md:41-45` says BSD-3-Clause |
| Whether `client_version = "3.?.?"` is *why* no TS6 capability negotiation is needed, or merely tolerated | **Not verified** — no source states the server's policy; only that a working client does it |
