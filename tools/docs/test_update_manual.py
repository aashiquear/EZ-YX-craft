"""Unit tests for update_manual.py (no network, no git needed).

Run: python3 -m unittest discover tools/docs
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import update_manual as um  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

FAKE_REGISTRY = {
    "app_version": "9.9.0",
    "modules": [
        {"id": "pipeline", "title": "Pipeline", "chapter": "pipeline/index.md", "summary": "Stock, design, toolpaths, output."},
        {"id": "machine", "title": "Machine Control", "chapter": "machine/index.md", "summary": "Run jobs."},
    ],
    "features": [
        {
            "id": "pipeline.stock",
            "module": "pipeline",
            "title": "Stock setup",
            "summary": "Describe your material. It drives every later step.",
            "how_to": ["Open Stock.", "Enter the size."],
            "since": "9.0.0",
            "shots": ["pipeline_stock"],
            "chapter": "pipeline/stock.md",
        },
        {
            "id": "pipeline.overview",
            "module": "pipeline",
            "title": "Pipeline overview",
            "summary": "Four steps in order.",
            "how_to": [],
            "since": "9.0.0",
            "shots": [],
        },
        {
            "id": "machine.jog",
            "module": "machine",
            "title": "Jog the machine",
            "summary": "Move the spindle by hand.",
            "how_to": ["Connect.", "Press the arrows."],
            "since": "9.1.0",
            "shots": ["machine_jog"],
            "chapter": "machine/jog.md",
        },
    ],
    "shots": [
        {"id": "pipeline_stock", "workspace": "pipeline", "step": "stock", "caption": "Stock step", "size": [1480, 920]},
        {"id": "machine_jog", "workspace": "machine", "step": "jog", "caption": "Jog pad", "size": [1480, 920]},
    ],
}


def snapshot(root: Path) -> dict[str, bytes]:
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in sorted(root.rglob("*")) if p.is_file()}


class TempManual(unittest.TestCase):
    """A throw-away repository root with a manual and a fake registry; git is stubbed out."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="ezyx-manual-test-"))
        (self.tmp / "docs/manual/src/img").mkdir(parents=True)
        self.write_registry(FAKE_REGISTRY)
        self.src("introduction.md").write_text("# Introduction\n\nHand-written.\n")
        self.src("getting-started.md").write_text("# Getting started\n\nHand-written.\n")
        self.git_patch = mock.patch.object(um, "git", return_value=None)
        self.git_patch.start()
        self.env_patch = mock.patch.dict(os.environ, {"ANTHROPIC_API_KEY": "", "ANTHROPIC_AUTH_TOKEN": "", "GITHUB_ACTIONS": ""})
        self.env_patch.start()

    def tearDown(self) -> None:
        self.git_patch.stop()
        self.env_patch.stop()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def ctx(self) -> um.Ctx:
        return um.make_ctx(self.tmp, quiet=True)

    def write_registry(self, data: dict) -> None:
        (self.tmp / "docs/manual/registry.json").write_text(json.dumps(data, indent=2), encoding="utf-8")

    def src(self, rel: str) -> Path:
        return self.tmp / "docs/manual/src" / rel

    def render_shots(self) -> None:
        for sid in ("pipeline_stock", "machine_jog"):
            self.src(f"img/{sid}.png").write_bytes(b"\x89PNG fake " + sid.encode())


# --------------------------------------------------------------------------------------


class MarkerTests(unittest.TestCase):
    TEXT = "# T\n\nintro\n\n<!-- feature:a.b -->\nA body\n<!-- /feature:a.b -->\n\nmiddle\n\n<!--feature:c-d-->\nC\n<!--  /feature:c-d  -->\n"

    def test_parse_blocks(self) -> None:
        blocks, errs = um.parse_blocks(self.TEXT)
        self.assertEqual(errs, [])
        self.assertEqual([b.id for b in blocks], ["a.b", "c-d"])
        self.assertEqual(blocks[0].body(self.TEXT).strip(), "A body")
        self.assertEqual(blocks[1].body(self.TEXT).strip(), "C")

    def test_errors(self) -> None:
        cases = {
            "<!-- feature:x -->\nno end\n": "never closed",
            "<!-- /feature:x -->\n": "without an opening",
            "<!-- feature:x -->\n<!-- feature:y -->\n<!-- /feature:y -->\n<!-- /feature:x -->\n": "inside unclosed",
            "<!-- feature:x -->\n<!-- /feature:y -->\n": "does not match",
            "<!-- feature:x -->a<!-- /feature:x -->\n<!-- feature:x -->b<!-- /feature:x -->\n": "duplicate",
        }
        for text, needle in cases.items():
            _, errs = um.parse_blocks(text)
            self.assertTrue(any(needle in e for _, e in errs), (text, errs))

    def test_replace_block_preserves_outside_text(self) -> None:
        new = um.replace_block_body(self.TEXT, "a.b", "New body\n\nmore")
        self.assertIn("<!-- feature:a.b -->\nNew body\n\nmore\n<!-- /feature:a.b -->", new)
        self.assertIn("intro", new)
        self.assertIn("middle", new)
        self.assertIn("<!--feature:c-d-->\nC\n", new)
        with self.assertRaises(KeyError):
            um.replace_block_body(self.TEXT, "zzz", "x")

    def test_outside_blocks(self) -> None:
        blocks, _ = um.parse_blocks(self.TEXT)
        out = um.outside_blocks(self.TEXT, blocks)
        self.assertNotIn("A body", out)
        self.assertNotIn("feature:", out)
        self.assertIn("middle", out)

    def test_markers_and_images_in_code_are_ignored_for_images(self) -> None:
        text = "```markdown\n![x](missing.png)\n```\n\n`![y](missing2.png)`\n\n![z](img/real.png)\n"
        self.assertEqual([u for _, u in um.image_refs(text)], ["img/real.png"])


