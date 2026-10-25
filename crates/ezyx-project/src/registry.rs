//! The **feature registry**: one list of every user-facing feature, shared by the GUI (button
//! labels and tooltips), the CLI (`ezyx docs-registry`) and the documentation pipeline
//! (`tools/docs/update_manual.py`), which turns each entry into a section of the user manual with
//! its screenshots. Adding a feature to the app means adding an entry here; CI then fails until
//! the manual has a section for it.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub id: &'static str,
    pub title: &'static str,
    pub chapter: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Feature {
    pub id: &'static str,
    pub module: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub how_to: &'static [&'static str],
    pub since: &'static str,
    pub shots: &'static [&'static str],
    pub chapter: &'static str,
    pub keywords: &'static [&'static str],
}

/// A screenshot the GUI renders in docs mode (`EZYX_DOC_SHOTS`).
#[derive(Debug, Clone, Serialize)]
pub struct Shot {
    pub id: &'static str,
    /// `pipeline`, `image`, `solid` or `machine`.
    pub workspace: &'static str,
    /// Pipeline step (`stock`, `design`, `toolpaths`, `output`) or module tab.
    pub step: &'static str,
    /// Project to open first (repo-relative); empty = demo project.
    pub project: &'static str,
    /// Extra scripted setup understood by the app's docs mode (e.g. `simulate`, `connect_sim`,
    /// `template:Enclosure`, `image:assets/icon.png`, `select_all`).
    pub setup: &'static [&'static str],
    pub caption: &'static str,
    pub size: [u32; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct Registry {
    pub app_version: &'static str,
    pub modules: Vec<Module>,
    pub features: Vec<Feature>,
    pub shots: Vec<Shot>,
}

const V: &str = "0.2.0";

macro_rules! feat {
    ($id:expr, $m:expr, $ch:expr, $t:expr, $s:expr, [$($h:expr),* $(,)?], [$($sh:expr),* $(,)?], [$($k:expr),* $(,)?]) => {
        Feature { id: $id, module: $m, title: $t, summary: $s, how_to: &[$($h),*], since: V, shots: &[$($sh),*], chapter: $ch, keywords: &[$($k),*] }
    };
}

pub fn modules() -> Vec<Module> {
    vec![
        Module {
            id: "pipeline",
            title: "The carving pipeline",
            chapter: "pipeline/index.md",
            summary: "The main window. A job moves through four steps — Stock, Design, Toolpaths, Output — and every design element is a feature on the timeline.",
        },
        Module {
            id: "image",
            title: "Image Studio",
            chapter: "image/index.md",
            summary: "A separate window that turns photos, sketches and logos into 2D cuts, 2.5D reliefs, photo V-carves or 3D models and sends them to the pipeline.",
        },
        Module {
            id: "solid",
            title: "Solid Modeler",
            chapter: "solid/index.md",
            summary: "A separate parametric part modeller (sketches, constraints, features, parameters) whose parts are carved in the pipeline as 2.5D reliefs or 3D models.",
        },
        Module {
            id: "machine",
            title: "Machine Control",
            chapter: "machine/index.md",
            summary: "A separate window to connect to the CNC over USB, Wi-Fi or WebSocket, jog, zero, probe and run G-code from files or from the pipeline.",
        },
        Module {
            id: "assistant",
            title: "AI assistant",
            chapter: "assistant.md",
            summary: "Claude, OpenAI or a local model that edits the project through the same commands as the GUI.",
        },
        Module { id: "settings", title: "Windows, layout & settings", chapter: "settings.md", summary: "How the module windows are arranged and other preferences." },
    ]
}

pub fn features() -> Vec<Feature> {
    vec![
        // ------------------------------------------------------------------ pipeline
        feat!(
            "pipeline.overview",
            "pipeline",
            "pipeline/index.md",
            "The four steps",
            "The step bar at the top of the main window walks a job from raw stock to G-code: ① Stock, ② Design, ③ Toolpaths, ④ Output. You can jump between steps at any time; later steps update automatically when you change an earlier one.",
            ["Click a step in the step bar (or press Ctrl+1 … Ctrl+4).", "Use Next → in the ribbon to move on when a step is done."],
            ["pipeline_design"],
            ["workflow", "steps", "wizard"]
        ),
        feat!(
            "pipeline.stock",
            "pipeline",
            "pipeline/stock.md",
            "Stock setup",
            "Decide what the machine cuts from: material, shape (rectangular board, round blank or rotary cylinder), size, where Z zero is (stock top or bed) and where XY zero sits on the stock. The machine profile and post-processor are chosen here too.",
            [
                "Open ① Stock.",
                "Pick a material — its colour is used for the rendered preview and its group for feeds and speeds.",
                "Choose the shape and type the width, height and thickness.",
                "Click a dot in the XY-zero grid to set the datum and choose Z zero.",
                "Choose your machine; the matching post-processor is selected for you."
            ],
            ["pipeline_stock"],
            ["job setup", "material", "thickness", "datum", "round", "rotary"]
        ),
        feat!(
            "pipeline.timeline",
            "pipeline",
            "pipeline/timeline.md",
            "Feature timeline",
            "The left panel lists the stock and every feature in design order. Each feature shows what it is (2D cut, 2.5D relief, 3D model, photo V-carve), where it came from (drawn, file, Image Studio, Solid Modeler), its revision and any conflicts. Features can be renamed, reordered, hidden (suppressed) or reopened in the editor that made them.",
            [
                "Click a feature to select it and see its properties.",
                "Use ▲ ▼ to reorder, 👁 to suppress, 🗑 to delete.",
                "Click ✏ Edit in Image Studio / Solid Modeler to return to the original editor."
            ],
            ["pipeline_design"],
            ["features", "history", "tree", "revision"]
        ),
        feat!(
            "pipeline.draw",
            "pipeline",
            "pipeline/design.md",
            "Drawing tools",
            "Basic 2D drawing in the Design step: rectangle, circle, ellipse, polygon, star, polyline and TrueType text. While you draw you can type exact values (quick keys) instead of dragging.",
            [
                "Pick a tool in the Draw group (R rectangle, C circle, P polyline, T text).",
                "Drag on the stock — or click once and type the size, e.g. 120,60 then Enter.",
                "Press Esc to go back to Select."
            ],
            ["pipeline_design"],
            ["rectangle", "circle", "text", "polyline", "quick keys"]
        ),
        feat!(
            "pipeline.edit",
            "pipeline",
            "pipeline/design.md",
            "Editing, arranging & layers",
            "Offset, weld (union), subtract and intersect closed shapes; move, scale, rotate and mirror with exact numbers; align and distribute several shapes; centre on the stock; organise vectors on layers you can hide or lock.",
            [
                "Select shapes (click, Shift+click or drag a box).",
                "Use the Edit and Arrange groups, or type position and size in the Selection panel.",
                "Open the Layers section in the right panel to add, hide or lock layers."
            ],
            [],
            ["offset", "boolean", "weld", "align", "distribute", "mirror", "layers", "transform"]
        ),
        feat!(
            "pipeline.snap",
            "pipeline",
            "pipeline/design.md",
            "Snapping & measuring",
            "Snap the cursor to the grid, to shape corners and midpoints, and to the stock edges and centre. The measure tool shows distance and angle between two points.",
            ["Toggle grid and object snap in the viewport toolbar (hold Alt to snap off temporarily).", "Pick 📏 Measure and click two points."],
            [],
            ["snap", "grid", "measure", "dimension"]
        ),
        feat!(
            "pipeline.carve",
            "pipeline",
            "pipeline/design.md",
            "Carve tools",
            "Turn selected vectors into a feature with one click: Cut out, Inside cut, Pocket, V-carve, Engrave, Drill or On the line. The feature gets a sensible depth for your stock and its toolpaths are planned automatically.",
            ["Select the vectors.", "Click a button in the Carve group.", "Adjust the depth in the feature panel on the right."],
            ["pipeline_design"],
            ["cut out", "pocket", "v-carve", "engrave", "drill", "profile"]
        ),
        feat!(
            "pipeline.import",
            "pipeline",
            "pipeline/design.md",
            "Importing designs",
            "Bring in SVG/DXF vectors as 2D features and STL/OBJ meshes as 3D model features. Images open in the Image Studio, solid parts are made in the Solid Modeler; both send their results here.",
            ["Design → Import → SVG/DXF or STL/OBJ.", "For pictures use Design → Image Studio; for parts use Design → Solid Modeler."],
            [],
            ["svg", "dxf", "stl", "obj", "import"]
        ),
        feat!(
            "pipeline.merge",
            "pipeline",
            "pipeline/timeline.md",
            "Editing a feature again (merge)",
            "Features that came from the Image Studio or the Solid Modeler remember their source. Reopen the editor, change the design and press Update feature: the new revision replaces the old one in the same place on the stock, your bit choices are kept, toolpaths are regenerated and a merge report lists what changed downstream.",
            [
                "Select the feature and click ✏ Edit in … (or double-click it).",
                "Change the design in the editor window.",
                "Click ⟳ Update feature in the editor.",
                "Read the merge report: size change, regenerated toolpaths, re-targeted operations and new or resolved conflicts."
            ],
            [],
            ["reimport", "revision", "merge", "round trip"]
        ),
        feat!(
            "pipeline.conflicts",
            "pipeline",
            "pipeline/timeline.md",
            "Conflict checks",
            "The pipeline continuously checks for features that run off the stock, cut deeper than the stock is thick, overlap each other, lost their geometry, or whose bit is too big to cut anything, and for hand-made operations made for an older revision. Problems show as badges on the timeline and in the Conflicts list.",
            ["Look for ⚠ / ✖ badges in the timeline.", "Click a conflict to select the features involved."],
            [],
            ["conflict", "overlap", "too deep", "warnings", "check"]
        ),
        feat!(
            "pipeline.toolpaths",
            "pipeline",
            "pipeline/toolpaths.md",
            "Automatic toolpaths",
            "When you open ③ Toolpaths every feature already has the best roughing and finishing cycle for it: cut-outs get a roughing profile with allowance plus a finishing profile with tabs; pockets a large-bit clearing pass and a smaller-bit finish; V-carves a flat-floor clearance pass when the shape is wide; reliefs and 3D models a flat-end-mill roughing pass and a ball-nose finish. Operations run in a safe order: drilling, clearing, finishing, cut-outs last.",
            ["Open ③ Toolpaths.", "Check the plan per feature in the left panel.", "Click ⚡ Recalculate after changes, or 🪄 Auto-plan all to reset to the defaults."],
            ["pipeline_toolpaths"],
            ["toolpath", "roughing", "finishing", "auto", "strategy"]
        ),
        feat!(
            "pipeline.bits",
            "pipeline",
            "pipeline/toolpaths.md",
            "Choosing bits",
            "Every feature's plan can use different bits for roughing and finishing, skip a pass, change the finishing allowance or turn tabs off. Your choices are kept when toolpaths are recalculated or the feature is re-imported.",
            ["Select a feature in ③ Toolpaths.", "Pick the roughing and finishing bit (type to filter, e.g. 'ball 1/8').", "Click ⚡ Recalculate."],
            [],
            ["tool", "bit", "end mill", "ball nose", "v-bit", "feeds"]
        ),
        feat!(
            "pipeline.manual_ops",
            "pipeline",
            "pipeline/toolpaths.md",
            "Hand-made operations (advanced)",
            "Add your own profile, pocket, V-carve, drill, engrave, laser or 3D operation with full control over feeds, depths and options. Hand-made operations are never overwritten by the planner.",
            ["③ Toolpaths → Advanced → pick an operation type.", "Edit it in the right panel."],
            [],
            ["operation", "custom", "laser", "feeds and speeds"]
        ),
        feat!(
            "pipeline.preview",
            "pipeline",
            "pipeline/toolpaths.md",
            "Toolpath preview",
            "Show roughing, finishing or both toolpaths in 2D or 3D, coloured by depth, and simulate material removal for exactly what is shown.",
            ["Use the Show: Rough / Finish / Both switch.", "Click 👁 Simulate to cut the virtual stock."],
            ["pipeline_preview"],
            ["preview", "simulate", "rough", "finish"]
        ),
        feat!(
            "pipeline.render",
            "pipeline",
            "pipeline/toolpaths.md",
            "Final rendered view",
            "The simulated result is rendered in your material (walnut, maple, oak, MDF, aluminium…) with machining advice such as scallop height, burn risk and overload warnings.",
            ["Click ✨ Final render.", "Orbit with the left mouse button; use the view buttons for top/front/iso."],
            ["pipeline_preview"],
            ["render", "material", "result", "advice"]
        ),
        feat!(
            "pipeline.output",
            "pipeline",
            "pipeline/output.md",
            "G-code & job sheet",
            "Check the summary (time estimate, bits in order, tool changes, warnings), then save G-code for your machine's post-processor or a printable job sheet.",
            ["Open ④ Output.", "Confirm the machine and post-processor.", "Click 💾 Download G-code (or 📄 Job sheet)."],
            ["pipeline_output"],
            ["gcode", "post", "export", "job sheet", "nc"]
        ),
        feat!(
            "pipeline.send_machine",
            "pipeline",
            "pipeline/output.md",
            "Send to Machine Control",
            "Hand the G-code straight to the Machine Control window, which opens with the job loaded and ready to run.",
            ["Open ④ Output.", "Click 🎮 Send to Machine Control."],
            [],
            ["send", "machine", "stream"]
        ),
        // ------------------------------------------------------------------ image studio
        feat!(
            "image.studio",
            "image",
            "image/index.md",
            "Image Studio",
            "Its own window with an image viewer, live processing preview and the conversion settings. Open it from the Design step or the module buttons at the top; it can sit next to the main window.",
            ["Click 🖼 Image Studio.", "Load a photo, sketch or logo.", "Choose a conversion mode and tune it while watching the preview."],
            ["image_studio"],
            ["image", "picture", "photo", "logo"]
        ),
        feat!(
            "image.outline",
            "image",
            "image/index.md",
            "Trace outline → 2D cut",
            "Logos and silhouettes become closed vectors for cut-outs, pockets or V-carving. Threshold (auto, fixed, adaptive, edges), invert, smoothing, gap closing and speck removal clean up the input.",
            ["Mode: Outline.", "Tune the threshold until the preview looks right.", "Pick the strategy and click Send to pipeline."],
            [],
            ["trace", "vectorize", "outline", "logo"]
        ),
        feat!(
            "image.sketch",
            "image",
            "image/index.md",
            "Sketch → single line",
            "Pen and pencil drawings become single centre-line paths for engraving.",
            ["Mode: Sketch.", "Use Adaptive threshold for pencil on uneven paper.", "Send to pipeline (strategy: Engrave)."],
            [],
            ["centreline", "centerline", "sketch", "engrave"]
        ),
        feat!(
            "image.relief",
            "image",
            "image/index.md",
            "Image → 2.5D relief",
            "Greyscale becomes height: bright = high (or invert), with gamma, smoothing, size and depth.",
            ["Mode: Relief.", "Set width, depth and gamma.", "Send to pipeline as a 2.5D relief."],
            [],
            ["relief", "heightmap", "lithophane", "2.5d"]
        ),
        feat!(
            "image.photo_vcarve",
            "image",
            "image/index.md",
            "Photo V-carve",
            "Shaded photos become V-grooves whose depth follows the darkness, as lines, waves or circles.",
            ["Mode: Photo V-carve.", "Choose the V-bit angle, line spacing and pattern.", "Send to pipeline."],
            [],
            ["photo", "v-carve", "grooves", "portrait"]
        ),
        feat!(
            "image.to3d",
            "image",
            "image/index.md",
            "Image → 3D model",
            "A silhouette (transparent PNGs work best) is inflated into a 3D model that is carved top-down. AI image-to-3D services can be plugged in.",
            ["Mode: 3D model.", "Set the thickness.", "Send to pipeline as a 3D model."],
            [],
            ["3d", "inflate", "image to 3d"]
        ),
        feat!(
            "image.send",
            "image",
            "image/index.md",
            "Send to pipeline / update feature",
            "Send creates a new feature in the pipeline, centred on the stock. When the studio was opened from an existing feature the button becomes Update feature and the result is merged in place.",
            ["Click Send to pipeline (new feature) or ⟳ Update feature (new revision)."],
            [],
            ["send", "update", "pipeline"]
        ),
        // ------------------------------------------------------------------ solid modeler
        feat!(
            "solid.modeler",
            "solid",
            "solid/index.md",
            "Solid Modeler",
            "A SolidWorks-style part modeller in its own window: the FeatureManager tree on the left, a 3D view in the middle, the PropertyManager on the right and a CommandManager with Sketch, Features, Evaluate and Export tabs on top.",
            ["Click 🧊 Solid Modeler.", "Start from a template or an empty part.", "Build the part from sketches and features."],
            ["solid_modeler"],
            ["cad", "solidworks", "part", "model"]
        ),
        feat!(
            "solid.sketch",
            "solid",
            "solid/sketch.md",
            "Sketches & constraints",
            "Draw lines, rectangles, circles, arcs and splines on the XY, XZ or YZ plane, then constrain them: coincident, horizontal, vertical, parallel, perpendicular, tangent, equal, concentric, midpoint, symmetric, fix, and driving dimensions (length, distance, radius, diameter, angle) that may use parameters. The solver shows whether the sketch is fully defined.",
            [
                "Sketch tab → New sketch → pick a plane.",
                "Draw with the sketch tools; click two entities and a constraint button.",
                "Add dimensions; type a value or a parameter expression such as width/2.",
                "Watch the status: under-defined (degrees of freedom left), fully defined or over-defined."
            ],
            ["solid_sketch"],
            ["sketch", "constraint", "dimension", "solver"]
        ),
        feat!(
            "solid.features",
            "solid",
            "solid/features.md",
            "Extrude, revolve, sweep & loft",
            "Extrude a sketch (blind, mid-plane or through all, with draft and rounded or chamfered cap edges), revolve it (full or partial), sweep a profile along a path, or loft between sections. Each feature adds, cuts or intersects with the body.",
            ["Features tab → choose the feature.", "Set its options in the PropertyManager.", "Choose Add / Cut / Intersect."],
            [],
            ["extrude", "boss", "cut", "revolve", "sweep", "loft"]
        ),
        feat!(
            "solid.dress",
            "solid",
            "solid/features.md",
            "Fillet, chamfer, shell & draft",
            "Round or chamfer where a feature meets the body, fillet the whole part, hollow it with a shell (optionally open at the top, for enclosures) and add draft to extrusions.",
            ["Select a feature and set Blend (round / chamfer) in the PropertyManager.", "Or Features → Fillet all / Shell."],
            [],
            ["fillet", "chamfer", "shell", "draft", "round"]
        ),
        feat!(
            "solid.holes",
            "solid",
            "solid/features.md",
            "Hole wizard & threads",
            "Simple, drilled, counterbored, countersunk and tapped holes, and modelled external or internal threads.",
            ["Features → Hole wizard.", "Pick the hole type and size, place it on the top face."],
            [],
            ["hole", "counterbore", "countersink", "tap", "thread"]
        ),
        feat!(
            "solid.patterns",
            "solid",
            "solid/features.md",
            "Patterns & mirror",
            "Linear, grid and circular patterns of a feature, and mirror about the YZ, XZ or XY plane.",
            ["Select a feature → Pattern / Mirror in the PropertyManager."],
            [],
            ["pattern", "array", "mirror", "circular"]
        ),
        feat!(
            "solid.bodies",
            "solid",
            "solid/features.md",
            "Split, combine & scale",
            "Split the part by a plane, combine it with another part (add, subtract, common) and scale it.",
            ["Features → Split / Combine.", "For Combine pick the other part and the operation."],
            [],
            ["split", "combine", "boolean", "multibody", "scale"]
        ),
        feat!(
            "solid.parameters",
            "solid",
            "solid/parameters.md",
            "Parameters & configurations",
            "Named parameters with unit-aware expressions (e.g. wall = 2 mm, width = 3 in, hole = width/8) drive dimensions and feature values. Configurations override parameters to make size variants of one part.",
            ["Open the Parameters tab of the FeatureManager.", "Add a parameter and bind it to a feature value with the ƒ button.", "Add a configuration and switch between them."],
            [],
            ["parameter", "expression", "equation", "configuration", "variant"]
        ),
        feat!(
            "solid.evaluate",
            "solid",
            "solid/evaluate.md",
            "Mass properties & interference",
            "Volume, surface area, centre of mass, bounding box and mass for the chosen material, section area at a height, and the interference volume between two parts.",
            ["Evaluate tab → Mass properties.", "Evaluate → Interference: pick another part."],
            [],
            ["mass", "volume", "weight", "center of mass", "interference"]
        ),
        feat!(
            "solid.templates",
            "solid",
            "solid/index.md",
            "Starter templates",
            "Ready-made parametric parts to learn from or adapt: enclosure, flange with bolt circle, knob, threaded rod, lofted vase.",
            ["New part → From template."],
            [],
            ["template", "example", "starter"]
        ),
        feat!(
            "solid.export",
            "solid",
            "solid/evaluate.md",
            "Export & send to pipeline",
            "Send the part to the pipeline as a 2.5D relief (top-down height field) or as a 3D model, update the feature made from it, export STL/OBJ, or a tetrahedral mesh (VTK/Gmsh) for analysis.",
            ["Export tab → Send to pipeline as 2.5D or 3D.", "After editing, ⟳ Update pipeline feature.", "Or export STL / OBJ / tet mesh."],
            [],
            ["export", "stl", "obj", "tet mesh", "send"]
        ),
        feat!(
            "solid.solidcraft",
            "solid",
            "solid/index.md",
            "SolidCraft bridge",
            "If the open-source SolidCraft CAD is installed, open the part's mesh in it for B-rep work (STEP, assemblies, sheet metal) and import its STL/OBJ exports back as a pipeline feature.",
            ["Export tab → Open in SolidCraft (shown when solidcraft is on your PATH).", "Export STL from SolidCraft, then Import STL/OBJ here."],
            [],
            ["solidcraft", "step", "brep", "interop"]
        ),
        // ------------------------------------------------------------------ machine control
        feat!(
            "machine.control",
            "machine",
            "machine/index.md",
            "Machine Control window",
            "A separate window that never changes your design: connection bar, big position read-out, jog pad, job queue with a path preview, run controls, overrides and console. It works with files from anywhere as well as jobs sent from the pipeline.",
            ["Click 🎮 Machine Control."],
            ["machine_control"],
            ["machine", "control", "sender", "dro"]
        ),
        feat!(
            "machine.connect",
            "machine",
            "machine/index.md",
            "Connecting: USB, Wi-Fi, WebSocket, simulator",
            "Choose USB serial (port + baud), Wi-Fi TCP/telnet, WebSocket, or the built-in simulator. Scan your network for controllers.",
            ["Pick the link type.", "Choose the port or host (or 🔍 Find machines).", "Click Connect."],
            [],
            ["usb", "serial", "wifi", "websocket", "grbl", "connect"]
        ),
        feat!(
            "machine.jog",
            "machine",
            "machine/index.md",
            "Jogging, homing & zeroing",
            "Large jog buttons with step sizes and feed, home, unlock, and zero any axis. Keyboard jogging with the arrow keys and Page Up/Down when the window has focus.",
            ["Choose a step size.", "Click the arrows (or use the keyboard).", "Zero X, Y and Z at your datum."],
            [],
            ["jog", "home", "zero", "unlock", "keyboard"]
        ),
        feat!(
            "machine.probe",
            "machine",
            "machine/index.md",
            "Z probe",
            "Probe the stock top with a touch plate of known thickness.",
            ["Attach the clip and place the plate.", "Set the plate thickness and click ⬇ Probe Z."],
            [],
            ["probe", "touch plate", "z zero"]
        ),
        feat!(
            "machine.jobs",
            "machine",
            "machine/index.md",
            "Running jobs",
            "Load G-code files or receive jobs from the pipeline, check the path preview and estimated time, then run, pause, resume or stop. A safety checklist is shown before the machine moves.",
            ["Load a file or pick a job sent from the pipeline.", "Click ▶ Run and confirm the checklist."],
            [],
            ["run", "job", "gcode", "pause", "stop"]
        ),
        feat!(
            "machine.console",
            "machine",
            "machine/index.md",
            "Overrides & console",
            "Feed and spindle overrides while running, and a console for G-code and $ commands.",
            ["Use the ±10 % buttons while a job runs.", "Type a command in the console and press Enter."],
            [],
            ["override", "console", "$ settings"]
        ),
        // ------------------------------------------------------------------ assistant / settings
        feat!(
            "assistant.chat",
            "assistant",
            "assistant.md",
            "AI assistant",
            "Ask in plain language — \"make a 200×100 walnut sign that says Workshop\" — and the assistant builds features, plans toolpaths and (only after you confirm) writes files or moves the machine. Every change is undoable.",
            ["Click 🤖 Assistant.", "Choose Claude, OpenAI or Ollama in its settings.", "Type a request."],
            [],
            ["ai", "assistant", "claude", "chat"]
        ),
        feat!(
            "settings.windows",
            "settings",
            "settings.md",
            "Module windows side by side",
            "Image Studio, Solid Modeler and Machine Control open as their own windows so they can sit next to the pipeline. ⧉ Tile places the open windows side by side; a preference shows them inside the main window instead.",
            ["Open a module from the top bar.", "Click ⧉ Tile to arrange the windows.", "Settings → Windows to choose separate or embedded windows."],
            [],
            ["windows", "layout", "side by side", "tile"]
        ),
        feat!(
            "settings.graphics",
            "settings",
            "settings.md",
            "Graphics: GPU checks and CPU fallback",
            "At start-up EZ-YX-craft checks the graphics adapters. GPUs that can't run the viewport (no driver, tiny buffer or texture limits, can't draw to the window) are skipped, and if none is usable the app renders on the CPU instead and tells you why. While you work, previews are sized to the GPU's buffer limit; if the GPU still runs out of memory the app simplifies the view and offers to restart with CPU rendering instead of crashing.",
            [
                "If a ⚠ Graphics message appears, read why and choose Restart with CPU rendering or OK.",
                "File › ⚙ Settings › Graphics shows the adapter in use and its limits.",
                "Choose Render with Auto, GPU or CPU and click Restart now (or start with EZYX_RENDER=cpu)."
            ],
            [],
            ["gpu", "graphics", "driver", "cpu rendering", "software", "out of memory", "crash"]
        ),
        feat!(
            "settings.general",
            "settings",
            "settings.md",
            "Units, theme & performance",
            "Millimetres or inches, light or dark theme, and simulation detail (set from your GPU tier).",
            ["Open ⚙ Settings."],
            [],
            ["units", "inches", "dark mode", "gpu"]
        ),
    ]
}

pub fn shots() -> Vec<Shot> {
    let s = |id, workspace, step, setup: &'static [&'static str], caption| Shot { id, workspace, step, project: "", setup, caption, size: [1480, 920] };
    vec![
        s("pipeline_stock", "pipeline", "stock", &[], "① Stock: material, shape, size and datum"),
        s("pipeline_design", "pipeline", "design", &[], "② Design: drawing and carve tools with the feature timeline"),
        s("pipeline_toolpaths", "pipeline", "toolpaths", &["select_feature:0"], "③ Toolpaths: the automatic roughing and finishing plan"),
        s("pipeline_preview", "pipeline", "toolpaths", &["simulate"], "Simulated result rendered in the stock material"),
        s("pipeline_output", "pipeline", "output", &[], "④ Output: summary, G-code and job sheet"),
        s("image_studio", "image", "outline", &["image:examples/images/tree_logo.png"], "Image Studio tracing a logo"),
        s("solid_modeler", "solid", "features", &["template:0"], "Solid Modeler with a parametric template"),
        s("solid_sketch", "solid", "sketch", &["sketch_demo"], "Constrained sketch with driving dimensions"),
        s("machine_control", "machine", "control", &["connect_sim"], "Machine Control connected to the simulator"),
    ]
}

