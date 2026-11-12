# Rust client workspace

Start with the [root README](../README.md) for requirements, installation, status, and licensing.

- [Architecture](../docs/architecture.md): component boundaries and session polling constraints.
- [Development](../docs/DEVELOPMENT.md): CLI, configuration, checks, and troubleshooting.
- [Historical measurements](../docs/verification.md): previous beta13.1 experiments and limitations.
- [Audit](../docs/AUDIT.md): current verification and publication blockers.
- [Upstream source and patch policy](UPSTREAM.md).
- [Third-party notices](THIRD_PARTY.md).

This directory is the Cargo workspace root. Run `./scripts/build.ps1 test --locked --workspace`
from here on Windows; the helper sets the CMake compatibility policy for bundled libopus.
All first-party project content and contributions must be written in English.