class GlobAndMappingTests(unittest.TestCase):
    def test_glob_to_regex(self) -> None:
        rx = um.glob_to_regex
        self.assertTrue(rx("crates/ezyx-app/src/solid/**").match("crates/ezyx-app/src/solid/a/b.rs"))
        self.assertTrue(rx("crates/ezyx-app/src/solid/**").match("crates/ezyx-app/src/solid/mod.rs"))
        self.assertFalse(rx("crates/ezyx-app/src/solid/**").match("crates/ezyx-app/src/solidity.rs"))
        self.assertTrue(rx("**/tests/**").match("tests/x.rs"))
        self.assertTrue(rx("**/tests/**").match("crates/a/tests/data/x.nc"))
        self.assertTrue(rx("**/*.md").match("README.md"))
        self.assertTrue(rx("**/*.md").match("docs/a/b.md"))
        self.assertFalse(rx("crates/*.rs").match("crates/a/b.rs"))
        self.assertTrue(rx("crates/?.rs").match("crates/a.rs"))

    def test_first_match_wins(self) -> None:
        rules = [{"glob": "a/special/**", "module": "x"}, {"glob": "a/**", "module": "y"}, {"glob": "a/ignored/**", "module": None}]
        self.assertEqual(um.module_for_path("a/special/f.rs", rules), "x")
        self.assertEqual(um.module_for_path("a/other.rs", rules), "y")
        self.assertEqual(um.module_for_path("./a/other.rs", rules), "y")
        self.assertEqual(um.module_for_path("a/ignored/f.rs", rules), "y")  # earlier rule wins
        self.assertIsNone(um.module_for_path("b/f.rs", rules))

    def test_repository_config(self) -> None:
        rules = um.load_config(REPO_ROOT)["path_modules"]
        expect = {
            "crates/ezyx-app/src/solid/sketch.rs": "solid",
            "crates/ezyx-app/src/panels/machine.rs": "machine",
            "crates/ezyx-app/src/panels/ai.rs": "assistant",
            "crates/ezyx-app/src/app.rs": "pipeline",
            "crates/ezyx-machine/src/grbl.rs": "machine",
            "crates/ezyx-project/src/agent_host.rs": "assistant",
            "crates/ezyx-project/src/ops.rs": "pipeline",
            "crates/ezyx-cad/src/lib.rs": "solid",
            "crates/ezyx-image/src/trace.rs": "image",
            "crates/ezyx-cam/tests/vcarve.rs": None,
            "docs/manual/src/pipeline/stock.md": None,
            "crates/ezyx-cam/README.md": None,
            "Cargo.lock": None,
        }
        for path, module in expect.items():
            self.assertEqual(um.module_for_path(path, rules), module, path)
        grouped = um.modules_for_paths(["crates/ezyx-machine/a.rs", "crates/ezyx-ai/b.rs", "crates/ezyx-machine/c.rs", "x.txt"], rules)
        self.assertEqual(grouped, {"machine": ["crates/ezyx-machine/a.rs", "crates/ezyx-machine/c.rs"], "assistant": ["crates/ezyx-ai/b.rs"]})


