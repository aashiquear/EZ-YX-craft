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

By contributing you agree your work is dual-licensed under MIT OR Apache-2.0.
