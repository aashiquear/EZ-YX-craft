# Feature matrix

Inspiration only — feature names below are generic descriptions; no code, assets or file formats
are copied. ✅ = implemented in v0.1 · 🟡 = partial / basic · 🔜 = planned.

| Capability | VCarve Pro / Aspire (reference) | Easel (reference) | EZ-YX-craft v0.1 |
|---|---|---|---|
| Job setup: single/double-sided, rotary, datum, Z-zero | ✔ | basic | ✅ single + rotary; 🟡 two-sided (model only) |
| Shapes, text, node editing | ✔ | basic | ✅ shapes/text/polyline; 🔜 node editing UI |
| Offsets, weld/subtract/intersect, fillets, dog-bones | ✔ | weld | ✅ offsets & booleans; 🔜 dog-bone/T-bone |
| Import SVG / DXF / EPS / PDF | ✔ | SVG | ✅ SVG, DXF; 🔜 PDF/EPS |
| Bitmap trace | ✔ | ✔ | ✅ outline + **centreline (sketch → engrave)** |
| Profile (tabs, ramps, lead-in) | ✔ | ✔ (tabs) | ✅ tabs, ramps, allowance, finish pass; 🔜 lead-in/out arcs |
| Pocket (offset/raster, islands) | ✔ | ✔ | ✅ |
| V-carve, flat-bottom, clearance tool | ✔ | ✔ (basic) | ✅ (offset method); 🔜 medial-axis speed-up |
| Inlay, fluting, prism, texture, moulding | ✔ | — | 🔜 |
| Drilling | ✔ | ✔ | ✅ peck |
| 3D roughing / finishing | ✔ | — | ✅ drop-cutter, any bit incl. tapered ball |
| 3D component modelling (reliefs, dome, sculpt) | Aspire | — | ✅ image relief, dome, mesh→relief; 🔜 sculpt brushes |
| Solid part design (feature tree) | — | — | ✅ SDF CAD (extrude/revolve/booleans/fillet/patterns) |
| Tetrahedral meshing for FEA | — | — | ✅ VTK/Gmsh export + quality metrics |
| Image → 3D | — | — | ✅ offline inflation; 🔜 neural backends (pluggable) |
| Photo V-carve | ✔ (gadget) | — | ✅ lines / waves / circles |
| Rotary wrapping / rotary 3D | ✔ | — | ✅ wrap + feed compensation; ✅ cylindrical mesh raster |
| Preview in material | ✔ | ✔ | ✅ procedural woods/metals + machining advice |
| Tool database with vendor feeds | ✔ | ✔ | ✅ Genmitsu full table + chip-load calculator |
| Post-processors | many | Easel-only | ✅ 9 built-in, JSON-defined |
| Machine control | — (external) | ✔ (USB) | ✅ USB + **Wi-Fi TCP/WebSocket** + simulator |
| Nesting, tiling, job sheets | ✔ | — | 🔜 |
| AI assistant with tool use | — | — | ✅ Claude / OpenAI / Ollama |
| Cross-platform desktop | Windows | Web | ✅ Linux / macOS / Windows |