class RegistryTests(unittest.TestCase):
    def test_valid_fake_registry(self) -> None:
        reg, errs = um.parse_registry(FAKE_REGISTRY)
        self.assertEqual(errs, [])
        self.assertEqual(reg.chapters(), ["pipeline/index.md", "pipeline/stock.md", "machine/index.md", "machine/jog.md"])
        self.assertEqual([f["id"] for f in reg.features_in_chapter("pipeline/index.md")], ["pipeline.overview"])
        self.assertEqual(reg.modules_of_chapter("machine/jog.md"), ["machine"])

    def test_registry_errors(self) -> None:
        bad = json.loads(json.dumps(FAKE_REGISTRY))
        bad["features"].append(dict(bad["features"][0]))  # duplicate id
        bad["features"][1]["module"] = "nope"
        bad["features"][2]["shots"] = ["missing_shot"]
        bad["modules"][0]["chapter"] = "../escape.md"
        _, errs = um.parse_registry(bad)
        joined = "\n".join(errs)
        for needle in ("duplicate feature id", "unknown module", "unknown shot", "'..'"):
            self.assertIn(needle, joined)

    def test_feature_hash_tracks_shot_changes(self) -> None:
        reg, _ = um.parse_registry(FAKE_REGISTRY)
        h1 = reg.feature_hash(reg.feature_by_id["pipeline.stock"])
        changed = json.loads(json.dumps(FAKE_REGISTRY))
        changed["shots"][0]["caption"] = "A different caption"
        reg2, _ = um.parse_registry(changed)
        self.assertNotEqual(h1, reg2.feature_hash(reg2.feature_by_id["pipeline.stock"]))
        self.assertEqual(reg.feature_hash(reg.feature_by_id["machine.jog"]), reg2.feature_hash(reg2.feature_by_id["machine.jog"]))


class ScaffoldTests(TempManual):
    def test_scaffold_creates_everything_and_is_idempotent(self) -> None:
        self.assertEqual(um.scaffold(self.ctx()), 0)
        first = snapshot(self.tmp)
        for rel in ("SUMMARY.md", "pipeline/index.md", "pipeline/stock.md", "machine/index.md", "machine/jog.md", "reference/features.md", "changelog.md"):
            self.assertIn(f"docs/manual/src/{rel}", first, rel)
        stock = self.src("pipeline/stock.md").read_text()
        self.assertTrue(stock.startswith("# Stock setup\n"))
        self.assertIn("<!-- feature:pipeline.stock -->", stock)
        self.assertIn("1. Open Stock.\n2. Enter the size.", stock)
        self.assertIn("![Stock step](../img/pipeline_stock.png)", stock)
        self.assertNotIn("## Stock setup", stock)  # the feature is the chapter: no duplicate heading
        index = self.src("pipeline/index.md").read_text()
        self.assertIn("## Pipeline overview", index)
        summary = self.src("SUMMARY.md").read_text()
        self.assertIn("- [Pipeline](pipeline/index.md)\n  - [Stock setup](pipeline/stock.md)\n- [Machine Control](machine/index.md)\n  - [Jog the machine](machine/jog.md)", summary)
        self.assertEqual(summary.count("reference/features.md"), 1)
        features = self.src("reference/features.md").read_text()
        self.assertIn("| [Jog the machine](../machine/jog.md) | Move the spindle by hand. | 9.1.0 |", features)
        self.assertIn("[Pipeline overview](../pipeline/index.md#pipeline-overview)", features)
        self.assertIn("| Describe your material. |", features)  # first sentence only

        self.assertEqual(um.scaffold(self.ctx()), 0)
        self.assertEqual(snapshot(self.tmp), first, "second scaffold run changed files")

    def test_scaffold_preserves_hand_written_text_and_adds_new_blocks(self) -> None:
        um.scaffold(self.ctx())
        path = self.src("pipeline/index.md")
        text = path.read_text().replace("# Pipeline\n", "# Pipeline\n\nHAND WRITTEN INTRO.\n") + "\n## Troubleshooting\n\nHAND WRITTEN END.\n"
        text = um.replace_block_body(text, "pipeline.overview", "## Pipeline overview\n\nRevised prose.")
        path.write_text(text)
        reg = json.loads(json.dumps(FAKE_REGISTRY))
        reg["features"].append({"id": "pipeline.output", "module": "pipeline", "title": "Save G-code", "summary": "Write the file.", "since": "9.2.0", "shots": []})
        self.write_registry(reg)
        self.assertEqual(um.scaffold(self.ctx()), 0)
        new = path.read_text()
        self.assertIn("HAND WRITTEN INTRO.", new)
        self.assertIn("HAND WRITTEN END.", new)
        self.assertIn("Revised prose.", new)  # existing blocks are never regenerated
        # The new block goes right after the last existing block, before hand-written trailing sections.
        self.assertLess(new.index("<!-- /feature:pipeline.overview -->"), new.index("<!-- feature:pipeline.output -->"))
        self.assertLess(new.index("<!-- /feature:pipeline.output -->"), new.index("## Troubleshooting"))
        self.assertIn("Save G-code", self.src("reference/features.md").read_text())
        before = snapshot(self.tmp)
        um.scaffold(self.ctx())
        self.assertEqual(snapshot(self.tmp), before)

    def test_summary_respects_hand_written_entries(self) -> None:
        self.src("SUMMARY.md").write_text(
            "# Summary\n\n[Introduction](introduction.md)\n\n- [Machines](machine/index.md)\n\n# Modules\n\n" + um.SUMMARY_BEGIN + "\n" + um.SUMMARY_END + "\n"
        )
        self.src("introduction.md").write_text("# Introduction\n")
        um.scaffold(self.ctx())
        summary = self.src("SUMMARY.md").read_text()
        self.assertEqual(summary.count("machine/index.md"), 1)
        self.assertIn("- [Jog the machine](machine/jog.md)", summary)  # child emitted at top level of the region


