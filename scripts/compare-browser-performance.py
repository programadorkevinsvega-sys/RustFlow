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
        "p90_ms": percentile(values, 0.90),
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
        "isolated_keyed_mutations": {},
    }
    secondary_regression_limit = 25.0
    secondary_drift_limit = 15.0

    # The 10k-row mount remains a one-pass optimized stress observation from
    # the informational benchmark step; replaying it against the slower base
    # reconciler made the A/B/A control exceed Playwright's test deadline.
    # Keep the controlled A/B/A gate on 100/1k mounts plus the isolated 100/1k/5k mutations.
    for count in (100, 1_000):
        name = f"keyed_list_initial_mount_{count}"
        min_samples = 25 if count == 100 else 7
        expected_warmups = 3 if count == 100 else 1
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
            if item.get("warmup_samples") != expected_warmups:
                violations.append(
                    f"{label}: {name} requires exactly {expected_warmups} discarded warm-up samples"
                )
            raw_count = len(item.get("samples_raw", []))
            if item.get("measured_samples") != raw_count:
                violations.append(
                    f"{label}: {name} declared measured_samples does not match {raw_count} raw samples"
                )

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
        minimum_samples = 25 if depth in (20, 80) else 7
        expected_warmups = 3 if depth in (20, 80) else 1
        for label, item, actual_samples in [
            ("optimized-before", before_item, sample_counts["optimized-before"]),
            ("unoptimized", control_item, sample_counts["unoptimized"]),
            ("optimized-after", after_item, sample_counts["optimized-after"]),
        ]:
            if item.get("warmup_samples") != expected_warmups:
                violations.append(
                    f"{label}: nested component depth {depth} requires "
                    f"{expected_warmups} discarded warm-up samples; "
                    f"got {item.get('warmup_samples')}"
                )
            if item.get("measured_samples") != actual_samples:
                violations.append(
                    f"{label}: nested component depth {depth} declared measured_samples "
                    f"does not match {actual_samples} raw samples"
                )
            if actual_samples < minimum_samples:
                violations.append(
                    f"{label}: nested component depth {depth} requires at least "
                    f"{minimum_samples} measured samples; got {actual_samples}"
                )
        if min(sample_counts.values()) < minimum_samples:
            continue

        optimized_samples = before_samples + after_samples
        comparison = {
            "component_instances": before_item.get("component_instances"),
            "minimum_samples_per_variant": minimum_samples,
            "warmup_samples_per_variant": expected_warmups,
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

    # Isolated mutations run as measured pairs: insertion starts at N,
    # removal starts at N+1, and each pair returns the fixture to baseline N.
    for operation in ("insert", "remove"):
        for count in (100, 1_000, 5_000):
            name = f"keyed_list_single_{operation}_{count}"
            before_item = scenario(opt_before, "keyed_list_isolated_mutations", name)
            control_item = scenario(control, "keyed_list_isolated_mutations", name)
            after_item = scenario(opt_after, "keyed_list_isolated_mutations", name)
            items = [
                ("optimized-before", before_item),
                ("unoptimized", control_item),
                ("optimized-after", after_item),
            ]
            sample_sets: dict[str, list[float]] = {}
            for label, item in items:
                raw = [float(x) for x in item.get("samples_raw", [])]
                sample_sets[label] = raw
                expected_start_rows = count if operation == "insert" else count + 1
                expected_end_rows = count + 1 if operation == "insert" else count
                if item.get("baseline_dom_rows") != count or item.get("final_dom_rows") != count:
                    violations.append(
                        f"{label}: {name} must restore the steady-state baseline of {count} rows"
                    )
                if item.get("operation_start_rows") != expected_start_rows:
                    violations.append(
                        f"{label}: {name} expected operation_start_rows={expected_start_rows}; "
                        f"got {item.get('operation_start_rows')}"
                    )
                if item.get("operation_end_rows") != expected_end_rows:
                    violations.append(
                        f"{label}: {name} expected operation_end_rows={expected_end_rows}; "
                        f"got {item.get('operation_end_rows')}"
                    )
                expected_pair = "single_remove" if operation == "insert" else "single_insert"
                if item.get("paired_operation") != expected_pair:
                    violations.append(
                        f"{label}: {name} must declare paired_operation={expected_pair}"
                    )
                if item.get("warmup_samples") != 1:
                    violations.append(f"{label}: {name} requires one discarded warm-up pair")
                if item.get("measured_samples") != len(raw):
                    violations.append(f"{label}: {name} measured_samples does not match raw samples")
                if len(raw) < 20:
                    violations.append(f"{label}: {name} requires 20 samples; got {len(raw)}")
                if operation == "remove":
                    expected_positions = {0, (count + 1) // 2, count}
                    actual_positions = set(item.get("deletion_target_indexes", []))
                    if actual_positions != expected_positions:
                        violations.append(
                            f"{label}: {name} must cover first, middle and last positions; "
                            f"got {sorted(actual_positions)}"
                        )
            if any(len(sample_sets[label]) < 20 for label, _ in items):
                continue

            before_samples = sample_sets["optimized-before"]
            control_samples = sample_sets["unoptimized"]
            after_samples = sample_sets["optimized-after"]
            optimized_samples = before_samples + after_samples
            comparison = {
                "minimum_samples_per_variant": 20,
                "unoptimized": summarize(control_samples),
                "optimized": summarize(optimized_samples),
                "optimized_before_dom_operation_counts": before_item.get("dom_operation_counts"),
                "unoptimized_dom_operation_counts": control_item.get("dom_operation_counts"),
                "optimized_after_dom_operation_counts": after_item.get("dom_operation_counts"),
            }
            gain = improvement(comparison["unoptimized"]["median_ms"], comparison["optimized"]["median_ms"])
            comparison["median_improvement_percent"] = gain
            comparison["p95_improvement_percent"] = improvement(
                comparison["unoptimized"]["p95_ms"], comparison["optimized"]["p95_ms"]
            )
            first_median = statistics.median(before_samples)
            second_median = statistics.median(after_samples)
            drift = (
                abs(second_median - first_median) / min(first_median, second_median) * 100
                if min(first_median, second_median) > 0 else 0.0
            )
            comparison["optimized_before_median_ms"] = first_median
            comparison["optimized_after_median_ms"] = second_median
            comparison["optimized_a_to_a_median_drift_percent"] = drift
            secondary_comparisons["isolated_keyed_mutations"][name] = comparison
            if drift > secondary_drift_limit:
                violations.append(
                    f"{name}: optimized A/A median drift {drift:.1f}% exceeds "
                    f"{secondary_drift_limit:.0f}%; comparison is unstable"
                )
            if gain < -secondary_regression_limit:
                violations.append(
                    f"{name}: optimized median regressed by {-gain:.1f}%, "
                    f"exceeding {secondary_regression_limit:.0f}%"
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
                "nested_component_depth_5",
                "nested_component_depth_20",
                "nested_component_depth_80",
                "keyed_list_single_insert_100",
                "keyed_list_single_insert_1000",
                "keyed_list_single_insert_5000",
                "keyed_list_single_remove_100",
                "keyed_list_single_remove_1000",
                "keyed_list_single_remove_5000",
            ],
            "informational_only_scenarios": ["keyed_list_initial_mount_10000"],
            "minimum_isolated_keyed_samples_per_variant": 20,
            "minimum_secondary_samples_per_variant": 7,
            "minimum_initial_mount_warmup_samples_per_variant": 1,
            "isolated_keyed_mutation_sampling": {
                "steady_state_baseline_rows": [100, 1000, 5000],
                "operation_start_rows": {"insert": "N", "remove": "N+1"},
                "operation_end_rows": {"insert": "N+1", "remove": "N"},
                "discarded_warmup_pairs": 1,
                "measured_samples_per_operation": 20,
                "timed_scope": "one Add click and one paired Remove click are measured separately; input preparation, selectors, and verification are excluded; each pair returns to the steady-state baseline",
                "dom_boundary_counters": "instrumented separately from latency samples",
            },
            "initial_mount_sampling": {
                "100": {"discarded_warmup_samples": 3, "measured_samples": 25},
                "1000": {"discarded_warmup_samples": 1, "measured_samples": 7},
                "10000": {"discarded_warmup_samples": 1, "measured_samples": 7},
            },
            "nested_component_sampling": {
                "depth_5": {"discarded_warmup_samples": 1, "measured_samples": 7},
                "depth_20": {"discarded_warmup_samples": 3, "measured_samples": 25},
                "depth_80": {"discarded_warmup_samples": 3, "measured_samples": 25},
            },
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
