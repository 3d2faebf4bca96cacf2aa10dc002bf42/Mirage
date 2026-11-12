# Mirage publication preparation

Date: 2026-11-12. Target: [Mirage on GitHub](https://github.com/3d2faebf4bca96cacf2aa10dc002bf42/Mirage).

## Clean import and preserved history

The owner authorized importing the complete sanitized source as a descendant of
the existing remote main commit. The original local development history contains
personal paths, identifying metadata, and token-like upstream examples and is
intentionally not an ancestor of the publication commit. It remains private in
the original checkout. No force push, history rewrite, remote overwrite, or tag
publication is needed. The remote MIT copyright notice is retained alongside the
existing first-party notice. Dependency/upstream license notices are preserved.

The source includes all five Rust crates, React UI, vendored protocol source,
lockfiles, the standalone probe, tests, scripts, policies, documentation, templates,
and workflows. Generated builds, npm dependencies, testbed binaries/runtime,
identities, private configuration, audit captures, and ignored research clones are
excluded. They are not required for an offline source build. The proprietary
server must be obtained separately for live tests.

## Cryptography remediation

P-256 and elliptic-curve were migrated from 0.13 to 0.14. Both first-party lockfiles
now resolve a patched crypto-bigint 0.7 release. The RUSTSEC-2026-0387 vulnerability
reported in the [historical audit](AUDIT.md) no longer appears in either lockfile
audit. No advisory ignore was added. See [upstream patch policy](../client/UPSTREAM.md)
and `client/patches/rustcrypto-0.14.patch` for the reproducible adapter changes.
The new P-256 RNG uses the compatible rand 0.10 API; Ed25519 retains its installed
rand 0.8 API. SEC1/DER identity and wire formats were preserved intentionally.

Verification completed after migration:

- `cargo check --locked --workspace --manifest-path client/Cargo.toml` passed.
- `cargo test --locked --workspace --manifest-path client/Cargo.toml`: 34 passed.
- `cargo test --locked --manifest-path client/Cargo.toml -p tsproto-types`: 8 passed,
  including key parsing, identity imports, ECDH, and signatures.
- Both `cargo audit --file client/Cargo.lock` and the standalone probe lockfile
  audit passed with zero reported vulnerabilities. Unmaintained-dependency warnings
  for audiopus_sys and pin-utils remain follow-up work, not security guarantees.
- Strict first-party Clippy and explicit-package formatting passed.
- Isolated `cargo build --locked --workspace --manifest-path client/Cargo.toml
  --target-dir .audit-local/target` passed; the standalone probe check passed.
- UI production build, TypeScript checking, and npm audit passed.

## Fresh live interoperability after migration

The original local UDP server was no longer listening during the first rerun;
both the old and migrated binaries failed with Windows socket error 10054. This
was not classified as a crypto failure. A separate instance of the existing
official server binary was started on IPv4 loopback with a fresh ignored runtime
directory, retaining the original testbed database and configuration untouched.
The first isolated startup lacked an explicit SQL resource directory and failed;
setting the installed server's SQL directory fixed that test setup issue.

The migrated bridge then passed connect, snapshot, nickname, mute/unmute, refresh,
and disconnect checks. The sender-correlated synthetic voice regression also
passed. Existing protected audit identities were reused, exercising identity
loading across the dependency migration. The helper stopped only its own server
and bridge processes. No microphone capture was enabled. Raw private server logs,
identities, databases, and audit reports are not distributed.

These tests do not establish compatibility with every TS6 deployment, acoustic
microphone quality, a packaged desktop application, or implemented chat/video/file
features. See [compatibility status](compatibility-status.md).

## Hosted settings and remaining work

Read-only GitHub API inspection reported public visibility, secret scanning and
push protection enabled, Dependabot security updates disabled, and main without
branch protection. Private vulnerability reporting was disabled. These settings
were not changed; administrators must separately establish reporting channels,
branch/ruleset requirements, and appropriate dependency alerts/updates.

No production-ready or vulnerability-free guarantee is made. Audit the final
publication tree and its actual ancestors, rather than the private original local
history. Hosted workflow results must be inspected after the push; adding a
workflow is not evidence that it ran successfully.

## Whitespace review scope

Whole-import whitespace review exposed legacy vendor formatting, unified-diff
context prefixes, and trailing spaces in first-party captures/scripts. First-party
trailing whitespace was normalized. Git attributes exempt patch-format context
and retained upstream formatting from generic whitespace diagnostics; this does
not exempt either from secret scanning or content review. First-party Rust still
passes explicit-package formatting. No security/advisory scan is ignored.