class CheckTests(TempManual):
    def test_check_passes_after_scaffold_with_unrendered_shot_warnings(self) -> None:
        um.scaffold(self.ctx())
        ctx = self.ctx()
        self.assertEqual(um.check(ctx), 0)
        self.assertTrue(any("has not been rendered" in w for w in ctx.log.warnings))
        self.assertEqual(um.check(self.ctx(), strict_images=True), 1)
        self.render_shots()
        ctx = self.ctx()
        self.assertEqual(um.check(ctx, strict=True), 0, ctx.log.errors)

    def test_check_errors(self) -> None:
        um.scaffold(self.ctx())
        self.render_shots()
        jog = self.src("machine/jog.md")
        jog.write_text("# Jog the machine\n\nNo markers here. ![x](../img/nothere.png) [see](../nowhere.md)\n")
        self.src("SUMMARY.md").write_text(self.src("SUMMARY.md").read_text().replace("  - [Stock setup](pipeline/stock.md)\n", ""))
        ctx = self.ctx()
        self.assertEqual(um.check(ctx), 1)
        joined = "\n".join(ctx.log.errors)
        self.assertIn("'machine.jog' has no", joined)
        self.assertIn("'../img/nothere.png' does not exist", joined)
        self.assertIn("missing file '../nowhere.md'", joined)
        self.assertIn("'pipeline/stock.md' is not listed in SUMMARY.md", joined)

    def test_stale_blocks_are_reported(self) -> None:
        um.scaffold(self.ctx())
        self.render_shots()
        reg = json.loads(json.dumps(FAKE_REGISTRY))
        reg["features"] = [f for f in reg["features"] if f["id"] != "pipeline.overview"]
        self.write_registry(reg)
        ctx = self.ctx()
        self.assertEqual(um.check(ctx), 0)
        self.assertTrue(any("stale feature block 'pipeline.overview'" in w for w in ctx.log.warnings))
        self.assertEqual(um.check(self.ctx(), strict=True), 1)

    def test_placeholders(self) -> None:
        um.scaffold(self.ctx())
        fallback = self.src("img/cli_sim_preview.png")
        fallback.write_bytes(b"\x89PNG fallback")
        self.src("img/machine_jog.png").write_bytes(b"\x89PNG real jog shot")
        self.assertEqual(um.placeholders(self.ctx(), fallback), 0)
        self.assertEqual(self.src("img/pipeline_stock.png").read_bytes(), b"\x89PNG fallback")
        self.assertEqual(self.src("img/machine_jog.png").read_bytes(), b"\x89PNG real jog shot")
        self.assertEqual(um.load_placeholders(self.ctx()), {"pipeline_stock"})
        ctx = self.ctx()
        self.assertEqual(um.check(ctx), 0)
        self.assertTrue(any("placeholder" in w for w in ctx.log.warnings))
        # A real render replaces the placeholder; the next run forgets it.
        self.src("img/pipeline_stock.png").write_bytes(b"\x89PNG real stock shot")
        um.placeholders(self.ctx(), fallback)
        self.assertEqual(um.load_placeholders(self.ctx()), set())
        self.assertFalse(self.src("img/placeholders.json").exists())


