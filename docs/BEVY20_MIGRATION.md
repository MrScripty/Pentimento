# Bevy 20 development candidate

This branch uses official Bevy 0.20.0, official bevy_egui 0.43.0-rc.1
(prerelease), and egui 0.36.2. The checked dependency graph is committed in
Cargo.lock. Qualification used Rust 1.97.1.

The candidate carries the native painting/sculpting panels and the chronological
Canvas, Direct UV, and Sculpt input/history integration from cloud/egui-parity
commit c825e0a2e49f704611c5ff261898561971b85b53. The Bevy migration changes
render scheduling, WESL imports, and required API bindings. Geometry algorithms,
safety tolerances, and history restoration/replay bodies are preserved.

## Completed checks

- Production Scene: `cargo check -p pentimento-scene --features selection,mesh_painting,mesh_editing,sculpting,atmosphere --locked --offline`.
- Production native App: `cargo check -p pentimento --features egui --locked --offline`.
- Production egui presentation widgets: 14 tests passed with egui 0.36.2.
- Independent source review: the final 56-file source/lock freeze and the narrow
  App/geometry API whitelist matched; required public exports and Bevy UI
  features were corrected before the successful App check.

The 303 passing controller/Scene/widget tests on c825e0a were run with Bevy 0.18
and egui 0.33. Those results support the carried input/history integration, but
are not Bevy 20 runtime qualification.

## Remaining validation

The full native executable has not yet been linked or launched. GPU/WESL
execution, depth/outline behavior, device recovery, physical input, and complete
Bevy 20 history/pipeline execution remain unqualified. Dioxus/CEF feature builds
and frontend runtime parity also remain unqualified. Frontend selection routes
are retained.

At the initial handoff the build filesystem had only about 3.4 MiB above its
32 MiB reserve. Dioxus also needed official accesskit 0.25.1 and additional
AnyRender/Vello packages. Further qualification requires reclaiming obsolete
build outputs or more build capacity and acquiring missing official dependencies.

Generated build outputs, logs, source snapshots and qualification archives are
kept outside Git. Main is unchanged; this is a reviewable development candidate.
