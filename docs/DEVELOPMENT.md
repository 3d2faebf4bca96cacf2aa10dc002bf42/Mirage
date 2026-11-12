# Development, configuration, and testing

Use the [root README](../README.md) for setup and [architecture](architecture.md) before changing session polling. Work from a clone with the committed vendored source and lockfiles. No external reference clones or proprietary artifacts are needed for offline builds.

## Offline checks

From the repository root in PowerShell:

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

The build helper sets `CMAKE_POLICY_VERSION_MINIMUM=3.5` for bundled libopus under CMake 4. Direct Cargo builds need the same environment setting. Select the five first-party packages explicitly with `cargo fmt -p ...`: `--all` also traverses local path dependencies and would reformat the vendored workspace. Clippy may emit upstream deprecation warnings; inspect the actual result rather than hiding warnings. The current first-party Clippy baseline is recorded in [AUDIT.md](AUDIT.md).

Rust tests cover codec/meter/resampling/mixing, state/configuration, key parsing/gating, bridge serialization, access policy, and safety regressions. They do not prove real microphone quality, voice latency, server permissions, or cross-platform operation. After building both UI and bridge, run `python scripts/smoke_bridge.py --binary client/target/debug/ts6-bridge.exe` from the repository root. This launches and stops only its own local bridge, checks HTTP and WebSocket boundaries, and sends no server-connect or audio commands.

The UI build runs TypeScript checking; there is no established automated UI interaction suite.

For dependency security, install the audited tool version separately and run:

```powershell
cargo install cargo-audit --version 0.23.0 --locked
cargo audit --file client/Cargo.lock
cargo audit --file recon/probe-ts6/Cargo.lock
```

The historical cryptography advisory was resolved through the migration recorded in [PUBLICATION.md](PUBLICATION.md). Never suppress new audit failures to obtain a passing result. Gitleaks 8.30.1 was used locally with `gitleaks git --redact --log-opts=--all`; run it before publication and scan a nonignored working-tree snapshot as well. Keep reports private; never print secret values. Older Git commits still require owner review.

## Bridge and UI configuration

No `.env` loader exists. Set environment variables in the launching shell. Do not create a credential-bearing `.env.example` for options the program does not read.

| Setting | Default / behavior |
| --- | --- |
| `TS6_BRIDGE_PORT` | 8787; valid environment value takes precedence over `--port` |
| `--port <u16>` | Alternative bridge port; invalid input currently falls back to a default |
| `TS6_UI_DIR` | Overrides `--ui`; otherwise build-time source-tree `apps/ui/dist` |
| `--ui <directory>` | Static asset path; set it when relocating the binary |
| `ALLOW_PRIVATE_TARGETS=1` | Explicit private/loopback destination override in vendored resolver |
| `RUST_LOG` | Bridge tracing filter; default `info,ts6_bridge=debug` |
| `APPDATA` | First choice for identity directory, suffixed with `ts6client` |
| `XDG_CONFIG_HOME` | Used if APPDATA is absent; suffixed with `ts6client` |
| `HOME` | Final fallback: `.config/ts6client` below HOME; empty HOME falls back to a relative location |

The default private identity is `identity.json` inside that config directory. CLI `--identity <path>` selects a different file. Protect it with a user-only ACL on Windows. Do not share identities across simultaneous clients; the server can reject clones. Keep protected backups. On malformed JSON the client fails instead of replacing the account.

For UI development, run the bridge and then `npm run dev` under `client/apps/ui`. Vite uses port 5174 and proxies `/ws` to `ws://127.0.0.1:8787`. Use the exact local URL displayed; only HTTP origins on localhost/127.0.0.1 at bridge port or 5174 are allowed. The Vite proxy target is fixed: change it deliberately if testing a different bridge port. Never expose the development server or bridge publicly.

## Diagnostic CLI

From `client`, prefix these commands with `cargo run --locked -p ts6-cli --`:

```text
probe <address> [seconds]
chat <address> <server|channel|@client-id> <text...>
join <address> <channel>
nick <address> <nickname>
raw <address> <command> [key=value ...]
stream <address> [--type 2] [--bitrate 2500] [--seconds 6]
audio-send <address> [seconds]
audio-recv <address> [seconds]
audio-call <address> [seconds] [--tone] [--ptt F13]
devices
ptt-probe <key> [seconds]
```

