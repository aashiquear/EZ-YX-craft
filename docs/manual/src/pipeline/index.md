# The carving pipeline

The main window of EZ-YX-craft is the **pipeline**. Every job moves through the same four steps, from
the raw board on your bench to the G-code your machine runs. This chapter explains how the steps fit
together; the following chapters cover each step in detail.

<!-- feature:pipeline.overview -->
## The four steps

The step bar at the top of the main window walks a job from raw stock to G-code:

| Step | What you decide | Chapter |
|---|---|---|
| **① Stock** | Material, shape and size of the blank, where zero is, which machine | [Stock setup](stock.md) |
| **② Design** | The features to carve: drawn shapes, imports, images, solid parts | [Design](design.md), [Timeline](timeline.md) |
| **③ Toolpaths** | Roughing and finishing for every feature, bits, preview | [Toolpaths](toolpaths.md) |
| **④ Output** | G-code for your machine, job sheet, hand-off to Machine Control | [Output](output.md) |

You can jump between steps at any time. Later steps update automatically when you change an earlier
one: resize the stock and the conflict checks run again; change a feature's depth and its toolpaths
are re-planned.

**Steps**

1. Click a step in the step bar (or press `Ctrl+1` … `Ctrl+4`).
2. Use **➡ Next** at the right end of the ribbon to move on when a step is done.
3. Watch the health badge next to the steps: **✔ no conflicts**, or the number of errors (✖) and
   warnings (⚠) the pipeline found.

![② Design: drawing and carve tools with the feature timeline](../img/pipeline_design.png)

*Available since version 0.2.0.*
<!-- /feature:pipeline.overview -->

## How the window is laid out

- **Top bar** — the **File** menu, the **🛠 Pipeline** tab and the module buttons (**🖼 Image Studio**,
  **🧊 Solid Modeler**, **🎮 Machine Control**), **⧉ Tile**, undo/redo, the theme switch and the
  **🤖 Assistant**.
- **Step bar** — the four steps, the conflict badge and a one-line summary of the stock.
- **Ribbon** — the tools for the current step, grouped by task.
- **Timeline (left)** — the stock card and the list of features. See [Feature timeline](timeline.md).
- **Viewer (centre)** — the stock, your shapes, models and toolpaths in 2D or 3D. Press `Tab` to switch,
  `F` to fit the view. Right or middle mouse button pans, the wheel zooms, the left button orbits in 3D.
- **Panel (right)** — the settings for whatever you are working on in the current step.

## Modules work beside the pipeline

Pictures, solid parts and the machine each have their own module window. They never change your
design behind your back: when you are happy with an image conversion or a part, you **send** it to
the pipeline, where it becomes a feature. If you later reopen the module from that feature and send
again, the pipeline **merges** the new revision in place and tells you what changed. See
[Editing a feature again](timeline.md#editing-a-feature-again-merge).

> **Tip:** With a wide screen, open a module and press **⧉ Tile** to put the pipeline and the module
> windows side by side.
