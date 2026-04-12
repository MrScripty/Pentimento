# Plan: Bevy egui Native Frontend

## Objective

Add a new native Bevy + egui frontend path that reproduces the current Dioxus
frontend behavior while preserving Pentimento's existing architectural
boundaries, shared IPC contract ownership, and standards-compliant documentation,
testing, and launcher workflows.

## Scope

### In Scope

- Add a feature-gated `egui` native frontend path to the workspace.
- Keep egui-specific presentation code in a dedicated crate and keep Bevy-side
  orchestration in a thin `crates/app/src/render/ui_egui/` adapter.
- Clone the current Dioxus-visible UX surface for toolbar, add-object menu,
  side panels, paint controls, hotkeys, and backend command flow.
- Extract or introduce framework-neutral native UI logic where direct copying
  would otherwise create duplicated, overgrown modules.
- Update launcher, README, ADR/traceability, and verification coverage required
  for a standards-compliant frontend path.

### Out of Scope

- Making egui the default frontend.
- Replacing or removing CEF, Electron, or Dioxus.
- Adding new product features beyond parity with the current Dioxus path.
- Adding persisted egui-specific layout/preferences.
- Expanding the support matrix beyond Linux x86_64.

## Inputs

### Problem

Pentimento already has a native Dioxus frontend and a browser-based frontend
contract. The requested egui implementation should behave like the current
Dioxus path, but a direct file-for-file port would likely violate the code and
architecture standards by duplicating large UI modules, mixing framework logic
into the app composition layer, and creating drift between native frontend
implementations.

### Constraints

- `crates/ipc` remains the source of truth for shared frontend/backend message
  contracts per `ADR-001`.
- `crates/app` remains the composition root; framework runtimes must stay out of
  scene/domain code and out of generic render/input abstractions where possible.
- New `src/` directories require standards-compliant `README.md` files.
- Current Dioxus modules already exceed soft decomposition thresholds in places,
  so the egui path must not normalize more large copy-pasted files.
- Linux x86_64 remains the required verification target and `launcher.sh`
  remains the canonical workflow.
- Shared dependency versions must stay centralized in the workspace manifest.
- The repo is on Bevy `0.18`; the integration choice must be compatible with
  that version.

### Assumptions

- "Clone of the current Dioxus version" means parity with the current Dioxus UX
  and command surface, not full parity with every browser-only behavior.
- The egui path can render to a Bevy-managed texture or image-backed overlay
  instead of requiring a literal world-space quad, as long as the user-visible
  result matches the existing full-screen overlay behavior.
- The existing `UiToBevy` and `BevyToUi` contracts cover most parity needs; any
  missing state can be added additively.
- No project file or save-file schema changes are required for the initial egui
  implementation.

### Dependencies

- `bevy_egui` as the preferred Bevy-native egui integration path for Bevy
  `0.18`.
- Existing reference implementation in `crates/dioxus-ui/` and
  `crates/app/src/render/ui_dioxus/`.
- Existing launcher, README, IPC sample tests, and source-directory README
  verification scripts.

### Affected Structured Contracts

- `crates/app/src/config.rs` frontend mode selection.
- `launcher.sh` frontend selector and canonical build/run/test surface.
- `crates/ipc` message enums and contract samples, but only if parity work
  reveals missing state or commands.
- New Rust public facade for the egui crate and its Bevy-side orchestration
  module.

### Affected Persisted Artifacts

- `Cargo.lock` and workspace manifests.
- README/ADR/plan traceability documents.
- New module `README.md` files.
- None identified for user project/save artifacts as of 2026-04-12.
- Reason: the proposed frontend is a runtime-only implementation path.
- Revisit trigger: egui adds saved layout presets, UI state persistence, or
  frontend-specific serialized assets.

### Concurrency and Race-Risk Review

- Immediate-mode egui can re-run every frame, so outbound UI commands must be
  edge-triggered or value-change-triggered rather than emitted continuously
  during steady-state rendering.
