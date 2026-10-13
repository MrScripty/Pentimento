# Browser runtime upgrade

- Status: Verifying. Objective acceptance: partial; required rendered CI pending.
- Objective: update the supported Linux x86_64 CEF and Electron frontends to the latest stable releases available on 2026-10-13, preserving Bevy 0.20 and existing paint/sculpt behavior.
- Base: PR #27, `93f67c7d86c045cf298395b172acb0aa1ece14a6` (`perf/native-brush-workspace`). Work is isolated on `chore/browser-runtime-upgrades`; no merge or deployment.
- Current phase: main reconciliation and exact integrated-head qualification.
- Next slice: publish the reviewed main integration candidate, run its unchanged full frontend, rendered CEF/Electron and strict audit gates, and merge only when required claims are qualified. Retain every production rendering, security and input assertion.

## Main integration boundary

The Canvas ordering repair is included at `0d654286` and its five stack jobs now
pass, but that stack is based behind current main. Integration branch
`integration/qualified-runtime-stack` merges main `50eae5db` with this runtime
head, preserving published topology and main's gates. The [main integration report](reports/main-integration.md) records its reviewed
resolution, automatic-merge repairs, local evidence and unresolved exact-head
claims. Earlier stack acceptance does not qualify this integration.

## Binding decisions

The frontend maintainers own runtime selection and supported acquisition. Official Electron releases and npm identify Electron 44.7.0 (Chromium 152.0.7977.130). The official CEF distributor identifies CEF 154.0.34+g14c5a08+chromium-154.0.8037.98; the published cef-rs binding is 154.5.0+154.0.34. Stable is deliberate: Electron 45 beta and CEF 156 beta are outside scope. Exact direct pins and checked lockfiles preserve the reviewed application graph. Both runtimes retain their upstream license/notice requirements.

CEF host and helper must consume the same binding/runtime identity. Prefer the binding's existing authoritative downloader over a separately versioned unchecked archive. Keep launcher installation explicit. No new sandbox bypass, remote debugging exposure, or network security relaxation is authorized. The existing CEF no-sandbox setting is tracked as pre-existing debt; a need to weaken protection further is a blocker. Electron keeps isolated, sandboxed preload access.

The active host is `crates/webview`, while `crates/frontend-cef` is a separately compilable adapter and must not be left on an incompatible binding. The latter does not become an active frontend in this change.

Composed-design review: re-opened after representative Electron CI exposed Bevy WebGL constraints. The renderer remains the authority for actual target formats and depth support. IPC reports its depth capability and authoritative enabled state; each frontend consumes those facts without optimistic local success. Native supported depth remains available. Browser depth was previously a logged no-op, so an unavailable capability must be explicit rather than presented as restored functionality. Runtime acquisition stays within the launcher boundary; persistence ownership is unchanged. The bounded repair includes per-view postprocess specialization, byte-exact outline ID encoding, capability reporting, and fatal-log qualification. A separate browser color-depth renderer is outside this upgrade.

## Acceptance claims

| ID | Observable criterion | Evidence | Environment/mode | Status |
| --- | --- | --- | --- | --- |
| BR1 | Pins resolve to verified latest stable identities, with unchanged Bevy 0.20 | registry metadata, generated lockfiles, dependency graph | representative/automated | satisfied |
| BR2 | CEF host/helper and standalone adapter compile against matching 154 APIs; launcher selects the matching runtime | real local distribution checks plus first-head native CI build | representative/automated | satisfied, including repaired native head713653f |
| BR3 | Electron explicit install produces the pinned executable; production assets load from launcher output; IPC/preload remains isolated | shell contract tests, TypeScript, production run38414235793 on e368821 | representative/automated | satisfied on e368821; final-head rerun required |
| BR4 | Actual CEF app visibly renders toolbar and preserves strict native paint/sculpt assertions, including one-shot release/history ordering | native run38410829970 on713653f; run38414236795 on e368821 exposed a Canvas ordering gap | representative/automated | reopened for native input repair and final-head rerun |
| BR5 | Existing frontend and engine gates pass; independent review finds no unresolved introduced defect | aggregate run38414236123 on e368821, engine suites and review | representative/automated | passed on e368821; new ordering repair review and final-head rerun required |
| BR6 | WebGL renders the real scene with lossless outline IDs; browser depth reports its existing limitation, and native supported depth enables/disables without breaking scene restoration | descriptor/ID/capability tests plus real browser/native interaction evidence | representative/automated | CPU tests and rendered native713653f/Electron e368821 evidence pass; final-head rerun required |