class ValidationTests(TempManual):
    def setUp(self) -> None:
        super().setUp()
        um.scaffold(self.ctx())
        self.render_shots()
        self.chapter = self.src("pipeline/index.md")
        self.original = self.chapter.read_text()

    def validate(self, revised: str, required=("pipeline.overview",), removed=(), pending=()):
        return um.validate_revision(
            self.original,
            revised,
            chapter_file=self.chapter,
            src_dir=self.tmp / "docs/manual/src",
            required_ids=list(required),
            removed_ids=list(removed),
            pending_images=pending,
        )

    def good(self) -> str:
        return (
            "# Pipeline\n\nEvery job goes through four steps.\n\n<!-- feature:pipeline.overview -->\n## Pipeline overview\n\n"
            "Work left to right.\n\n![Stock](../img/pipeline_stock.png)\n<!-- /feature:pipeline.overview -->\n"
        )

    def test_good_revision(self) -> None:
        errs, warns = self.validate(self.good())
        self.assertEqual(errs, [])

    def test_dropped_feature(self) -> None:
        bad = "# Pipeline\n\nAll prose, no markers. " + "x" * 400 + "\n"
        errs, _ = self.validate(bad)
        self.assertTrue(any("'pipeline.overview' is missing" in e for e in errs), errs)

    def test_broken_or_invented_markers(self) -> None:
        errs, _ = self.validate(self.good().replace("<!-- /feature:pipeline.overview -->", ""))
        self.assertTrue(any("never closed" in e for e in errs), errs)
        errs, _ = self.validate(self.good() + "\n<!-- feature:pipeline.invented -->\nx\n<!-- /feature:pipeline.invented -->\n")
        self.assertTrue(any("unknown feature block 'pipeline.invented'" in e for e in errs), errs)

    def test_images(self) -> None:
        errs, _ = self.validate(self.good().replace("pipeline_stock.png", "made_up.png"))
        self.assertTrue(any("made_up.png" in e for e in errs), errs)
        errs, _ = self.validate(self.good().replace("../img/pipeline_stock.png", "https://example.com/a.png"))
        self.assertTrue(any("external image" in e for e in errs), errs)
        pending = self.src("img/future_shot.png")
        errs, _ = self.validate(self.good().replace("pipeline_stock.png", "future_shot.png"), pending=[pending])
        self.assertEqual(errs, [])

    def test_lost_title_and_truncation(self) -> None:
        errs, _ = self.validate(self.good().replace("# Pipeline\n", ""))
        self.assertTrue(any("title" in e for e in errs), errs)
        self.original = self.original + "\n" + ("Long hand-written paragraph. " * 60) + "\n"
        errs, _ = self.validate(self.good())
        self.assertTrue(any("much shorter" in e for e in errs), errs)

    def test_removed_hand_written_section_warns(self) -> None:
        self.original += "\n## Troubleshooting\n\nIf it fails, retry.\n"
        _, warns = self.validate(self.good())
        self.assertTrue(any("Troubleshooting" in w for w in warns), warns)

    def test_extract_revision(self) -> None:
        self.assertIsNone(um.extract_revision("<unchanged/>"))
        self.assertEqual(um.extract_revision("Sure!\n<revised_chapter>\n# A\n\ntext\n</revised_chapter>\n"), "# A\n\ntext\n")
        self.assertEqual(um.extract_revision("<revised_chapter>\n```markdown\n# A\n```\n</revised_chapter>"), "# A\n")
        with self.assertRaises(ValueError):
            um.extract_revision("Here is the chapter: # A")