- One module must own backend snapshot ingestion and one module must own
  UI-to-Bevy command dispatch. Do not split message lifecycle ownership across
  app systems and egui widgets.
- If the egui path uses a render-to-image target, one Bevy-side lifecycle owner
  must create, resize, and retire that target to prevent overlap and stale-handle
  races during resize/restart paths.
- Avoid new polling loops or timers. Prefer Bevy schedules and event-driven
  state updates so the new frontend does not introduce lifecycle ambiguity.

### Risks

| Risk | Impact | Mitigation |
| ---- | ------ | ---------- |
| Directly copying Dioxus modules into egui creates long-term frontend drift and violates decomposition thresholds. | High | Extract framework-neutral native UI state, intent, and widget helper layers before broad parity work. |
| Egui pointer/keyboard focus may conflict with scene camera and tool input. | High | Define explicit input-absorption rules and acceptance checks before porting complex widgets. |
| A custom egui render path could recreate work already solved by Bevy integration crates. | Medium | Spike the `bevy_egui` multipass and render-to-image path first, then only go lower-level if that path fails required behavior. |
| Dioxus parity may expose missing backend-to-UI snapshots or commands. | Medium | Audit the parity surface early and make append-only `crates/ipc` changes before UI implementation scales out. |
| Launcher and README could become inaccurate if egui is added without support/verification updates. | Medium | Treat docs and launcher updates as part of the implementation, not follow-up cleanup. |

## Definition of Done

- `pentimento` can build and run with an `egui` frontend selection without
  breaking the existing CEF, Electron, or Dioxus paths.
- The egui frontend reproduces the current Dioxus-visible workflow for toolbar,
  add-object menu, side panels, paint controls, and current hotkeys.
- Backend-owned state still flows through shared contracts instead of drifting
  into framework-local business state.
- New source directories include standards-compliant `README.md` files.
- Required documentation and launcher updates are landed together with the code.
- Verification coverage exists for buildability, contract integrity, and the
  main egui interaction smoke path.

## Architecture Notes

### Ownership and Lifecycle Note

- `crates/app/src/render/mod.rs` remains the composition root and is the only
  place that selects the egui runtime path.
- `crates/egui-ui/` owns egui presentation, layout, and intent translation only.
- `crates/app/src/render/ui_egui/` owns Bevy resource setup, resize lifecycle,
  render-target lifecycle, and UI/backend message bridging.
- No background task, timer, or retry loop should be introduced for the egui
  path unless a later requirement proves it necessary and documents the owner,
  start/stop points, and overlap prevention rules.

### Public Facade Preservation Note

- Use facade-first preservation.
- Extend frontend selection, launcher routing, render/input dispatch, and IPC
  handling additively.
- Do not break existing `cef`, `dioxus`, or `electron` entrypoints while
  landing egui support.

## Milestones

### Milestone 1: Architecture Spike and Parity Inventory

**Goal:** Prove the intended egui integration shape before large-scale porting.

**Tasks:**
- [ ] Inventory the current Dioxus surface that must be cloned: toolbar,
      add-object flow, side panel, paint panel, paint toolbar, and hotkeys.
- [ ] Decide the package split: `crates/egui-ui/`,
      `crates/app/src/render/ui_egui/`, and any framework-neutral shared native
      UI core needed to avoid duplication.
- [ ] Validate the preferred Bevy-native egui render path with a minimal spike,
      favoring `bevy_egui` multipass plus render-to-image or equivalent overlay
      composition.
- [ ] Record whether egui will launch as an active frontend immediately or ship
      as experimental until the canonical launcher/test surface is updated.

**Verification:**
- `cargo check -p pentimento --features egui` for the spike path.
- Manual smoke check: transparent egui content renders over the Bevy scene,
  resizes correctly, and does not crash on startup.
- Architecture review against `PLAN-STANDARDS.md`, `CODING-STANDARDS.md`, and
  `ARCHITECTURE-PATTERNS.md`.

**Status:** Completed

