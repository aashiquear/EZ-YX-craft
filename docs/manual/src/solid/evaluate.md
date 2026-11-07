# Evaluate & export

Check a part before you carve it, then send it to the pipeline or export it as a file.

<!-- feature:solid.evaluate -->
## Mass properties & interference

Volume, surface area, centre of mass, bounding box and mass for the material chosen in the pipeline's
stock, the section area at a height, and the interference volume between two parts.

**Steps**

1. **Evaluate** tab: mass properties are computed in the background and shown in the right panel.
2. Drag **Z** under **Section** to see the cross-section area at that height.
3. Under **Interference** pick another part and click **Check interference**.

*Available since version 0.2.0.*
<!-- /feature:solid.evaluate -->

<!-- feature:solid.export -->
## Export & send to pipeline

Send the part to the pipeline as a 2.5D relief (top-down height field) or as a 3D model, update the
feature made from it, export STL/OBJ, or a tetrahedral mesh (VTK/Gmsh) for analysis.

| Send as | What the router carves | Good for |
|---|---|---|
| **⛰ 2.5D relief** | The part seen from above; undercuts become vertical walls | Plaques, knobs, signs, anything carved from one side |
| **🧊 3D model** | The mesh with 3D roughing and finishing (wrapped on rotary jobs) | Organic shapes, rotary work |

**Steps**

1. **Export** tab → **⛰ As 2.5D** or **🧊 As 3D**. The part appears in the pipeline timeline, centred
   on the stock. The panel warns if the part is taller than the stock is thick.
2. After editing the part, click **⟳ Update** — the pipeline feature gets the new revision in the same
   place and you see a merge report.
3. **💾 STL** / **💾 OBJ** save the mesh; **🔷 Tet mesh** writes `.vtk` and `.msh` files for FEA tools.

*Available since version 0.2.0.*
<!-- /feature:solid.export -->
