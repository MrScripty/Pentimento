# Primary source map

Sources were discovered or inspected on 3 October 2026. A located paper is not a claim that every theorem, experiment or implementation detail in it has already been checked. Each chapter must upgrade its sources to the depth needed before publishing a derivation. Source versions and access dates must be pinned where documentation changes over time.

## Geometry and representations

**S01 — Dey, Edelsbrunner, Guha and Nekhayev, Topology Preserving Edge Contraction, 1999.** [Institutional record and original publication link](https://research-explorer.ista.ac.at/record/3582). Use for formal conditions on contractions of simplicial complexes, including boundary assumptions. The record, abstract and definitions were read, and an [academic-hosted paper copy](https://graphics.stanford.edu/courses/cs448b-00-winter/papers/dey_edge_contract.pdf) was opened; theorem-level inspection remains necessary before a proof is included. Do not replace its condition with the count of shared neighboring vertices.

**S02 — Shewchuk, Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates, 1997.** [Author publication and implementation page](https://www.cs.cmu.edu/~quake/robust.html). Use for reliable determinant-sign decisions and the distinction between adaptive predicates and arbitrary geometric tolerances. The author page and implementation caveats were read. Any adopted implementation needs a compatible license and compiler/arithmetic assumptions checked separately.

**S03 — CGAL Triangulated Surface Mesh Simplification.** [Official reference](https://doc.cgal.org/latest/Surface_mesh_simplification/group__PkgSurfaceMeshSimplificationRef.html). Use as a mature public implementation/specification reference for eligible collapse operations and cost/placement policies. The official user manual was also opened; the library was not tested. Pin a CGAL version and inspect actual preconditions before building a comparison oracle or integrating it; license suitability is unresolved.

**S04 — Garland and Heckbert, Surface Simplification Using Quadric Error Metrics, 1997, and the 1998 attribute extension.** [Author's research page and papers](https://mgarland.org/research/quadrics.html). Use for the quadratic approximation objective and extensions to material attributes. The author page and the [1997 original paper](https://mgarland.org/files/papers/quadrics.pdf) were opened; full derivation review is pending. Its ability to simplify non-manifold inputs must not be misrepresented as a topology-preservation guarantee. Pentimento's current tangent-biased collapse placement is a separate algorithm.

**S05 — Museth, VDB High-Resolution Sparse Volumes with Dynamic Topology, 2013.** [Official OpenVDB publication index](https://www.openvdb.org/documentation/). Use for a sparse volume representation candidate and its supporting literature. Documentation index read; paper-level data-structure and performance claims remain to be checked. Published workloads are not Pentimento benchmarks.

**S06 — Ju, Losasso, Schaefer and Warren, Dual Contouring of Hermite Data, 2002.** [Original paper hosted by Stanford](https://graphics.stanford.edu/courses/cs164-10-spring/Handouts/paper_p339-ju.pdf), [author publication index](https://www.cs.rice.edu/~jwarren/research/index.html). Use for Hermite samples, local quadratic fitting and adaptive extraction. Abstract and source located. Full extraction and topology conditions require deeper reading before implementation; ordinary dual contouring is not a blanket manifold promise.

**S07 — OpenVDB library documentation and FAQ.** [Official documentation](https://www.openvdb.org/documentation/doxygen/overview.html), [FAQ](https://www.openvdb.org/documentation/doxygen/faq.html). Use for the difference between grids, transforms, trees, active voxels, tiles and background values, and for framing benchmark-dependent representation choices. Documentation excerpts inspected. These sources do not select Pentimento's authoritative shape representation by themselves.

**S08 — Jakob, Tarini, Panozzo and Sorkine-Hornung, Instant Field-Aligned Meshes, 2015.** [Original paper hosted by a coauthor institution](https://vcg-legacy.isti.cnr.it/Publications/2015/JTPS15/instant-meshes-SA-2015-jakob-et-al-compressed.pdf). Use for field-aligned mesh generation and the distinction between geometric approximation and desirable edge flow. Paper located; full optimization derivation and implementation study pending.

**S09 — Huang, Zhou, Nießner, Shewchuk and Guibas, QuadriFlow A Scalable and Robust Method for Quadrangulation, 2018.** [Author project page](https://web.stanford.edu/~jingweih/papers/quadriflow/), [paper](https://web.stanford.edu/~jingweih/papers/quadriflow/quadriflow.pdf). Use for quadrangulation constraints and treatment of singularities. Project abstract read. Its reported timing and quality numbers belong to the authors' experiment, not to a proposed Pentimento integration.

## Appearance and projection

**S10 — Burley and Lacewell, Ptex Per-Face Texture Mapping for Production Rendering, 2008.** [Original Disney paper](https://media.disneyanimation.com/technology/opensource/ptex/ptex.pdf), [Disney open-source index](https://disneyanimation.com/open-source/). Use for adjacency-aware per-face texture storage and filtering. Search located the primary paper; direct text extraction was blocked by a tool file-size limit. Before the chapter is finalized, retrieve/read the paper through a supported file route and inspect the official implementation. Do not equate Pentimento's per-face square storage with complete Ptex compatibility.

**S11 — Pharr, Jakob and Humphreys, Physically Based Rendering, fourth edition, Image Texture.** [Primary book chapter](https://www.pbr-book.org/4ed/Textures_and_Materials/Image_Texture). Use for image reconstruction, mip selection and anisotropic filtering, especially footprint-aware EWA. Chapter located and opened. Derivations in the book should be original explanations with short attribution, not copied prose or code listings.

**S12 — Physically Based Rendering, fourth edition, Texture Sampling and Antialiasing.** [Primary book chapter](https://pbr-book.org/4ed/Textures_and_Materials/Texture_Sampling_and_Antialiasing). Use for the sampling rationale behind projected texture footprints and alias control. Located through primary text excerpts. Read the full relevant sections before selecting numerical filters or reproducing equations beyond elementary derivation.

**S13 — Möller and Trumbore, Fast Minimum Storage Ray Triangle Intersection, 1997.** [Original paper hosted by Utah](https://my.eng.utah.edu/~cs6965/papers/MT97.pdf), [author publication index](https://fileadmin.cs.lth.se/cs/Personal/Tomas_Akenine-Moller/publications.html). Use for barycentric ray/triangle intersection. Paper located. A fast intersection test still needs explicit backface, degeneracy, tolerance and visibility policies in Pentimento.

**S14 — Igehy, Tracing Ray Differentials, 1999.** [Original Stanford paper](https://graphics.stanford.edu/papers/trd/trd_jpg.pdf). Use for propagation of differential footprints through ray geometry. Located; full derivation pending. Perspective projection painting may derive its local Jacobian directly where that is simpler than adopting an entire renderer's ray-differential machinery.

**S15 — Porter and Duff, Compositing Digital Images, 1984.** [Original paper](https://keithp.com/~keithp/porterduff/p253-porter.pdf). Use for alpha compositing algebra and premultiplied representations. Original paper located. A small independent numeric example will demonstrate that different stroke orders can produce different colors; it will not imply every paint operation is reducible to simple over compositing.

## Depth and reconstruction

**S16 — Curless and Levoy, A Volumetric Method for Building Complex Models from Range Images, 1996.** [Stanford paper page](https://graphics.stanford.edu/papers/volrange/paper_1_level/paper.html), [PDF](https://graphics.stanford.edu/papers/volrange/volrange.pdf). Use for weighted distance fusion and the treatment of range observations. Primary abstract and text excerpts inspected. Registration, outlier handling and unknown-space policy must be explicit in any proposed implementation.

**S17 — Newcombe and colleagues, KinectFusion Real-Time 3D Reconstruction and Interaction Using a Moving Depth Camera, 2011.** [Microsoft Research paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/kinectfusion-uist-comp.pdf). Use as a public integrated tracking/reconstruction pipeline reference. Paper located. Its sensor, hardware and scene assumptions must not be transplanted into Pentimento without a new measurement and uncertainty model.

**S24 — Kazhdan, Bolitho and Hoppe, Poisson Surface Reconstruction, 2006; Kazhdan and Hoppe, Screened Poisson Surface Reconstruction, 2013.** [Author project page](https://hhoppe.com/proj/poissonrecon/), [2013 paper](https://www.cs.jhu.edu/~misha/Fall13b/Papers/Kazhdan13.pdf). Use as an alternative reconstruction family for oriented point data. Located. Check normal quality, boundary behavior and interpolation/smoothing trade-offs before treating it as a substitute for TSDF fusion.

## Public engineering and product references

**S18 — Blender Mesh Painting and Sculpting developer documentation.** [Official technical page](https://developer.blender.org/docs/features/sculpt_paint/mesh_paint/). Use to locate publicly documented runtime data structures and shared sculpt/paint concepts, then inspect a pinned Blender source revision for exact algorithms. Search excerpts were available; direct page opening returned a fetch error. That limits this milestone to a source pointer, not an implementation audit of Blender.

**S19 — Autodesk Mudbox retopology documentation.** [Official retopology page](https://help.autodesk.com/cloudhelp/ENU/Mudbox/files/GUID-3F981486-5AAD-44BA-B1B3-84B19DA50C1E.htm). Use only for documented workflow capabilities, mesh-validation expectations and comparison requirements. It is not evidence of Autodesk's unpublished objective function or solver internals. No artist tutorial content is planned for the book.

**S20 — Foundry Mari paint buffer and feature documentation.** [Paint buffer](https://learn.foundry.com/mari/4.5/Content/user_guide/painting/paint_buffer.html), [current product features](https://www.foundry.com/products/mari/features). Use for the documented separation of projection painting and committed model appearance, and for UDIM-scale requirements. The paint-buffer reference is explicitly version 4.5 and historical. Proprietary cache, compositing and projection implementations remain unknown.

**S21 — MyPaint brush engine and libmypaint settings.** [Official backend documentation](https://www.mypaint.app/en/docs/backend/brush-engine/), [official settings source](https://github.com/mypaint/libmypaint/blob/master/brushsettings.json). Use for an existing adjustable brush engine, public parameter semantics and time-aware filtering. Backend text read. Pin an implementation revision and check licensing, state capture, FFI and deterministic replay before adopting it. Pentimento's painting brush currently describes its own implementation as a placeholder for possible libmypaint integration.

**S26 — 3DCoat voxel and surface documentation.** [Surface mode](https://3dcoat.com/documentation/manual/workspaces-rooms/sculpt/surface-mode/), [voxel controls](https://3dcoat.com/docs/3dcoat/Menu.php?lang=English&section=VoxelsMenu). Use for public capability distinctions and practical requirements around density and conversion. It does not establish the internals of voxel storage, meshing, dynamic topology or retopology.

## Distributed editing

**S22 — Shapiro, Preguiça, Baquero and Zawirski, Conflict-Free Replicated Data Types, 2011.** [Author-hosted paper](https://perso.lip6.fr/Marc.Shapiro/papers/2011/CRDTs_SSS-2011.pdf). Use for explicit convergence conditions and the difference between state- and operation-based models. Paper opened and introductory/model portions inspected. It does not automatically make noncommutative sculpting operations conflict-free or preserve artistic intent.

**S23 — Lamport, Time Clocks and the Ordering of Events in a Distributed System, 1978.** [Author institution publication page](https://www.microsoft.com/en-us/research/publication/time-clocks-ordering-events-distributed-system/), [paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/12/Time-Clocks-and-the-Ordering-of-Events-in-a-Distributed-System.pdf). Use for causality and logical ordering. Primary publication located. A tie-break order can make replay deterministic while still causing user-visible conflict; the book must distinguish these goals.

**S25 — Iroh official protocol documentation.** [Protocol overview](https://www.iroh.computer/proto), [iroh-gossip protocol module](https://docs.rs/iroh-gossip/latest/iroh_gossip/proto/index.html), [iroh-docs engine](https://docs.rs/iroh-docs/latest/iroh_docs/engine/struct.Engine.html). Use to evaluate the separate roles of gossip, verifiable blob transfer and multiwriter synchronization. Official pages inspected; APIs are not pinned to an implementation version yet. A transport choice does not supply Pentimento's operation identity, authorization, undo or topology conflict semantics.

## Source gaps to close before chapter acceptance

1. Read the full contraction and extraction papers for exact hypotheses and exceptional cases.
2. Add original references for cotangent Laplacians, stable deformation, parameterization, camera calibration and uncertainty propagation.
3. Inspect pinned Blender and libmypaint source for any claimed implementation detail; do not infer algorithms from familiar product behavior.
4. Read Ptex through a supported file route and distinguish file-format support from a similarly shaped internal storage scheme.
5. Select dataset sources with explicit redistribution rights; synthetic fixtures are the default until that is checked.
6. Record exact library revisions, compiler versions and licenses for any incorporated code. Research citation is not permission to copy a whole implementation.
