# Changelog

No tagged releases have been established in this checkout. Entries describe repository changes, not a release announcement.

## Unreleased

- Add English onboarding, shared agent instructions, contribution and security policies, issue/PR templates, and CI/dependency monitoring.
- Translate UI messages and first-party fixtures; consolidate outdated architecture and historical verification notes.
- Restrict browser WebSocket access by Host/Origin and bound frame/message sizes.
- Remove credential fields from bridge command diagnostics; fail closed on invalid PTT bindings.
- Bound idle PTT gate shutdown and prevent overwriting an identity during creation.
- Replace personal script paths, sanitize upstream token-like examples, and repair the standalone probe's vendored dependency path.
- Record unresolved dependency advisories, historical privacy findings, and release prerequisites in `docs/AUDIT.md`.

- Keep the CLI tone sender polling network I/O during transmission; add a sender-correlated live progress regression to detect delivery deferred until transmission ends.

- Keep separate Opus decoder state per sender in CLI measurements, add a two-stream regression, and add live TS6 bridge control verification.

- Upgrade the vendored P-256/elliptic-curve stack to 0.14, resolve the crypto-bigint advisory in both lockfiles, and verify identity/crypto and live TS6 interoperability before the owner-authorized clean GitHub import.
