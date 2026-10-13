# Local qualification

Date: 2026-10-13. Linux x86_64 cloud workspace; Rust1.97.1, Node24.19.0, npm11.9.0. Full native desktop development libraries and an X11 display are unavailable here.

## Passed

- `cargo update -p cef --precise 154.5.0`: generated the reviewed root lockfile. Only cef, cef-dll-sys, and download-cef changed; exact Bevy0.20.0 is preserved.
- `cargo build --locked -p pentimento-cef-helper` against the real CEF154.0.34 distribution. The helper selects the compiled versioned API before subprocess entry. Direct execution without subprocess arguments returns its documented browser-process rejection code1, with no API mismatch error. This is library loading/startup evidence, not browser rendering.
- Active CEF adapter source compiled in an ephemeral crate containing the real `platform_linux_cef.rs` and `error.rs`, plus their real dependencies. No adapter code was substituted; GTK/WebKitGTK integration is outside that source check.
- `cargo check --locked --manifest-path crates/frontend-cef/Cargo.toml`: standalone adapter compiles against the real downloaded154 distribution, without dox. Its seven existing unused/private-interface warnings remain; compilation does not qualify standalone browser rendering. An earlier dox-only typecheck also passed.
- `node scripts/cef-runtime.mjs <built-binary-directory> Cargo.lock`: actual copied CEF154 archive metadata/resources pass. Four focused tests cover matching artifacts, older/newer archives, missing resources, malformed metadata and absent/duplicate lock entries.
- Electron clean locked package install, expected missing-binary preflight rejection, explicit runtime installation, successful read-only preflight, and real `--version` output44.7.0.
- Final `npm run verify`: active Rust formatting, zero-warning Svelte accessibility/typecheck, Electron typecheck and78 contract/UI tests passed after review fixes. Independent review also ran all78 successfully.
- `npm run build`: production Svelte assets generated successfully. Four real-process Electron supervisor regressions also passed.
- `./scripts/check-source-readmes.sh --all`, `bash -n launcher.sh scripts/setup-cef.sh`, and `git diff --check` passed.

## Acquisition details

The crate downloader's local HTTP transport returned `connection refused`. curl successfully accessed the same official distributor through the environment's supported proxy. For this local check only, the official Linux minimal archive was retrieved with curl and verified against the distributor index SHA1 `15c91ad767097217b2e82ba8b933fe30b134be69`. Its Release/Resources/include/wrapper layout and archive.json followed download-cef3.0.0's extraction contract. This was a local build input; no binary or custom downloader is committed. Normal provisioning continues through the locked upstream cef-dll-sys downloader.

## Unsatisfied claims

`cargo check --locked -p pentimento --features cef` was attempted and stopped in glib-sys because pkg-config could not locate `glib-2.0.pc` (required >=2.70). Actual CEF native application compilation and pixel qualification require the Linux development libraries/display provided by the existing CI job. Actual Electron/WASM production rendering and ordinary sandbox behavior require its separate representative CI job. Compilation, mocked contract tests, metadata checks and version output do not satisfy those rendered claims. No hardware/stylus or unsupported-platform qualification is claimed.
