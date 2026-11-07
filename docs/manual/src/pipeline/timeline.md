# Feature timeline

Everything you carve is a **feature**: a group of vectors with a cutting strategy, a 2.5D relief, a
3D model or a set of photo V-carve grooves. The timeline on the left of the main window lists them in
design order, under the stock card.

<!-- feature:pipeline.timeline -->
The left panel lists the stock and every feature in design order. Each feature shows what it is
(2D cut, 2.5D relief, 3D model, photo V-carve), where it came from (drawn, file, Image Studio, Solid
Modeler), its revision and any conflicts. Features can be renamed, reordered, hidden (suppressed) or
reopened in the editor that made them.

**Steps**

1. Click a feature to select it and see its properties in the right panel.
2. Use **▲ ▼** to reorder, **👁** to suppress (a suppressed feature gets no toolpaths), **🗑** to delete.
3. Click **✏** (or double-click the card) to return to the Image Studio or Solid Modeler.

In **③ Toolpaths** and **④ Output** each card also lists the feature's operations: **•** planned
automatically, **✋** hand-made or hand-tuned, with the bit and the estimated time.

![② Design: drawing and carve tools with the feature timeline](../img/pipeline_design.png)

*Available since version 0.2.0.*
<!-- /feature:pipeline.timeline -->

<!-- feature:pipeline.merge -->
## Editing a feature again (merge)

Features that came from the Image Studio or the Solid Modeler remember their source — the image and
its conversion settings, or the part and its revision. You can go back, change the design, and send it
again. The pipeline replaces the old revision **in the same place on the stock**, keeps the bits you
chose, regenerates the toolpaths and shows a **merge report**.

**Steps**

1. Select the feature and click **✏ Edit in …** (or double-click it in the timeline).
2. Change the design in the module window — move a slider in the Image Studio, change a dimension in
   the Solid Modeler.
3. Click **⟳ Update feature** in the module.
4. Read the merge report:
   - the footprint before and after;
   - how many toolpaths were regenerated;
   - hand-made operations that were re-targeted to the new geometry;
   - conflicts that appeared (⚠) or were resolved (✔) by this revision.

*Available since version 0.2.0.*
<!-- /feature:pipeline.merge -->

<!-- feature:pipeline.conflicts -->
## Conflict checks

The pipeline checks the whole job every time something changes:

| Check | Severity | What to do |
|---|---|---|
| A feature runs past the stock edge (or outside a round blank) | ✖ error | Move or shrink it, or use larger stock |
| A feature is deeper than the stock is thick | ✖ error | Reduce the depth or use thicker stock |
| A bit is too big to cut anything in a feature | ✖ error | Pick a smaller bit in ③ Toolpaths |
| A feature lost its geometry | ✖ error | Re-import it or delete it |
| Two features overlap | ⚠ warning | The deeper cut wins there — check that's intended |
| Pocket / V-carve / cut-out with open vectors | ⚠ warning | Close the shapes (open ones are ignored) |
| Cut-out without tabs | ℹ info | Clamp or tape the part |
| A hand-made operation was made for an older revision | ℹ info | Review the operation |

**Steps**

1. Look for **✖** / **⚠** badges on the timeline cards and next to the step bar.
2. Click a conflict in the **Conflicts** list to select the features involved.

*Available since version 0.2.0.*
<!-- /feature:pipeline.conflicts -->
