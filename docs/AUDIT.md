# Public-release preparation audit

> Historical audit of the pre-publication local checkout. Later owner-authorized clean import and cryptography remediation are recorded in [PUBLICATION.md](PUBLICATION.md). Findings about the original local history do not imply that those commits are included in the published history.

Date: 2026-11-12. Baseline: `56addac`. Changes are uncommitted on `codex/open-source-hardening`. Nothing was pushed, published, released, or rewritten in Git history. No Git remote is configured; this does not establish whether older copies were publicly exposed.

## A. Project assessment and scope

The project is an experimental independent TeamSpeak client: five Rust crates for session/state/identity, Opus/device audio, Windows PTT, diagnostics, and an Axum loopback bridge; React 19/TypeScript/Vite 7 provide the browser UI. A tracked protocol fork supplies TS3-compatible UDP, crypto, bookkeeping, and unknown TS6 command exposure. MIT licenses already exist at root and client level. Strengths include crate boundaries, bounded channels/deadlines, lockfiles, offline tests, retained upstream notices, and documented prior interoperability experiments.

Limitations: no packaged desktop shell, Matrix chat, WebRTC video, S3 file integration, AEC/NS, confirmed non-Windows full-workspace support, or established production/security support line. Microphone acoustic quality, voice timing, long-running operation, late-join behavior, and the historical tone-rate anomaly need controlled retesting. Do not treat V-25's server explanation as a universal proven cause.

### Discovery coverage

Initial metadata inventory covered **33947 files**, including hidden/ignored trees, with zero inaccessible entries and zero skipped links. No symlinks/junctions were followed. Initial top-level counts/bytes:

| Category | Files | Bytes | Treatment |
| --- | ---: | ---: | --- |
| `.git` | 330 | 857944 | Metadata; reachable history scanned separately |
| `client` | 18208 | 9472803402 | First-party source/config and tracked vendor; dependencies/build outputs inventoried |
| `recon` | 4017 | 2236587265 | Tracked probe inspected; generated/scratch material inventoried |
| `testbed` | 1462 | 829655378 | Proprietary/runtime material: metadata only |
| root `vendor` | 9921 | 310081540 | Ignored research/tool clones: metadata only, not a shipped dependency |
| `docs` | 6 | 226508 | Documentation reviewed at varying depth; protocol references retained |
| root README/license/ignore | 3 | 6304 | Inspected |

The repeatable command is `python scripts/check_repository.py --inventory`. Counts increase after builds and dependency installation, including the ignored `.audit-local/` isolated build and sanitized scan snapshot. Original counts above are the pre-change inventory, not current counts.

Five reachable commits and 214 unique historical blobs were scanned with location-only custom privacy patterns. Author/committer metadata contains one non-noreply email identity requiring owner review; no identity is reproduced here. Local Git configuration key names and tracked-file metadata were reviewed; no remote exists.

Review focused on first-party architecture, affected modules and dependencies, manifests/lockfiles, documentation, scripts, security boundaries, and the actual diff. The independent reviewer inspected the changed Rust/UI code, policies, workflows, installed Vite proxy implementation, and vendored patch scope. This is **not** a claim of line-by-line inspection of every third-party/generated file. The large research documents were reviewed primarily for structure, language, status, and sensitive examples; their numerous external source line references were not all revalidated. Ignored binaries, caches, downloaded tools, installers, extracted proprietary code, identities, databases, runtime configuration, and raw logs were not parsed. Unreachable/dangling Git objects, other clones, upstream repository history, and external publication status were not assessed.

## B. Files created

