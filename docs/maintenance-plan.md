# Open-source preparation plan

Date: 2026-11-12. Scope: the existing repository; no publishing, pushing, history rewriting, or license change.

## Design

Preserve the five-crate Rust workspace and React UI. Prefer focused hardening over a framework migration or broad refactor. Preserve third-party licenses and protocol research. Replace outdated Portuguese onboarding and historical notes with accurate English summaries; distinguish previous observations from current verification. Local artifacts are inventoried by metadata only, without opening databases, identities, binaries, or caches.

## Execution

- [x] Inventory hidden and ignored trees without following links; inspect tracked metadata and five reachable commits.
- [x] Establish a passing Rust baseline.
- [x] Add regression tests for browser access control, credential-safe command diagnostics, bounded PTT shutdown, and invalid PTT fail-closed behavior.
- [x] Implement focused fixes; restrict WebSocket payloads; protect identity creation; remove personal script paths.
- [x] Translate first-party UI/messages/fixtures; consolidate English documentation and accurate project status.
- [x] Add shared agent policy, contributor/community policies, templates, reproducible CI, and dependency monitoring.
- [x] Run Rust tests/build/format/lint, UI clean install/build/audit, repository checks, and history secret scan.
- [x] Review the actual diff and document remaining publication blockers and administrative settings.

## Review focus

Reject hostile browser origins and rebound hosts; retain Vite and native probes. Never log passwords through Debug. Stop idle gates even while a sender survives. Reject invalid PTT rather than transmit continuously. Never overwrite an identity during creation races. Windows is the only locally tested platform; no live audio/server claims arise from unit tests.

## Decisions

Work directly on `codex/open-source-hardening` in the supplied workspace so all requested changes remain reviewable. No new production dependency is needed. GitHub settings and private reporting contacts require owner setup. Automated language checks can detect known regressions but cannot prove natural-language correctness. Review remains mandatory.

## Verification ledger

33 Windows Rust tests pass. Isolated build, standalone probe check, strict first-party Clippy, explicit-package formatting, UI clean install/build/audit, repository policy checks, YAML parsing, and live HTTP/WebSocket smoke checks pass. Current source Gitleaks scan is clean. Full-history Gitleaks retains five upstream sample findings; both Cargo lockfiles retain the cryptography advisory. These are disclosed blockers, not accepted ignores. Hosted CI, POSIX tests, rendered UI, and real server/microphone behavior were not verified. See `AUDIT.md` for complete commands and scope.

Review corrected Vite proxy Host handling, YAML quoting, an English typo, and queued-audio fail-closed behavior. The reviewer rechecked the queue fix and found no new important concern. Build-output isolation avoids terminating an existing bridge process. No commits, publishing, pushes, history rewriting, or license selection occurred.

Final review also corrected inventory junction detection to use reparse metadata rather than a Python-version-specific API. A temporary in-repository junction was detected and skipped; it and its empty target were removed after verification.
