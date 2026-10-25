# CLAUDE.md — working memory for Claude Code on EZ-YX-craft

Read this first. It is the project's source of truth for *how we work*; `docs/` holds the deeper
design notes. Keep this file current when you change architecture, conventions or status.

## What this is
EZ-YX-craft is an open-source (MIT OR Apache-2.0) Rust desktop app for CNC wood & metal carving:
2D/2.5D/3D design, CAD part modelling, meshing (incl. tet meshes), image→carve pipelines,
toolpaths, G-code posts, material-removal preview, machine control over USB/Wi-Fi, and an
LLM assistant that operates the app through tools. Targets Linux (Ubuntu), macOS, Windows.
Inspiration (features, not code or assets): Vectric VCarve/Aspire (power) + Easel (friendliness).
Owner: Aashique (machine: Genmitsu 4040 PRO MAX with Wi-Fi module; dev box: Ubuntu).

## Golden rules
1. **All internal units are millimetres.** Convert only at UI (`ezyx_core::Units`) and post (`ezyx-gcode`).
2. **Z = 0 is the stock top, negative is into material**, X right, Y away from operator.
3. **Every project mutation goes through `ezyx_project::Command`** (GUI, CLI and AI share it →
   undo/redo + consistency). Add a `Command` variant rather than mutating `Project` ad hoc. Bulk
   edits outside commands must call `History::checkpoint` first.
4. **Anything that moves the machine or writes files on the AI's behalf must be
   `requires_confirmation: true`** in its `ToolSpec`. Never relax this.
5. Core crates stay GUI-free and testable headless. Only `ezyx-app` depends on egui/eframe.
6. No proprietary formats/assets copied. Vectric `.crv/.vtdb` import would need a clean-room spec.
7. Tests next to code (`#[cfg(test)]`). New algorithms ship with at least one numeric test
   (areas/volumes/depths), not just "doesn't panic".

## Workspace map (crates/)
| crate | role | key entry points |
|---|---|---|
| ezyx-core | geometry (Polyline, Region offset/booleans via `geo`), Heightmap, JobSetup | `region::Region::offset`, `Heightmap::sample` |
| ezyx-tooldb | bits + feeds/speeds; bundles Genmitsu table JSON (`data/`) | `ToolLibrary::bundled`, `Tool::profile_height`, `feeds::calculate` |
| ezyx-materials | woods/metals/plastics + procedural appearance | `library()`, `Material::albedo` |
| ezyx-cam | toolpaths: profile/pocket/vcarve/drill/engrave/raster3d/rotary/polyline3d | `ops::*::generate`, `link::PathBuilder` |
| ezyx-gcode | JSON-configurable posts + G-code parser | `builtin_posts`, `emit`, `parse` |
| ezyx-machine | GRBL protocol, transports (serial, TCP, WebSocket, simulator), streamer | `Controller::connect`, `grbl::cmd` |
| ezyx-image | trace outline/centreline, relief, photo V-carve, inflation | `trace::*`, `photo_vcarve::grooves` |
| ezyx-mesh | TriMesh, STL/OBJ, marching tets, tet meshing + quality, img→3D backends | `mt::extract`, `tet::relief_solid`, `tet::from_sdf` |
| ezyx-cad | SDF feature-tree part modeller: constrained sketches (LM solver), params/expressions/configurations, extrude/revolve/sweep/loft/holes/threads, patterns, split/combine, mass props, templates | `Part::{sdf,mesh,heightmap,resolved}`, `sketch::Sketch::solve_with`, `expr::eval`, `templates()` |
| ezyx-sim | Z-map material removal, preview mesh, machining advice | `StockSim`, `advise` |
| ezyx-gpu | wgpu adapter probe, GPU family/tier classification, adapter choice + render budgets | `probe`, `classify`, `choose_adapter`, `render_budget` |
| ezyx-ai | Anthropic/OpenAI/OpenAI-compatible providers, tool-use agent loop | `Agent::run_turn`, `ToolHost` |
| ezyx-project | Project doc, Commands/History, **pipeline** (features timeline, auto-planner, conflicts, re-import merge), job sheet, **feature registry** (GUI tooltips + manual), demo project, machine profiles, op pipeline, AI tool host, SVG/DXF import, text | `pipeline::{plan_feature,replan,reimport,analyze}`, `registry::registry`, `ops::export_gcode` |
| ezyx-app | egui/eframe GUI (binary `ez-yx-craft`): pipeline main window + module windows | `app.rs` (shell, actions, windows), `pipeline/*`, `image_studio.rs`, `solid/*`, `machine_control.rs`, `gpu_guard.rs`, `docshots.rs` |
| ezyx-cli | headless CLI (binary `ezyx`) | `main.rs` subcommands |

