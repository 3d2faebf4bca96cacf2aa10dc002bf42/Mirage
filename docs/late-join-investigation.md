# Late-join investigation continuation

Date: 2026-11-12. This continues the previous handoff without assuming its server-fault conclusion is correct.

## Source and resource inspection

Read the current session receive path, vendored audio queue initialization, packet codec, and audio receive permission gate. The packet codec explicitly exempts Voice/VoiceWhisper from the normal receive-window rejection. AudioQueue initializes its next packet ID from the first received packet. Neither supports the simple hypothesis that joining mid-stream necessarily rejects a nonzero initial sequence number.

The library may still discard audio when output mute/hardware flags or unencrypted-audio policy disallow reception. Further diagnostics must distinguish UDP arrival, successful decoding/decryption, library event delivery, and application consumption.

A read-only ASCII-string inspection of the existing extracted TeamSpeak.dll found familiar candidates including channelsubscribe, channelsubscribeall, channelunsubscribe, clientmute/clientunmute, client_input_hardware, client_output_hardware, client_output_muted, and client_outputonly_muted. No new verified voice-subscription command was established. Strings are candidates, not proof that a command is used or accepted. No proprietary code was copied or executed.

## Fresh local comparison

The existing local UDP server was reachable on port 9987. No server database, configuration, existing process, or other clients were changed. Two newly launched CLI peers used separate audit-only identities; only these processes were supervised. Synthetic audio was used, not microphone capture. Local logs and identity files remain ignored under .audit-local/late-join and must not be published.

For each order, launch audio-send with duration argument 8 and audio-recv with duration argument 12, waiting for the first peer's readiness output before launching the second. Each peer uses --identity with its own ignored file, --name with a test alias, and ALLOW_PRIVATE_TARGETS=1. Process readiness had a 35-second bound; remaining processes were terminated only if they exceeded supervision bounds.

| Order | Receiver summary | Process exit codes |
| --- | --- | --- |
| Listener before sender | 259 received voice packets | 0, 0 |
| Sender before listener | 259 received voice packets | 0, 0 |

The previous zero-packet late-join result was **not reproduced** in this comparison. Equal aggregate counts do not establish real-time delivery, expected packet cadence, correct measurement intervals, or official-client equivalence. The server was not reset or isolated from preexisting clients, and its version was not freshly queried. Do not infer that the historical bug is fixed or that the original server diagnosis was proven.

## Next discriminating experiment

Add per-packet monotonic arrival timestamps and sender identity correlation to a dedicated reproducible harness. Verify readiness, actual emission cadence, overlap interval, and peer identity before comparing listener-first and sender-first. Repeat with an independently observed official client and packet-level arrival evidence, without resetting user state. Investigate application polling/decryption/output-policy boundaries before changing protocol behavior. Packaging and new media features should not substitute for that evidence.

## Follow-up: sender diagnostic did not drive network progress

Source review identified that `audio-send` called `Session::send_audio` on a timer without polling the connection event stream throughout transmission. Sending enqueues packets; the connection event stream drives network I/O. This invalidates using that diagnostic's aggregate successful packet count as proof of real-time delivery.

A manual regression against the existing local server compared the previous executable with the corrected implementation. The previous executable's first positive receiver report occurred after **8.023 seconds**, at the end of the eight-second sender interval, despite both processes exiting successfully. The initial corrected run reported **0.171 seconds**. A stronger follow-up correlated the receiver's sender set with the transmitting client's reported client ID and reported **0.140 seconds**. These timings are application report delays, not network latency benchmarks; report cadence and handshake overlap affect them. The previous uncorrelated comparison alone could not exclude unrelated voice traffic.

The corrected `send_tone` polls the connection and frame timer together under a fixed overall deadline, wakes network processing after enqueueing, and checks the timer even when the network produces an event. It does not recreate short receive timeouts. Only the transmission deadline bounds the polling future. This fixes a demonstrated CLI diagnostic defect; it does not prove that every historical bridge/server symptom had this cause.

Reproduce the sender-correlated local-server regression from the repository root after rebuilding the CLI:

```powershell
python scripts/check_voice_progress.py --binary .audit-local/target/debug/ts6-cli.exe
```

The script requires a trusted local server at port 9987. It creates separate ignored identities, uses synthetic audio, waits for peer readiness, requires the expected sender ID in positive receiver reports, and fails when the first report is not within three seconds of sender readiness. The three-second bound detects end-of-transmission delivery; it is not a production latency requirement. It supervises only its own children and is deliberately excluded from offline CI. It neither resets server state nor authenticates to administration interfaces. Fresh server version and official-client comparison remain unverified.
