# EZ-YX-craft user manual — style guide

This guide is for everyone who writes the manual: maintainers and the LLM revision step in
`tools/docs/update_manual.py revise` (which sends it with every request). Aim for the
task-oriented clarity of a good CNC software guide, such as the Vectric VCarve user guide:
the reader has a job to get done and wants to get back to the machine.

## Audience

Hobbyists and small-shop makers with a desktop CNC router (for example a Genmitsu 3018 or
4040), from first-time users to people who already know other CAM software. Assume
they know what a router bit is; do not assume they know CAM terms such as *stepover* or
*climb milling* without a short explanation the first time each appears in a chapter.

## Voice

- Speak to the reader as **you**, in the present tense and active voice:
  "Enter the thickness of your board", not "The thickness should be entered".
- Be friendly and direct. Short sentences. One idea per sentence.
- Explain *why* when it helps the reader decide ("A smaller stepover leaves a smoother
  surface but takes longer"), not when it is obvious.
- Do not write marketing copy ("powerful", "seamless", "simply") or talk about the
  implementation (crates, structs, threads). Describe what the user sees and does.
- Use British or American spelling consistently within a chapter; prefer the spelling the
  app's own labels use (the app says "centreline").

## Structure of a chapter

1. A `# Title` that names the module or task.
2. One or two sentences on what the chapter helps you do.
3. One section per feature (inside its feature markers — see below), each with:
   - a `## Heading` naming the task ("Set up the stock"), unless the feature *is* the chapter;
   - a short paragraph: what it does and when you need it;
   - **numbered steps**, one action per step, in the order the user clicks;
   - the feature's screenshot(s) right after the steps they illustrate;
   - tips, warnings and troubleshooting where they help.
4. Optional hand-written sections ("Troubleshooting", "Good to know") at the end.

Keep steps short: start each with a verb, name the control in **bold**, and say what
happens when it isn't obvious.

```markdown
1. Open the **Stock** step.
2. Enter the **Width**, **Height** and **Thickness** of your board.
3. Choose the material from **Material**. The preview and the suggested feeds update.
```

## Formatting

- UI elements (buttons, fields, tabs, step names, window names) in **bold**, spelled exactly
  as the app shows them. Menu paths use ›: **File › Save As**.
- Keys and shortcuts in code style: `Ctrl+Z`, `Space`.
- File names, G-code and commands in code style: `oak_sign.ezyx`, `G21`, `ezyx sim`.
- Numbers with units have a space: `6 mm`, `1200 mm/min`, `18000 RPM`, `45°` (no space for degrees).
- Lists of three or more parallel items use bullets; sequences the user follows use numbers.
- Tables for comparisons and option reference (option | what it does | typical value).
- Headings in sentence case: "Connect to your machine", not "Connect To Your Machine".

## Units and coordinates

- The manual uses **millimetres**. Give inch equivalents only for bit sizes that are sold in
  inches: `6.35 mm (1/4 in)`.
- X runs left to right, Y away from the operator, **Z = 0 is the top of the stock** and
  negative Z is inside the material. Say "a depth of 3 mm" rather than "Z −3" unless the
  coordinate itself matters.
- Feeds in mm/min, spindle speed in RPM, depths and stepovers in mm (stepover may also be
  given as a percentage of bit diameter).

## Tips, warnings and notes

Use block quotes with a bold label, one idea each:

```markdown
> **Tip:** Hold `Shift` while jogging for fine 0.1 mm steps.

> **Note:** Changing the stock size does not move your design.

> **Warning:** Check that the bit is clear of clamps before you start the job.
```

- **Tip:** a shortcut or good habit.
- **Note:** something that may surprise the reader.
- **Warning:** anything that can break a bit, damage the machine or the work, or hurt
  someone. Always warn before steps that move the machine, start the spindle, or run a job;
  never remove an existing safety warning.

## Screenshots and images

- Use only images that exist in `src/img/` (screenshots are generated from the feature
  registry; `cli_sim_preview.png` is a headless preview render). Reference them with
  relative paths: `![Stock setup step](../img/pipeline_stock.png)` from a chapter in a
  subfolder, `img/...` from a top-level chapter.
- The alt text is a short caption describing what the image shows.
- Place an image after the steps it illustrates, not before the first paragraph.
- Do not describe details of a screenshot you cannot see; describe the task.

## Feature markers (machine-maintained blocks)

Each registry feature's documentation sits between markers:

```markdown
<!-- feature:pipeline.stock -->
## Set up the stock
...
<!-- /feature:pipeline.stock -->
```

- Never rename, remove or reorder the marker comments of current features; keep each on
  its own line.
- Everything about a feature goes inside its markers. Text outside markers is hand-written
  by maintainers and is preserved by the tooling.
- When a feature is removed from the app, its block is deleted.

## Accuracy

- The feature registry (`docs/manual/registry.json`) is the source of truth for names,
  steps and behaviour. If prose and registry disagree, the registry wins.
- Do not invent controls, defaults, limits or keyboard shortcuts. If a detail is not known,
  leave it out.
- Feeds, speeds and depths in examples must be safe and realistic for a hobby machine; say
  that they are starting points and recommend a test cut.
- Mention the version that introduced a feature only when it helps ("Available since
  version 0.2.0").
