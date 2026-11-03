#!/usr/bin/env python3
"""Compare native benchmark medians with the checked-in Phase 8.1 candidate baseline.

Hosted-runner timing comparisons are warning-only. This script intentionally uses
only the Python standard library so the Rust job does not need Node or extra tools.
"""

from __future__ import annotations

import json
import statistics
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASELINE_PATH = ROOT / "docs/performance/baseline-2026-11-03.json"
RESULTS_DIR = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "performance-results/native"
TOLERANCE = 0.25


def load_measurements(path: Path) -> dict[str, dict[str, float]]:
    result: dict[str, dict[str, float]] = {}
    with path.open(encoding="utf-8") as source:
        for line in source:
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("type") != "measurement":
                continue
            result[record["name"]] = record
    return result


def main() -> int:
    baseline = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
    groups = [
        (
            "reactivity",
            "native_reactivity_median_ns_per_operation_across_three_runs",
            "median_ns_per_op",
        ),
        (
            "ssr",
            "native_ssr_median_ns_per_render_across_three_runs",
            "median_ns_per_render",
        ),
    ]

    review: dict[str, object] = {
        "schema_version": 1,
        "baseline": str(BASELINE_PATH.relative_to(ROOT)),
        "timing_policy": "warning_only",
        "tolerance_fraction": TOLERANCE,
        "groups": {},
        "warnings": [],
    }
    warnings: list[dict[str, object]] = []

    for suite, baseline_key, metric_key in groups:
        run_files = sorted(RESULTS_DIR.glob(f"{suite}-[1-3].jsonl"))
        if len(run_files) != 3:
            raise RuntimeError(
                f"expected exactly three {suite} JSONL repetitions, found {len(run_files)}"
            )

        repetitions = [load_measurements(path) for path in run_files]
        suite_baseline = baseline[baseline_key]
        compared: dict[str, object] = {}

        for scenario, baseline_value in suite_baseline.items():
            values: list[float] = []
            for repetition in repetitions:
                record = repetition.get(scenario)
                if record is None:
                    raise RuntimeError(f"missing {suite} scenario in artifact: {scenario}")
                values.append(float(record[metric_key]))

            measured = statistics.median(values)
            ceiling = float(baseline_value) * (1.0 + TOLERANCE)
            compared[scenario] = {
                "baseline_median": baseline_value,
                "repetition_medians": values,
                "measured_median": measured,
                "warning_ceiling": ceiling,
            }
            if measured > ceiling:
                warning = {
                    "suite": suite,
                    "scenario": scenario,
                    "measured_median": measured,
                    "warning_ceiling": ceiling,
                    "unit": "ns",
                }
                warnings.append(warning)
                print(
                    "::warning::RustFlow "
                    + suite
                    + " "
                    + scenario
                    + " median "
                    + f"{measured:.3f} ns exceeds warning ceiling {ceiling:.3f} ns"
                )

        review["groups"][suite] = {
            "repetitions": [path.name for path in run_files],
            "scenarios": compared,
        }

    review["warnings"] = warnings
    output = RESULTS_DIR / "native-budget-review.json"
    output.write_text(json.dumps(review, indent=2) + "\\n", encoding="utf-8")
    print(f"Wrote native timing budget review to {output}")
    print(
        f"Compared {sum(len(group['scenarios']) for group in review['groups'].values())} "
        f"native scenarios; {len(warnings)} warning(s)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