### Milestone 2: Shared Native UI Core Extraction

**Goal:** Create a framework-neutral layer for shared native frontend behavior so
egui does not become a second copy of Dioxus.

**Tasks:**
- [ ] Extract shared native UI concepts that are not framework-specific:
      view-model shaping, intent helpers, constants, and backend snapshot
      adapters.
- [ ] Keep framework-specific widget trees inside the Dioxus and egui crates;
      do not move egui-specific or Dioxus-specific rendering concerns into the
      shared layer.
- [ ] Add focused unit tests for extracted pure logic.
- [ ] Refactor the Dioxus path to consume the shared layer where appropriate so
      both native frontends exercise the same abstractions.

**Verification:**
- `cargo check -p pentimento-dioxus-ui`.
- Unit tests for shared pure modules.
- Decomposition review for any module approaching file-size or responsibility
  thresholds.

**Status:** Completed

### Milestone 3: Egui UI Crate Implementation

**Goal:** Implement the egui widget tree and UI-only local state with parity to
the current Dioxus frontend.

**Tasks:**
- [ ] Create `crates/egui-ui/` with a small, documented module tree that keeps
      widgets and panels under the repo's decomposition thresholds.
- [ ] Port the Dioxus-visible controls and hotkeys to egui using the shared
      native UI core and existing IPC vocabulary.
- [ ] Keep backend-owned state read-only in the egui layer except through
      explicit intent dispatch.
- [ ] Ensure widgets emit commands only on user edges or meaningful value
      changes, not every frame.

**Verification:**
- `cargo rustc -p pentimento-egui-ui --lib -- -D warnings`.
- Unit tests for any pure helpers, reducers, or widget-state adapters.
- Manual parity check against the Dioxus frontend for the implemented widgets.

**Status:** Completed

### Milestone 4: Bevy Integration and Input Arbitration

**Goal:** Integrate egui into Pentimento's Bevy runtime without leaking egui
details into generic render, input, scene, or IPC layers.

**Tasks:**
- [ ] Add the `egui` feature, workspace dependencies, and new frontend mode
      selection in `crates/app`.
- [ ] Implement `crates/app/src/render/ui_egui/` as a thin orchestration layer
      analogous to `ui_dioxus/`, including resource setup, resize handling, and
      message bridging.
- [ ] Reuse the existing overlay composition model where practical rather than
      inventing a parallel frontend pipeline.
- [ ] Integrate egui input capture rules so camera orbit, paint gestures, and
      viewport shortcuts do not fire when egui owns the interaction.
- [ ] Keep scene/domain crates unaware of egui-specific runtime types.

**Verification:**
- `cargo check -p pentimento --features egui`.
- Manual smoke checks for startup, resize, focus changes, mouse capture, and
  keyboard hotkey behavior.
- Acceptance check that UI interactions do not leak through to viewport actions
  while egui is actively consuming input.

**Status:** Completed

### Milestone 5: Tooling, Documentation, and Launcher Integration

**Goal:** Make the egui path discoverable, verifiable, and honest in repo
documentation and tooling.

**Tasks:**
- [ ] Update `launcher.sh` to build/run the egui frontend if the implementation
      is considered active at merge time.
- [ ] Update root README support tables and active frontend descriptions.
- [ ] Add or update an ADR if the active frontend set or ownership model changes.
- [ ] Update `scripts/rustfmt-active.sh`, README coverage, and verification
      scripts for any new active-source directories.
- [ ] Add standards-compliant `README.md` files for all new `src/` directories.

**Verification:**
- `./scripts/check-source-readmes.sh --all`
- `./scripts/rustfmt-active.sh --check`
- `cargo check -p pentimento --features egui`
- If egui is active: `./launcher.sh --build --frontend egui` and
  `./launcher.sh --run --frontend egui`

**Status:** Completed

### Milestone 6: Acceptance, Parity Review, and Promotion Decision

**Goal:** Verify that the egui path is ready for ongoing maintenance without
introducing contract drift or support ambiguity.

