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


def validate_nested_component_cycle_report(
    label: str, item: dict[str, Any], depth: int
) -> tuple[list[float], list[str]]:
    """Validate batched complete mount/unmount lifecycle measurements."""
    violations: list[str] = []
    samples = [float(value) for value in item.get("samples_raw", [])]
    expected_warmup_cycles = 10
    expected_cycles_per_sample = 100
    expected_cycles_per_timing_batch = 20
    expected_timing_batches_per_sample = 5
    expected_instances = depth + 1

    if item.get("warmup_cycles") != expected_warmup_cycles:
        violations.append(
            f"{label}: nested component depth {depth} requires "
            f"{expected_warmup_cycles} discarded warm-up mount/unmount cycles; "
            f"got {item.get('warmup_cycles')}"
        )
    if item.get("cycles_per_sample") != expected_cycles_per_sample:
        violations.append(
            f"{label}: nested component depth {depth} requires "
            f"{expected_cycles_per_sample} complete lifecycle cycles per measured sample; "
            f"got {item.get('cycles_per_sample')}"
        )
    if item.get("cycles_per_timing_batch") != expected_cycles_per_timing_batch:
        violations.append(
            f"{label}: nested component depth {depth} requires "
            f"{expected_cycles_per_timing_batch} cycles per timing batch; "
            f"got {item.get('cycles_per_timing_batch')}"
        )
    if item.get("timing_batches_per_sample") != expected_timing_batches_per_sample:
        violations.append(
            f"{label}: nested component depth {depth} requires "
            f"{expected_timing_batches_per_sample} timing batches per sample; "
            f"got {item.get('timing_batches_per_sample')}"
        )
    if item.get("component_instances") != expected_instances:
        violations.append(
            f"{label}: nested component depth {depth} expected "
            f"{expected_instances} component instances; got {item.get('component_instances')}"
        )
    if item.get("measured_samples") != len(samples):
        violations.append(
            f"{label}: nested component depth {depth} declared measured_samples "
            f"does not match {len(samples)} raw lifecycle samples"
        )
    expected_verified_cycles = expected_warmup_cycles + len(samples) * expected_cycles_per_sample
    if item.get("verified_mounts") != expected_verified_cycles:
        violations.append(
            f"{label}: nested component depth {depth} did not complete all expected mounts "
            f"({expected_verified_cycles})"
        )
    if item.get("verified_unmounts") != expected_verified_cycles:
        violations.append(
            f"{label}: nested component depth {depth} did not complete all expected unmounts "
            f"({expected_verified_cycles})"
        )
    if item.get("final_component_count") != expected_instances:
        violations.append(
            f"{label}: nested component depth {depth} did not finish with "
            f"{expected_instances} mounted component nodes"
        )
    return samples, violations


def check_a_to_a_median_drift(
    scenario_name: str,
    before_samples: list[float],
    after_samples: list[float],
    maximum_drift_percent: float,
) -> tuple[float, str | None]:
    """Return median drift and a fail-closed violation above the configured limit."""
    before_median = statistics.median(before_samples)
    after_median = statistics.median(after_samples)
    drift = (
        abs(after_median - before_median) / min(before_median, after_median) * 100
        if min(before_median, after_median) > 0
        else 0.0
    )
    if drift > maximum_drift_percent:
        return (
            drift,
            f"{scenario_name}: optimized A-to-A median drift {drift:.1f}% exceeds "
            f"{maximum_drift_percent:.0f}%; comparison is unstable",
        )
    return drift, None