## Commands
```bash
cargo test --workspace                 # all unit tests (no GPU/network needed)
cargo run -p ezyx-app --release        # GUI
cargo run -p ezyx-cli --release -- --help
cargo run -p ezyx-cli --release -- demo -o demo.ezyx && \
  cargo run -p ezyx-cli --release -- sim demo.ezyx -o preview.png   # quick end-to-end check
cargo clippy --workspace --all-targets
cargo run -p ezyx-cli -- pipeline examples/oak_sign.ezyx   # features, planned toolpaths, conflicts
cargo run -p ezyx-cli -- docs-registry && python3 tools/docs/update_manual.py scaffold && python3 tools/docs/update_manual.py check
tools/docs/render_shots.sh                 # manual screenshots (xvfb; needs libxkbcommon-x11-0, mesa)
python3 -m unittest discover tools/docs    # docs tooling tests
EZYX_GPU_BACKEND=gl cargo run -p ezyx-app   # force OpenGL (old Radeon TeraScale etc.)
```
Linux build deps for the GUI: `libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev` (see CI).
Serial uses `serialport` with default features off (no libudev needed).

## App structure (v0.2)
- **Main window = pipeline**: ① Stock → ② Design → ③ Toolpaths → ④ Output (`ribbon::Step`). Left: feature
  timeline; right: per-step panel; centre: `pipeline/view.rs`.
- **Modules** (`ribbon::Module`): Image Studio, Solid Modeler, Machine Control. Separate native windows via
  `ctx.show_viewport_immediate` (or tabs when `Prefs::separate_windows = false`, always in docs mode).
  Modules only touch the pipeline through `Command::ImportFeature` / `Command::ReimportFeature`
  (merge report) — Machine Control never touches the project (own job queue).
- **Features** (`pipeline::DesignFeature`) own geometry refs + `FeatureSource` (which editor to reopen) +
  `PlanSettings` (user bit choices). Planned ops have `auto = true` and are regenerated by `replan`;
  hand-made/tuned ops (`auto = false`) are never touched. Editing an auto op flips it to hand-tuned and
  sets `plan.hand_rough/hand_finish`.
- **Docs pipeline**: every user-facing feature has an entry in `ezyx_project::registry` (ids used for
  GUI tooltips via `ribbon::tip`). `ezyx docs-registry` → `docs/manual/registry.json`;
  `EZYX_DOC_SHOTS=… ez-yx-craft` renders the screenshots; `tools/docs/update_manual.py
  {scaffold,revise,check,all}` keeps `docs/manual` (mdBook) in sync — CI `docs.yml` opens a PR.
  **When you add or change a feature: add/update its registry entry, run scaffold, write the manual
  section.** `check` fails CI otherwise.

## Conventions
- Rust 2021, `rustfmt.toml` max_width 160. `anyhow` only in binaries; libraries use `thiserror`.
- egui is **0.37**: `App::ui(&mut self, ui, frame)`, panels are `egui::Panel::{top,left,right,bottom}(id).show(ui, ..)`,
  `egui::CentralPanel::no_frame().show(ui, ..)`, windows `Window::show(ctx, ..)`. Check the egui
  source in `~/.cargo/registry` before assuming older APIs.
- GUI state changes happen in `EzApp::handle(Action)`; ribbon/panels emit `Action`s.
- Long work (toolpaths, simulation, part meshing) runs in `tasks::Tasks::spawn` threads.
- Posts are data: add a `PostConfig` (or JSON in the user's config dir) instead of code paths.
- Genmitsu table numbers are for a 3018 (9k RPM, 1200 mm/min); `ops::resolve_cutting` rescales.

## Status (v0.2.0 — keep updated)
Working & tested: geometry/offsets, all CAM ops listed above, 9 posts, GRBL streaming (sim-tested),
image pipelines, STL/OBJ/SVG/DXF IO, marching tets, tet meshing + VTK/Gmsh export, SDF CAD with
constrained sketches/parameters/holes/threads/sweep/loft, pipeline planner + conflicts + merge,
Z-map simulation + material rendering, GPU classification + adapter choice/CPU-rendering fallback, AI agent loop (offline-tested), CLI,
docs registry/scaffold/check + headless screenshots. GUI: pipeline stepper + 3 module windows
(screenshot-verified under xvfb). Not yet verified on real hardware.

## Known gaps / next steps (prioritised) — see docs/ROADMAP.md
1. Verify the **Genmitsu Wi-Fi module protocol** on the owner's 4040 PRO MAX (capture traffic
   from the vendor app; add a transport if it is not raw TCP/WebSocket GRBL).
