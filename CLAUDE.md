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
| ezyx-cad | SDF feature-tree part modeller | `Part::{sdf,mesh,heightmap,tet_mesh}` |
| ezyx-sim | Z-map material removal, preview mesh, machining advice | `StockSim`, `advise` |
| ezyx-gpu | wgpu adapter probe + GPU family/tier classification | `probe`, `classify`, `defaults_for` |
| ezyx-ai | Anthropic/OpenAI/OpenAI-compatible providers, tool-use agent loop | `Agent::run_turn`, `ToolHost` |
| ezyx-project | Project doc, Commands/History, machine profiles, op pipeline, AI tool host, SVG/DXF import, text | `ops::export_gcode`, `agent_host::ProjectHost` |
| ezyx-app | egui/eframe GUI (binary `ez-yx-craft`) | `app.rs`, `ribbon.rs`, `viewport.rs`, `panels/*` |
| ezyx-cli | headless CLI (binary `ezyx`) | `main.rs` subcommands |

## Commands
```bash
cargo test --workspace                 # all unit tests (no GPU/network needed)
cargo run -p ezyx-app --release        # GUI
cargo run -p ezyx-cli --release -- --help
cargo run -p ezyx-cli --release -- demo -o demo.ezyx && \
  cargo run -p ezyx-cli --release -- sim demo.ezyx -o preview.png   # quick end-to-end check
cargo clippy --workspace --all-targets
EZYX_GPU_BACKEND=gl cargo run -p ezyx-app   # force OpenGL (old Radeon TeraScale etc.)
```
Linux build deps for the GUI: `libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev` (see CI).
Serial uses `serialport` with default features off (no libudev needed).

## Conventions
- Rust 2021, `rustfmt.toml` max_width 160. `anyhow` only in binaries; libraries use `thiserror`.
- egui is **0.37**: `App::ui(&mut self, ui, frame)`, panels are `egui::Panel::{top,left,right,bottom}(id).show(ui, ..)`,
  `egui::CentralPanel::no_frame().show(ui, ..)`, windows `Window::show(ctx, ..)`. Check the egui
  source in `~/.cargo/registry` before assuming older APIs.
- GUI state changes happen in `EzApp::handle(Action)`; ribbon/panels emit `Action`s.
- Long work (toolpaths, simulation, part meshing) runs in `tasks::Tasks::spawn` threads.
- Posts are data: add a `PostConfig` (or JSON in the user's config dir) instead of code paths.
- Genmitsu table numbers are for a 3018 (9k RPM, 1200 mm/min); `ops::resolve_cutting` rescales.

## Status (v0.1.0 scaffold — keep updated)
Working & tested: geometry/offsets, all CAM ops listed above, 9 posts, GRBL streaming (sim-tested),
image pipelines, STL/OBJ/SVG/DXF IO, marching tets, tet meshing + VTK/Gmsh export, SDF CAD,
Z-map simulation + material rendering, GPU classification, AI agent loop (offline-tested), CLI.
GUI: complete layout & flows, CPU-tessellated viewport. Not yet verified on real hardware.

## Known gaps / next steps (prioritised) — see docs/ROADMAP.md
1. Verify the **Genmitsu Wi-Fi module protocol** on the owner's 4040 PRO MAX (capture traffic
   from the vendor app; add a transport if it is not raw TCP/WebSocket GRBL).
2. GPU viewport: egui_wgpu paint callback with depth buffer (replace painter's algorithm).
3. Medial-axis V-carve (Voronoi via `spade`) to cut only centreline passes (faster jobs).
4. GPU compute drop-cutter + simulation (same kernels as `raster3d::DropKernel`, `ezyx-sim`).
5. Sketch constraints (SolveSpace-like) and B-rep backend (truck/OCCT) for STEP export.
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
- GRBL "ok" accounting: manual lines and job lines share one queue; don't send manual lines
  while a job streams (UI disables run controls appropriately).
