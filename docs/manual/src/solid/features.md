# Features

Features build the part one step at a time. Each one **adds** material, **cuts** it away or keeps
only the **intersection** with what is above it in the tree. Select a feature in the FeatureManager to
edit it in the PropertyManager; use ▲ ▼ to reorder and the checkbox to suppress it.

<!-- feature:solid.features -->
## Extrude, revolve, sweep & loft

Extrude a sketch (blind, mid-plane or through all, with draft and rounded or chamfered cap edges),
revolve it (full or partial), sweep a profile along a path, or loft between sections. Each feature
adds, cuts or intersects with the body. Primitives (box, cylinder, sphere, cone, torus) are there for
quick shapes.

**Steps**

1. **Features ›** choose **⬆ Extrude**, **🔄 Revolve**, **➰ Sweep**, **🏺 Loft** or a primitive.
   Extrude and revolve open a sketch first.
2. Set the options in the PropertyManager:
   - extrude: **Depth**, **End** (*Blind*, *Mid-plane*, *Through all*), **Draft °**, **Reverse**,
     and **Cap edges** (sharp, fillet or chamfer for the top and bottom edges);
   - revolve: **Angle °** (360 for a full turn);
   - sweep: the **Path points** and **Twist °**;
   - loft: the **Z** of each section.
3. Choose **Add**, **Cut** or **Intersect**, and move or rotate it under **Placement**.

*Available since version 0.2.0.*
<!-- /feature:solid.features -->

<!-- feature:solid.dress -->
## Fillet, chamfer, shell & draft

Round or chamfer where a feature meets the body, fillet the whole part, hollow it with a shell
(optionally open at the top, for enclosures) and add draft to extrusions.

**Steps**

1. Select a feature and set **Blend with body** with **round** or **chamfer**.
2. Or **Features › ◜ Fillet all** / **▢ Shell**. Whole-part finishes are listed under **Whole part** in
   the tree; edit or delete them there.

*Available since version 0.2.0.*
<!-- /feature:solid.dress -->

<!-- feature:solid.holes -->
## Hole wizard & threads

Simple, drilled (118° point), counterbored, countersunk and tapped holes, and modelled external or
internal threads.

**Steps**

1. **Features › ⚲ Hole**. The hole is placed on the top of the part and goes down.
2. Choose the **Type** and set the diameter, depth and the counterbore / countersink / pitch values.
3. Move it with **Placement**, or pattern it (see below).
4. **🔩 Thread** adds a modelled thread; tick **Internal** and use **Cut** for a threaded hole.

> **Tip:** Modelled threads need a fine preview resolution (an eighth of the pitch or less). For CNC
> carving a cosmetic tapped hole (tap-drill size) is usually what you want.

*Available since version 0.2.0.*
<!-- /feature:solid.holes -->

<!-- feature:solid.patterns -->
## Patterns & mirror

Linear, grid and circular patterns of a feature, and mirror about the YZ, XZ or XY plane.

**Steps**

1. Select the feature → **Patterns & modifiers** → **⋯ Linear**, **▦ Grid**, **◌ Circular** or
   **⇔ Mirror**.
2. Set the count and spacing (or angle). A circular pattern of a hole placed at radius *r* gives a bolt
   circle of radius *r*.

*Available since version 0.2.0.*
<!-- /feature:solid.patterns -->

<!-- feature:solid.bodies -->
## Split, combine & scale

Split the part by a plane, combine it with another part (add, subtract, common) and scale it.

**Steps**

1. **Features › ✂ Split** keeps one side of a plane; set the plane and offset under **Whole part**.
2. **⊕ Combine…** lists your other parts: **Add**, **Subtract** or **Common** (intersection).
3. **⤢ Scale** on a feature scales it about its origin.

*Available since version 0.2.0.*
<!-- /feature:solid.bodies -->

## Editing values as JSON

Every feature can also be edited as text with **{ } Edit as JSON** — handy for loft profiles or for
pasting a feature from another part.
