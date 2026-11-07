# ② Design

The Design step is where you draw and arrange shapes and decide how each group of shapes is carved.
Shapes only become something the machine cuts when you turn them into a **feature** with one of the
**Carve** buttons.

<!-- feature:pipeline.draw -->
## Drawing tools

Basic 2D drawing: rectangle, circle, ellipse, polygon, star, polyline and TrueType text. While you
draw you can type exact values (**quick keys**) instead of dragging.

**Steps**

1. Pick a tool in the **Draw** group (`R` rectangle, `C` circle, `P` polyline, `T` text, `M` measure).
2. Drag on the stock — or click once to anchor the shape and type its size, for example `120,60`, then
   press `Enter`. For a circle type the radius, for an ellipse the two radii.
3. For a polyline click the points; double-click or press `Enter` to finish. Finishing near the first
   point closes the shape.
4. Press `Esc` to go back to **Select**.

![② Design: drawing and carve tools with the feature timeline](../img/pipeline_design.png)

*Available since version 0.2.0.*
<!-- /feature:pipeline.draw -->

<!-- feature:pipeline.edit -->
## Editing, arranging & layers

Offset, weld (union), subtract and intersect closed shapes; move, scale, rotate and mirror with exact
numbers; align and distribute several shapes; centre on the stock; organise vectors on layers you can
hide or lock.

**Steps**

1. Select shapes: click, `Shift`+click to add, or drag a box. `Ctrl+A` selects everything on unlocked
   layers.
2. Use the **Edit** group (**Offset**, **Weld**, **Subtract**, **Intersect**, **Delete**) and the
   **Arrange** group (align left/centre/right/top/middle/bottom, distribute, mirror, centre).
   One selected shape aligns to the stock; several align to each other.
3. Type exact **X**, **Y**, **W** and **H** in the selection panel. **Keep proportions** scales both
   directions together.
4. In **Layers** add a layer, then move the selection to it with **Layer**. Untick a layer to hide it,
   or click 🔓 to lock it so it can't be selected.

*Available since version 0.2.0.*
<!-- /feature:pipeline.edit -->

<!-- feature:pipeline.snap -->
## Snapping & measuring

Snap the cursor to the grid, to shape corners, midpoints and centres, and to the stock edges and
centre. A green ring shows when an object snap is active. The measure tool shows distance and angle
between two points.

**Steps**

1. Toggle grid (**#**) and object (**⌖**) snap in the viewer toolbar; set the grid size in
   **Snapping**. Hold `Alt` to draw without snapping.
2. Pick **📏 Measure** and click two points. The distance, angle and ΔX/ΔY stay on screen until you
   click again.

*Available since version 0.2.0.*
<!-- /feature:pipeline.snap -->

<!-- feature:pipeline.carve -->
## Carve tools

Turn selected vectors into a feature with one click. The feature gets a sensible depth for your stock
and its toolpaths are planned straight away.

| Button | Cuts | Default depth |
|---|---|---|
| **Cut out** | Around the outside of closed shapes, through the stock, with tabs | Stock thickness |
| **Inside cut** | Along the inside of closed shapes | ¼ of the thickness (1–6 mm) |
| **Pocket** | Clears the area inside closed shapes | ¼ of the thickness (1–6 mm) |
| **V-carve** | V-bit carving whose depth follows the shape width (signs, lettering) | 30 % of the thickness (1–6 mm) |
| **Engrave** | A V-groove along the vectors | 1 mm |
| **Drill** | Holes at circle centres (bigger holes are bored with an end mill) | Stock thickness |
| **On the line** | An end mill centred on the vectors | 1 mm |

**Steps**

1. Select the vectors.
2. Click a button in the **Carve the selection** group (or in the selection panel — ★ marks the
   strategy suggested for those shapes).
3. Adjust **Strategy** and **Depth** in the feature panel on the right.

*Available since version 0.2.0.*
<!-- /feature:pipeline.carve -->

<!-- feature:pipeline.import -->
## Importing designs

Bring in SVG/DXF vectors as 2D features and STL/OBJ meshes as 3D model features. Images open in the
Image Studio, solid parts are made in the Solid Modeler; both send their results here.

**Steps**

1. **Design › Bring in › SVG/DXF** or **STL/OBJ** (also in the **File** menu). Imported vectors become
   a feature with a suggested strategy; models are scaled to fit the stock.
2. For pictures click **🖼 Image**; for parts click **🧊 Solid**.

> **Tip:** An imported feature keeps the file name as its source. To replace it with a newer version,
> delete the feature and import again — or make it in the Image Studio / Solid Modeler, which can
> update it in place.

*Available since version 0.2.0.*
<!-- /feature:pipeline.import -->