2. GPU viewport: egui_wgpu paint callback with depth buffer (replace painter's algorithm).
3. Medial-axis V-carve (Voronoi via `spade`) to cut only centreline passes (faster jobs).
4. GPU compute drop-cutter + simulation (same kernels as `raster3d::DropKernel`, `ezyx-sim`).
5. B-rep backend (truck, as SolidCraft uses) for STEP export, assemblies with joints, sheet metal;
   sketch dragging with live solve; feature references to faces/edges (fillet a chosen edge).
6. Vectric-style extras: inlays, fluting, prism/texture toolpaths, nesting, job sheets, tiling.
7. Neural image→3D backend (local ONNX TripoSR/TRELLIS) behind `ezyx_mesh::img3d::Backend`.
8. Installers via cargo-packager in CI release workflow (config already in ezyx-app/Cargo.toml).

## Gotchas learned
- `tet::from_sdf` (stuffing-lite) can leave slivers near the surface (seen: min dihedral ≈2°, mean radius ratio 0.70 on the demo part); `relief_solid` is high quality (>35°). Proper BCC stuffing is roadmap item.
- Screenshots/smoke test of the GUI headless: `xvfb-run -s "-screen 0 1480x920x24"` with `WGPU_BACKEND=gl EZYX_START_PREVIEW=1 ez-yx-craft examples/oak_sign.ezyx`.
- glam 0.34 has no `Mat4::look_at_rh/perspective_rh/orthographic_rh` here — we use our own in `viewport.rs`.
- Marching-tet output must be welded (`TriMesh::weld`) before smoothing or volume collapses.
- `Region::from_polylines` uses even-odd (XOR) → letter counters become holes automatically.
- Tool profile `Tool::profile_height` drives drop-cutter, V-carve depth and simulation alike —
  fix tool geometry bugs there once.
- `Part::sdf()` resolves parameters/bindings/sketches every call (clone + solve) — cache meshes by
  `Part::revision_hash()` (the Solid Modeler does).
- Pipeline footprints: Pocket/V-carve use the filled region, every other 2D strategy a ±1.6 mm band
  along the vectors (otherwise an engraved border "overlaps" everything inside it).
- Docs screenshots must be deterministic (no timings/clock in visible text when `app.docs` is set).
- **egui puts a whole window's triangles in ONE vertex buffer per frame** (20 B/vertex, 60 B/triangle,
  default wgpu limit 256 MiB). A fine simulation mesh overflowed it and wgpu panicked. Everything the
  viewport draws goes through `viewport::Draw` / `gpu_guard::take_{triangles,segments}` (per-window,
  per-pass vertex budget = `RenderBudget::frame_vertices`); big meshes are pre-decimated
  (`TriMesh::decimated`), sims coarsened (`ezyx_sim::cell_for_budget`, `StockSim::lod_for_budget`).
  New drawing code must reserve from the budget too. Even with `on_uncaptured_error` installed,
  egui-wgpu panics on its own after an oversized buffer ("Failed to create staging buffer") — the
  budget must prevent it; `gpu_guard::install_panic_fallback` relaunches with `EZYX_RENDER=cpu` on the
  autosaved `config_dir/ezyx-craft/recovery.ezyx` as a last resort.
- GPU test hooks: `EZYX_RENDER=auto|gpu|cpu`, `EZYX_MAX_BUFFER_MB=8` (fake small GPU),
  `EZYX_FRAME_BUDGET=<tris>` (forces overflow → panic fallback), `EZYX_SIM_CELL=0.05` (huge sim).
  Software adapters (llvmpipe/lavapipe/WARP) are `DeviceType::Cpu`; xvfb runs exercise that path.
- GRBL "ok" accounting: manual lines and job lines share one queue; don't send manual lines
  while a job streams (UI disables run controls appropriately).
