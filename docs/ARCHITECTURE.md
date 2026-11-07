# Architecture

```
                         ┌──────────────────────────────┐
                         │  ezyx-app (egui/eframe/wgpu) │   ezyx-cli (clap)
                         └──────────────┬───────────────┘        │
                                        │ Actions → Commands      │
                         ┌──────────────▼────────────────────────▼─┐
                         │ ezyx-project: Project, Command, History, │◄── ezyx-ai (agent loop,
                         │ machine profiles, op pipeline, AI host,  │     providers, ToolHost)
                         │ SVG/DXF import, text                     │
                         └─┬──────┬──────┬───────┬──────┬───────┬───┘
                           │      │      │       │      │       │
                  ezyx-cam ┘  ezyx-gcode │  ezyx-image  │  ezyx-sim
                     │            │      │       │      │       │
                ezyx-tooldb       │  ezyx-cad ── ezyx-mesh      ezyx-materials
                     │            │      │       │
                     └──────── ezyx-core (geometry, Heightmap, JobSetup) ────────┘
   ezyx-machine (GRBL, transports, streamer)     ezyx-gpu (wgpu probe, tiers)
```

## Application structure
```
 ┌──────────── main window: PIPELINE ─────────────┐     separate module windows (egui viewports)
 │ ① Stock → ② Design → ③ Toolpaths → ④ Output    │     ┌ Image Studio ┐ ┌ Solid Modeler ┐
 │ timeline of DesignFeatures ← ImportFeature ─────┼─────┤ send/update  │ │ send/update   │
 │   (source, revision, PlanSettings)  ReimportFeature (merge report)    └──────────────┘ └───────────────┘
 │ planner: plan_feature → auto ops (rough+finish) │     ┌ Machine Control ┐ own job queue,
 │ analyze → conflicts      export_gcode ──────────┼────►│ (files / pipeline) never edits the project
 └─────────────────────────────────────────────────┘     └─────────────────┘
```
* `ezyx_project::pipeline` — features, auto-planner (bit selection by inscribed radius / feature size,
  machining order: drill → clear → finish → cut-outs), conflict analysis, re-import merge.
* `ezyx_project::registry` — every user-facing feature (id, module, summary, how-to, screenshots). The GUI
  takes its tooltips from it; `tools/docs/update_manual.py` turns it into the user manual (`docs/manual`).
* `ezyx-app`: `app.rs` (shell, `Action` dispatch, module windows via `show_viewport_immediate`),
  `pipeline/` (step ribbons, timeline, panels, viewer), `image_studio.rs`, `solid/` (CommandManager,
  FeatureManager, PropertyManager, sketch canvas), `machine_control.rs`, `docshots.rs`.

## Documentation pipeline
`ezyx docs-registry` → `registry.json` → `update_manual.py scaffold` (chapters, marker blocks,
SUMMARY, feature reference, changelog) → `render_shots.sh` (GUI screenshots under xvfb via
`EZYX_DOC_SHOTS`, CLI `ezyx sim` fallback) → `update_manual.py revise` (Claude rewrites the chapters
affected by new commits / registry changes, validated before writing) → `check` → mdBook → PR + Pages
(`.github/workflows/docs.yml`).

## Data flow
1. **Design** — vectors (`Polyline`), reliefs (`Heightmap`), meshes (`TriMesh`), CAD parts
   (`ezyx_cad::Part`) live in `Project`; `Project::features` groups them into pipeline features.
2. **Operations** — `OperationSpec` = kind + tool id + depths + vector ids + options (+ `feature_id`,
   `auto`). The planner generates the `auto` ones from each feature's `PlanSettings`.
3. **Compute** — `ops::compute_one` resolves tool & feeds (rescaling community data to the
   machine), calls the CAM op, wraps for rotary, collects `ezyx_sim::advise` hints.
4. **Post** — `ezyx_gcode::emit` applies datum / Z-zero / units and the dialect.
5. **Run** — `ezyx_machine::Controller` streams with GRBL character counting; or save the file.
6. **Preview** — `ezyx_sim::StockSim` sweeps the same tool profiles through a Z-map and renders it
   with `ezyx_materials::Material::albedo`.

## Key algorithms
| Area | Method | Where |
|---|---|---|
| Offsets / booleans | `geo` (i_overlay) buffer & boolean ops, even-odd region building | `ezyx-core/region.rs` |
| V-carve | offset contours at depth `(d − tip_r)/tan α`; flat-depth floor clearing | `ezyx-cam/ops/vcarve.rs` |
| 3D toolpaths | drop-cutter with precomputed kernel from `Tool::profile_height`; Z-level roughing | `ezyx-cam/ops/raster3d.rs` |
| Rotary | linear Y→A wrap + per-move feed compensation; cylindrical ray-cast radius maps | `ops/rotary.rs`, `mesh/raster.rs` |
| Tracing | pixel-edge boundary following, RDP + Chaikin; Zhang–Suen skeleton + junction-aware walker | `ezyx-image/trace.rs` |
| Distance transform | exact separable EDT (Felzenszwalb–Huttenlocher) | `ezyx-image/bitmap.rs` |
| Photo V-carve | groove depth ∝ darkness, capped so adjacent V widths don't overlap | `photo_vcarve.rs` |
| Isosurfaces | marching tetrahedra (Kuhn 6-tet split) + weld + Taubin smoothing | `ezyx-mesh/mt.rs` |
| Tet meshing | structured Kuhn columns for reliefs; lattice stuffing with surface snapping for SDFs; radius-ratio & dihedral metrics | `ezyx-mesh/tet.rs` |
| CAD | SDF tree with smooth min/max (round/chamfer) blends, offset-based fillets, shells, patterns, sweeps (parallel-transported frames), lofts, holes, helical threads; sphere-traced heightmaps | `ezyx-cad` |
| Sketch solver | Levenberg–Marquardt on point coords + radii, DOF from Jacobian rank, parameter-driven dims | `ezyx-cad/sketch.rs` |
| Toolpath planning | inscribed radius (grid + pattern search) → largest fitting cutter; rough/finish split with allowance | `ezyx-project/pipeline.rs` |
| Simulation | Z-map stamping of the cutter profile along feed moves | `ezyx-sim` |
| Streaming | GRBL character-counting protocol, 5 Hz status polling, feed-hold+reset stop | `ezyx-machine/controller.rs` |

## Threading
UI thread: egui. Background: `tasks::Tasks` (toolpaths, simulation, part meshing), machine IO
thread (`ezyx-machine`), AI agent thread (works on a cloned `Project`; confirmations round-trip
through channels; result merges back with an undo checkpoint). Rayon is used inside heavy
kernels (drop-cutter rows, blur, distance fields, SDF sampling).

## GPU strategy
`ezyx-gpu` classifies adapters into tiers that set simulation resolution, viewport LOD and
whether GPU compute will be used. The renderer is wgpu (Vulkan/DX12/Metal/GL), so older AMD
TeraScale and NVIDIA Kepler cards run through OpenGL. `EZYX_GPU_BACKEND` overrides the choice.
