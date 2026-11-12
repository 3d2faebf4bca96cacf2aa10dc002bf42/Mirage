# Security policy

## Status and supported versions

The project is experimental and has no published supported release line or security response commitment. Current publication verification and remaining limitations are described in [docs/PUBLICATION.md](docs/PUBLICATION.md); [docs/AUDIT.md](docs/AUDIT.md) records the earlier local findings. A passing scan does not certify security.

## Reporting a vulnerability privately

No repository remote or private security contact is configured in this checkout. Before publication, the owner must enable GitHub private vulnerability reporting and verify that the repository's **Security → Advisories → Report a vulnerability** entry is available to reporters. If available, use that private mechanism. If it is unavailable, do not open a public issue with exploit details or credentials; request a private reporting channel from the repository owner without disclosing the vulnerability. No email address or response deadline is implied.

Include the affected revision/version, impacted component, sanitized reproduction, prerequisites, expected impact, and a proposed fix if available. Never send actual passwords, private identity keys, cookies, access tokens, or unrelated personal data. Coordinate disclosure and avoid exposing exploit details until a fix and disclosure plan have been discussed.

## Security principles

- Treat identities as account private keys. Do not commit or share them. Verify Windows directory ACLs; POSIX identity files use 0600.
- Keep the bridge on loopback and trust only the local machine. Origin/Host checks reduce browser attacks; they do not authenticate local processes or protect against malicious local software.
- Avoid verbose protocol logs outside isolated debugging; they may contain credentials or remote personal data. Command debug output in the bridge is limited to operation names.
- Private destinations are blocked by the vendored resolver unless explicitly enabled. Do not turn that override into an implicit default for untrusted callers.
- Inspect crypto, permission, dependency, workflow, release, and installation changes with particular care. Never execute untrusted PRs with repository credentials.
- If a real secret was committed, revoke/rotate it first. `.gitignore` does not erase commits. History rewriting requires explicit owner authorization and coordination with existing clones.

This document defines reporting expectations; it does not claim any legal compliance or security guarantee.
