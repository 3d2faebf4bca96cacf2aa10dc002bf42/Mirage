# Third-party notices and review status

First-party code uses the existing MIT license. Dependencies retain their own licenses. This is a source-level inventory of direct components, not a complete binary-distribution notice bundle or legal compatibility certification.

| Component | Location | Declared license / status |
| --- | --- | --- |
| tsclientlib, tsproto, tsproto-packets, ts-bookkeeping, tsproto-structs, tsproto-types | `vendor/tsclientlib/` | MIT OR Apache-2.0; MIT option retained, both license files preserved |
| tsdeclarations | `vendor/tsclientlib/utils/tsproto-structs/declarations/` | MIT OR Apache-2.0; separate license files preserved |
| quicklz Rust crate | Cargo-resolved protocol dependency | MIT OR Apache-2.0 in installed package metadata; not the QuickLZ C source bundled by research projects |
| audiopus | Direct Rust audio dependency | ISC in installed package metadata |
| audiopus_sys and bundled libopus | Transitive native codec build | audiopus_sys declares ISC; inspect its bundled source license notices before binary distribution |
| cpal | Direct audio devices dependency | Apache-2.0 in installed package metadata |
| React, React DOM | UI runtime dependencies | MIT in installed package metadata |
| Vite, TypeScript, plugin/types packages | UI development/build dependencies | Inspect installed `node_modules/*/package.json` and retained license notices before packaging |

All resolved Rust dependencies are recorded in `Cargo.lock`; UI dependencies in `apps/ui/package-lock.json`. Generate a complete resolved license inventory and review native/transitive notices before shipping binaries. The audit verified preserved vendored licenses and direct installed metadata, not every transitive source file or copied-code provenance.

## Research-only sources

The ignored `vendor/astrum-ts6`, `vendor/ts3j`, and WebSpeak clones are research references, not dependencies of the client workspace. Their protocols are discussed in `../docs/`. Astrum declares AGPL-3.0; the historical ts3j review noted mixed licensing, including bundled GPL/commercial QuickLZ code. Do not copy their implementation into this MIT client without a separate license review. Historical research does not by itself establish legal conclusions about all components.

The proprietary TeamSpeak server/client are neither vendored nor redistributed. Do not include ignored installers, extracted binaries, databases, configuration, or runtime secrets in source archives or releases.
