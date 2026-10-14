# Retained legacy frontends

WebKit capture/overlay and Tauri are discontinued (see
[ADR-001](../docs/adr/ADR-001-active-frontends-and-contract-ownership.md)).
This independent Cargo workspace preserves their source and entry points; it
is outside the supported CEF, Electron, Dioxus and experimental egui workflow.

The root workspace must not depend on these manifests. `legacy/app` and
`legacy/webview` compile the original `crates/app/src/main.rs` and
`crates/webview/src/lib.rs` with WebKit enabled. Keeping one source owner avoids
copying the application, renderer, input or IPC contract. The retained standalone
`crates/frontend-webkit`, `crates/frontend-overlay` and `src-tauri` packages
explicitly belong to this workspace.

## Entry points

From the repository root, build the existing Svelte UI first (`npm run build`).
On Linux, install the GTK 3/WebKitGTK 4.1 development libraries, plus the native
Bevy requirements documented in the root README. Then use:

```sh
# Original native capture/overlay app, with the same binary and environment selectors
PENTIMENTO_COMPOSITE=capture cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit
PENTIMENTO_COMPOSITE=overlay cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit

# Original capture example and standalone backends
cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-webview-legacy --example capture_test
cargo check --locked --manifest-path legacy/Cargo.toml -p pentimento-frontend-webkit -p pentimento-frontend-overlay

# Original Tauri app (frontend assets and native Tauri dependencies are still required)
cargo run --locked --manifest-path src-tauri/Cargo.toml -p pentimento-tauri
```

`just legacy-capture`, `just legacy-overlay`, `just legacy-build` and
`just legacy-check` are conveniences for these explicit manifests. Tauri's
configuration, build script, Rust entry points and frontend-dist path remain
unchanged. Its WASM assets must be prepared by the historical Tauri workflow.
No legacy command is added to `launcher.sh` or the active support matrix.

## Dependency and verification boundary

`legacy/Cargo.lock` is independent from the root lockfile and is tracked.
It deliberately retains GTK/WebKit/Tauri dependencies and their known audit
findings. Isolation is not a fix or security approval for those findings. Do
not copy its lockfile into the supported workspace or disable the root audit.
Legacy changes require their own lock review; use `--locked` for normal builds.

Workspace membership and retained entrypoint paths are checked through real
`cargo metadata --no-deps` consumers and the focused frontend dependency
contracts. These checks establish resolution boundaries and source ownership,
not a GUI support promise. Native compilation requires the actual GTK/WebKit
system development libraries; missing libraries must be reported as a blocker,
not replaced with fake pkg-config files or a skipped native check.

### Patched retained Tauri cohort (2026-10-14)

The separately maintained legacy host is constrained to Tauri **2.11.6**,
`tauri-runtime` **2.11.3** and `tauri-runtime-wry` **2.11.4**. Those runtime
versions are the stable pair declared by the official
[Tauri 2.11.6 host manifest](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri/Cargo.toml)
and [runtime-wry manifest](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri-runtime-wry/Cargo.toml).
Exact constraints in `src-tauri/Cargo.toml` prevent independent regeneration
from reintroducing incompatible runtime traits. Its companion macros/codegen/build/plugin helpers are constrained to **2.6.3**
and utils to **2.9.3**, matching that tag; shell-plugin remains locked to 2.3.4.

#### Advisory and observed reachability

The official HIGH
[GHSA-w28w-mhc8-qvjv advisory](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv)
affects Tauri 2.0.0 through 2.11.5; 2.11.6 is the first patched version. Its
internal channel-data fetch could expose or drain another WebView's queued
response data. The patched
[channel implementation](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri/src/ipc/channel.rs)
scopes queued responses and removal to their owning WebView.

The retained source configures one main WebView, one application command that
returns unit, and no explicit application Channel or additional remote WebView.
The shell plugin has a channel-producing spawn command, but no application
capability file or caller of that command was found. This inspection does not
demonstrate the advisory's complete cross-WebView exploit path, and it does not
make the vulnerable framework safe. The patched host is selected rather than
retaining the affected version or changing permissions/security configuration.

