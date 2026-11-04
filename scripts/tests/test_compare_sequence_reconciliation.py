from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "compare-sequence-reconciliation.py"
SPEC = importlib.util.spec_from_file_location("compare_sequence_reconciliation", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
COMPARATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPARATOR)


def report(value: float) -> dict[str, object]:
    samples = [value + index / 100 for index in range(12)]
    return {
        "suite": "rustflow-browser-dom",
        "node_version": "v22.23.4",
        "browser": {"engine": "Chromium", "version": "156.0.8078.4"},
        "sequential_sibling_reconciliation": {
            "size": 5_000,
            "operation": "move one keyed anchor from first to last across unkeyed siblings",
            "warmup_samples": 2,
            "measured_samples": 12,
            "samples": 12,
            "samples_raw": samples,
            "final_dom_children": 5_001,
            "identity_preserved": True,
            "final_anchor_position": "first",
        },
    }


class SequentialSiblingComparisonTests(unittest.TestCase):
    def compare(self, before, base, after):
        return COMPARATOR.compare_reports(
            before,
            base,
            after,
            base_sha="base-sha",
            head_sha="head-sha",
        )

    def test_accepts_stable_optimization_with_required_improvement(self) -> None:
        result = self.compare(report(5.0), report(50.0), report(5.2))
        self.assertTrue(result["accepted"])
        self.assertEqual(result["violations"], [])
        self.assertGreaterEqual(result["optimized_before_improvement_percent"], 20.0)
        self.assertGreaterEqual(result["optimized_after_improvement_percent"], 20.0)
        self.assertLessEqual(result["optimized_a_to_a_median_drift_percent"], 15.0)

    def test_rejects_insufficient_improvement_without_lowering_gate(self) -> None:
        result = self.compare(report(40.0), report(50.0), report(40.5))
        self.assertFalse(result["accepted"])
        self.assertTrue(any("at least 20%" in item for item in result["violations"]))

    def test_rejects_unstable_optimized_a_to_a_measurements(self) -> None:
        result = self.compare(report(5.0), report(50.0), report(8.0))
        self.assertFalse(result["accepted"])
        self.assertTrue(any("A-to-A median drift" in item for item in result["violations"]))

    def test_rejects_incomplete_samples(self) -> None:
        incomplete = report(5.0)
        scenario = incomplete["sequential_sibling_reconciliation"]
        assert isinstance(scenario, dict)
        scenario["samples_raw"] = scenario["samples_raw"][:-3]
        scenario["samples"] = 9
        scenario["measured_samples"] = 9
        result = self.compare(incomplete, report(50.0), report(5.0))
        self.assertFalse(result["accepted"])
        self.assertTrue(any("at least 10 measured samples" in item for item in result["violations"]))

    def test_rejects_mismatched_browser_metadata(self) -> None:
        base = report(50.0)
        base["browser"] = {"engine": "Chromium", "version": "different"}
        result = self.compare(report(5.0), base, report(5.0))
        self.assertFalse(result["accepted"])
        self.assertTrue(any("same browser version" in item for item in result["violations"]))


if __name__ == "__main__":
    unittest.main()
