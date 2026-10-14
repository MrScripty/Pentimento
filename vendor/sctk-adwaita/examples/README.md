# Real Wayland client-decoration smoke

`wayland_title_smoke` is adapted from the official 0.10.1 window example. It
connects to the supplied Wayland compositor and uses `WindowDecorations::ClientOnly`
to ensure it exercises this fork's Adwaita frame. The app ID is
`io.github.pentimento.wayland-title-smoke`.

The default title is `Pentimento AV To é Ω 日本 😀`. Set
`PENTIMENTO_WAYLAND_CONTROL` to an explicit test-only text file and replace its
contents to update both the xdg window title and rendered title. Window resize
and output scale arrive from the compositor. The client logs configure,
client-decoration submission, and compositor frame acknowledgment. A compositor
close request follows the ordinary clean-shutdown path.

Screenshots and compositor metadata, rather than stdout alone, must prove that
the actual client decoration and title are visible. The harness must observe
client exit and compositor cleanup. Software-rendered headless Sway is a
qualified smoke environment, not a claim about hardware GPU rendering.
