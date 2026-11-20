from __future__ import annotations
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CI_WORKFLOW = ROOT / ".github/workflows/ci.yml"
OFFICIAL_WORKFLOW = ROOT / ".github/workflows/official-framework-benchmarks.yml"
PATCHER = ROOT / "scripts/patch-js-framework-benchmark.py"
AGGREGATOR = ROOT / "scripts/summarize-js-framework-benchmark.py"


class OfficialFrameworkWorkflowTests(unittest.TestCase):
    def test_pull_request_ci_keeps_fast_and_rustflow_owned_gates_only(self) -> None:
        ci = CI_WORKFLOW.read_text(encoding="utf-8")
        self.assertNotIn("Run pinned upstream keyed framework comparison", ci)
        self.assertNotIn("official_framework_comparison", ci)
        self.assertNotIn("fa510a76ffe066af1d30f3e75fa51d3fcbc14009", ci)
        self.assertIn("Run Clippy", ci)
        self.assertIn("Run tests", ci)
        self.assertIn("Compile the DOM adapter for WebAssembly", ci)
        self.assertIn("Run browser end-to-end tests", ci)
        self.assertIn("npm run check:bundle", ci)
        self.assertIn("Controlled same-runner keyed reconciliation A/B/A", ci)
        self.assertIn("Run browser DOM and initialization benchmarks", ci)
        self.assertIn("python3 -m unittest discover -s scripts/tests -v", ci)

    def test_official_workflow_is_manual_and_scheduled_only(self) -> None:
        workflow = OFFICIAL_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("schedule:", workflow)
        self.assertNotIn("pull_request:", workflow)
        self.assertIn("ref: ${{ github.event.inputs.rustflow_commit || github.sha }}", workflow)
        self.assertIn("REQUESTED_SHA: ${{ github.event.inputs.rustflow_commit }}", workflow)
        self.assertIn("RUSTFLOW_EXPECTED_COMMIT", workflow)
        self.assertIn("git rev-parse HEAD", workflow)
        self.assertNotIn("download-artifact", workflow)

    def test_workload_and_sample_counts_are_preserved(self) -> None:
        workflow = OFFICIAL_WORKFLOW.read_text(encoding="utf-8")
        patcher = PATCHER.read_text(encoding="utf-8")
        aggregator = AGGREGATOR.read_text(encoding="utf-8")
        for framework in ("keyed/react-hooks", "keyed/vue", "keyed/svelte", "keyed/rustflow"):
            self.assertIn(framework, workflow)
        for scenario in (
            "01_run1k", "02_replace1k", "03_update10th1k_x16", "04_select1k",
            "05_swap1k", "06_remove-one-1k", "07_create10k", "08_create1k-after1k_x2",
            "09_clear1k_x8", "10_update10th10k", "11_append1k-after10k", "12_clear10k",
        ):
            self.assertIn(scenario, workflow)
        self.assertIn("--count 15", workflow)
        self.assertIn("--count 5", workflow)
        self.assertIn("--count 1", workflow)
        self.assertGreaterEqual(patcher.count("warmupCount: 5"), 3)
        self.assertIn('"cpu": 15', aggregator)
        self.assertIn('"memory": 5', aggregator)
        self.assertIn('"size": 1', aggregator)
        self.assertIn(r"156\.0\.8078\.4", aggregator)

    def test_timeout_and_partial_artifacts_are_explicit(self) -> None:
        workflow = OFFICIAL_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("timeout-minutes: 350", workflow)
        self.assertIn("timeout-minutes: 320", workflow)
        self.assertIn("Preserve partial upstream benchmark output", workflow)
        self.assertIn("if: always()", workflow)
        self.assertIn("cp -a", workflow)
        self.assertIn("framework-comparison.json", workflow)
        self.assertIn("environment.txt", workflow)
        self.assertIn("upload-artifact@v7", workflow)
        self.assertIn("Generate fail-closed report for complete or partial data", workflow)
        self.assertIn("156\\.0\\.8078\\.4", workflow)

    def test_both_documents_describe_the_separation(self) -> None:
        docs = [
            (ROOT / "docs/PERFORMANCE.md").read_text(encoding="utf-8"),
            (ROOT / "website/src/content/docs/guides/performance.md").read_text(encoding="utf-8"),
        ]
        for content in docs:
            self.assertIn("official-framework-benchmarks.yml", content)
            self.assertIn("workflow_dispatch", content)
            self.assertIn("framework-comparison.json", content)
            self.assertIn("15 muestras", content)
            self.assertIn("no declara Phase 8.1 completada", content)


if __name__ == "__main__":
    unittest.main()
