# Introduction

Welcome to **EZ-YX-craft**, a free, open-source desktop application for carving wood, plastics and soft
metals on a CNC router. It takes you all the way from an idea to a finished part: you set up your
material, draw or import a design, choose bits and cutting strategies, preview the result in your
actual material, save the G-code, and run the job on your machine.

EZ-YX-craft aims to be powerful enough for detailed reliefs, V-carved signs and 3D parts, and
friendly enough that your first job is a matter of minutes. Sensible defaults are filled in
everywhere, and you can always see what the machine is about to do before it does it.

## How the app is organised

Everything you make flows through one **Pipeline** in the main window. The pipeline has four steps,
always in the same order:

1. **Stock** — the piece of material you are carving: its size, what it is made of, and where the
   zero point is.
2. **Design** — the shapes and features you want to cut: vectors, text, imported drawings,
   reliefs and models.
3. **Toolpaths** — how each feature is cut. Every feature gets a roughing and a finishing pass with
   recommended bits automatically; you can change the bits and preview roughing, finishing or both.
4. **Output** — the predicted result, the G-code file for your machine, and the job summary.

Around the pipeline sit specialised modules. Each one opens in **its own window**, so you can keep it
beside the main window while you work, and each one **sends its result into the pipeline** when you
are done:

| Module | Use it to | Sends to the pipeline |
|---|---|---|
| **Image Studio** | Trace logos and sketches, turn photos into V-carve grooves, reliefs or 3D models | A 2D-cut, relief, photo V-carve or 3D-model feature |
| **Solid Modeler** | Design parametric parts: constrained sketches, extrude/revolve/sweep/loft, holes, threads, fillets, patterns | A 2.5D-relief or 3D-model feature |
| **Machine Control** | Connect over USB or Wi-Fi, jog, zero, probe and run jobs | (runs the pipeline's output) |
| **Assistant** | Ask the built-in AI helper to do the clicking for you | Any project change, undoable |

Because every module talks to the same project, anything you change can be undone, and the
pipeline always shows the current state of the whole job. When you go back to a module to change
something you already sent, the pipeline merges the new version into place and reports any conflicts
with the rest of the job.

## Conventions used in this manual

- **Units are millimetres.** You can switch the display to inches in the settings; the manual uses mm.
- **Coordinates:** X runs left to right, Y runs away from you, and **Z = 0 is the top of the stock**.
  Negative Z values are inside the material, so a depth of 3 mm means Z = −3 mm.
- Buttons and fields are written in **bold**, for example **Calculate**.

> **Tip:** Tips like this one point out shortcuts and good habits.

> **Warning:** Warnings like this one appear wherever you could damage a bit, your machine, or
> yourself. Read them before you run a job.

## Where to go next

- New to EZ-YX-craft? Start with [Getting started](getting-started.md).
- Looking for a particular feature? The [Feature reference](reference/features.md) lists every
  feature by module, with the version that introduced it.
