# Parameters & configurations

Parameters give names to the sizes that matter — `width`, `wall`, `hole` — so one change resizes the
whole part. Configurations save sets of parameter values, such as a small and a large version of the
same enclosure.

<!-- feature:solid.parameters -->
## Parameters, expressions and configurations

Named parameters with unit-aware expressions (for example `wall = 2mm`, `width = 3in`,
`hole = width/8`) drive dimensions and feature values. Configurations override parameters to make size
variants of one part.

**Steps**

1. Open the **ƒ Parameters** tab of the FeatureManager and click **➕ Parameter**. Give it a name and an
   expression; the evaluated value is shown next to it.
2. Bind it to a feature value: select the feature, **ƒ Bind a value…**, pick the field, then type the
   expression (for example `width - 2*wall`). Sketch dimensions take expressions directly.
3. Click **➕ Configuration from current values**, then switch between configurations with the radio
   buttons and edit the overrides of the active one.

*Available since version 0.2.0.*
<!-- /feature:solid.parameters -->

## Expression reference

| Kind | Examples |
|---|---|
| Numbers and operators | `12.5`, `1e-3`, `+ - * /`, `^` (power), parentheses |
| Units | `mm`, `cm`, `m`, `in`/`inch`, `ft`, `deg`, `rad` — results are in mm or degrees |
| Functions | `sin cos tan asin acos atan atan2 sqrt abs min max round floor ceil` (angles in degrees) |
| Constants | `pi` |
| Other parameters | any name, in any order — cycles are reported as errors |

> **Tip:** A unit applies to the number or bracket right before it: write `(1/4)in` for a quarter
> inch. `1/4in` means 1 divided by 4 inches.
