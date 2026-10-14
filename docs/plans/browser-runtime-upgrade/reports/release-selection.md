# Runtime selection

## Fresh published-stable check, 2026-10-14 UTC

- [Electron 44.7.1](https://releases.electronjs.org/release/v44.7.1) is the latest
  published stable, released 2026-10-13 at 17:54:22 UTC with Chromium
  152.0.7977.149 and Node 24.21.0. Its [GitHub release](https://github.com/electron/electron/releases/tag/v44.7.1)
  is neither draft nor prerelease. 44.7.2 is an active build, not a published
  stable selection.
- The [official release checksums](https://github.com/electron/electron/releases/download/v44.7.1/SHASUMS256.txt)
  and the actual installed npm package's checksum metadata independently agree:
  Linux x64 archive SHA256 is
  `fd4349ce666251f66acba2c88064ae6d65d5c61c0a44e68dd028d7c7f30eaa47`.
  Protected provisioning uses this reviewed digest; it does not follow an
  unverified mutable latest archive.
- The shell lock was regenerated through the official registry with Node
  22.23.3/npm 10.9.9. Only Electron's root declaration and package identity,
  tarball and integrity changed. Root Cargo/npm locks are unchanged. Clean
  install/typecheck, 45 focused Node tests, 17 provisioner tests and four
  supervisor tests pass. Actual 44.7.1 sandboxed renderer/shutdown acceptance
  remains a required final-head hosted observation.
- The [official CEF build index](https://cef-builds.spotifycdn.com/index.json)
  still selects 154.0.34+g14c5a08+chromium-154.0.8037.98 as highest stable across
  its platform entries. CEF 156 entries are beta. The Linux standard build is
  dated 2026-10-05 at 04:07:59 UTC.
- [Official cef crate metadata](https://crates.io/api/v1/crates/cef) still selects
  non-yanked stable 154.5.0+154.0.34, published 2026-10-07 at 04:50:19 UTC.
  CEF's binding/runtime cohort remains unchanged.

These are fresh selection and focused supporting facts. Earlier runtime
captures below belong to their recorded identities; their checksums and version
claims are not rewritten into 44.7.1 qualification.

## Historical selection, checked 2026-10-13 UTC

- Electron official latest stable: [44.7.0](https://releases.electronjs.org/release/v44.7.0), released 2026-10-07, Chromium152.0.7977.130, Node24.21.0. npm `electron@latest` independently agrees. Existing manifest ^38.0.0 and lock38.8.6 do not satisfy the requested update.
- CEF official [automated builds](https://cef-builds.spotifycdn.com/index.html): current stable154.0.34+g14c5a08+chromium-154.0.8037.98, released2026-10-05. The CEF project [documents this distributor](https://github.com/chromiumembedded/cef/blob/master/docs/automated_build_setup.md).
- [cef-rs154.5.0+154.0.34](https://docs.rs/cef/154.5.0+154.0.34/cef/) is the published Rust binding for that CEF release. `cargo info cef@154.5.0` downloaded and reported the same identity and Apache-2.0 OR MIT license.
- Existing [cef-rs provisioning](https://github.com/tauri-apps/cef-rs#install-shared-cef-binaries) downloads the matching binary through cef-dll-sys. A manually selected CEF144 archive was inconsistent with the old143 binding and should not survive migration.

These sources establish available identities, not application compatibility. Compile, packaging, IPC and visible-rendering checks supply separate acceptance evidence.