**Tasks:**
- [ ] Run a parity checklist comparing Dioxus and egui behavior for the agreed
      controls and workflows.
- [ ] Resolve gaps in input handling, visual hierarchy, or backend command flow.
- [ ] Decide whether egui is promoted to an active frontend immediately or kept
      experimental until broader verification is added.
- [ ] Close out any remaining traceability updates tied to that status decision.

**Verification:**
- `./launcher.sh --test` after egui-related verification is wired in.
- Frontend acceptance smoke check covering add object, selection/material
  controls, paint controls, depth-view toggling, and hotkeys.
- Contract acceptance coverage if any `crates/ipc` messages changed.

**Status:** In progress

## Execution Notes

Update during implementation:
- 2026-04-12: Initial plan created from current repo architecture, standards,
  and official egui/Bevy integration references.
- 2026-04-12: Landed the egui frontend spike, shared native snapshot core,
  shared native command dispatch, and explicit egui input arbitration.
- 2026-04-12: Wired `launcher.sh`, README coverage, rustfmt scope, and source
  directory README verification for the experimental egui path.

## Commit Cadence Notes

- Commit when a logical slice is complete and verified.
- Keep commits atomic and reviewable.
- Follow commit format/history cleanup rules from `COMMIT-STANDARDS.md`.

## Re-Plan Triggers

- The `bevy_egui` spike cannot satisfy the required overlay, transparency, or
  input arbitration behavior.
- Parity work requires a materially broader IPC contract than the current plan
  assumes.
- The shared native UI extraction proves higher-risk than controlled duplication
  for an initial landing.
- Egui is required to ship as an active frontend before launcher/test/docs work
  can be completed in the same slice.
- Cross-platform support requirements expand beyond Linux x86_64.

## Recommendations

- Recommendation 1: Extract a small framework-neutral native UI core before
  broad egui parity work. This adds upfront scope but is the safest way to avoid
  duplicating the already-large Dioxus modules and keeps long-term maintenance
  aligned with the decomposition standards.
- Recommendation 2: Prefer `bevy_egui`'s Bevy-native integration and
  render-to-image workflow over a bespoke egui+winit pipeline or a literal
  world-space quad unless a world-space UI is a product requirement. This
  reduces render/input lifecycle risk and fits the existing Bevy composition
  model.
- Recommendation 3: If launcher/test/readme support cannot land in the same
  slice, ship egui as explicitly experimental first rather than silently making
  the support matrix inaccurate.

## Completion Summary

### Completed

- Milestones 1 through 5 landed on 2026-04-12.
- The egui path now builds through `launcher.sh`, shares native snapshot and
  command infrastructure with Dioxus, and participates in the canonical local
  verification suite as an experimental frontend.
- Revisit trigger: manual parity smoke results justify promoting egui from
  experimental to active.

### Deviations

- Promotion to an active supported frontend is deferred.
- Reason: the implementation now compiles and routes through the launcher, but
  the final parity and smoke decision in Milestone 6 still depends on manual
  runtime validation.

### Follow-Ups

- Run the manual parity checklist in Milestone 6 and record whether egui should
  remain experimental or be promoted to an active supported frontend.

### Verification Summary

- Reviewed against:
  `PLAN-STANDARDS.md`,
  `CODING-STANDARDS.md`,
  `ARCHITECTURE-PATTERNS.md`,
  `DEPENDENCY-STANDARDS.md`,
  `DOCUMENTATION-STANDARDS.md`,
  `TESTING-STANDARDS.md`,
  and `FRONTEND-STANDARDS.md`.

### Traceability Links

- Module README updated: N/A until implementation starts.
- ADR added/updated: required in Milestone 5 if egui changes the active
  frontend set or frontend ownership model.
- PR notes completed per `templates/PULL_REQUEST_TEMPLATE.md`: N/A until
  implementation starts.

## Brevity Note

Keep implementation updates concise. Expand detail only when an execution
decision, risk, or re-plan trigger requires it.
