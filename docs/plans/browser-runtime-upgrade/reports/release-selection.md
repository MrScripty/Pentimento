# Runtime selection, checked 2026-10-13 UTC

- Electron official latest stable: [44.7.0](https://releases.electronjs.org/release/v44.7.0), released 2026-10-07, Chromium152.0.7977.130, Node24.21.0. npm `electron@latest` independently agrees. Existing manifest ^38.0.0 and lock38.8.6 do not satisfy the requested update.
- CEF official [automated builds](https://cef-builds.spotifycdn.com/index.html): current stable154.0.34+g14c5a08+chromium-154.0.8037.98, released2026-10-05. The CEF project [documents this distributor](https://github.com/chromiumembedded/cef/blob/master/docs/automated_build_setup.md).
- [cef-rs154.5.0+154.0.34](https://docs.rs/cef/154.5.0+154.0.34/cef/) is the published Rust binding for that CEF release. `cargo info cef@154.5.0` downloaded and reported the same identity and Apache-2.0 OR MIT license.
- Existing [cef-rs provisioning](https://github.com/tauri-apps/cef-rs#install-shared-cef-binaries) downloads the matching binary through cef-dll-sys. A manually selected CEF144 archive was inconsistent with the old143 binding and should not survive migration.

These sources establish available identities, not application compatibility. Compile, packaging, IPC and visible-rendering checks supply separate acceptance evidence.
