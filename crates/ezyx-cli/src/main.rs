//! `ezyx` — headless EZ-YX-craft.

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use ezyx_core::DVec2;
use ezyx_project::{commands::Command, ops, History, Project};
use ezyx_tooldb::ToolLibrary;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "ezyx", version, about = "EZ-YX-craft command line")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List GPUs and the backend/tier EZ-YX-craft will use.
    Gpu,
    /// Search the tool library.
    Tools { query: Vec<String> },
    /// List materials.
    Materials,
    /// List machine profiles and post-processors.
    Machines,
    /// Create a demo sign project (text V-carve + border + cut-out).
    Demo {
        #[arg(short, long, default_value = "demo.ezyx")]
        out: PathBuf,
        #[arg(long, default_value = "EZ-YX")]
        text: String,
    },
    /// Export G-code for a project.
    Gcode {
        project: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        post: Option<String>,
    },
    /// Trace an image to vectors and save as a project.
    Trace {
        image: PathBuf,
        #[arg(short, long, default_value = "traced.ezyx")]
        out: PathBuf,
        #[arg(long, default_value_t = 100.0)]
        width: f64,
        /// Single-line centreline tracing (for pen sketches).
        #[arg(long)]
        centerline: bool,
    },
    /// Image → relief STL (2.5D).
    Relief {
        image: PathBuf,
        #[arg(short, long, default_value = "relief.stl")]
        out: PathBuf,
        #[arg(long, default_value_t = 120.0)]
        width: f64,
        #[arg(long, default_value_t = 6.0)]
        depth: f64,
    },
    /// Image → 3D mesh (offline inflation backend).
    Img3d {
        image: PathBuf,
        #[arg(short, long, default_value = "model.stl")]
        out: PathBuf,
        #[arg(long, default_value_t = 100.0)]
        width: f64,
        #[arg(long, default_value_t = 12.0)]
        thickness: f64,
    },
    /// Photo V-carve straight to G-code.
    PhotoVcarve {
        image: PathBuf,
        #[arg(short, long, default_value = "photo.nc")]
        out: PathBuf,
        #[arg(long, default_value_t = 150.0)]
        width: f64,
        #[arg(long, default_value_t = 1.2)]
        spacing: f64,
        #[arg(long, default_value = "generic.v60")]
        tool: String,
        #[arg(long, default_value = "hardwood.maple")]
        material: String,
    },
    /// Simulate a project and render the predicted result to PNG.
    Sim {
        project: PathBuf,
        #[arg(short, long, default_value = "preview.png")]
        out: PathBuf,
        #[arg(long, default_value_t = 0.2)]
        cell: f64,
    },
    /// Tet-mesh a relief image or the demo CAD part for FEA (VTK + Gmsh output).
    Tetmesh {
        /// Image to turn into a relief solid; omit for the demo CAD part.
        image: Option<PathBuf>,
        #[arg(short, long, default_value = "mesh")]
        out: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        size: f64,
    },
    /// Write the feature registry (drives GUI tooltips and the user manual) as JSON.
    DocsRegistry {
        #[arg(short, long, default_value = "docs/manual/registry.json")]
        out: PathBuf,
    },
    /// Show a project's pipeline: stock, features, planned toolpaths and conflicts.
    Pipeline {
        project: PathBuf,
        /// Re-plan every feature's toolpaths with the recommended bits and save the project.
        #[arg(long)]
        replan: bool,
    },
    /// Stream G-code to a machine.
    Send {
        file: PathBuf,
        #[arg(long)]
        serial: Option<String>,
        #[arg(long, help = "host:port, e.g. 192.168.1.50:23")]
        tcp: Option<String>,
        #[arg(long, help = "ws://host:81/")]
        ws: Option<String>,
        #[arg(long)]
        sim: bool,
    },
    /// Scan a /24 subnet for Wi-Fi CNC controllers, e.g. `ezyx scan 192.168.1`.
    Scan { prefix: String },
    /// Ask the AI assistant to work on a project (needs ANTHROPIC_API_KEY / OPENAI_API_KEY or Ollama).
    Ask {
        project: PathBuf,
        prompt: Vec<String>,
        #[arg(long, default_value = "anthropic")]
        provider: String,
        #[arg(long)]
        model: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let lib = ToolLibrary::bundled();
    match cli.cmd {
        Cmd::Gpu => {
            let gpus = ezyx_gpu::probe();
            if gpus.is_empty() {
                println!("No GPU adapters found (headless?). CPU paths will be used.");
            }
            for g in gpus {
                println!("{} | {:?} {:?} | backend {} | {} | tier {:?}", g.name, g.vendor, g.family, g.backend, g.device_type, g.tier);
                for n in g.notes {
                    println!("    note: {n}");
                }
                println!("    defaults: {:?}", ezyx_gpu::defaults_for(g.tier));
            }
        }
        Cmd::Tools { query } => {
            let q = query.join(" ");
            for t in lib.search(&q).into_iter().take(60) {
                println!("{:<48} {:<22} Ø{:>7.3} mm {}F  {}", t.id, t.shape.label(), t.diameter, t.flutes, t.name);
            }
        }
        Cmd::Materials => {
            for m in ezyx_materials::library() {
                println!("{:<26} {:<28} {:?}", m.id, m.name, m.group);
            }
        }
        Cmd::Machines => {
            for m in ezyx_project::machines::builtin() {
                println!("machine {:<24} {:<40} post={}", m.id, m.name, m.post_id);
            }
            for p in ezyx_gcode::builtin_posts() {
                println!("post    {:<24} {}", p.id, p.name);
            }
        }
        Cmd::Demo { out, text } => {
            let p = demo_project(&text)?;
            p.save(&out)?;
            println!("Wrote {} ({} vectors, {} operations)", out.display(), p.vectors.len(), p.operations.len());
        }
        Cmd::DocsRegistry { out } => {
            let mut json = serde_json::to_string_pretty(&ezyx_project::registry::registry())?;
            json.push('\n');
            if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&out, json)?;
            println!("Wrote {}", out.display());
        }
        Cmd::Pipeline { project, replan } => {
            let mut p = Project::load(&project)?;
            let lib = ToolLibrary::bundled();
            if replan {
                let (n, errs) = ezyx_project::pipeline::replan(&mut p, &lib, &[]);
                p.save(&project)?;
                println!("Planned {n} toolpaths{}", if errs.is_empty() { String::new() } else { format!(" ({})", errs.join("; ")) });
            }
            let j = &p.job;
            println!("① Stock: {} {:.1} × {:.1} × {:.1} mm, {}", p.stock_shape.label(), j.width, j.height, j.thickness, j.material);
            println!("② Features:");
            for f in &p.features {
                println!("  #{} {} — {} · {} · rev {}", f.id, f.name, f.kind.label(), f.source.label(), f.revision);
                for o in p.operations.iter().filter(|o| o.feature_id == Some(f.id)) {
                    println!("     {} {} [{}] {:.2} mm", if o.auto { "•" } else { "✋" }, o.name, o.tool_id, o.cut_depth);
                }
            }
            for o in p.operations.iter().filter(|o| o.feature_id.is_none()) {
                println!("  ✋ {} [{}]", o.name, o.tool_id);
            }
            let res = ops::compute_all(&p, &lib);
            let s = ezyx_project::jobsheet::summarize(&p, &lib, &res);
            println!("③ Toolpaths: {} · ≈ {:.1} min · {} tool change(s)", s.steps.len(), s.total_seconds / 60.0, s.tool_changes);
            let conflicts = ezyx_project::pipeline::analyze(&p, &lib);
            if conflicts.is_empty() {
                println!("✔ no conflicts");
            }
            for c in conflicts {
                println!("{:?}: {}", c.severity, c.message);
            }
        }
        Cmd::Gcode { project, out, post } => {
            let mut p = Project::load(&project)?;
            if let Some(id) = post {
                p.post_id = id;
            }
            let (g, warnings) = ops::export_gcode(&p, &lib).map_err(|e| anyhow!(e))?;
            let out =
                out.unwrap_or_else(|| project.with_extension(ezyx_gcode::post::find_post(&p.post_id).map(|p| p.extension).unwrap_or_else(|| "nc".into())));
            std::fs::write(&out, &g)?;
            let secs = ezyx_gcode::parse::estimate_seconds(&ezyx_gcode::parse(&g), p.machine.limits.max_feed);
            println!("Wrote {} ({} lines, est. {:.1} min)", out.display(), g.lines().count(), secs / 60.0);
            for w in warnings {
                println!("warning: {w}");
            }
        }
        Cmd::Trace { image, out, width, centerline } => {
            let mut p = Project::default();
            let options = ezyx_image::trace::TraceOptions { width_mm: width, origin: DVec2::new(10.0, 10.0), ..Default::default() };
            let msg = Command::TraceImage { path: image.display().to_string(), options, centerline }.apply(&mut p).map_err(|e| anyhow!(e))?;
            p.save(&out)?;
            println!("{msg} → {}", out.display());
        }
        Cmd::Relief { image, out, width, depth } => {
            let img = ezyx_image::load_gray(&image)?;
            let hm = ezyx_image::relief::image_to_relief(&img, &ezyx_image::relief::ReliefOptions { width_mm: width, depth, ..Default::default() });
            let m = ezyx_mesh::hm::solid(&hm, -(depth as f32) - 3.0, 1);
            ezyx_mesh::io::write_stl(&m, &out)?;
            println!("Wrote {} ({} triangles)", out.display(), m.indices.len());
        }
        Cmd::Img3d { image, out, width, thickness } => {
            let mut p = Project::default();
            let msg = Command::ImageTo3d { path: image.display().to_string(), width_mm: width, thickness }.apply(&mut p).map_err(|e| anyhow!(e))?;
            let m = &p.meshes[0].mesh;
            ezyx_mesh::io::write_stl(m, &out)?;
            println!("{msg} → {} ({} triangles, watertight: {})", out.display(), m.indices.len(), m.boundary_edge_count() == 0);
        }
        Cmd::PhotoVcarve { image, out, width, spacing, tool, material } => {
            let img = ezyx_image::load_gray(&image)?;
            let t = lib.get(&tool).ok_or_else(|| anyhow!("unknown tool {tool}"))?;
            let angle = match t.shape {
                ezyx_tooldb::ToolShape::VBit { angle, .. } => angle,
                _ => bail!("photo V-carve needs a V-bit"),
            };
            let opt =
                ezyx_image::photo_vcarve::PhotoVCarveOptions { width_mm: width, spacing, bit_angle: angle, origin: DVec2::new(5.0, 5.0), ..Default::default() };
            let aspect = img.height as f64 / img.width as f64;
            let mut p = Project::default();
            Command::SetJob { width: Some(width + 10.0), height: Some(width * aspect + 10.0), thickness: Some(12.0), material: Some(material), safe_z: None }
                .apply(&mut p)
                .map_err(|e| anyhow!(e))?;
            let msg = Command::PhotoVCarve { path: image.display().to_string(), options: opt, tool_id: tool }.apply(&mut p).map_err(|e| anyhow!(e))?;
            let (g, _) = ops::export_gcode(&p, &lib).map_err(|e| anyhow!(e))?;
            std::fs::write(&out, g)?;
            println!("{msg} → {}", out.display());
        }
        Cmd::Sim { project, out, cell } => {
            let p = Project::load(&project)?;
            let mut sim = ezyx_sim::StockSim::new(&p.job, cell);
            let results = ops::compute_all(&p, &lib);
            for r in &results {
                for tp in &r.toolpaths {
                    if !tp.rotary {
                        sim.apply(tp);
                    }
                }
                for a in &r.advice {
                    println!("[{:?}] {}", a.severity, a.message);
                }
            }
            let mat = ezyx_materials::find(&p.job.material).unwrap_or_else(|| ezyx_materials::library()[0].clone());
            render_png(&sim, &mat, &out)?;
            println!("Removed {:.1} cm³. Preview → {}", sim.removed_volume() / 1000.0, out.display());
            for w in &sim.warnings {
                println!("warning: {w}");
            }
        }
        Cmd::Tetmesh { image, out, size } => {
            let tm = match image {
                Some(img) => {
                    let g = ezyx_image::load_gray(&img)?;
                    let hm = ezyx_image::relief::image_to_relief(
                        &g,
                        &ezyx_image::relief::ReliefOptions { width_mm: 80.0, depth: 6.0, resolution: size, ..Default::default() },
                    );
                    ezyx_mesh::tet::relief_solid(&hm, -10.0, (10.0 / size).ceil() as usize)
                }
                None => ezyx_cad::demo_part().tet_mesh(size),
            };
            let q = tm.quality();
            std::fs::write(out.with_extension("vtk"), tm.to_vtk())?;
            std::fs::write(out.with_extension("msh"), tm.to_gmsh())?;
            println!(
                "{} tets, volume {:.1} mm³, dihedral {:.1}°–{:.1}°, radius ratio min {:.2} mean {:.2} → {}.vtk / .msh",
                q.count,
                q.volume,
                q.min_dihedral,
                q.max_dihedral,
                q.min_radius_ratio,
                q.mean_radius_ratio,
                out.display()
            );
        }
        Cmd::Send { file, serial, tcp, ws, sim } => {
            use ezyx_machine::{controller::Controller, Endpoint, Event, JobState};
            let ep = if sim {
                Endpoint::Simulator
            } else if let Some(port) = serial {
                Endpoint::Serial { port, baud: 115_200 }
            } else if let Some(t) = tcp {
                let (h, p) = t.rsplit_once(':').ok_or_else(|| anyhow!("use host:port"))?;
                Endpoint::Tcp { host: h.into(), port: p.parse()? }
            } else if let Some(url) = ws {
                Endpoint::WebSocket { url }
            } else {
                bail!("choose --serial, --tcp, --ws or --sim")
            };
            let g = std::fs::read_to_string(&file)?;
            let c = Controller::connect(ep)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            c.start_job(&g);
            loop {
                match c.events.recv()? {
                    Event::Progress { acked, total, state } => {
                        print!("\r{acked}/{total} {state:?}      ");
                        if matches!(state, JobState::Done | JobState::Failed | JobState::Idle) && acked > 0 {
                            println!();
                            break;
                        }
                    }
                    Event::Error(e) => eprintln!("\n{e}"),
                    Event::Disconnected(r) => bail!("disconnected: {r}"),
                    _ => {}
                }
            }
        }
        Cmd::Scan { prefix } => {
            println!("Scanning {prefix}.1-254 on ports {:?} …", ezyx_machine::discovery::WIFI_PORTS);
            for e in ezyx_machine::discovery::scan_subnet(&prefix, 250) {
                println!("  {}", e.label());
            }
        }
        Cmd::Ask { project, prompt, provider, model } => {
            let mut p = if project.exists() { Project::load(&project)? } else { Project::default() };
            let cfg = match provider.as_str() {
                "openai" => ezyx_ai::ProviderConfig::openai(model.as_deref().unwrap_or("gpt-5")),
                "ollama" => ezyx_ai::ProviderConfig::ollama(model.as_deref().unwrap_or("llama3.1")),
                _ => model.map(|m| ezyx_ai::ProviderConfig::anthropic(&m)).unwrap_or_default(),
            };
            let mut agent = ezyx_ai::Agent::new(cfg);
            let mut hist = History::default();
            let reply = {
                let mut host = ezyx_project::agent_host::ProjectHost {
                    project: &mut p,
                    history: &mut hist,
                    library: &lib,
                    confirm: Some(Box::new(|name, input| confirm_stdin(name, input))),
                    events: vec![],
                    extra: None,
                };
                agent.run_turn(ezyx_ai::ChatMessage::user(prompt.join(" ")), &mut host)?
            };
            println!("{reply}");
            p.save(&project).context("saving project")?;
            println!("(project saved to {})", project.display());
        }
    }
    Ok(())
}