Inspect `crates/ts6-cli/src/main.rs` for diagnostic-only modes including `duplex-probe`. Common connection options are `--name`, `--identity`, `--channel`, `--channel-password`, `--server-password`, `--local-ip`, and `--verbose`. Audio-call also accepts `--input` and `--output`. Passwords supplied on the command line can be visible to local process inspection; use only isolated test credentials. `--verbose` enables sensitive protocol logging. Never attach raw output to a public issue.

Bridge session-only diagnostics: `cargo run --locked -p ts6-bridge -- --probe 127.0.0.1:9987 --seconds 20`. `--no-tone`, `--no-commands`, and `--current-thread` vary that diagnostic. These modes connect to a server and may transmit synthetic audio; ordinary offline verification does not run them.

## Manual server/hardware testing

The proprietary server is not distributed. Create an isolated local server using its own version-specific configuration, keep secrets and state under ignored `testbed/`, and restrict query interfaces to the test machine. Never wipe server state as a routine troubleshooting step. Back it up and obtain authorization before any destructive reset.

Use distinct protected identities for peers. Connect the listener before the speaker; historical beta13.1 tests observed zero received packets with the reverse ordering. Wait for the sender to confirm actual transmission before measuring deltas. Reset cumulative counters, exclude abandoned peers, and record the exact interval, server version, order, devices, and result. A tone proves transport/codec behavior, not real acoustic microphone quality.

The legacy `v21-*.ps1` and `bridge-first.ps1` scripts are research helpers, not CI tests. Their receive timeout handling and process cleanup need further work: do not treat a timed-out `ReceiveAsync` as cancelled or start a second concurrent receive. Use them only on an isolated testbed with explicit process supervision. Do not infer a successful test from zero baseline traffic or a script's final exit status alone.

## Troubleshooting

- Native Opus build fails under CMake 4: use `scripts/build.ps1`; confirm MSVC C++ tools and CMake are on PATH.
- Private destination rejected: explicitly enable `ALLOW_PRIVATE_TARGETS=1` only for a trusted local test.
- UI missing: build `apps/ui/dist` and confirm `TS6_UI_DIR`/`--ui` points there.
- WebSocket rejected: use a permitted local HTTP origin and exact bridge Host/port; remove public proxies. Native probes may omit Origin.
- No microphone or unsupported format: enumerate `devices`; inspect capture errors. A selected device does not prove its stream started successfully.
- No chat delivery: TS3 text command acceptance is not Matrix chat integration.
- PTT invalid/unavailable: transmission is disabled; select a valid Windows binding. Do not remove the guard.

## GitHub administration

Repository files do not configure hosted settings. Before public launch, the owner should:

1. Set the real repository URL and confirm maintainers and a private conduct-reporting channel. No CODEOWNERS is created because no owner identities are verified.
2. Enable and test private vulnerability reporting, secret scanning, push protection, and dependency alerts where available. Review historical token-like samples and personal paths privately; rotate real credentials before deciding whether authorized history remediation is needed.
3. Run the supplied workflows on GitHub and then require their actual check names in a branch ruleset. Restrict direct pushes and force pushes; choose review requirements that the real maintainer team can meet.
4. Require maintainer approval for untrusted workflow runs, keep token permissions read-only, and avoid privileged fork execution. Review dependency and action-pin updates manually.
5. Add practical labels (`bug`, `enhancement`, `documentation`, `maintenance`, `good first issue`); do not put private vulnerabilities in public labels/issues.
6. Resolve known advisories and review all resolved/native dependency licenses before distributing binaries. Define release tags, verified platforms/server versions, and release approval separately; no release workflow is supplied.

The CI includes Windows build/test/lint/format, UI build/audit, repository policy checks, and Rust dependency audits. It has no deployment credentials or publishing steps. Gitleaks is a local pre-publication requirement; hosted secret scanning requires administrator setup. Hosted CI has not been executed merely by adding workflow files.

## Live voice progress regression

`python scripts/check_voice_progress.py --binary .audit-local/target/debug/ts6-cli.exe`
checks sender-correlated voice progress against an existing trusted local server.
It transmits a synthetic tone and uses separate ignored test identities; it does
not capture the microphone. This is a manual integration test, not offline CI.
See [late-join investigation](late-join-investigation.md) for the diagnosed sender
polling defect, measurements, and remaining limitations.
