# Machine Control

Machine Control runs your CNC. It has its own window and its own job queue, and it never changes your
design: it streams G-code from files on disk or jobs the pipeline sends it.

> **Warning:** A CNC machine can injure you. Clamp the stock firmly, wear eye and hearing protection,
> keep your hands clear of the spindle and know where the emergency stop is before you run a job.

<!-- feature:machine.control -->
## The Machine Control window

A separate window with the connection bar at the top, a large position read-out and jog pad on the
left, the job queue and run controls on the right, a path preview with live progress in the middle and
the console at the bottom. It works with files from anywhere as well as jobs sent from the pipeline.

**Steps**

1. Click **🎮 Machine Control** in the top bar (or **④ Output › 🎮 Send to Machine Control**).

![Machine Control connected to the simulator](../img/machine_control.png)

*Available since version 0.2.0.*
<!-- /feature:machine.control -->

<!-- feature:machine.connect -->
## Connecting: USB, Wi-Fi, WebSocket, simulator

Choose USB serial (port and baud rate), Wi-Fi TCP/telnet, WebSocket, or the built-in simulator. Scan
your network for controllers.

**Steps**

1. Pick **🔌 USB**, **📶 Wi-Fi TCP**, **📶 WebSocket** or **🧪 Simulator**.
2. USB: choose the port (**⟳** refreshes the list) and baud rate (115200 for GRBL). Wi-Fi: type the
   host and port, or **🔍 Find machines** to scan your network (for example `192.168.1`).
3. Click **Connect**. The state badge shows **IDLE**, **RUN**, **HOLD** or **ALARM**.

> **Tip:** The simulator behaves like a GRBL controller. Use it to practise jogging and to dry-run a
> job before you cut.

*Available since version 0.2.0.*
<!-- /feature:machine.connect -->

<!-- feature:machine.jog -->
## Jogging, homing & zeroing

Large jog buttons with step sizes and feed, home, unlock, and zero any axis. Keyboard jogging with the
arrow keys and Page Up/Down when the window has focus.

**Steps**

1. Choose a step size (**0.1**, **1**, **10**, **50** mm) and the jog feed.
2. Click **X±**, **Y±**, **Z±** (or press the arrow keys and `Page Up`/`Page Down`). **⊙** returns
   to X0 Y0.
3. With the bit at your datum, click **Zero** next to each axis or **Zero XYZ**.
4. **🏠 Home** runs the homing cycle; **🔓 Unlock** clears an alarm once it is safe.

*Available since version 0.2.0.*
<!-- /feature:machine.jog -->

<!-- feature:machine.probe -->
## Z probe

Probe the stock top with a touch plate of known thickness.

**Steps**

1. Attach the clip to the bit and put the plate on the stock under it.
2. Set the **Touch plate** thickness and click **⬇ Probe Z**. The bit moves down until it touches,
   sets Z zero at the stock top and lifts.

*Available since version 0.2.0.*
<!-- /feature:machine.probe -->

<!-- feature:machine.jobs -->
## Running jobs

Load G-code files or receive jobs from the pipeline, check the path preview and estimated time, then
run, pause, resume or stop. A safety checklist is shown before the machine moves.

**Steps**

1. **📂 Load G-code…** or **⬇ From pipeline** — the job appears in the list and is selected.
2. Check the preview (cuts coloured by depth, dashed rapids, the orange tool marker) and the size,
   line count and time under the job name.
3. Click **▶ Run** and confirm the checklist. Done lines turn green in the preview.
4. **⏸ Pause** (feed hold), **⏵ Resume**, **⏹ STOP** (feed hold and reset).

*Available since version 0.2.0.*
<!-- /feature:machine.jobs -->

<!-- feature:machine.console -->
## Overrides & console

Feed and spindle overrides while running, and a console for G-code and `$` commands.

**Steps**

1. While a job runs, use the **Feed** and **Spindle** **−10% / 100% / +10%** buttons.
2. Type a command such as `$$` or `G0 Z5` in the console and press `Enter` (disabled while a job is
   streaming, so manual lines can't interleave with the program).

*Available since version 0.2.0.*
<!-- /feature:machine.console -->
