# ③ Toolpaths

When you open the Toolpaths step every feature already has a complete plan: the strategy, the bits,
a roughing pass that removes most of the material and a finishing pass that leaves a clean surface.
You can accept it as it is, change bits, or take full control of individual operations.

<!-- feature:pipeline.toolpaths -->
## Automatic toolpaths

Every feature gets the best roughing and finishing cycle for it:

| Feature | Roughing | Finishing |
|---|---|---|
| Cut out / inside cut | Profile with 0.3 mm allowance, largest end mill that fits | Full-depth profile with tabs, same or next smaller end mill |
| Pocket | Largest end mill that fits, leaves an allowance | Smaller end mill reaches the corners |
| V-carve | Flat-floor clearance with an end mill when the shape is wider than the V can reach | V-bit (60° for fine lettering, 90° for wide shapes) |
| Engrave / on the line | — | 90° V-bit / small end mill |
| Drill | Drill nearest to the hole size (bigger holes are bored) | — |
| 2.5D relief, 3D model | Flat end mill in Z levels | Ball nose raster |
| Photo V-carve | — | V-bit grooves |

Operations run in a safe order: drilling, then clearing, then finishing, and **cut-outs last** so the
part stays held until the end.

**Steps**

1. Open **③ Toolpaths**.
2. Check the plan per feature in the timeline on the left (bit and minutes per operation).
3. Click **⚡ Recalculate** after changes, or **🪄 Auto-plan** to reset every feature to the
   recommended bits.

![③ Toolpaths: the automatic roughing and finishing plan](../img/pipeline_toolpaths.png)

*Available since version 0.2.0.*
<!-- /feature:pipeline.toolpaths -->

<!-- feature:pipeline.bits -->
## Choosing bits

Every feature's plan can use different bits for roughing and finishing, skip a pass, change the
finishing allowance or turn tabs off. Your choices are kept when toolpaths are recalculated or the
feature is re-imported.

**Steps**

1. Select a feature in **③ Toolpaths**.
2. Pick the **Roughing** and **Finishing** bit. Type in the list to filter, for example `ball 1/8`,
   `v 60` or `3.175`. **🪄 Auto (recommended)** goes back to the planner's choice, which is shown in
   the field.
3. Untick a pass to skip it, set **Allowance for finishing**, or untick **Holding tabs**.
4. Click **⚡ Recalculate**.

> **Tip:** The feeds and speeds come from the bundled bit library and are rescaled for your machine's
> spindle and feed limits. For metals the planner picks single-flute cutters automatically.

*Available since version 0.2.0.*
<!-- /feature:pipeline.bits -->

<!-- feature:pipeline.manual_ops -->
## Hand-made operations (advanced)

Add your own profile, pocket, V-carve, drill, engrave, laser or 3D operation with full control over
feeds, depths and options. Hand-made operations are never overwritten by the planner.

**Steps**

1. Select the vectors (in ② Design) or the feature the operation belongs to.
2. **③ Toolpaths › Advanced › ➕ Add operation…** and pick the type.
3. Edit it in the right panel: bit, **Custom feeds**, start and cut depth, direction and the
   strategy's own options (tabs, ramps, raster angle, flat depth…).

Clicking a planned operation in the timeline opens the same editor. Changing it makes it
**hand-tuned** (✋): the planner stops generating that pass for the feature and keeps your version.
**↺ Use planned passes again** in the feature's plan undoes that.

*Available since version 0.2.0.*
<!-- /feature:pipeline.manual_ops -->

<!-- feature:pipeline.preview -->
## Toolpath preview

Show roughing, finishing or both toolpaths in 2D or 3D, coloured by depth, and simulate material
removal for exactly what is shown. Roughing paths are drawn in warm amber-to-red, finishing paths in
cool teal-to-violet; dashed red lines are rapid moves.

**Steps**

1. Use **Show: 🪓 Rough / ✨ Finish / ◫ Both** in the ribbon or the right panel.
2. Click **👁 Simulate** to cut the virtual stock with the passes shown — for example only the
   roughing, to see what the finishing pass still has to remove.

*Available since version 0.2.0.*
<!-- /feature:pipeline.preview -->

<!-- feature:pipeline.render -->
## Final rendered view

The simulated result is rendered in your material (walnut, maple, oak, MDF, aluminium…) with
machining advice such as scallop height, burn risk and overload warnings.

**Steps**

1. Click **🎨 Final render**. The view switches to 3D.
2. Orbit with the left mouse button; use **Top**, **Front**, **Right**, **Iso** to jump to a view.
3. **✖ Clear** goes back to the toolpath view.

![Simulated result rendered in the stock material](../img/pipeline_preview.png)

> **Tip:** The **sim cell** slider sets the simulation resolution. Smaller cells show finer detail
> but take longer; the default comes from your graphics card's performance tier.

*Available since version 0.2.0.*
<!-- /feature:pipeline.render -->