fn confirm_stdin(name: &str, input: &serde_json::Value) -> bool {
    use std::io::Write;
    print!("Assistant wants to run '{name}' with {input}. Allow? [y/N] ");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    s.trim().eq_ignore_ascii_case("y")
}

fn demo_project(text: &str) -> Result<Project> {
    ezyx_project::demo::sign(text).map_err(|e| anyhow!(e))
}

/// Orthographic top-down render with material albedo and a low sun for relief shading.
fn render_png(sim: &ezyx_sim::StockSim, mat: &ezyx_materials::Material, out: &Path) -> Result<()> {
    let hm = &sim.hm;
    let mut img = image::RgbImage::new(hm.width as u32, hm.height as u32);
    let light = [-0.45f32, 0.55, 0.70];
    let ll = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
    for y in 0..hm.height {
        for x in 0..hm.width {
            let n = hm.normal(x, y);
            let c = hm.cell_center(x, y);
            let a = mat.albedo([c.x as f32, c.y as f32, hm.get(x, y)], n, sim.cut[hm.idx(x, y)]);
            let lambert = ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / ll).max(0.0);
            let shade = 0.35 + 0.75 * lambert;
            let px = image::Rgb([(a[0] * shade * 255.0).min(255.0) as u8, (a[1] * shade * 255.0).min(255.0) as u8, (a[2] * shade * 255.0).min(255.0) as u8]);
            img.put_pixel(x as u32, (hm.height - 1 - y) as u32, px);
        }
    }
    img.save(out)?;
    Ok(())
}
