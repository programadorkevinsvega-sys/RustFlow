from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


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


if __name__ == "__main__":
    unittest.main()
