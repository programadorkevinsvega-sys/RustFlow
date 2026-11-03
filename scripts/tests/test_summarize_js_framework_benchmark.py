from __future__ import annotations
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "summarize-js-framework-benchmark.py"
SPEC = importlib.util.spec_from_file_location("summarize_js_framework_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
AGGREGATOR = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = AGGREGATOR
SPEC.loader.exec_module(AGGREGATOR)
RUSTFLOW_SHA = "a" * 40


def write_environment(directory: Path, rustflow_sha: str = RUSTFLOW_SHA) -> None:
    (directory / "environment.txt").write_text(
        "\n".join([
            f"rustflow_commit={rustflow_sha}",
            f"upstream_commit={AGGREGATOR.UPSTREAM_COMMIT}",
            "node=v22.0.0",
            "npm=11.0.0",
            "rustc=rustc 1.85.0",
            "chromium_version=Chromium 156.0.8078.4",
            "runner_os=Linux",
        ]) + "\n",
        encoding="utf-8",
    )


def write_complete_results(directory: Path) -> None:
    write_environment(directory)
    names = {
        "react": "React 19.2.0 keyed react-hooks",
        "vue": "Vue 3.5.39 keyed vue",
        "svelte": "Svelte 5.42.1 keyed svelte",
        "rustflow": "RustFlow 0.1.0 keyed rustflow",
    }
    for framework, full_name in names.items():
        scale = {"react": 2.0, "vue": 1.8, "svelte": 1.6, "rustflow": 1.0}[framework]
        for operation in AGGREGATOR.CPU_OPERATIONS + AGGREGATOR.EXTENDED_CPU_OPERATIONS:
            body = {
                "framework": full_name,
                "benchmark": operation,
                "type": "cpu",
                "values": {
                    metric: {"values": [scale * factor * (1 + sample / 1000) for sample in range(15)]}
                    for metric, factor in (("total", 1.0), ("script", 0.6), ("paint", 0.4))
                },
            }
            (directory / f"{framework}-{operation}.json").write_text(json.dumps(body), encoding="utf-8")
        for operation in AGGREGATOR.MEMORY_OPERATIONS:
            body = {
                "framework": full_name, "benchmark": operation, "type": "memory",
                "values": {"DEFAULT": {"values": [scale * (10 + sample / 10) for sample in range(5)]}},
            }
            (directory / f"{framework}-{operation}.json").write_text(json.dumps(body), encoding="utf-8")
        for operation in AGGREGATOR.SIZE_OPERATIONS:
            body = {
                "framework": full_name, "benchmark": operation, "type": "size",
                "values": {"DEFAULT": {"values": [scale * 100]}},
            }
            (directory / f"{framework}-{operation}.json").write_text(json.dumps(body), encoding="utf-8")
    startup = {
        "warmup_rounds": 3,
        "measured_rounds": 15,
        "frameworks": {name: {"samples_raw": [{"final_dom_rows": 1000}] * 15} for name in names},
    }
    (directory / "startup-comparison.json").write_text(json.dumps(startup), encoding="utf-8")


class BenchmarkAggregatorTests(unittest.TestCase):
    def run_aggregator(self, directory: Path, expected_sha: str = RUSTFLOW_SHA) -> int:
        with (
            patch.object(sys, "argv", ["summarize-js-framework-benchmark.py", str(directory)]),
            patch.dict("os.environ", {"RUSTFLOW_EXPECTED_COMMIT": expected_sha}),
        ):
            return AGGREGATOR.main()

    def test_complete_report_checks_every_required_sample(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            write_complete_results(directory)
            self.assertEqual(self.run_aggregator(directory), 0)
            report = json.loads((directory / "framework-comparison.json").read_text(encoding="utf-8"))
            self.assertEqual(report["schema_version"], 3)
            self.assertEqual(report["status"], "valid")
            self.assertTrue(report["accepted"])
            self.assertEqual(report["rustflow_commit"], RUSTFLOW_SHA)
            self.assertEqual(report["upstream_commit"], AGGREGATOR.UPSTREAM_COMMIT)
            self.assertEqual(set(report["comparisons"]), {
                "cpu_upstream_9", "cpu_phase81_extended_3", "cpu_combined_12", "memory", "size"
            })
            for framework in ("react", "vue", "svelte", "rustflow"):
                for operation in AGGREGATOR.CPU_OPERATIONS + AGGREGATOR.EXTENDED_CPU_OPERATIONS:
                    self.assertEqual(report["benchmark_status"][framework][operation]["status"], "complete")
                    self.assertEqual(report["benchmark_status"][framework][operation]["actual_samples"],
                                     {"total": 15, "script": 15, "paint": 15})
                for operation in AGGREGATOR.MEMORY_OPERATIONS:
                    self.assertEqual(report["benchmark_status"][framework][operation]["actual_samples"], {"DEFAULT": 5})
                for operation in AGGREGATOR.SIZE_OPERATIONS:
                    self.assertEqual(report["benchmark_status"][framework][operation]["actual_samples"], {"DEFAULT": 1})
                self.assertEqual(report["startup_status"][framework]["status"], "complete")
            self.assertIn("Status: **VALID**", (directory / "framework-comparison.md").read_text(encoding="utf-8"))

    def test_incomplete_results_emit_diagnostics_but_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            write_environment(directory)
            body = {
                "framework": "RustFlow 0.1.0 keyed rustflow",
                "benchmark": AGGREGATOR.CPU_OPERATIONS[0],
                "values": {
                    "total": {"values": [1.0] * 15},
                    "script": {"values": [0.5] * 15},
                    "paint": {"values": [0.5] * 15},
                },
            }
            (directory / "partial.json").write_text(json.dumps(body), encoding="utf-8")
            self.assertEqual(self.run_aggregator(directory), 1)
            report = json.loads((directory / "framework-comparison.json").read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "incomplete")
            self.assertFalse(report["valid"])
            self.assertFalse(report["accepted"])
            self.assertTrue(report["validation_errors"])
            self.assertNotIn("cpu_upstream_9", report["comparisons"])
            self.assertIn("partial diagnostic artifact", (directory / "framework-comparison.md").read_text(encoding="utf-8"))

    def test_wrong_sample_count_is_not_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            write_complete_results(directory)
            path = directory / f"react-{AGGREGATOR.CPU_OPERATIONS[0]}.json"
            body = json.loads(path.read_text(encoding="utf-8"))
            body["values"]["total"]["values"].pop()
            path.write_text(json.dumps(body), encoding="utf-8")
            self.assertEqual(self.run_aggregator(directory), 1)
            report = json.loads((directory / "framework-comparison.json").read_text(encoding="utf-8"))
            item = report["benchmark_status"]["react"][AGGREGATOR.CPU_OPERATIONS[0]]
            self.assertEqual(item["status"], "invalid")
            self.assertEqual(item["actual_samples"]["total"], 14)

    def test_evaluated_sha_must_match_environment_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            write_complete_results(directory)
            self.assertEqual(self.run_aggregator(directory, "b" * 40), 1)
            report = json.loads((directory / "framework-comparison.json").read_text(encoding="utf-8"))
            self.assertFalse(report["valid"])
            self.assertTrue(any("SHA mismatch" in error for error in report["validation_errors"]))


if __name__ == "__main__":
    unittest.main()
