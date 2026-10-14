# Maintained window-title renderer

`skrifa_renderer.rs` adapts the official upstream skrifa renderer to tiny-skia
0.11. Font parsing, cmap/metrics/variation/outline drawing, hinting, and legacy
kern-table decoding are delegated to maintained skrifa/read-fonts APIs.
The local code owns only title layout and tiny-skia composition.

Retained behavior includes gsettings/fc-match font selection, point-to-pixel
units, integer surface scale, regular bundled Cantarell fallback, variable
`wght=700` for Bold, control-character skipping, and simple one-line glyph
layout. There is no shaping or per-character fallback promise. Actual ink
bounds and 1px padding protect overhangs and combining marks from cropping.
Raster output can differ from the replaced ab_glyph implementation.

Cantarell's embedded copyright and the SIL OFL 1.1 text are retained in
Cantarell-OFL.txt. Tests use the separately licensed Google Fonts Roboto fixture.