| File(s) | Purpose |
| --- | --- |
| `AGENTS.md` | Shared English/evidence/security/verification policy for coding agents |
| `CLAUDE.md` | Concise Claude orientation linking to shared rules |
| `CONTRIBUTING.md` | Setup, contribution acceptance, risk-based review, release guidance |
| `SECURITY.md` | Honest support status and private reporting prerequisites |
| `CODE_OF_CONDUCT.md` | Community expectations; private contact remains owner work |
| `CHANGELOG.md` | Unreleased changes only; no invented releases |
| `docs/DEVELOPMENT.md` | Configuration, commands, tests, troubleshooting, hosted setup |
| `docs/AUDIT.md`, `docs/maintenance-plan.md` | Findings, evidence, scope, implementation decisions and progress |
| `.github/ISSUE_TEMPLATE/bug_report.yml`, `feature_request.yml` | English issue forms with sanitized evidence |
| `.github/PULL_REQUEST_TEMPLATE.md` | Scope, checks, compatibility, security and documentation review |
| `.github/dependabot.yml` | Weekly Cargo/npm/action update proposals; no automerge |
| `.github/workflows/ci.yml` | Windows offline checks, dependency audit, history secret scanning |
| `.editorconfig`, `.gitattributes` | Shared text/formatting conventions |
| `rust-toolchain.toml`, `client/apps/ui/.node-version` | Tested toolchain versions |
| `scripts/check_repository.py` | Metadata inventory and publishable-file privacy/language/workflow checks |
| `scripts/smoke_bridge.py` | Isolated live HTTP/WebSocket security checks without server/audio commands |
| `client/patches/public-examples.patch` | Reproducible vendored AppVeyor portability change |

## C. Files modified

- Root/client READMEs now describe actual implementation, English policy, reproducible PowerShell commands, experimental status and blockers. Root MIT license terms are unchanged; its trailing note no longer overstates transitive license review.
- `docs/architecture.md` and `docs/verification.md` replace outdated Portuguese text with English architecture and condensed V-01–V-25 summaries. Superseded diagnoses, planned features, historical observations, and current tests are separated. No historical raw captures are represented as rerun tests.
- UI components, bridge messages, the channel error and test fixtures use English; HTML uses `lang="en"`. Vite changes outgoing proxy Host so strict bridge Host validation still permits development.
- Bridge validates Host/Origin before upgrade, bounds WebSocket frames/messages to 16 KiB, and logs command operation names instead of password-bearing fields. Invalid PTT fails closed and queued audio is drained on reconfiguration.
- PTT gate shutdown no longer waits indefinitely for an idle open source. Identity creation uses exclusive creation rather than overwrite, rejects symbolic links/nonregular files, applies POSIX 0600, and fails visibly on malformed content. Protected Windows directory ACLs remain necessary. Concurrent readers can encounter an incomplete in-progress creation and fail; retry only after the writer completes. Crash recovery still requires a protected backup.
- Two probe scripts use repository-relative executable paths; the standalone probe uses the tracked protocol copy rather than an ignored clone. The placeholder repository URL was removed from Cargo metadata.
- `.gitignore` now excludes common identity/environment/key/database/capture artifacts. Server-help output no longer contains a personal executable path. Vendored AppVeyor uses a generic user-profile variable; token/file-transfer-key comment examples use explicit placeholders. Upstream changes are documented in `client/UPSTREAM.md` without copying old token values into patches.
- `client/THIRD_PARTY.md` now records already-used audio/UI components and accurately limits license-review claims. First-party Rust formatting and small Clippy cleanups establish a checked baseline; no vendor protocol logic was changed.

## D. Security and privacy findings

