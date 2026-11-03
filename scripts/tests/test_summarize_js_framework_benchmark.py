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


class BenchmarkAggregatorTests(unittest.TestCase):
    def test_complete_four_framework_report_contains_all_metric_families(self) -> None:
        framework_names = {
            "react": "React 19.2.0 keyed react-hooks",
            "vue": "Vue 3.5.39 keyed vue",
            "svelte": "Svelte 5.42.1 keyed svelte",
            "rustflow": "RustFlow 0.1.0 keyed rustflow",
        }
        cpu_operations = AGGREGATOR.CPU_OPERATIONS + AGGREGATOR.EXTENDED_CPU_OPERATIONS
        competitor_scale = {"react": 2.0, "vue": 1.8, "svelte": 1.6, "rustflow": 1.0}

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for framework, full_name in framework_names.items():
                scale = competitor_scale[framework]
                for operation in cpu_operations:
                    payload = {
                        "framework": full_name,
                        "benchmark": operation,
                        "type": "cpu",
                        "values": {
                            metric: {"values": [scale * multiplier, scale * multiplier * 1.02, scale * multiplier * 0.98]}
                            for metric, multiplier in (("total", 1.0), ("script", 0.6), ("paint", 0.4))
                        },
                    }
                    (directory / f"{framework}-{operation}.json").write_text(
                        json.dumps(payload), encoding="utf-8"
                    )

                for operation in AGGREGATOR.MEMORY_OPERATIONS:
                    payload = {
                        "framework": full_name,
                        "benchmark": operation,
                        "type": "memory",
                        "values": {"DEFAULT": {"values": [scale * 10, scale * 10.2, scale * 9.8]}},
                    }
                    (directory / f"{framework}-{operation}.json").write_text(
                        json.dumps(payload), encoding="utf-8"
                    )

                for operation in AGGREGATOR.SIZE_OPERATIONS:
                    payload = {
                        "framework": full_name,
                        "benchmark": operation,
                        "type": "size",
                        "values": {"DEFAULT": {"values": [scale * 100, scale * 102, scale * 98]}},
                    }
                    (directory / f"{framework}-{operation}.json").write_text(
                        json.dumps(payload), encoding="utf-8"
                    )

            with patch.object(sys, "argv", ["summarize-js-framework-benchmark.py", str(directory)]):
                self.assertEqual(AGGREGATOR.main(), 0)

            report = json.loads((directory / "framework-comparison.json").read_text(encoding="utf-8"))
            self.assertEqual(report["schema_version"], 2)
            self.assertFalse(report["competitive_superiority_claimed"])
            self.assertEqual(
                len(report["frameworks"]["rustflow"]["cpu"]),
                len(AGGREGATOR.CPU_OPERATIONS) + len(AGGREGATOR.EXTENDED_CPU_OPERATIONS),
            )
            self.assertEqual(
                set(report["comparisons"]),
                {"cpu_upstream_9", "cpu_phase81_extended_3", "cpu_combined_12", "memory", "size"},
            )
            for competitor in ("react", "vue", "svelte"):
                self.assertLess(
                    report["comparisons"]["cpu_combined_12"][competitor][
                        "geometric_mean_ratio_rustflow_over_competitor"
                    ],
                    1,
                )
                self.assertIn(
                    "23_update5-memory",
                    report["comparisons"]["memory"][competitor]["operations"],
                )
                self.assertIn(
                    "42_size-compressed",
                    report["comparisons"]["size"][competitor]["operations"],
                )
            markdown = (directory / "framework-comparison.md").read_text(encoding="utf-8")
            self.assertIn("cpu_upstream_9", markdown)
            self.assertIn("cpu_phase81_extended_3", markdown)
            self.assertIn("cpu_combined_12", markdown)
            self.assertIn("## Memory", markdown)
            self.assertIn("## Bundle and first paint", markdown)

    def test_incomplete_framework_results_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            payload = {
                "framework": "RustFlow 0.1.0 keyed rustflow",
                "benchmark": AGGREGATOR.CPU_OPERATIONS[0],
                "type": "cpu",
                "values": {
                    "total": {"values": [1.0]},
                    "script": {"values": [0.5]},
                    "paint": {"values": [0.5]},
                },
            }
            (directory / "partial.json").write_text(json.dumps(payload), encoding="utf-8")
            with patch.object(sys, "argv", ["summarize-js-framework-benchmark.py", str(directory)]):
                with self.assertRaises(SystemExit):
                    AGGREGATOR.main()


if __name__ == "__main__":
    unittest.main()