def compare_optimized_isolated_keyed_mutations(
    optimized_before: dict[str, Any],
    optimized_after: dict[str, Any],
    maximum_drift_percent: float,
) -> tuple[dict[str, Any], list[str]]:
    """Validate the six isolated mutation workloads on two optimized passes.

    The pre-optimization keyed matcher scans the old list for every new child
    (quadratic work). Running 20 insert/remove pairs at 5,000 rows against that
    control makes the A/B/A job exceed its deadline. Keep these required 20
    sample workloads on both optimized passes and use them as a strict A/A
    stability check; the critical A/B/A control remains the 100-row incremental
    insertion and reverse scenarios.
    """
    comparisons: dict[str, Any] = {}
    violations: list[str] = []

    for operation in ("insert", "remove"):
        for count in (100, 1_000, 5_000):
            name = f"keyed_list_single_{operation}_{count}"
            before_item = scenario(optimized_before, "keyed_list_isolated_mutations", name)
            after_item = scenario(optimized_after, "keyed_list_isolated_mutations", name)
            items = [("optimized-before", before_item), ("optimized-after", after_item)]
            sample_sets: dict[str, list[float]] = {}

            expected_start_rows = count if operation == "insert" else count + 1
            expected_end_rows = count + 1 if operation == "insert" else count
            expected_pair = "single_remove" if operation == "insert" else "single_insert"
            for label, item in items:
                raw = [float(value) for value in item.get("samples_raw", [])]
                sample_sets[label] = raw
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
                if item.get("paired_operation") != expected_pair:
                    violations.append(
                        f"{label}: {name} must declare paired_operation={expected_pair}"
                    )
                if item.get("warmup_samples") != 1:
                    violations.append(f"{label}: {name} requires one discarded warm-up pair")
                if item.get("measured_samples") != len(raw):
                    violations.append(f"{label}: {name} measured_samples does not match raw samples")
                if len(raw) != 20:
                    violations.append(f"{label}: {name} requires exactly 20 samples; got {len(raw)}")
                if operation == "remove":
                    expected_positions = {0, (count + 1) // 2, count}
                    actual_positions = set(item.get("deletion_target_indexes", []))
                    if actual_positions != expected_positions:
                        violations.append(
                            f"{label}: {name} must cover first, middle and last positions; "
                            f"got {sorted(actual_positions)}"
                        )

            if any(len(values) != 20 for values in sample_sets.values()):
                continue

            before_samples = sample_sets["optimized-before"]
            after_samples = sample_sets["optimized-after"]
            combined_samples = before_samples + after_samples
            first_median = statistics.median(before_samples)
            second_median = statistics.median(after_samples)
            drift = (
                abs(second_median - first_median) / min(first_median, second_median) * 100
                if min(first_median, second_median) > 0
                else 0.0
            )
            comparison = {
                "minimum_samples_per_optimized_pass": 20,
                "optimized_before": summarize(before_samples),
                "optimized_after": summarize(after_samples),
                "optimized_combined": summarize(combined_samples),
                "optimized_before_dom_operation_counts": before_item.get("dom_operation_counts"),
                "optimized_after_dom_operation_counts": after_item.get("dom_operation_counts"),
                "optimized_a_to_a_median_drift_percent": drift,
                "base_control_comparison": False,
                "base_control_exclusion_reason": (
                    "The pre-optimization keyed matcher scans the old list for every new child; "
                    "20 paired mutations at 5,000 rows are quadratic and exceed the CI Playwright deadline."
                ),
            }
            comparisons[name] = comparison
            if drift > maximum_drift_percent:
                violations.append(
                    f"{name}: optimized A/A median drift {drift:.1f}% exceeds "
                    f"{maximum_drift_percent:.0f}%; comparison is unstable"
                )

    return comparisons, violations


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
        expected_warmups = 10 if count == 100 else 1
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
            for item in report.get("nested_component_depth_mount_cycles", []):
                if int(item.get("depth", -1)) == depth:
                    return item
            raise ValueError(f"missing nested component lifecycle scenario for depth {depth}")

        before_item = find_nested(opt_before)
        control_item = find_nested(control)
        after_item = find_nested(opt_after)
        before_samples, before_errors = validate_nested_component_cycle_report(
            "optimized-before", before_item, depth
        )
        control_samples, control_errors = validate_nested_component_cycle_report(
            "unoptimized", control_item, depth
        )
        after_samples, after_errors = validate_nested_component_cycle_report(
            "optimized-after", after_item, depth
        )
        violations.extend(before_errors)
        violations.extend(control_errors)
        violations.extend(after_errors)

        sample_counts = {
            "optimized-before": len(before_samples),
            "unoptimized": len(control_samples),
            "optimized-after": len(after_samples),
        }
        minimum_samples = 25
        if min(sample_counts.values()) < minimum_samples:
            for label, actual_samples in sample_counts.items():
                if actual_samples < minimum_samples:
                    violations.append(
                        f"{label}: nested component depth {depth} requires at least "
                        f"{minimum_samples} measured lifecycle samples; got {actual_samples}"
                    )
            continue

        optimized_samples = before_samples + after_samples
        comparison = {
            "component_instances": before_item.get("component_instances"),
            "measurement": (
                "Average complete synchronous recursive subtree unmount+mount lifecycle cycle "
                "over five timed batches of twenty cycles per sample in one initialized WASM runtime"
            ),
            "minimum_samples_per_variant": minimum_samples,
            "warmup_cycles_per_variant": 10,
            "cycles_per_measured_sample": 100,
            "cycles_per_timing_batch": 20,
            "timing_batches_per_sample": 5,
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
        drift, drift_violation = check_a_to_a_median_drift(
            f"nested component depth {depth}",
            before_samples,
            after_samples,
            secondary_drift_limit,
        )
        comparison["optimized_before_median_ms"] = first_median
        comparison["optimized_after_median_ms"] = second_median
        comparison["optimized_a_to_a_drift_percent"] = drift
        secondary_comparisons["nested_component_depths"][str(depth)] = comparison
        if drift_violation is not None:
            violations.append(drift_violation)
        if gain < -secondary_regression_limit:
            violations.append(
                f"nested component depth {depth}: optimized median regressed by "
                f"{-gain:.1f}%, exceeding the {secondary_regression_limit:.0f}% same-runner limit"
            )

    # The six 20-sample isolated mutation series run on both optimized passes.
    # They are validated and checked for A/A stability, but are not replayed on
    # the pre-optimization quadratic keyed matcher at 5,000 rows.
    isolated_comparisons, isolated_violations = compare_optimized_isolated_keyed_mutations(
        opt_before,
        opt_after,
        secondary_drift_limit,
    )
    secondary_comparisons["isolated_keyed_mutations"] = isolated_comparisons
    violations.extend(isolated_violations)

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
                "nested_component_depth_mount_cycles_5",
                "nested_component_depth_mount_cycles_20",
                "nested_component_depth_mount_cycles_80",
                "keyed_list_single_insert_100",
                "keyed_list_single_insert_1000",
                "keyed_list_single_insert_5000",
                "keyed_list_single_remove_100",
                "keyed_list_single_remove_1000",
                "keyed_list_single_remove_5000",
            ],
            "informational_only_scenarios": ["keyed_list_initial_mount_10000"],
            "minimum_isolated_keyed_samples_per_optimized_pass": 20,
            "minimum_secondary_samples_per_variant": 7,
            "minimum_initial_mount_warmup_samples_per_variant": 1,
            "isolated_keyed_mutation_sampling": {
                "steady_state_baseline_rows": [100, 1000, 5000],
                "operation_start_rows": {"insert": "N", "remove": "N+1"},
                "operation_end_rows": {"insert": "N+1", "remove": "N"},
                "discarded_warmup_pairs": 1,
                "measured_samples_per_operation": 20,
                "optimized_passes": ["optimized_before", "optimized_after"],
                "base_control_comparison": False,
                "base_control_exclusion_reason": "The pre-optimization keyed matcher scans the old list for every new child; replaying the 20-pair 5,000-row scenarios against it is quadratic and exceeds the CI Playwright deadline.",
                "timed_scope": "one Add click and one paired Remove click are measured separately; input preparation, selectors, and verification are excluded; each pair returns to the steady-state baseline",
                "dom_boundary_counters": "instrumented separately from latency samples",
            },
            "initial_mount_sampling": {
                "100": {"discarded_warmup_samples": 10, "measured_samples": 25},
                "1000": {"discarded_warmup_samples": 1, "measured_samples": 7},
                "10000": {"discarded_warmup_samples": 1, "measured_samples": 7},
            },
            "nested_component_sampling": {
                "depth_5": {
                    "discarded_warmup_mount_unmount_cycles": 10,
                    "measured_samples": 25,
                    "complete_lifecycle_cycles_averaged_per_sample": 100,
                    "cycles_per_timing_batch": 20,
                    "timing_batches_per_sample": 5,
                },
                "depth_20": {
                    "discarded_warmup_mount_unmount_cycles": 10,
                    "measured_samples": 25,
                    "complete_lifecycle_cycles_averaged_per_sample": 100,
                    "cycles_per_timing_batch": 20,
                    "timing_batches_per_sample": 5,
                },
                "depth_80": {
                    "discarded_warmup_mount_unmount_cycles": 10,
                    "measured_samples": 25,
                    "complete_lifecycle_cycles_averaged_per_sample": 100,
                    "cycles_per_timing_batch": 20,
                    "timing_batches_per_sample": 5,
                },
                "measurement_scope": (
                    "Synchronous recursive subtree unmount+mount lifecycle in one initialized WASM runtime; "
                    "each sample averages one hundred complete cycles in five timed batches; "
                    "DOM depth/count and stable control identity are verified after each batch"
                ),
            },
            "maximum_secondary_optimized_a_to_a_median_drift_percent": 15.0,
            "maximum_secondary_median_regression_percent": 25.0,
            "timing_budgets": "absolute timing thresholds remain warning-only; same-runner A/B/A gates the critical improvements, requires stable secondary A/A medians, and checks the isolated 100/1,000/5,000-row workloads across two optimized passes",
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