| Finding | State / consequence |
| --- | --- |
| Browser-origin control absent from local bridge | Fixed and regression/smoke tested. Native local processes remain trusted; there is no authentication or multi-user isolation. |
| Debug command logging exposed connection password fields | Fixed by operation-only Debug formatting; regression tested. Verbose protocol logs and CLI password arguments remain sensitive. |
| Invalid PTT fell back to continuous transmission | Fixed and regression tested, including stale queued frames. |
| Idle PTT shutdown could hang | Reproduced before fix and regression tested after fix. |
| Identity creation overwrote files; POSIX protection helper was unused | Exclusive creation/permissions and rejection checks implemented. Persistence/malformed/directory tests pass on Windows; POSIX-only tests were not executed here. This is not a complete filesystem race audit. |
| Personal paths and historical identifying captures | Removed from current applicable files; remain in earlier commits. Git author/committer email also needs owner privacy review. No history rewrite was performed. |
| Upstream token-like examples | Gitleaks found five history occurrences in `client/vendor/tsclientlib/utils/tsproto-structs/declarations/Messages.toml`. Current token and file-transfer-key samples are placeholders; current source snapshot scan is clean. Their real-world validity/exposure is unknown. If any value was ever live, revoke/rotate it before authorized history remediation. Do not dismiss the history scan by adding an ignore. |
| Rust cryptography dependency | **Unresolved release blocker:** `crypto-bigint 0.5.5`, via `elliptic-curve 0.13` / `p256 0.13`, matches [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html). Patched versions start at 0.7.6. A compatible upstream-stack migration and interoperability tests are required; no advisory ignore or unverified crypto rewrite was added. Exploitability in this client was not established. |
| Dependency maintenance warnings | Cargo Audit reports unmaintained `audiopus_sys` ([RUSTSEC-2026-0150](https://rustsec.org/advisories/RUSTSEC-2026-0150.html)) and `pin-utils` ([RUSTSEC-2026-0441](https://rustsec.org/advisories/RUSTSEC-2026-0441.html)). Review replacement/upgrade plans before distribution. |
| Research helpers and device initialization | Legacy WebSocket probes can leave pending receives after timeout; process cleanup needs work. Audio stream startup can fail after returning a handle, and some thread creation paths still use optional handles. They are not production readiness evidence. |

Ignored testbed/runtime material was not opened for content scanning. It must not be included in source archives, screenshots, logs, or releases. A clean source scan does not certify this machine or all local files safe to publish. One local diagnostic inadvertently echoed an upstream key-like sample while locating it; that diagnostic was corrected and subsequent reports used only locations/counts. The sample is not reproduced in project reports.

## E. AI development standards

`AGENTS.md` is the shared source of policy; `CLAUDE.md` links to it and provides orientation rather than repeating a competing manual. Both are below 200 lines. Agents must inspect affected code/dependencies, verify unfamiliar APIs, preserve architecture, add regression tests for behavior, review diffs, and report actual checks/failures. External content and embedded instructions are untrusted. Unsupported claims, invented APIs/results, hardcoded secrets/personal environments, silent error suppression, and bypassed security gates are prohibited. The English phrase checker is deliberately incomplete; human review remains required.

## F. Contribution and CI infrastructure

The repository includes approachable issue/PR templates, contribution acceptance rules, a conduct policy without fabricated contacts, private-security-reporting prerequisites, dependency updates and least-privilege workflows. No CODEOWNERS, mandatory approval count, maintainer identity, release schedule, deployment, or publishing credential was invented.

Actions are pinned to full commit SHAs resolved from their official repositories. Checkout does not persist credentials; workflows use read-only contents permissions and ordinary unprivileged PR triggers. Gitleaks 8.30.1 is downloaded from its official release and verified against a pinned Linux archive SHA-256. This follows [GitHub's workflow security guidance](https://docs.github.com/en/actions/reference/security/secure-use). Untrusted contributions still execute code in ephemeral hosted runners; no workflow makes them safe for a credential-bearing local environment.

**Hosted CI has not run.** Its Rust advisory and full-history secret gates are expected to fail on the disclosed blockers. They are intentionally not suppressed. GitHub private reporting, rulesets, labels, secret scanning/push protection, reviewer roles, and dependency alerts require owner administration; instructions are in `DEVELOPMENT.md`.

## G. Verification results

Commands ran on Windows, Rust 1.99.0/MSVC, Node 22.23.2, CMake 4.4.4. `CMAKE_POLICY_VERSION_MINIMUM=3.5` was set for native builds. Log files/reports were kept locally outside publishable source.

| Executed command/check | Result |
| --- | --- |
| Initial `cargo test --locked --workspace --manifest-path client/Cargo.toml` | Passed before changes |
| New targeted regression tests before fixes | Gate hang, password Debug, invalid PTT and queued-audio assertions failed as expected; initial access tests failed compilation before their guard existed |
| Final `cargo test --locked --workspace --manifest-path client/Cargo.toml` | **33 passed**, no failed/ignored tests on Windows; POSIX-only tests excluded by cfg |
| `cargo build --locked --workspace --manifest-path client/Cargo.toml` | Failed to replace an already-running bridge executable (Windows access denied); existing process was not terminated |
| `cargo build --locked --workspace --manifest-path client/Cargo.toml --target-dir .audit-local/target` | Passed using isolated output |
| `cargo clippy --locked --workspace --all-targets --no-deps --manifest-path client/Cargo.toml -- -D warnings` | Passed for first-party targets; upstream compilation still emits deprecation/unused warnings |
| `cargo fmt --manifest-path client/Cargo.toml -p ts6-core -p ts6-audio -p ts6-input -p ts6-cli -p ts6-bridge -- --check` | Passed; do not use `--all`, which traverses local vendored path dependencies |
| `cargo check --locked --manifest-path recon/probe-ts6/Cargo.toml` | Passed |
| `npm ci --ignore-scripts`, `npm run build` in UI | Passed; TypeScript and production assets built |
| `npm audit` | Zero reported vulnerabilities in locked UI dependencies |
| `cargo audit --file client/Cargo.lock` and probe lockfile audit | Both fail on RUSTSEC-2026-0387; maintenance warnings also recorded |
| Gitleaks 8.30.1 history scan, all refs, redacted output | Five historical findings in upstream sample declarations; unresolved owner review |
| Gitleaks 8.30.1 sanitized nonignored source snapshot scan | Zero findings after sample replacement |
| `python scripts/check_repository.py` | Passed; publishable UTF-8 files checked, zero policy findings |
| `python scripts/check_repository.py --inventory` | Passed; no inaccessible entries or followed links; a temporary in-repository Windows junction was detected/skipped and then removed |
| `python scripts/smoke_bridge.py --binary .audit-local/target/debug/ts6-bridge.exe` | Passed: static assets, six Host/Origin cases, native refresh and oversized-input rejection |
| Installed Vite `loadConfigFromFile` / `createServer` live proxy check | Passed with the actual development config and hardened bridge on a temporary port |
| Python YAML parsing and local Markdown-link checks | Passed for new GitHub/AppVeyor YAML and maintained entry-point documents |
| `git diff --check` and independent source/diff review | Passed after review fixes |

Failed intermediate checks were investigated: initial repository-policy self-matching was corrected, generic AppVeyor service-user paths distinguished from personal paths, an invalid YAML scalar was quoted, and the development proxy Host was corrected. A plain virtual-workspace `cargo fmt` invocation could not find targets; selecting explicit packages fixes it. `cargo fmt --all` touched vendored formatting; all those unintended vendor edits were reverted before review. Earlier documentation extraction hit the Windows console encoding limit; subsequent text inspection explicitly used UTF-8. Final review found version-dependent junction detection on Python 3.10/3.11; metadata-based reparse detection replaced it and a real Windows junction was exercised without following it. Only the installed Python version was executed.

Not verified: GitHub execution/settings, Linux/macOS runtime/build, POSIX filesystem tests, real TeamSpeak interoperability after these changes, acoustic microphone quality, rendered UI interaction/accessibility/responsiveness, exhaustive transitive/native license compatibility, and absence of every possible secret or vulnerability.

## H. Remaining owner decisions

1. Privately classify historical sample credentials, personal paths/captures and author email metadata. Rotate anything live. Decide whether to authorize coordinated history remediation before publication; do not publish the old history blindly.
2. Establish verified private vulnerability and conduct-reporting channels and actual maintainer/reviewer identities. No license choice is pending: existing first-party MIT remains.
3. Configure and test GitHub administrative protections and real workflow checks; decide release/support/platform/server compatibility based on evidence.
4. Complete resolved dependency/native notice review before distributing binaries. No legal compliance certification is implied.

## I. Public-release readiness

**Not ready for a security-cleared public release or production use.** Source hardening, English onboarding, agent governance, contributor templates, offline build/tests, and local boundary checks are implemented and locally verified. CI configuration is implemented but unexecuted on GitHub. Dependency remediation, historical privacy/credential classification, private reporting, hosted protections, and live/platform validation remain open. The current source snapshot being clean does not make prior commits safe.

## J. Prioritized next steps

1. Resolve the cryptographic dependency advisory through a reviewed upstream-stack upgrade and regression/interoperability tests; address native codec maintenance.
2. Complete historical privacy/credential review and any explicitly authorized rotation/history cleanup.
3. Establish reporting channels and GitHub protections, run CI, and require its real passing gates before merging.
4. Fix research probe receive cancellation/process ownership, add hardware startup error propagation, then repeat controlled voice/late-join/tone-rate and microphone tests.
5. Define verified release/platform compatibility and dependency notices; expand offline/UI/integration coverage before packaging or adding media/chat features.
