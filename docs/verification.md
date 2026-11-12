# Historical interoperability verification

These are condensed English summaries of the earlier local experiment log committed on 2026-11-12, not experiments rerun during the maintenance audit. Machine-specific paths, user identifiers, device names, raw captures, and repeated speculative diagnoses have been removed from this document. Original historical commits still contain the earlier content and need privacy review before publication.

The experiments used the proprietary TeamSpeak Server **6.0.0-beta13.1** on a Windows local testbed unless stated otherwise. They establish observations for that setup, not universal protocol or security guarantees. Current offline verification is in [AUDIT.md](AUDIT.md).

## V-01 — Independent client login

The standalone `recon/probe-ts6` logged into `127.0.0.1:9987` and received initial bookkeeping. Historical server output identified protocol version 8 and 32 available client slots. Reproduction requires a separately configured server; it is not an offline unit test.

## V-02 — Snapshot synchronization

`ts6-cli probe 127.0.0.1:9987 4` reported server, channel, and client snapshots. Account identifiers from the capture are intentionally omitted.

## V-03 — Stream command acceptance

`ts6-cli stream 127.0.0.1:9987 --type 2 --bitrate 2500` received a successful `setupstream` response and `notifystreamstarted`. This proves command acceptance in that setup, not media delivery or a WebRTC implementation.

## V-04 — Permissions, creation, and movement

`channelcreate` without sufficient rights returned `PermissionsClientInsufficient`. After granting test privileges, channel creation succeeded and joining was checked against the snapshot. Do not publish or reuse privilege tokens from a testbed.

## V-05 — TS3 text acceptance without delivery

Two clients with distinct identities in one channel tested `sendtextmessage`. The sender received success, but the receiver saw no corresponding message notification. Accepted commands are not proof of chat delivery.

## V-06 — Matrix research

Earlier resource inspection of official client 6.0.0-beta5.1 found Matrix API strings, `org.matrix.login.jwt`, and `notifychatlogintoken`. This is historical research evidence for an external chat plane, not a supported integration in this client.

## V-07 — File service research

Server configuration exposed S3-related options and a services WebSocket. Legacy file transfer availability does not prove this client supports official TS6 file/avatar workflows.

## V-08 — Query interfaces

The earlier generated beta13 configuration listed SSH query (10022), HTTP query (10080), and HTTPS query (10443), disabled by default. Consult the installed server's own configuration; do not enable these on an exposed interface for routine client testing.

## V-09 — WebQuery assertions

Local WebQuery `whoami` and `version` calls succeeded with a privately supplied `x-api-key`. No key is retained in this summary.

## V-10 — Testbed limitations

Duplicate identities were rejected as clones, repeated connections triggered flood bans, and database reset commands produced errors. Do not delete or reset a user's database to reproduce an experiment; use a fresh isolated testbed with explicit authorization.

## V-11 — Stream notification research

Earlier official-client string inspection found stream-related commands including `joinstreamrequest`, `respondjoinstreamrequest`, `streamsignaling`, and `notifystreamattendees`. Strings alone do not verify endpoint semantics or current support.

## V-12 — Opus transport

A synthetic 440 Hz tone sent with `audio-send` and decoded with `audio-recv` yielded 255 received packets and approximately 0.32 RMS. This was a historical server/codec experiment, not a current latency or packet-loss benchmark.

## V-13 — Continuous polling

Repeated short receive timeouts were associated with no live delivery and a final burst. Continuous polling delivered 255 of 259 packets during transmission. Preserve single-owner continuous polling; causality beyond the observed setup was not established.

## V-14 — CMake compatibility

Bundled Opus requested a CMake compatibility version rejected by CMake 4. `CMAKE_POLICY_VERSION_MINIMUM=3.5`, set by the build helper, allowed the build. This workaround is also exercised by the current offline build.

## V-15 — Playback and synthetic duplex

Earlier playback testing reported 235 packets, 672576 rendered frames, and zero queue drops on a real output device. Synthetic duplex exercised transport without proving acoustic microphone capture quality.

## V-16 — Duplex scheduling

The custom poll path multiplexed microphone and network activity; a closed microphone channel previously caused a hang without a deadline. Keep wake/deadline behavior explicit and bounded.

## V-17 — Initial PTT/capture status

At this stage PTT had not yet been implemented and real microphone content had not been verified. V-18 supersedes the PTT status only.

## V-18 — Windows PTT experiment

A global F13 key test reported 73 transmitted frames, 73 received frames, and 604 suppressed frames. This used Windows key polling and the transmit gate; no other platform was tested.

## V-19 — Missing chat service

A synchronized local self-hosted server connection did not receive `notifychatlogintoken` in a six-second window. Matrix chat remains unimplemented and requires a suitable service; a missing notification does not prove all TS6 deployments behave identically.

## V-20 — Browser UI and bridge

React UI plus a local WebSocket bridge provided connection, channels/clients, and audio controls. Early runtime descriptions in the original log are outdated: current source uses a multi-thread Tokio runtime with the session future owned by the blocking thread. Tauri packaging is not implemented.

## V-21 — Superseded bridge receive diagnosis

An apparent inability to receive during transmission was reported. V-23 identified measurement flaws; V-25 showed ordering-dependent reception. Do not treat this section as evidence of a currently confirmed bridge-specific bug.

## V-22 — Capture callbacks versus audio quality

An input device appeared later in the session and callbacks/encoding were observed. Real acoustic capture quality was still not verified. Device identities are omitted for privacy.

## V-23 — Measurement corrections and bounded progress

Counters accumulated across sessions; sender startup under load took 20–30 seconds; abandoned clients affected measurements. The code added per-session counter resets, a three-second disconnect drain limit, a sync hard deadline and bounded pending queue, bounded microphone batches, and absolute tone scheduling. These changes did not establish the proposed runtime explanation for all receive symptoms. V-25 supersedes that diagnosis.

## V-24 — Unresolved tone-rate anomaly

A receiver counted 1347 packets in eight seconds (about 168/s), while the UI showed 1779 transmitted frames. The expected nominal cadence is 50/s. Pacing changes did not conclusively explain the discrepancy. Multiple producers, duplicate sends, or measurement contamination remain hypotheses requiring controlled retesting.

## V-25 — Ordering-dependent receive observation

| Historical experiment | Observation |
| --- | --- |
| Listener connected before speaker | 227 received packets |
| Speaker already transmitting before listener | 0 received packets in a 6–7 second window |
| Bridge listener before speaker | 389 received packets for 389 sent frames |

Changing output hardware flags, subscribing to the channel, transmitting as a late joiner, and changing identity did not fix the earlier late-join observation. A later speaker produced a burst rather than verified real-time delivery. The original log attributed this to the beta server, but no packet-level comparison with an independent client was recorded. Treat server recipient caching as a hypothesis, not a proven general root cause.

For new experiments, connect the listener first, confirm sender startup, compare counter deltas, isolate other peers, and capture sanitized command/version/order/result evidence. See [development](DEVELOPMENT.md#manual-serverhardware-testing).
