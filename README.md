# Mirage

An independent, experimental TeamSpeak 6 desktop client written in Rust with a React interface. First-party code is MIT licensed. This project is not affiliated with TeamSpeak and does not redistribute the proprietary client or server.

**English is required for all first-party code comments, documentation, UI text, logs, tests, and contributions.** Protocol identifiers, upstream legal notices, and externally supplied server/device names retain their original values.

## Status

Implemented: TS3-compatible UDP session establishment, channel/client snapshots, correlated commands, Opus voice, capture/playback, Windows global push-to-talk, and a local WebSocket UI bridge.

Previous local experiments used TeamSpeak Server **6.0.0-beta13.1**. Their results are summarized in [verification](docs/verification.md); they are not a promise of compatibility with other releases. The UI is a browser app, not a packaged Tauri application.

Not implemented: Matrix chat integration, WebRTC video/screensharing, S3 file integration, acoustic echo cancellation, and noise suppression. Real microphone quality, long-running stability, and non-Windows operation remain unverified. Voice late-join and tone-rate anomalies need further investigation.

**Experimental software:** the RustCrypto advisory identified in the historical audit has been addressed by a dependency migration. Publication uses a clean source import; unsafe local historical commits are excluded. Private security reporting and production readiness still require follow-up. See [publication verification](docs/PUBLICATION.md) and the [historical audit](docs/AUDIT.md).

Fresh local-server results and the remaining feature gaps are tracked in [compatibility status](docs/compatibility-status.md).

## Requirements

Locally checked on Windows with Rust **1.99.0** (MSVC), Node **22.23.2**, npm, CMake **4.4.4**, and the Visual Studio C++ build tools. These are tested versions, not an established minimum compatibility range. The UI's Vite dependency requires Node 20.19+ or 22.12+; Node 22 is the documented setup. PowerShell 7 is used by the helper scripts. Python 3.10+ runs repository policy checks. Other operating systems are not supported by the complete workspace yet: the global key binding implementation is Windows-only.

## Build and run

From the repository root in PowerShell:

```powershell
cd client/apps/ui
npm ci --ignore-scripts
npm run build
cd ../..
./scripts/build.ps1 build --locked --workspace
cargo run --locked -p ts6-bridge
```

Open **http://127.0.0.1:8787**. The bridge serves `client/apps/ui/dist`, owns one session, and accepts commands from local browser origins and native local probes. Keep it on a trusted machine; it does not authenticate local processes. `--ui <directory>` overrides the asset directory for relocated executables.

For a server you control on loopback or a private network, explicitly enable private destinations before starting the bridge or CLI:

```powershell
$env:ALLOW_PRIVATE_TARGETS = '1'
cd client
cargo run --locked -p ts6-cli -- probe 127.0.0.1:9987 5 --name tester
```

`ALLOW_PRIVATE_TARGETS` disables the vendored resolver's private-address guard for that process. Do not enable it for untrusted remote callers. A server is not included. Download and configure a server separately under the ignored `testbed/` directory if needed; keep administrative query interfaces disabled unless your isolated test requires them.

## Develop and verify

```powershell
python scripts/check_repository.py
cd client
./scripts/build.ps1 test --locked --workspace
cargo fmt -p ts6-core -p ts6-audio -p ts6-input -p ts6-cli -p ts6-bridge -- --check
cargo clippy --locked --workspace --all-targets --no-deps -- -D warnings
```

See [development and configuration](docs/DEVELOPMENT.md), [architecture](docs/architecture.md), and [historical verification](docs/verification.md). The CLI prints its available subcommands when run without arguments.

## Repository map

| Path | Responsibility |
| --- | --- |
| `client/crates/ts6-core` | Session, identity, state projection, commands, continuous duplex polling |
| `client/crates/ts6-audio` | Opus, resampling, metering, mixer, capture/playback threads |
| `client/crates/ts6-input` | Key parsing, Windows key polling, transmit gate |
| `client/crates/ts6-cli` | Diagnostics and manual measurement commands |
| `client/crates/ts6-bridge` | Loopback HTTP/WebSocket service and local audio orchestration |
| `client/apps/ui` | React 19, TypeScript 7, Vite 8, CSS |
| `client/vendor/tsclientlib` | Tracked protocol source, separate workspace and licenses |
| `recon/probe-ts6` | Standalone early handshake probe |
| `docs` | Architecture, research references, verification, audit |

## Contribute and report security issues

Read [CONTRIBUTING.md](CONTRIBUTING.md), [community expectations](CODE_OF_CONDUCT.md), and [SECURITY.md](SECURITY.md). Coding agents start with [AGENTS.md](AGENTS.md); Claude-specific orientation is in [CLAUDE.md](CLAUDE.md). Technical claims require source or executed verification evidence.

The [MIT license](LICENSE) already applies to first-party code. Retain all [third-party notices](client/THIRD_PARTY.md) and follow [vendoring rules](client/UPSTREAM.md). No releases or release-support commitments are established by this repository.
