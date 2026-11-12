# Contributing

This is an experimental Windows client. Start with the [README](README.md), [architecture](docs/architecture.md), and [development commands](docs/DEVELOPMENT.md). Use the committed lockfiles and the documented toolchain. Keep changes focused; discuss major protocol, UI, or dependency changes in a feature issue before implementing them.

## Language and evidence

Write all first-party content in English, including comments, tests, logs, UI text, issues, pull requests, and commit messages. Preserve protocol identifiers and third-party legal notices. Every technical claim needs code, configuration, an official source, or an executed check. Describe uncertainty explicitly. AI-assisted contributions follow [AGENTS.md](AGENTS.md); contributors remain responsible for every submitted line.

## Work and submit

1. Branch from the current default branch using a descriptive name such as `fix/idle-gate` or `docs/setup` (`codex/` for agent-created branches).
2. Inspect affected modules and preserve public interfaces unless a breaking change is intentional and explained.
3. Add regression tests for behavioral changes. Network and hardware tests are separate from offline tests; do not make ordinary CI depend on proprietary servers, microphones, or credentials.
4. Run the checks in [development](docs/DEVELOPMENT.md). Review the diff for secrets, personal paths, accidental generated files, licensing changes, and documentation drift.
5. Use a short imperative English commit subject; optional prefixes include `fix:`, `feat:`, `docs:`, and `ci:`. Do not invent releases in the changelog.
6. Open a focused PR using the template. Include actual checks/results, compatibility impact, security implications, related issues, and documentation changes. Failed or unavailable checks must be visible.
7. Respond to review with the reason for the chosen solution and fresh verification after substantive changes. First-time contributions and questions are welcome.

## Acceptance and risk-based review

A contribution is ready when its behavior is understood, relevant tests pass, failures are explained, documentation matches the implementation, and no credentials or machine-specific configuration are introduced. Review both human- and AI-authored code for correctness. Maintainers should scrutinize identity/crypto, authentication, authorization, network destinations, native code, dependencies, installation hooks, workflows, and release scripts. Do not execute untrusted scripts on a machine containing credentials. Do not bypass advisory or secret-scanning failures to merge.

Dependency PRs must explain why the new version is needed, review changed licenses and install/build hooks, update the appropriate lockfile, and rerun checks. Vendored changes require provenance and a documented patch under `client/patches/`; preserve upstream notices.

## Reports and proposals

Use the bug/feature issue templates. Bugs need sanitized reproduction steps, tool/server versions, expected/observed behavior, and relevant results. Do not attach private identity JSON, databases, passwords, full environment dumps, or raw logs. Follow [SECURITY.md](SECURITY.md) for vulnerabilities. Community behavior follows [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Releases and compatibility

There is no established release history or supported stable line. Before a first tagged release, review [publication blockers](docs/AUDIT.md), choose verified platform/server compatibility, and produce a complete dependency/license inventory. Recommended practice is semantic versioning with explicit pre-1.0 breaking-change notes, migration guidance, and deprecation notices before removals where practical. This is guidance, not a release schedule or support promise. Record substantive changes under Unreleased until an actual release is made.

Triage issues as bugs, features, documentation, security (private), or maintenance. Labels, required reviews, and branch protection require administrator configuration; see [maintainer setup](docs/DEVELOPMENT.md#github-administration).
