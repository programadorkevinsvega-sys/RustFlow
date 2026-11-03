#!/usr/bin/env python3
"""Compare optimized and base keyed DOM reconciliation on the same CI runner."""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path
from typing import Any


def scenario(report: dict[str, Any], collection: str, name: str) -> dict[str, Any]:
    for item in report.get(collection, []):
        if item.get("name") == name:
            return item
    raise ValueError(f"missing {collection} scenario {name}")


def combine_samples(reports: list[dict[str, Any]], collection: str, name: str) -> list[float]:
    values: list[float] = []
    for report in reports:
        values.extend(
            float(value)
            for value in scenario(report, collection, name).get("samples_raw", [])
        )
    if not values:
        raise ValueError(f"no raw samples for {name}")
    return values


def percentile(values: list[float], p: float) -> float:
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * p
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = position - lower
    return ordered[lower] * (1 - fraction) + ordered[upper] * fraction


def summarize(values: list[float]) -> dict[str, Any]:
    return {
        "samples": len(values),
        "median_ms": statistics.median(values),
        "p95_ms": percentile(values, 0.95),
        "raw_ms": values,
    }


def improvement(before: float, after: float) -> float:
    return ((before - after) / before) * 100 if before else 0.0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--optimized-before", required=True, type=Path)
    parser.add_argument("--unoptimized", required=True, type=Path)
    parser.add_argument("--optimized-after", required=True, type=Path)
    parser.add_argument("--environment", required=True, type=Path)
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    opt_before = json.loads(args.optimized_before.read_text(encoding="utf-8"))
    control = json.loads(args.unoptimized.read_text(encoding="utf-8"))
    opt_after = json.loads(args.optimized_after.read_text(encoding="utf-8"))

    for label, report in [
        ("optimized-before", opt_before),
        ("unoptimized", control),
        ("optimized-after", opt_after),
    ]:
        if report.get("browser", {}).get("engine") != "Chromium":
            raise ValueError(f"{label}: expected Chromium browser benchmark")
        if not report.get("commit"):
            raise ValueError(f"{label}: missing commit metadata")

    identities = {
        (
            report.get("node_version"),
            report.get("browser", {}).get("version"),
            report.get("suite"),
        )
        for report in (opt_before, control, opt_after)
    }
    if len(identities) != 1:
        raise ValueError(f"tool/browser identity differs across A/B runs: {identities}")

    insert_name = "keyed_list_incremental_insert_100"
    insert_before = [
        float(value)
        for value in scenario(opt_before, "keyed_list_incremental_insertions", insert_name)[
            "samples_raw"
        ]
    ]
    insert_after = [
        float(value)
        for value in scenario(opt_after, "keyed_list_incremental_insertions", insert_name)[
            "samples_raw"
        ]
    ]
    optimized_insert = insert_before + insert_after
    control_insert = combine_samples(
        [control], "keyed_list_incremental_insertions", insert_name
    )

    reverse_before = [
        float(value) for value in opt_before["keyed_list_reverse_100"]["samples_raw"]
    ]
    reverse_after = [
        float(value) for value in opt_after["keyed_list_reverse_100"]["samples_raw"]
    ]
    optimized_reverse = reverse_before + reverse_after
    control_reverse = [
        float(value) for value in control["keyed_list_reverse_100"]["samples_raw"]
    ]

    comparisons = {
        "keyed_list_incremental_insert_100": {
            "unoptimized": summarize([float(x) for x in control_insert]),
            "optimized": summarize([float(x) for x in optimized_insert]),
        },
        "keyed_list_reverse_100": {
            "unoptimized": summarize([float(x) for x in control_reverse]),
            "optimized": summarize([float(x) for x in optimized_reverse]),
        },
    }
    violations: list[str] = []
    optimized_pairs = {
        "keyed_list_incremental_insert_100": (insert_before, insert_after),
        "keyed_list_reverse_100": (reverse_before, reverse_after),
    }
    for name, data in comparisons.items():
        before = data["unoptimized"]["median_ms"]
        after = data["optimized"]["median_ms"]
        gain = improvement(before, after)
        data["median_improvement_percent"] = gain
        data["p95_improvement_percent"] = improvement(
            data["unoptimized"]["p95_ms"], data["optimized"]["p95_ms"]
        )

        first, second = optimized_pairs[name]
        first_median = statistics.median(first)
        second_median = statistics.median(second)
        drift = (
            abs(second_median - first_median) / min(first_median, second_median) * 100
            if min(first_median, second_median) > 0
            else 0.0
        )
        data["optimized_before_median_ms"] = first_median
        data["optimized_after_median_ms"] = second_median
        data["optimized_a_to_a_drift_percent"] = drift
        if drift > 15.0:
            violations.append(
                f"{name}: optimized A-to-A median drift {drift:.1f}% exceeds 15%; runner comparison is unstable"
            )
        if gain < 20.0:
            violations.append(
                f"{name}: expected at least 20% median improvement on same-runner A/B; got {gain:.1f}%"
            )

    # Compare the full initial-mount scale and recursive component workloads as
    # secondary same-runner scenarios. Require adequate samples and stable A/A
    # medians before interpreting either an improvement or a regression.
    secondary_comparisons: dict[str, Any] = {
        "initial_keyed_list_mounts": {},
        "nested_component_depths": {},
    }
    secondary_regression_limit = 25.0
    secondary_drift_limit = 15.0

    for count in (100, 1_000, 10_000):
        name = f"keyed_list_initial_mount_{count}"
        min_samples = 3 if count == 10_000 else 7
        before_item = scenario(opt_before, "keyed_list_initial_mounts", name)
        control_item = scenario(control, "keyed_list_initial_mounts", name)
        after_item = scenario(opt_after, "keyed_list_initial_mounts", name)
        for label, item in [
            ("optimized-before", before_item),
            ("unoptimized", control_item),
            ("optimized-after", after_item),
        ]:
            if item.get("final_dom_rows") != count:
                violations.append(f"{label}: {name} expected {count} verified DOM rows")

        before_samples = [float(x) for x in before_item.get("samples_raw", [])]
        control_samples = [float(x) for x in control_item.get("samples_raw", [])]
        after_samples = [float(x) for x in after_item.get("samples_raw", [])]
        sample_counts = {
            "optimized-before": len(before_samples),
            "unoptimized": len(control_samples),
            "optimized-after": len(after_samples),
        }
        for label, actual_samples in sample_counts.items():
            if actual_samples < min_samples:
                violations.append(
                    f"{label}: {name} requires at least {min_samples} samples; got {actual_samples}"
                )
        if min(sample_counts.values()) < min_samples:
            continue

        optimized_samples = before_samples + after_samples
        comparison = {
            "minimum_samples_per_variant": min_samples,
            "unoptimized": summarize(control_samples),
            "optimized": summarize(optimized_samples),
        }
        gain = improvement(
            comparison["unoptimized"]["median_ms"],
            comparison["optimized"]["median_ms"],
        )
        comparison["median_improvement_percent"] = gain
        comparison["p95_improvement_percent"] = improvement(
            comparison["unoptimized"]["p95_ms"],
            comparison["optimized"]["p95_ms"],
        )
        first_median = statistics.median(before_samples)
        second_median = statistics.median(after_samples)
        drift = (
            abs(second_median - first_median) / min(first_median, second_median) * 100
            if min(first_median, second_median) > 0
            else 0.0
        )
        comparison["optimized_before_median_ms"] = first_median
        comparison["optimized_after_median_ms"] = second_median
        comparison["optimized_a_to_a_drift_percent"] = drift
        secondary_comparisons["initial_keyed_list_mounts"][name] = comparison
        if drift > secondary_drift_limit:
            violations.append(
                f"{name}: optimized A-to-A median drift {drift:.1f}% exceeds "
                f"{secondary_drift_limit:.0f}%; secondary runner comparison is unstable"
            )
        if gain < -secondary_regression_limit:
            violations.append(
                f"{name}: optimized median regressed by {-gain:.1f}%, "
                f"exceeding the {secondary_regression_limit:.0f}% same-runner limit"
            )

    for depth in (5, 20, 80):
        def find_nested(report: dict[str, Any]) -> dict[str, Any]:
            for item in report.get("nested_component_depth_initial_mounts", []):
                if int(item.get("depth", -1)) == depth:
                    return item
            raise ValueError(f"missing nested component depth {depth}")

        before_item = find_nested(opt_before)
        control_item = find_nested(control)
        after_item = find_nested(opt_after)
        before_samples = [float(x) for x in before_item.get("samples_raw", [])]
        control_samples = [float(x) for x in control_item.get("samples_raw", [])]
        after_samples = [float(x) for x in after_item.get("samples_raw", [])]
        sample_counts = {
            "optimized-before": len(before_samples),
            "unoptimized": len(control_samples),
            "optimized-after": len(after_samples),
        }
        for label, actual_samples in sample_counts.items():
            if actual_samples < 7:
                violations.append(
                    f"{label}: nested component depth {depth} requires at least 7 samples; "
                    f"got {actual_samples}"
                )
        if min(sample_counts.values()) < 7:
            continue

        optimized_samples = before_samples + after_samples
        comparison = {
            "component_instances": before_item.get("component_instances"),
            "minimum_samples_per_variant": 7,
            "unoptimized": summarize(control_samples),
            "optimized": summarize(optimized_samples),
        }
        gain = improvement(
            comparison["unoptimized"]["median_ms"],
            comparison["optimized"]["median_ms"],
        )
        comparison["median_improvement_percent"] = gain
        comparison["p95_improvement_percent"] = improvement(
            comparison["unoptimized"]["p95_ms"],
            comparison["optimized"]["p95_ms"],
        )
        first_median = statistics.median(before_samples)
        second_median = statistics.median(after_samples)
        drift = (
            abs(second_median - first_median) / min(first_median, second_median) * 100
            if min(first_median, second_median) > 0
            else 0.0
        )
        comparison["optimized_before_median_ms"] = first_median
        comparison["optimized_after_median_ms"] = second_median
        comparison["optimized_a_to_a_drift_percent"] = drift
        secondary_comparisons["nested_component_depths"][str(depth)] = comparison
        if drift > secondary_drift_limit:
            violations.append(
                f"nested component depth {depth}: optimized A-to-A median drift "
                f"{drift:.1f}% exceeds {secondary_drift_limit:.0f}%; "
                f"secondary runner comparison is unstable"
            )
        if gain < -secondary_regression_limit:
            violations.append(
                f"nested component depth {depth}: optimized median regressed by "
                f"{-gain:.1f}%, exceeding the {secondary_regression_limit:.0f}% same-runner limit"
            )

    environment = args.environment.read_text(encoding="utf-8")
    output = {
        "schema_version": 1,
        "comparison": "same-job same-runner A/B/A",
        "base_source_sha": args.base_sha,
        "optimized_source_sha": args.head_sha,
        "node_version": opt_before["node_version"],
        "browser": opt_before["browser"],
        "environment_file": str(args.environment),
        "environment": environment,
        "runs": {
            "optimized_before": str(args.optimized_before),
            "unoptimized_control": str(args.unoptimized),
            "optimized_after": str(args.optimized_after),
        },
        "comparisons": comparisons,
        "secondary_comparisons": secondary_comparisons,
        "policy": {
            "critical_scenarios": [
                "keyed_list_incremental_insert_100",
                "keyed_list_reverse_100",
            ],
            "minimum_median_improvement_percent": 20.0,
            "maximum_optimized_a_to_a_median_drift_percent": 15.0,
            "secondary_scenarios": [
                "keyed_list_initial_mount_100",
                "keyed_list_initial_mount_1000",
                "keyed_list_initial_mount_10000",
                "nested_component_depth_5",
                "nested_component_depth_20",
                "nested_component_depth_80",
            ],
            "minimum_secondary_samples_per_variant": 7,
            "minimum_10000_row_samples_per_variant": 3,
            "maximum_secondary_optimized_a_to_a_median_drift_percent": 15.0,
            "maximum_secondary_median_regression_percent": 25.0,
            "timing_budgets": "absolute timing thresholds remain warning-only; same-runner A/B/A gates the critical improvements, requires stable secondary A/A medians, and rejects secondary median regressions above 25%",
        },
        "violations": violations,
        "accepted": not violations,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(output, indent=2))
    for error in violations:
        print(f"::error::{error}")
    if violations:
        return 1
    print("Controlled same-runner keyed reconciliation A/B comparison passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
