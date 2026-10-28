# Roadmap

## v0.2 — pipeline & modules (this release)
- [x] Pipeline main window: Stock → Design → Toolpaths → Output, feature timeline, conflict checks
- [x] Automatic rough + finish planning per feature with per-pass bit overrides
- [x] Image Studio, Solid Modeler and Machine Control as separate (tileable) windows
- [x] Re-import from Image Studio / Solid Modeler with merge report
- [x] Solid Modeler: constrained sketches, parameters/configurations, sweep/loft/holes/threads, mass props
- [x] VCarve-style UX: snapping, measure, quick-key sizes, align/distribute/mirror, layers, job sheet
- [x] Documentation pipeline: feature registry → manual scaffold, screenshots, LLM revision, CI PR

## v0.2.x — hardware & polish
- [ ] Verify on Genmitsu 4040 PRO MAX: USB streaming, Wi-Fi module protocol (see MACHINE_CONTROL.md)
- [ ] Node editing, snapping to geometry, dimensions/measure tool
- [ ] Dog-bone / T-bone fillets, lead-in/out arcs, tab editing by click
- [ ] Toolpath order/merge by tool, tool-change optimisation, job sheet (PDF/HTML)
- [ ] Per-user tool library editing UI + import CSV / Fusion 360 JSON tool libraries
- [ ] Settings window (units default, grid, theme, assistant, image→3D backend)

## v0.3 — performance
- [ ] wgpu paint-callback viewport (depth buffer, GPU-resident meshes, instanced toolpaths)
- [ ] GPU compute drop-cutter and Z-map simulation (WGSL), CPU fallback retained
- [ ] Medial-axis V-carve (Voronoi with `spade`) — centreline-only passes
- [ ] Adaptive (constant-engagement) clearing for metals
- [ ] Mesh decimation (quadric error) for large STLs

## v0.4 — advanced carving
- [ ] V-carve inlays (male/female with start depth + gap), fluting, prism carving, textures
- [ ] Nesting (true-shape), tiling for large jobs, two-sided flip workflow with dowel holes
- [ ] Sculpting brushes on reliefs (add/subtract/smooth/erase), relief blending modes UI
- [ ] Rotary: indexed 4-axis roughing, spiral finishing

## v0.5 — design depth
- [ ] Sketch: drag with live solve, trim/extend, offset entities, sketch text, reference other features' edges
- [ ] B-rep backend (truck, like SolidCraft, or OpenCascade FFI) for exact edges and STEP import/export;
      fillet/chamfer of picked edges; assemblies with joints and motion; sheet metal
- [ ] Toolpath templates (save/apply plan sets), nesting, tiling
- [ ] BCC isosurface stuffing with guaranteed dihedral bounds; boundary-conforming tet export with surface tags
- [ ] Neural image→3D: local ONNX (TripoSR/TRELLIS) via `ort` with GPU EPs; remote API presets

## Always
- Keep CLAUDE.md "Status" and "Known gaps" current.
- Every new algorithm: numeric unit test + CLI exposure for headless verification.