The same host update also includes the fix for
[GHSA-7gmj-67g7-phm9](https://github.com/tauri-apps/tauri/security/advisories/GHSA-7gmj-67g7-phm9),
the moderate Windows/Android local-origin confusion advisory affecting Tauri
2.0 through 2.11.0 and patched in 2.11.1. The current source inspection does not
demonstrate an attacker-controlled remote page, and Linux-only hosted build/
capture results do not establish Windows/Android runtime acceptance.

#### Macro producer and host consumer compatibility

The first native compile exposed a second independent mismatch: permissive
transitive constraints had kept macros 2.7.1 with host 2.11.6. The host imports
`tauri_macros::do_menu_item`, whose 2.7 expansion names
`Error::UnexpectedMenuKind`; the
[2.11.6 host Error enum](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri/src/error.rs)
has no such variant in any feature configuration. It is not a missing tray-icon
feature. Enabling tray or another optional feature cannot repair that producer/
consumer mismatch without expanding the product's feature boundary.

The official
[2.6.3 macros manifest](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri-macros/Cargo.toml)
and [menu expansion](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri-macros/src/menu.rs)
match this host. Code generation is also a host API: the exact constraints cover
[codegen](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri-codegen/Cargo.toml),
[build](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri-build/Cargo.toml),
[plugin helpers](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.6/crates/tauri-plugin/Cargo.toml)
and their shared utils producer rather than leaving the next regeneration able
to select a newer incompatible generator.

Tauri 2.12.2 was considered: its Error enum has the new variant, but its official
cohort also changes runtime/runtime-wry, Wry to 0.57, Tao to 0.37, Muda to 0.20
and tray-icon to 0.25. Matching the complete official patched 2.11.6 cohort
preserves the already selected native framework boundary and original features.
No framework or application source is vendored or patched to mask the mismatch.

#### Native compatibility and features

Tauri's maintained runtime requires Wry 0.55.x, whose
[official manifest](https://github.com/tauri-apps/wry/blob/wry-v0.55.1/Cargo.toml)
pins `webkit2gtk` and its native `web_kit2` links crate to 2.0.2. The former Wry
0.53.x pinned 2.0.1; both native-links versions cannot coexist in one Cargo
workspace. The legacy workspace therefore coordinates **Wry 0.55.1 / WebKit
2.0.2** across Tauri, capture and overlay. The standalone WebKit manifest now
inherits that workspace selection.

The upstream
[Wry 0.55.1 changelog](https://github.com/tauri-apps/wry/blob/wry-v0.55.1/CHANGELOG.md)
records the 0.55 API removals and 0.55.1 Android fix. Every retained native
consumer already uses `WebViewBuilder` and `build_gtk`; none uses the removed
`WebViewAttributes`, `WebView::new` or `WebView::new_as_child`. No application or
backend source, entry point or Tauri configuration changes are needed for the
selected manifest cohort. Native compilation and capture remain the acceptance
checks, rather than treating this source inspection as build evidence.

The historical effective Tauri defaults (`wry`, `compression`,
`common-controls-v6`, `dynamic-acl`, `x11`) plus `devtools` are explicit.
`custom-protocol` remains the app default feature. Newer Tauri's added `dbus`
default is not enabled: locked Linux metadata and the resolved feature tree
confirm its absence in Tauri, runtime-wry and Tao. The public configuration,
security settings, original Rust source and active frontend support boundary
are preserved.

#### Resolver review and acceptance

The focused update used exact host/runtime/Wry manifest constraints and
`cargo update --manifest-path legacy/Cargo.toml -p tauri --precise 2.11.6`.
The separate focused helper update used
`cargo update --manifest-path legacy/Cargo.toml -p tauri-macros --precise 2.6.3`
under the official companion constraints. Neither operation regenerated or
broadly updated the workspace. The
[normalized lock delta](tauri-cohort-delta.json) records the complete package
identity, checksum and resolved-edge review against the original retained lock.

- Tauri host/runtime/runtime-wry, Wry/WebKit, Tao, Muda and tray-icon move to the
  maintained compatible cohort; mobile Reqwest/wasm-streams follow its declared
  requirements.
- Wry's upstream Android HTML parser migration replaces the old kuchikiki
  subtree with dom_query and its parser cohort. Its now-unused rand 0.7 and
  getrandom 0.1 subtree is removed.
- Apple binding feature changes add core-location/user-notifications and remove
  unused core-video/javascript-core/security packages.
- The official helper cohort selects its matching config, compression, JSON and
  URL-pattern dependencies. The newer helper-only DOM/parser versions leave;
  Wry's 0.27 DOM/parser cohort remains.
- Package count changes from 950 to 915: 80 old identities leave and 45 enter;
  unchanged package identities retain their checksums. There are 10 meaningful
  unchanged-package edge changes; other lock diff lines normalize references
  after obsolete versions are removed.
- The root lock is unchanged. Retained GTK/glib advisory findings are not
  security-cleared by this Tauri fix.

Locked full Linux Cargo metadata, reverse consumer trees, resolved feature
inspection and the focused workspace/entrypoint contracts pass locally. The
genuine selected macros crate passes `cargo check` and `cargo build`; a narrow
compiler consumer using its actual `do_menu_item` macro compiles and dispatches
against an Error shape without the newer variant. This verifies the causal
macro/host Error compatibility seam, not the complete native application. The
strict hosted native all-target check, tests and actual capture smoke remain
required before acceptance because this local environment lacks genuine
GTK/WebKit development libraries. No gate is skipped or weakened.
