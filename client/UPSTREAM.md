# Vendored protocol library

`vendor/tsclientlib/` is a **source copy** (no git submodule, no `.git`) of:

- upstream: <https://github.com/ReSpeak/tsclientlib> — MIT OR Apache-2.0
- base commit: `2e77949` on branch `webspeak3` of <https://github.com/Moepchi/tsclientlib>
  ("Block connections to private/internal addresses by default", 2026-09-27)
- declarations: <https://github.com/Moepchi/tsdeclarations> @ `c2992f9`

## Why a fork base instead of upstream master

Upstream `master` (= published crates `tsproto 0.2.0` / `tsclientlib 0.2.0`, 2021) drops
unknown server commands. TeamSpeak 6 delivers its whole streaming/media feature set as
`notify*` commands that do not exist in the TS3 declarations (`notifystreamstarted`,
`notifyjoinstreamrequest`, `notifystreamsignaling`, ...). Without surfacing them there is no
way to implement TS6 video on top of the library.

The fork branch provides exactly that, as `StreamItem::UnknownCommand { name, content }`
(`tsclientlib/src/lib.rs`), plus:

- `client_is_streaming` on client info (TS6 state),
- fixes to filetransfer/license parsing and to the DNS resolver,
- TeaSpeak identity support (`tsclientlib/src/teaspeak_identity.rs`).

It also adds an SSRF guard that refuses private/loopback targets. That is opt-out with the
environment variable `ALLOW_PRIVATE_TARGETS=1`, which our local testbed needs (see
`docs/architecture.md`).

## Rules for this copy

1. **Never edit `vendor/tsclientlib/` in place for a one-off fix.** Put the change in our own
   crate if possible. If the library itself must change, record it in `patches/` as a patch file
   plus a note in this document.
2. Every upgrade must keep `LICENSE-MIT` and `LICENSE-APACHE` and the declarations licences.
3. Upgrading = re-copy the tree from a known commit, re-run `cargo test -p tsclientlib`, and
   re-run the interop milestone (`ts6-cli probe` against the official TS6 server).

## Licence compliance

`tsclientlib` and `tsproto` are dual-licensed MIT OR Apache-2.0; the vendored declarations carry
both licence files as well. Our project takes the **MIT** option and must therefore keep the
upstream copyright notice — see `THIRD_PARTY.md`.

## Local patch: publication-safe examples

`patches/public-examples.patch` records the portable CI path change made during the
2026-11-12 audit. Additionally, token-like and file-transfer-key comment examples in
`utils/tsproto-structs/declarations/Messages.toml` now use `<example-token>` or `<example-file-key>`.
Their original values are intentionally omitted from the patch to avoid copying
potential credentials. Use `%USERPROFILE%` in the legacy AppVeyor configuration.
The publication-example patch changes no protocol logic or license notices. Reapply or reassess this patch
when updating the vendored copy. These example values remain in old Git commits;
see `../docs/AUDIT.md` for the publication review requirement.

## Local patch: RustCrypto 0.14 migration

`patches/rustcrypto-0.14.patch` upgrades P-256 and elliptic-curve to 0.14,
resolving the crypto-bigint advisory through a patched 0.7 release selected in
both first-party lockfiles. The adapter uses the new SEC1 traits, hybrid-array
field bytes, fallible private-key slice parsing, and the Generate trait.
P-256 uses rand 0.10 via an explicit alias; Ed25519 retains rand 0.8 because its
installed curve dependency uses that API. No key format or wire protocol was
intentionally changed. Re-run the eight upstream crypto tests, first-party tests,
lockfile audits, and live TS6 interoperability after any update. Record the exact
verification outcome in `../docs/PUBLICATION.md`; successful unit tests alone do
not establish server compatibility.