class ReviseTests(TempManual):
    def setUp(self) -> None:
        super().setUp()
        um.scaffold(self.ctx())
        self.render_shots()
        self.calls: list[str] = []

    def fake_call(self, revise_text=None):
        def call(system, user, settings):
            self.calls.append(user)
            self.assertEqual(system[-1]["cache_control"], {"type": "ephemeral"})
            self.assertEqual(settings["model"], "claude-opus-5-5")
            original = user.split("<current_chapter>\n", 1)[1].split("\n</current_chapter>", 1)[0] + "\n"
            new = revise_text(original) if revise_text else original.replace("\n\n", "\n\nRevised.\n\n", 1)
            return um.ModelResult(f"<revised_chapter>\n{new}</revised_chapter>", "end_turn", {"input_tokens": 1})

        return call

    def test_without_api_key_only_reports(self) -> None:
        ctx = self.ctx()
        self.assertEqual(um.revise(ctx), 0)
        self.assertFalse(ctx.state_path.exists())

    def test_dry_run_makes_no_calls(self) -> None:
        ctx = um.make_ctx(self.tmp, dry_run=True, quiet=True)
        self.assertEqual(um.revise(ctx, call=self.fake_call()), 0)
        self.assertEqual(self.calls, [])

    def test_revise_writes_chapters_and_state_then_tracks_changes(self) -> None:
        self.assertEqual(um.revise(self.ctx(), call=self.fake_call()), 0)
        self.assertEqual(len(self.calls), 4)  # first revision: every chapter
        self.assertIn("Revised.", self.src("pipeline/stock.md").read_text())
        state = json.loads((self.tmp / "docs/manual/.manual-state.json").read_text())
        self.assertEqual(set(state["features"]), {"pipeline.stock", "pipeline.overview", "machine.jog"})
        self.assertEqual(state["app_version"], "9.9.0")

        # Nothing changed -> nothing to revise.
        self.calls.clear()
        self.assertEqual(um.plan_revisions(self.ctx(), um.load_registry(self.ctx())[0], um.load_state(self.ctx())), [])

        # A changed registry entry and a new feature are detected per chapter.
        reg = json.loads(json.dumps(FAKE_REGISTRY))
        reg["features"][2]["summary"] = "Move the spindle by hand, in steps of 0.1 mm to 10 mm."
        reg["features"].append({"id": "machine.probe", "module": "machine", "title": "Probe Z", "summary": "Find the top.", "since": "9.9.0", "shots": [], "chapter": "machine/jog.md"})
        self.write_registry(reg)
        um.scaffold(self.ctx())
        plans = um.plan_revisions(self.ctx(), um.load_registry(self.ctx())[0], um.load_state(self.ctx()))
        self.assertEqual([p.chapter for p in plans], ["machine/jog.md"])
        self.assertEqual(plans[0].changed_ids, ["machine.jog"])
        self.assertEqual(plans[0].new_ids, ["machine.probe"])
        self.assertEqual(um.revise(self.ctx(), call=self.fake_call()), 0)
        self.assertEqual(len(self.calls), 1)
        self.assertIn("CHANGED", self.calls[0])
        self.assertIn("Previous registry entry", self.calls[0])
        self.assertIn("Move the spindle by hand.", self.calls[0])  # previous summary shown

    def test_invalid_response_keeps_old_chapter(self) -> None:
        before = self.src("machine/jog.md").read_text()
        ctx = self.ctx()
        rc = um.revise(ctx, chapters=["machine/jog.md"], call=self.fake_call(lambda orig: "# Jog the machine\n\nMarkers gone.\n"))
        self.assertEqual(rc, 0)
        self.assertEqual(self.src("machine/jog.md").read_text(), before)
        self.assertTrue(any("rejected revision" in w for w in ctx.log.warnings))
        self.assertFalse(ctx.state_path.exists())  # nothing succeeded -> no new revision recorded

    def test_partial_run_keeps_other_chapters_pending(self) -> None:
        self.assertEqual(um.revise(self.ctx(), chapters=["pipeline/stock.md"], call=self.fake_call()), 0)
        state = um.load_state(self.ctx())
        self.assertIn("pipeline.stock", state["features"])
        self.assertNotIn("machine.jog", state["features"])  # still "new" for the next run
        plans = um.plan_revisions(self.ctx(), um.load_registry(self.ctx())[0], state)
        self.assertNotIn("pipeline/stock.md", [p.chapter for p in plans])
        self.assertIn("machine/jog.md", [p.chapter for p in plans])

    def test_api_failure_everywhere_is_an_error(self) -> None:
        def boom(system, user, settings):
            raise um.ModelError("401 invalid x-api-key")

        self.assertEqual(um.revise(self.ctx(), call=boom), 3)

    def test_stop_reason_max_tokens_is_rejected(self) -> None:
        before = self.src("machine/jog.md").read_text()
        call = lambda s, u, st: um.ModelResult("<revised_chapter>\n# Jog\n", "max_tokens", {})  # noqa: E731
        um.revise(self.ctx(), chapters=["machine/jog.md"], call=call)
        self.assertEqual(self.src("machine/jog.md").read_text(), before)


class ChangelogTests(unittest.TestCase):
    def test_conventional_grouping(self) -> None:
        commits = [
            um.Commit("1" * 40, "1111111", "feat(solid): add fillets", ["crates/ezyx-cad/a.rs"]),
            um.Commit("2" * 40, "2222222", "fix!: correct V-bit depth", ["crates/ezyx-cam/a.rs"]),
            um.Commit("3" * 40, "3333333", "ci: cache cargo", [".github/workflows/ci.yml"]),
            um.Commit("4" * 40, "4444444", "Tidy up the ribbon", ["crates/ezyx-app/a.rs"]),
        ]
        lines = um.render_change_groups(commits, {"ci"})
        text = "\n".join(lines)
        self.assertIn("### New features\n\n- **solid:** add fillets (`1111111`)", text)
        self.assertIn("- correct V-bit depth (breaking change) (`2222222`)", text)
        self.assertNotIn("cache cargo", text)
        self.assertIn("### Other changes\n\n- Tidy up the ribbon (`4444444`)", text)
        self.assertLess(text.index("New features"), text.index("Fixes"))

    def test_docs_only_commits(self) -> None:
        ctx = um.make_ctx(REPO_ROOT, quiet=True)
        self.assertTrue(um.docs_only(um.Commit("a", "a", "docs: update user manual", ["docs/manual/src/x.md", "docs/manual/.manual-state.json"]), ctx))
        self.assertFalse(um.docs_only(um.Commit("a", "a", "feat: x", ["docs/manual/src/x.md", "crates/a.rs"]), ctx))

    def test_slugify_matches_mdbook(self) -> None:
        self.assertEqual(um.slugify("Set up the stock"), "set-up-the-stock")
        self.assertEqual(um.slugify("V-carve & `inlays`!"), "v-carve--inlays")