Hardware/stylus responsiveness and unsupported Windows/macOS support remain outside this upgrade's claims. A software-rendered CI run cannot qualify actual hardware behavior. Native GUI acceptance remains blocked if the execution environment cannot supply it; checks must not be weakened.

## Milestones and ownership

1. Implemented: migrate dependency/provisioning consumers. Write set: CEF Cargo manifests/locks, CEF API adapters/helper as needed, launcher/setup scripts, `src-electron`, focused runtime tests, affected CI, runtime docs. Gates: BR1–BR3.
2. Verifying: execute aggregate and native qualification, diagnose blank CEF toolbar without suppressing assertions. Write set: minimal CEF capture/composition repair if evidence identifies an upgrade-relevant defect, focused regression and evidence reports. Gates: BR4–BR5.
   - Added bounded repair after the first Electron run: renderer depth/outline passes and shaders; existing IPC/native-state/WASM command consumers; current Svelte, egui and legacy Dioxus depth controls; focused source/runtime tests and fatal-log qualification. Preserve native depth behavior and all current paint/sculpt/workspace work. Acceptance requires exact-head production scene pixels and truthful capability state, not just compilation.
   - Added bounded input repair after the final CEF run exposed a real Canvas release/history race: preserve native event order through the existing stroke lifecycle consumer instead of mutating history earlier in PreUpdate. Direct regressions cover release-before-history, the inverse order and held strokes; no extra delay or repeated shortcut is acceptable. The ordered route is bounded to stable Canvas ownership with the color sampler unarmed. Existing Shift+Tab transition and armed-sampling frames retain their prior route and are regression-tested; a broader mode/sampling dispatcher redesign is outside this repair.
3. Active: independent review, atomic commit, publish draft stacked PR, inspect exact-head CI. Write set: evidence/plan closure and reviewed code only. Gates: all available evidence recorded, unavailable acceptance explicit.

## Blockers and re-plan triggers

Local executor currently lacks GTK/WebKitGTK/ALSA/udev development packages and display qualification. Prior PR #27 CEF startup readiness was corrected and first-head CEF154 runtime evidence confirms visible native toolbar/sculpt output. That run exposed a reversed vertical wheel sign, now repaired at the adapter boundary. First Electron production CI on PR #28 exposed unsupported WebGL VIEW_FORMATS, postprocess target format mismatch, and unsupported depth shader generation. The bounded renderer repair and truthful depth contract are implemented. Electron e368821 passes ordinary sandbox, real WASM rendering/selection and graceful process-family shutdown. CEF713653f passes all26 interactions, but the final e368821 rerun fails the last one-shot Redo after a focus click, exposing a real native Canvas same-frame release/history ordering gap. Repair that gap rather than claiming a harness settlement barrier fixes the product. Final aggregate and exact-head runtime acceptance remain required. Resolve through permitted native CI or a supported host. Re-plan if the pinned stable bindings cannot be acquired, changed APIs require a different architecture, sandbox operation requires new authority, the stack base changes, or required hosted jobs cannot run.

## Tracking

- [Execution ledger](execution-ledger.md)
- [Issues](issues.md)
- [Evidence and sources](reports/release-selection.md)
- Existing [frontend ownership ADR](../../adr/ADR-001-active-frontends-and-contract-ownership.md)

The task-created worktree is retained-protected while review/acceptance is open. Owner: runtime-upgrade task. Purpose: reproducible checks and draft review. Retirement: after the reviewed branch is integrated or explicitly abandoned, preserve its exact head on the remote or an archive ref before removing only this task's clean worktree.