pub fn registry() -> Registry {
    Registry { app_version: V, modules: modules(), features: features(), shots: shots() }
}

/// Look up a feature (for tooltips). Panics in debug builds on unknown ids so typos are caught.
pub fn feature(id: &str) -> Option<&'static Feature> {
    static ALL: std::sync::OnceLock<Vec<Feature>> = std::sync::OnceLock::new();
    let f = ALL.get_or_init(features).iter().find(|f| f.id == id);
    debug_assert!(f.is_some(), "unknown registry feature '{id}'");
    f
}

/// Tooltip text for a registry feature.
pub fn tip(id: &str) -> String {
    feature(id).map(|f| f.summary.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_is_consistent() {
        let r = registry();
        let mods: Vec<&str> = r.modules.iter().map(|m| m.id).collect();
        let shots: Vec<&str> = r.shots.iter().map(|s| s.id).collect();
        let mut ids = std::collections::HashSet::new();
        for f in &r.features {
            assert!(ids.insert(f.id), "duplicate {}", f.id);
            assert!(mods.contains(&f.module), "{} has unknown module", f.id);
            assert!(f.id.starts_with(f.module), "{} should be prefixed with its module", f.id);
            assert!(!f.summary.is_empty() && !f.how_to.is_empty());
            for s in f.shots {
                assert!(shots.contains(s), "{} references unknown shot {s}", f.id);
            }
        }
        // Every module has at least one feature and every shot is used.
        for m in &mods {
            assert!(r.features.iter().any(|f| f.module == *m), "module {m} has no features");
        }
        for s in &shots {
            assert!(r.features.iter().any(|f| f.shots.contains(s)), "shot {s} unused");
        }
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"pipeline.stock\""));
    }
}
