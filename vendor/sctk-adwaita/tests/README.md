# Font regression evidence

The unit tests live next to the title renderer and header compositor. They use
bundled Cantarell and the official Google Fonts Roboto variable fixture. The
Roboto copyright and SIL OFL are in fonts/Roboto-OFL.txt.

`title-metrics-baseline.csv` records a 180-case legacy/replacement comparison:
five fonts, regular/Bold preferences, scales 1/2/3, and six titles. Old metrics
come from the unchanged official 0.10.1 ab_glyph backend. The 72 immutable-font
cases are enforced against the legacy dimensions in ordinary unit tests.
System DejaVu and Latin Modern CFF rows are supplemental local observations;
those system fonts are not silently required by the deterministic metric test.
No old font parser is retained as a production or test dependency in this fork.

See the root qualification document for source identities and claim limits.
