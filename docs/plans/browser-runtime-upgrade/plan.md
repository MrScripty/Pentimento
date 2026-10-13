# Browser runtime upgrade

- Status: Verifying. Objective acceptance: partial; required rendered CI pending.
- Objective: update the supported Linux x86_64 CEF and Electron frontends to the latest stable releases available on 2026-10-13, preserving Bevy 0.20 and existing paint/sculpt behavior.
- Base: PR #27, `93f67c7d86c045cf298395b172acb0aa1ece14a6` (`perf/native-brush-workspace`). Work is isolated on `chore/browser-runtime-upgrades`; no merge or deployment.
- Current phase: independent review and representative runtime qualification.
- Next slice: publish the reviewed draft stack and evaluate exact-head native CEF and Electron production qualification.

## Binding decisions

The frontend maintainers own runtime selection and supported acquisition. Official Electron releases and npm identify Electron 44.7.0 (Chromium 152.0.7977.130). The official CEF distributor identifies CEF 154.0.34+g14c5a08+chromium-154.0.8037.98; the published cef-rs binding is 154.5.0+154.0.34. Stable is deliberate: Electron 45 beta and CEF 156 beta are outside scope. Exact direct pins and checked lockfiles preserve the reviewed application graph. Both runtimes retain their upstream license/notice requirements.

CEF host and helper must consume the same binding/runtime identity. Prefer the binding's existing authoritative downloader over a separately versioned unchecked archive. Keep launcher installation explicit. No new sandbox bypass, remote debugging exposure, or network security relaxation is authorized. The existing CEF no-sandbox setting is tracked as pre-existing debt; a need to weaken protection further is a blocker. Electron keeps isolated, sandboxed preload access.

The active host is `crates/webview`, while `crates/frontend-cef` is a separately compilable adapter and must not be left on an incompatible binding. The latter does not become an active frontend in this change.

Composed-design review: not-applicable. This preserves the existing CEF/Bevy and Electron/Svelte/WASM decomposition, IPC ownership, and persistence contracts. Runtime acquisition changes remain within the existing launcher boundary; no new domain composition is introduced.

## Acceptance claims

| ID | Observable criterion | Evidence | Environment/mode | Status |
| --- | --- | --- | --- | --- |
| BR1 | Pins resolve to verified latest stable identities, with unchanged Bevy 0.20 | registry metadata, generated lockfiles, dependency graph | representative/automated | satisfied |
| BR2 | CEF host/helper and standalone adapter compile against matching 154 APIs; launcher selects the matching runtime | compile plus focused provisioning tests | representative/automated | pending |
| BR3 | Electron explicit install produces the pinned executable; production assets load from launcher output; IPC/preload remains isolated | shell contract tests, TypeScript, production/runtime smoke | representative/automated | pending |
| BR4 | Actual CEF app visibly renders toolbar and preserves strict native paint/sculpt assertions | existing native qualification | representative/automated | pending |
| BR5 | Existing frontend and engine gates pass; independent review finds no unresolved introduced defect | launcher and focused suites, review | representative/automated | pending |

Hardware/stylus responsiveness and unsupported Windows/macOS support remain outside this upgrade's claims. A software-rendered CI run cannot qualify actual hardware behavior. Native GUI acceptance remains blocked if the execution environment cannot supply it; checks must not be weakened.

## Milestones and ownership

1. Implemented: migrate dependency/provisioning consumers. Write set: CEF Cargo manifests/locks, CEF API adapters/helper as needed, launcher/setup scripts, `src-electron`, focused runtime tests, affected CI, runtime docs. Gates: BR1–BR3.
2. Verifying: execute aggregate and native qualification, diagnose blank CEF toolbar without suppressing assertions. Write set: minimal CEF capture/composition repair if evidence identifies an upgrade-relevant defect, focused regression and evidence reports. Gates: BR4–BR5.
3. Active: independent review, atomic commit, publish draft stacked PR, inspect exact-head CI. Write set: evidence/plan closure and reviewed code only. Gates: all available evidence recorded, unavailable acceptance explicit.

## Blockers and re-plan triggers

Local executor currently lacks GTK/WebKitGTK/ALSA/udev development packages and display qualification. Prior PR #27 CEF run had a blank toolbar despite DOM readiness; its baseline origin is not established. Resolve through permitted native CI or a supported host. Re-plan if the pinned stable bindings cannot be acquired, changed APIs require a different architecture, sandbox operation requires new authority, the stack base changes, or required hosted jobs cannot run.

## Tracking

- [Execution ledger](execution-ledger.md)
- [Issues](issues.md)
- [Evidence and sources](reports/release-selection.md)
- Existing [frontend ownership ADR](../../adr/ADR-001-active-frontends-and-contract-ownership.md)

The task-created worktree is retained-protected while review/acceptance is open. Owner: runtime-upgrade task. Purpose: reproducible checks and draft review. Retirement: after the reviewed branch is integrated or explicitly abandoned, preserve its exact head on the remote or an archive ref before removing only this task's clean worktree.