class HttpFallbackTests(unittest.TestCase):
    """The urllib client against a local stand-in server (no external network)."""

    def test_request_shape(self) -> None:
        import http.server
        import threading

        seen: dict = {}

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):  # noqa: N802
                seen["path"] = self.path
                seen["headers"] = {k.lower(): v for k, v in self.headers.items()}
                seen["body"] = json.loads(self.rfile.read(int(self.headers["content-length"])))
                payload = json.dumps(
                    {"content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "<unchanged/>"}], "stop_reason": "end_turn", "usage": {"input_tokens": 5}, "model": "claude-opus-5-5"}
                ).encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def log_message(self, *args):
                pass

        server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            env = {"ANTHROPIC_API_KEY": "test-key", "ANTHROPIC_BASE_URL": f"http://127.0.0.1:{server.server_port}", "NO_PROXY": "127.0.0.1", "no_proxy": "127.0.0.1"}
            with mock.patch.dict(os.environ, env):
                system = [{"type": "text", "text": "style", "cache_control": {"type": "ephemeral"}}]
                settings = {"model": "claude-opus-5-5", "effort": "medium", "max_tokens": 1000, "fallbacks": True}
                res = um._call_urllib(system, "chapter", settings)
        finally:
            server.shutdown()
            server.server_close()
        self.assertEqual(res.text, "<unchanged/>")
        self.assertEqual(res.stop_reason, "end_turn")
        self.assertEqual(seen["path"], "/v1/messages")
        self.assertEqual(seen["headers"]["x-api-key"], "test-key")
        self.assertEqual(seen["headers"]["anthropic-version"], um.API_VERSION)
        self.assertEqual(seen["headers"]["anthropic-beta"], um.FALLBACK_BETA)
        body = seen["body"]
        self.assertEqual(body["model"], "claude-opus-5-5")
        self.assertEqual(body["fallbacks"], "default")
        self.assertEqual(body["output_config"], {"effort": "medium"})
        self.assertEqual(body["system"][0]["cache_control"], {"type": "ephemeral"})
        self.assertEqual(body["messages"], [{"role": "user", "content": "chapter"}])
        self.assertNotIn("temperature", body)

    def test_model_settings_env(self) -> None:
        ctx = um.make_ctx(REPO_ROOT, quiet=True)
        with mock.patch.dict(os.environ, {"EZYX_DOCS_MODEL": "claude-haiku-5-5"}, clear=False):
            st = um.model_settings(ctx)
        self.assertEqual(st["model"], "claude-haiku-5-5")
        self.assertFalse(st["fallbacks"])  # Haiku has no server-side fallback
        with mock.patch.dict(os.environ, {"EZYX_DOCS_MODEL": ""}, clear=False):
            st = um.model_settings(ctx)
        self.assertEqual(st["model"], "claude-opus-5-5")
        self.assertTrue(st["fallbacks"])

    def test_sdk_path_uses_streaming_and_fallbacks(self) -> None:
        import types

        captured: dict = {}

        class Block:
            def __init__(self, type_, text=""):
                self.type, self.text = type_, text

        class Usage:
            def model_dump(self):
                return {"input_tokens": 3, "cache_read_input_tokens": 2}

        class Msg:
            content = [Block("thinking"), Block("text", "<unchanged/>")]
            stop_reason = "end_turn"
            usage = Usage()
            model = "claude-opus-5-5"

        class Stream:
            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

            def get_final_message(self):
                return Msg()

        def stream(**kwargs):
            captured.update(kwargs)
            return Stream()

        class Client:
            def __init__(self, **kw):
                self.beta = types.SimpleNamespace(messages=types.SimpleNamespace(stream=stream))
                self.messages = types.SimpleNamespace(stream=stream)

        class APIStatusError(Exception):
            pass

        class APIConnectionError(Exception):
            pass

        fake = types.SimpleNamespace(Anthropic=Client, APIStatusError=APIStatusError, APIConnectionError=APIConnectionError)
        settings = {"model": "claude-opus-5-5", "effort": "medium", "max_tokens": 1000, "fallbacks": True}
        res = um._call_sdk(fake, [{"type": "text", "text": "s"}], "u", settings)
        self.assertEqual(res.text, "<unchanged/>")
        self.assertEqual(res.usage["cache_read_input_tokens"], 2)
        self.assertEqual(captured["betas"], [um.FALLBACK_BETA])
        self.assertEqual(captured["extra_body"], {"fallbacks": "default"})
        self.assertEqual(captured["output_config"], {"effort": "medium"})
        captured.clear()
        um._call_sdk(fake, [{"type": "text", "text": "s"}], "u", dict(settings, fallbacks=False))
        self.assertNotIn("betas", captured)


@unittest.skipUnless(shutil.which("git"), "git not installed")
class GitIntegrationTests(unittest.TestCase):
    """Source-change detection and the changelog against a real throw-away git repository."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="ezyx-manual-git-"))
        self.env = mock.patch.dict(
            os.environ,
            {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.com", "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.com", "GITHUB_ACTIONS": "", "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"},
        )
        self.env.start()
        self.git("init", "-q", "-b", "main")
        (self.tmp / "tools/docs").mkdir(parents=True)
        shutil.copy(REPO_ROOT / "tools/docs/config.json", self.tmp / "tools/docs/config.json")
        (self.tmp / "docs/manual/src/img").mkdir(parents=True)
        (self.tmp / "docs/manual/registry.json").write_text(json.dumps(FAKE_REGISTRY))
        for rel in ("introduction.md", "getting-started.md"):
            (self.tmp / "docs/manual/src" / rel).write_text("# Page\n")
        self.touch("crates/ezyx-machine/src/grbl.rs", "// v1\n")
        self.commit("chore: initial import")

    def tearDown(self) -> None:
        self.env.stop()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def git(self, *args: str) -> str:
        import subprocess

        return subprocess.run(["git", "-C", str(self.tmp), *args], check=True, capture_output=True, text=True).stdout

    def touch(self, rel: str, text: str) -> None:
        p = self.tmp / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def commit(self, msg: str) -> None:
        self.git("add", "-A")
        self.git("commit", "-q", "-m", msg)

    def ctx(self) -> um.Ctx:
        return um.make_ctx(self.tmp, quiet=True)

    def test_source_changes_trigger_module_chapters_and_changelog(self) -> None:
        um.scaffold(self.ctx())
        self.commit("docs: scaffold manual")
        echo = lambda s, u, st: um.ModelResult("<unchanged/>", "end_turn", {})  # noqa: E731
        self.assertEqual(um.revise(self.ctx(), call=echo), 0)
        state = um.load_state(self.ctx())
        self.assertEqual(state["commit"], self.git("rev-parse", "HEAD").strip())
        self.commit("docs: update user manual")  # the bot's own commit: docs-only, never in the changelog

        self.touch("crates/ezyx-machine/src/grbl.rs", "// v2: new jog increments\n")
        self.commit("feat(machine): finer jog increments")
        self.touch("crates/ezyx-machine/tests/jog.rs", "// test\n")
        self.commit("test(machine): cover jog")  # ignored commit type
        self.touch("crates/ezyx-cam/src/vcarve.rs", "// cam\n")
        self.commit("fix(cam): v-carve depth")

        reg = um.load_registry(self.ctx())[0]
        plans = um.plan_revisions(self.ctx(), reg, um.load_state(self.ctx()))
        by_chapter = {p.chapter: p for p in plans}
        self.assertEqual(sorted(by_chapter), ["machine/index.md", "machine/jog.md", "pipeline/index.md", "pipeline/stock.md"])
        jog = by_chapter["machine/jog.md"]
        self.assertEqual([c.subject for c in jog.module_commits["machine"]], ["feat(machine): finer jog increments"])
        self.assertEqual(jog.module_files["machine"], ["crates/ezyx-machine/src/grbl.rs"])
        prompt = um.build_user_prompt(self.ctx(), reg, jog, um.load_state(self.ctx()))
        self.assertIn("finer jog increments", prompt)
        self.assertIn("+// v2: new jog increments", prompt)  # condensed diff included
        self.assertNotIn("v-carve depth", prompt)

        um.scaffold(self.ctx())
        changelog = (self.tmp / "docs/manual/src/changelog.md").read_text()
        self.assertIn("### New features\n\n- **machine:** finer jog increments", changelog)
        self.assertIn("### Fixes\n\n- **cam:** v-carve depth", changelog)
        self.assertNotIn("docs: update user manual", changelog)

        # A successful revision freezes those entries into a permanent section.
        self.assertEqual(um.revise(self.ctx(), call=echo), 0)
        um.scaffold(self.ctx())
        changelog = (self.tmp / "docs/manual/src/changelog.md").read_text()
        self.assertIn("## Manual revision ", changelog)
        self.assertIn("_No changes since the last manual revision._", changelog)
        self.assertEqual(changelog.count("finer jog increments"), 1)
        self.assertEqual(um.plan_revisions(self.ctx(), reg, um.load_state(self.ctx())), [])


if __name__ == "__main__":
    unittest.main()
