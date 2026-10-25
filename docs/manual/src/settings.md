# Windows, layout & settings

<!-- feature:settings.windows -->
## Module windows side by side

Image Studio, Solid Modeler and Machine Control open as their own windows so they can sit next to the
pipeline. **⧉ Tile** places the open windows side by side; a preference shows them inside the main
window instead, as tabs next to **🛠 Pipeline**.

**Steps**

1. Open a module from the top bar (buttons marked ↗ open a window).
2. Click **⧉ Tile** to arrange the main window and the open modules across the screen.
3. **File › ⚙ Settings › Windows** to choose separate windows or tabs inside the main window.

*Available since version 0.2.0.*
<!-- /feature:settings.windows -->

<!-- feature:settings.graphics -->
## Graphics: GPU checks and CPU fallback

At start-up EZ-YX-craft checks the graphics adapters. GPUs that can't run the viewport (no driver,
tiny buffer or texture limits, can't draw to the window) are skipped, and if none is usable the app
renders on the CPU instead and tells you why. While you work, previews are sized to the GPU's buffer
limit; if the GPU still runs out of memory the app simplifies the view and offers to restart with CPU
rendering instead of crashing.

**Steps**

1. If a **⚠ Graphics** message appears, read why and choose **Restart with CPU rendering** (tick
   **always** to keep it) or **OK**.
2. **File › ⚙ Settings › Graphics** shows the adapter in use, its largest buffer and the preview
   budgets.
3. Choose **Render with**: **Auto**, **GPU** or **CPU**, then click **Restart now**. You can also start
   once in a mode with `EZYX_RENDER=cpu ez-yx-craft`.

What the app does for you:

| Situation | What happens |
|---|---|
| No GPU driver, or the system fell back to a software renderer | CPU rendering, ⚠ notice, **⚠ CPU rendering** in the status bar |
| GPU below the minimum limits (64 MiB buffers, 4096 px textures) | Skipped; the next GPU or the CPU renderer is used |
| GPU only reachable through OpenGL | Used, with a note to install the vendor's Vulkan/DirectX driver |
| Simulation would need more memory than allowed | The simulation cell is coarsened and the preview says so |
| A preview would exceed the GPU's buffer limit | Drawn simplified (**◇ simplified view** in the status bar) |
| The GPU runs out of memory or rejects a buffer | Budgets are halved, the view is simplified, you're offered a CPU restart |
| The GPU device is lost or the renderer crashes | Your project is saved to a recovery copy and the app restarts with CPU rendering |

> **Tip:** After a restart the recovered project has no file name — use **File › Save as…** to keep
> it. A recovery copy is written at most once a minute while you work.

*Available since version 0.2.0.*
<!-- /feature:settings.graphics -->

<!-- feature:settings.general -->
## Units, theme & performance

Millimetres or inches, light or dark theme, snapping, and simulation detail (set from your GPU tier).

**Steps**

1. **File › ⚙ Settings** for windows, snapping and the simulation cell size.
2. **🌓** in the top bar switches between light and dark.
3. Units are part of the job: **① Stock › Units**.

> **Tip:** On older graphics cards start the app with `EZYX_GPU_BACKEND=gl` to use OpenGL.

*Available since version 0.2.0.*
<!-- /feature:settings.general -->
