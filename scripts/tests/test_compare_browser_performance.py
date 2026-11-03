from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "compare-browser-performance.py"
SPEC = importlib.util.spec_from_file_location("compare_browser_performance", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
COMPARATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPARATOR)


def isolated_item(operation: str, count: int, value: float) -> dict[str, object]:
    start_rows = count if operation == "insert" else count + 1
    end_rows = count + 1 if operation == "insert" else count
    item: dict[str, object] = {
        "name": f"keyed_list_single_{operation}_{count}",
        "baseline_dom_rows": count,
        "final_dom_rows": count,
        "operation_start_rows": start_rows,
        "operation_end_rows": end_rows,
        "paired_operation": "single_remove" if operation == "insert" else "single_insert",
        "warmup_samples": 1,
        "measured_samples": 20,
        "samples_raw": [value + index / 100 for index in range(20)],
    }
    if operation == "remove":
        item["deletion_target_indexes"] = [0, (count + 1) // 2, count] * 7
    return item


def optimized_report(value: float) -> dict[str, object]:
    scenarios = [
        isolated_item(operation, count, value)
        for operation in ("insert", "remove")
        for count in (100, 1_000, 5_000)
    ]
    return {"keyed_list_isolated_mutations": scenarios}


class IsolatedKeyedMutationComparisonTests(unittest.TestCase):
    def test_all_six_twenty_sample_scenarios_are_validated_without_base_results(self) -> None:
        before = optimized_report(10.0)
        after = optimized_report(10.5)

        comparisons, violations = COMPARATOR.compare_optimized_isolated_keyed_mutations(
            before, after, 15.0
        )

        self.assertEqual(violations, [])
        self.assertEqual(len(comparisons), 6)
        for operation in ("insert", "remove"):
            for count in (100, 1_000, 5_000):
                name = f"keyed_list_single_{operation}_{count}"
                comparison = comparisons[name]
                self.assertEqual(comparison["optimized_before"]["samples"], 20)
                self.assertEqual(comparison["optimized_after"]["samples"], 20)
                self.assertEqual(comparison["optimized_combined"]["samples"], 40)
                self.assertFalse(comparison["base_control_comparison"])
                self.assertNotIn("unoptimized", comparison)

    def test_incomplete_samples_and_invalid_row_contract_fail_closed(self) -> None:
        before = optimized_report(10.0)
        after = optimized_report(10.0)
        scenarios = after["keyed_list_isolated_mutations"]
        assert isinstance(scenarios, list)
        scenarios[0]["samples_raw"] = scenarios[0]["samples_raw"][:-1]
        scenarios[1]["operation_start_rows"] = 5_000

        comparisons, violations = COMPARATOR.compare_optimized_isolated_keyed_mutations(
            before, after, 15.0
        )

        self.assertNotIn("keyed_list_single_insert_100", comparisons)
        self.assertTrue(any("requires exactly 20 samples" in item for item in violations))
        self.assertTrue(any("operation_start_rows" in item for item in violations))

    def test_optimized_a_to_a_drift_still_fails_closed(self) -> None:
        before = optimized_report(10.0)
        after = optimized_report(13.0)

        _comparisons, violations = COMPARATOR.compare_optimized_isolated_keyed_mutations(
            before, after, 15.0
        )

        self.assertTrue(any("A/A median drift" in item for item in violations))


def comparison_report(*, optimized: bool) -> dict[str, object]:
    scale = 1.0 if optimized else 2.0
    report: dict[str, object] = {
        "suite": "rustflow-browser-dom",
        "commit": "optimized" if optimized else "base",
        "node_version": "v22.23.4",
        "browser": {"engine": "Chromium", "version": "156.0.8078.4"},
        "keyed_list_incremental_insertions": [
            {
                "name": "keyed_list_incremental_insert_100",
                "samples_raw": [5.0 * scale + index / 100 for index in range(5)],
            }
        ],
        "keyed_list_reverse_100": {"samples_raw": [5.0 * scale + index / 100 for index in range(10)]},
        "keyed_list_initial_mounts": [],
        "nested_component_depth_initial_mounts": [],
        "keyed_list_isolated_mutations": [],
    }

    mounts = report["keyed_list_initial_mounts"]
    assert isinstance(mounts, list)
    for count, warmups, samples in ((100, 3, 25), (1_000, 1, 7)):
        mounts.append({
            "name": f"keyed_list_initial_mount_{count}",
            "final_dom_rows": count,
            "warmup_samples": warmups,
            "measured_samples": samples,
            "samples_raw": [10.0 * scale + index / 100 for index in range(samples)],
        })

    nested = report["nested_component_depth_initial_mounts"]
    assert isinstance(nested, list)
    for depth, warmups, samples in ((5, 1, 7), (20, 3, 25), (80, 3, 25)):
        nested.append({
            "depth": depth,
            "component_instances": depth + 1,
            "warmup_samples": warmups,
            "measured_samples": samples,
            "samples_raw": [10.0 * scale + index / 100 for index in range(samples)],
        })

    isolated = report["keyed_list_isolated_mutations"]
    assert isinstance(isolated, list)
    if optimized:
        for operation in ("insert", "remove"):
            for count in (100, 1_000, 5_000):
                isolated.append(isolated_item(operation, count, 10.0))
    else:
        # The quadratic base control intentionally does not run 5k isolated pairs.
        isolated = report["keyed_list_isolated_mutations"] = []
    return report


class ComparatorMainIntegrationTests(unittest.TestCase):
    def test_main_serializes_complete_report_and_accepts_a_valid_ab_a_run(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            optimized_before = root / "optimized-before.json"
            unoptimized = root / "unoptimized.json"
            optimized_after = root / "optimized-after.json"
            environment = root / "environment.txt"
            output = root / "browser-ab-comparison.json"

            optimized_before.write_text(
                json.dumps(comparison_report(optimized=True)), encoding="utf-8"
            )
            unoptimized.write_text(
                json.dumps(comparison_report(optimized=False)), encoding="utf-8"
            )
            optimized_after.write_text(
                json.dumps(comparison_report(optimized=True)), encoding="utf-8"
            )
            environment.write_text("runner=synthetic-test\\n", encoding="utf-8")

            argv = [
                "compare-browser-performance.py",
                "--optimized-before", str(optimized_before),
                "--unoptimized", str(unoptimized),
                "--optimized-after", str(optimized_after),
                "--environment", str(environment),
                "--base-sha", "base-sha",
                "--head-sha", "head-sha",
                "--output", str(output),
            ]
            with patch("sys.argv", argv):
                result = COMPARATOR.main()

            self.assertEqual(result, 0)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertTrue(payload["accepted"])
            self.assertEqual(payload["violations"], [])
            self.assertFalse(
                payload["policy"]["isolated_keyed_mutation_sampling"]["base_control_comparison"]
            )
            self.assertEqual(len(payload["secondary_comparisons"]["isolated_keyed_mutations"]), 6)



if __name__ == "__main__":
    unittest.main()
