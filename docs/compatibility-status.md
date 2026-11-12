# TeamSpeak 6 compatibility status

Date: 2026-11-12. This client is intended to connect directly to official TeamSpeak 6 servers. It is not a new server implementation, and it is not yet a complete replacement for the official client.

## Fresh verification

The local server identified itself through the client snapshot as **6.0.0-beta13.1**. A fresh CLI handshake succeeded, returned one channel and included the connecting client. Version reporting establishes the responding software's self-identification, not an independent binary authenticity proof.

The rebuilt bridge was launched on a separate local port using its own ignored profile. The actual UI JSON protocol successfully exercised connect, snapshot, nickname change, input/output mute and unmute, refresh, and disconnect against that server. No microphone or playback source was enabled. `scripts/check_ts6_bridge.py` reproduces this integration and stops only its own bridge process. The ordinary smoke test also passed HTTP assets, permitted/hostile WebSocket access, and oversized message rejection. These are service/wire tests, not a rendered-browser interaction or accessibility audit.

The sender-correlated live voice test passed with a first positive receiver report at **0.171 seconds** after sender readiness. This uses synthetic Opus audio and is not an acoustic microphone or latency certification. Only the tested local server release/environment is covered; arbitrary TS6 servers, permissions, network conditions, and future releases remain unverified.

An offline test interleaving two independent encoded voices reproduced decoder-state contamination when a single decoder was shared. `PeerDecoders` now keeps independent Opus state per sender in the CLI receiver. The fixed test compares each interleaved output against a dedicated reference decoder; **34 Rust tests passed**. The playback mixer already uses the upstream per-peer audio handler; no claim is made that this CLI fix changed production playback mixing.

UI TypeScript/production build and npm audit passed (zero reported UI vulnerabilities). Repository policy and HTTP/WebSocket smoke checks passed. The Rust cryptography advisory was subsequently resolved through the migration and verification recorded in [PUBLICATION.md](PUBLICATION.md).

## Feature matrix

| Feature | Current evidence / remaining gap |
| --- | --- |
| Official TS6 server login and identity | Fresh local beta13.1 handshake/snapshot verified |
| Channels and client state | Snapshot and UI-wire refresh verified; broad permission scenarios not repeated |
| Nickname and mute controls | Fresh bridge-to-server integration verified |
| Voice transport | Fresh synthetic sender-correlated reception verified |
| Multiple voice senders | Independent decoder state regression fixed; sustained multi-user acoustic calls not verified |
| Microphone, speakers and PTT | Implemented, partly covered by unit/historical tests; fresh acoustic end-to-end validation remains open |
| Browser UI | Builds; HTTP/wire integration passes; full rendered interaction/accessibility validation remains open |
| Text chat | UI disabled; delivery/integration not implemented and not claimed |
| Video/screensharing and files | Not implemented |
| Native desktop packaging | Not implemented |
| Production security readiness | Experimental; current dependency/history remediation and remaining reporting/maintenance work are documented in PUBLICATION.md |

## Research and chat uncertainty

Primary references checked during this continuation:

- Official [server releases](https://github.com/teamspeak/teamspeak6-server/releases) list beta13.1 and version-specific changes. Future compatibility must be verified rather than assumed.
- Official [server configuration](https://github.com/teamspeak/teamspeak6-server/blob/main/CONFIG.md) documents UDP voice and query controls. No administrative interfaces or credentials are required by the new bridge regression.
- A [community issue in the official repository](https://github.com/teamspeak/teamspeak6-server/issues/110) distinguishes voice-channel chat from persistent Matrix-backed group chats. Its description is user-reported, not an authoritative protocol specification. It is a reason to investigate the distinction, not proof of a new API.
- The [Matrix interoperability discussion](https://github.com/teamspeak/teamspeak6-server/issues/31) concerns a separate service integration. The historical failure of `sendtextmessage` in this checkout does not establish that every TS6 text workflow is Matrix-only.

The UI now says chat is not implemented rather than asserting a universal external-Matrix requirement. Before implementing chat, reproduce voice-channel text delivery with controlled peers and compare the official client's signaling using authorized observation. Separately establish the supported authentication/service requirements for persistent group chats. Do not invent endpoints, tokens, homeserver support, or delivery guarantees.

## Repeatable checks

From the repository root, after building the binaries and UI:

```powershell
python scripts/smoke_bridge.py --binary .audit-local/target/debug/ts6-bridge.exe
python scripts/check_ts6_bridge.py --binary .audit-local/target/debug/ts6-bridge.exe
python scripts/check_voice_progress.py --binary .audit-local/target/debug/ts6-cli.exe
python scripts/check_repository.py
```

The last two server integration helpers require the existing trusted local server on port 9987 and create ignored test identities. They are manual tests, not offline CI. Keep .audit-local private and out of releases. Test reports are application-level evidence, not packet captures or official-client equivalence.

## Fresh voice-channel text experiment

With a continuously polling independent listener, three `sendtextmessage` variants
were tested: channel mode with target omitted, target set to the current channel
ID (1 in this test snapshot), and legacy target 0. All command processes exited
successfully; the independent receiver observed none of the three unique fixture
messages during its 20-second observation interval and exited successfully.
This reproduces acceptance without observed delivery for the tested setup. It
does not identify the underlying TS6 text protocol or establish that every server
requires Matrix. No UI chat was enabled on the strength of command acceptance.
Private raw evidence is under the ignored `.audit-local/chat-compatibility/`.
