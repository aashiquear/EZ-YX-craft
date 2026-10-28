# Contributing to EZ-YX-craft

Thanks for helping! Start with `CLAUDE.md` (conventions, crate map) and `docs/ARCHITECTURE.md`.

* **Build & test:** `cargo test --workspace` must pass; add numeric tests for new algorithms.
* **Format & lint:** `cargo fmt --all` and `cargo clippy --workspace --all-targets`.
* **Units:** millimetres internally, always.
* **Mutations:** add a `ezyx_project::Command` variant for anything that edits a project.
* **Machine safety:** anything that moves hardware needs an explicit user confirmation path.
* **Posts:** prefer adding a JSON `PostConfig`; include a sample of real controller output in the PR.
* **Data:** only contribute tool/material data you have the right to share; cite the source.
* **Commits:** small, focused, imperative subject lines. PRs describe what was tested on which machine.

## User manual

The user manual (`docs/manual/`, an [mdBook](https://rust-lang.github.io/mdBook/)) is generated from
the app and kept current by CI. Please don't hand-edit generated parts; change their source instead.

**The feature registry is the source of truth.** `ezyx docs-registry` writes
`docs/manual/registry.json`: every user-facing feature (id, module, title, summary, how-to steps,
`since` version, chapter, screenshots) and every screenshot the GUI can take. The GUI's buttons and
tooltips come from the same entries, so adding a feature to the app means adding a registry entry.

**Adding or changing a feature**

1. Add or update its registry entry in the app (and a `shots` entry if it deserves a screenshot).
2. Regenerate the registry and the manual scaffold, then commit everything under `docs/manual/`:
   ```bash
   cargo run -q -p ezyx-cli --release -- docs-registry -o docs/manual/registry.json
   python3 tools/docs/update_manual.py scaffold   # new chapters, marker blocks, SUMMARY, reference pages
   python3 tools/docs/update_manual.py check      # what CI runs
   ```
3. Optionally write the prose yourself. Each feature's text lives between
   `<!-- feature:<id> -->` and `<!-- /feature:<id> -->`; text outside markers is hand-written and is
   always preserved. Follow `tools/docs/STYLE.md`.

`scaffold` is deterministic and idempotent; it never rewrites an existing feature block.

**Screenshots locally:** `tools/docs/render_shots.sh` builds the app and CLI, exports the registry,
renders `docs/manual/src/img/cli_sim_preview.png` headlessly (`ezyx sim`), and runs the GUI under
`xvfb-run` with `EZYX_DOC_SHOTS` to save one `<shot id>.png` per registry shot (needs `xvfb`; use
`--no-gui` for the CLI image only). Missing screenshots fall back to the CLI image and are listed
in `docs/manual/src/img/placeholders.json`.

**LLM revision:** `python3 tools/docs/update_manual.py revise` finds what changed since the revision
recorded in `docs/manual/.manual-state.json` (new or changed registry entries, removed features,
and commits under source paths mapped to modules in `tools/docs/config.json`) and asks Claude to
rewrite each affected chapter, keeping markers and hand-written text. A revision is written only
if it passes validation (markers intact, no missing images or links, no dropped features).
Use `plan` (or `revise --dry-run`) to see what would be revised; without `ANTHROPIC_API_KEY` it only
reports. `EZYX_DOCS_MODEL` overrides the model. `all` runs scaffold, revise, scaffold, check.
Tests: `python3 -m unittest discover tools/docs`.

**What CI does** (`.github/workflows/docs.yml`):

* **Pull requests:** builds the CLI, exports the registry, runs `scaffold` and `check`, and fails
  if that changes anything the PR didn't commit (except `changelog.md`). The fix is step 2 above.
* **Pushes to `main`:** renders screenshots, runs `update_manual.py all` with the
  `ANTHROPIC_API_KEY` secret, builds the book and opens or refreshes the
  **"docs: update user manual"** PR on branch `docs/manual-update`. Review it like any other PR.
* **Pages:** the committed manual on `main` is published to GitHub Pages.

By contributing you agree your work is dual-licensed under MIT OR Apache-2.0.
