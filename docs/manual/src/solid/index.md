# Solid Modeler

The Solid Modeler is a parametric part designer in the style of SolidWorks. You build a part from
sketches and features, drive its sizes with parameters, check its mass, and send it to the pipeline to
be carved as a 2.5D relief or a 3D model. It runs in its own window and never changes the pipeline
until you send the part.

<!-- feature:solid.modeler -->
## The Solid Modeler window

A SolidWorks-style part modeller in its own window:

- **CommandManager** (top): the **Sketch**, **Features**, **Evaluate** and **Export** tabs, the part
  picker and **🗋 New part**.
- **FeatureManager** (left): the feature tree in build order — features further down act on the result
  of the ones above — and the **ƒ Parameters** tab.
- **3D view** (middle): orbit with the left mouse button, pan with the right or middle button, zoom
  with the wheel. The axes mark the selected feature's origin.
- **PropertyManager** (right): the settings of the selected feature.

**Steps**

1. Click **🧊 Solid Modeler** in the top bar.
2. Start from a template, an empty part or a sketch with **🗋 New part**.
3. Build the part from sketches and features; select a feature in the tree to change it.

![Solid Modeler with a parametric template](../img/solid_modeler.png)

*Available since version 0.2.0.*
<!-- /feature:solid.modeler -->

<!-- feature:solid.templates -->
## Starter templates

Ready-made parametric parts to learn from or adapt: an enclosure (shell, standoffs, vents, driven by
width/depth/height/wall parameters), a flange with a counterbored bolt circle, a knob, a threaded rod,
a lofted vase and an L-bracket built from a constrained sketch.

**Steps**

1. **🗋 New part** and pick a template.
2. Open **ƒ Parameters** and change a value — the part rebuilds.

*Available since version 0.2.0.*
<!-- /feature:solid.templates -->

<!-- feature:solid.solidcraft -->
## SolidCraft bridge

[SolidCraft](https://github.com/tangency-labs/solidcraft) is an open-source B-rep CAD program. If it is
installed, open the part's mesh in it for work that needs exact edges (STEP, assemblies, sheet metal),
and bring its STL/OBJ exports back as a pipeline feature.

**Steps**

1. **Export › 🔗 SolidCraft** (works when `solidcraft` is on your `PATH`).
2. Export STL or OBJ from SolidCraft, then use **② Design › Bring in › STL/OBJ** in the pipeline.

*Available since version 0.2.0.*
<!-- /feature:solid.solidcraft -->

## Undo and the pipeline

Every change you make in the Solid Modeler is one undo step in the project (`Ctrl+Z` in either
window). Parts are stored in the project file. The pipeline only sees a part when you send it — see
[Export & send to pipeline](evaluate.md#export--send-to-pipeline).
