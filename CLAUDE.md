# Claude Code orientation

Read and follow [AGENTS.md](AGENTS.md), the shared policy for all coding agents.
Do not duplicate or override its rules in tool-specific files.

## Purpose and status

Independent experimental TeamSpeak 6 client: Rust network/audio core and a browser UI.
First-party license: MIT. English is mandatory throughout first-party contributions.
Previous interoperability experiments used server beta13.1; they are not fresh tests.
See [README](README.md) and [current audit](docs/AUDIT.md) for limits and blockers.

## Navigate

- `client/Cargo.toml`: five-crate workspace and lockfile.
- `ts6-core`: `Session`, `ClientConfig`, `Snapshot`, identity persistence, event polling.
- `ts6-audio`: Opus, mono 48 kHz processing, capture/playback on dedicated threads.
- `ts6-input`: Windows global PTT and bounded transmit gate.
- `ts6-cli`: manual measurement commands; no-argument invocation prints usage.
- `ts6-bridge`: one loopback HTTP/WebSocket service, one session, audio orchestration.
- `client/apps/ui`: React 19 + TypeScript + Vite 7; wire types in `src/types.ts`.
- `client/vendor/tsclientlib`: third-party workspace; read `client/UPSTREAM.md` before changes.
- `recon/probe-ts6`: independent early probe, outside the main workspace.

## Build and verify

See [development](docs/DEVELOPMENT.md) for PowerShell commands and configuration.
Tested local toolchain: Windows/MSVC, Rust 1.99.0, Node 22.23.2, CMake 4.4.4.

```powershell
python scripts/check_repository.py
cd client
./scripts/build.ps1 build --locked --workspace
./scripts/build.ps1 test --locked --workspace
cargo fmt -p ts6-core -p ts6-audio -p ts6-input -p ts6-cli -p ts6-bridge -- --check
cargo clippy --locked --workspace --all-targets --no-deps -- -D warnings
cd apps/ui
npm ci --ignore-scripts
npm run build
npm audit
```

Build the UI before running `cargo run --locked -p ts6-bridge` from `client`.
The default URL is http://127.0.0.1:8787. Private-server tests require an explicit
`ALLOW_PRIVATE_TARGETS=1` process environment setting; no `.env` loader exists.

## Critical pitfalls

Read [architecture](docs/architecture.md) before changing polling or audio threading.
The connection is `!Send` and requires continuous polling. Short receive timeouts,
competing polls, unbounded drains, and idle PTT joins have caused failures.
Do not log command fields or persist credentials in examples. Never weaken browser
access validation or turn a failed PTT binding into continuous transmission.
Keep identity files private and intact. Never assume a successful command response
means chat delivery, a server bug is proven, or hardware capture quality was tested.

Before reporting completion, review the diff, execute relevant checks, and report
failures and unverified areas. Follow [CONTRIBUTING](CONTRIBUTING.md) and the shared
agent verification protocol. Do not publish or rewrite history without authorization.
