# Exact-version source availability for executable distributions

Pentimento maintainers own this distribution requirement. The covered source
is the complete modified `vendor/sctk-adwaita/src/lib.rs` file containing
acknowledged snui-derived code. Distribute that source under MPL-2.0, including
the modifications actually used by the executable; preserve its file notice,
original derivation notice and upstream MIT contribution notices.

This repository's source distribution includes the complete vendored crate,
its modified source, Cargo manifest/lock, MIT/MPL/OFL license texts, and scoped
notices. The crate's nominal `0.10.1` version alone does not identify this fork.
An unmodified crates.io 0.10.1 archive, an upstream branch, or the repository's
moving main branch is not the exact modified source for a released executable.

Before distributing an executable containing this covered source, including
a downloadable CI binary, use this bounded procedure:

1. Identify the exact repository commit and reviewed root Cargo.lock consumed
   by that executable. Retain the manifest, lock and covered-source SHA256s in
   its release/build provenance. Dirty-source builds must identify and preserve
   their exact source as well; a commit label cannot conceal modifications.
2. Include the complete `vendor/sctk-adwaita` source directory from that same
   build input as a source archive beside the executable, or include it inside
   the download under `third_party/sctk-adwaita`. The source must include the
   exact modified `src/lib.rs`, not just a license or a link to upstream.
   Providing the full crate also retains the surrounding build inputs/notices.
3. Include a recipient-readable notice with the executable stating that this
   component contains MPL-2.0-covered source and identifying the actual bundled
   source directory or accompanying exact-version source archive. Give its real
   filename/location and build identity; do not ship unresolved placeholders.
4. If a hosted source archive is used instead of bundling, supply a verified
   exact-version URL accessible to those executable recipients. Keep it
   available by reasonable means in a timely manner at no more than the cost
   of distribution. A private repository URL recipients cannot open is not an
   adequate source offer. Bundling the archive avoids that access uncertainty.
5. Include this notice document, THIRD_PARTY_NOTICES.md, the unmodified
   LICENSE-MPL-2.0 and upstream MIT/font notices with the source/materials.
   Do not add terms restricting recipients' MPL Source Code Form rights.

A distributor may license the larger executable/work under separately chosen
terms consistent with MPL-2.0 section 3.3, while fulfilling the source and
notice requirements for this covered source file. This does not relicense
unrelated Pentimento application files or other separately located crate files.
The project's aggregate package metadata is not an MIT-only option for the
covered portions.

This is a concrete distribution obligation and release gate, not proof that a
future executable archive has already included its source. A release must
verify the produced artifact/recipient access before claiming compliance.
No executable release is authorized or performed by this notice change.
