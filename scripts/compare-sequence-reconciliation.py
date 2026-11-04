#!/usr/bin/env python3
"""Fail-closed same-runner A/B/A comparison for sequential sibling reconciliation."""

from __future__ import annotations

import argparse
import json
import math
import statistics
from pathlib import Path
from typing import Any


MINIMUM_SAMPLES = 10
MINIMUM_IMPROVEMENT_PERCENT = 20.0
MAXIMUM_A_TO_A_DRIFT_PERCENT = 15.0
EXPECTED_SIZE = 5_000


def load_report(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path}: report root must be an object")
    return value


def validate_report(report: dict[str, Any], label: str) -> tuple[list[float], list[str]]:
    violations: list[str] = []
    item = report.get("sequential_sibling_reconciliation")
    if not isinstance(item, dict):
        return [], [f"{label}: sequential_sibling_reconciliation is missing"]

    if item.get("size") != EXPECTED_SIZE:
        violations.append(
            f"{label}: expected {EXPECTED_SIZE} unkeyed siblings, got {item.get('size')!r}"
        )
    if item.get("operation") != "move one keyed anchor from first to last across unkeyed siblings":
        violations.append(f"{label}: benchmark operation does not match the required mixed sibling workload")
    if item.get("warmup_samples") != 2:
        violations.append(
            f"{label}: requires exactly 2 discarded warm-up samples; "
            f"got {item.get('warmup_samples')!r}"
        )
    if item.get("final_dom_children") != EXPECTED_SIZE + 1:
        violations.append(
            f"{label}: expected {EXPECTED_SIZE + 1} final DOM children; "
            f"got {item.get('final_dom_children')!r}"
        )
    if item.get("identity_preserved") is not True:
        violations.append(f"{label}: DOM identity verification did not pass")
    if item.get("final_anchor_position") != "first":
        violations.append(f"{label}: fixture did not return the keyed anchor to index zero")

    raw = item.get("samples_raw")
    if not isinstance(raw, list):
        violations.append(f"{label}: samples_raw is missing or is not an array")
        return [], violations
    try:
        values = [float(value) for value in raw]
    except (TypeError, ValueError):
        violations.append(f"{label}: raw samples contain a non-numeric value")
        return [], violations

    if len(values) < MINIMUM_SAMPLES:
        violations.append(
            f"{label}: requires at least {MINIMUM_SAMPLES} measured samples; got {len(values)}"
        )
    if item.get("measured_samples") != len(values) or item.get("samples") != len(values):
        violations.append(
            f"{label}: declared sample counts do not match the {len(values)} raw samples"
        )
    if any(not math.isfinite(value) or value <= 0 for value in values):
        violations.append(f"{label}: samples contain non-finite or non-positive timings")

    return values, violations


def median(values: list[float]) -> float:
    return statistics.median(values)


def improvement_percent(base_median: float, optimized_median: float) -> float:
    return ((base_median - optimized_median) / base_median) * 100.0 if base_median else 0.0


def a_to_a_drift_percent(before: float, after: float) -> float:
    denominator = min(before, after)
    return abs(after - before) / denominator * 100.0 if denominator else 0.0


def compare_reports(
    optimized_before: dict[str, Any],
    unoptimized: dict[str, Any],
    optimized_after: dict[str, Any],
    *,
    base_sha: str,
    head_sha: str,
) -> dict[str, Any]:
    violations: list[str] = []
    reports = [
        ("optimized-before", optimized_before),
        ("unoptimized", unoptimized),
        ("optimized-after", optimized_after),
    ]
    samples: dict[str, list[float]] = {}

    for label, report in reports:
        values, errors = validate_report(report, label)
        samples[label] = values
        violations.extend(errors)

    browser_versions = {
        json.dumps(report.get("browser", {}), sort_keys=True) for _, report in reports
    }
    node_versions = {report.get("node_version") for _, report in reports}
    if len(browser_versions) != 1:
        violations.append("A/B/A reports do not use the same browser version")
    if len(node_versions) != 1:
        violations.append("A/B/A reports do not use the same Node.js version")

    if any(not values for values in samples.values()):
        return {
            "schema_version": 1,
            "comparison": "same-job same-runner A/B/A",
            "scenario": "sequential_sibling_reconciliation_5000",
            "base_sha": base_sha,
            "head_sha": head_sha,
            "violations": violations,
            "accepted": False,
        }

    before_median = median(samples["optimized-before"])
    base_median = median(samples["unoptimized"])
    after_median = median(samples["optimized-after"])
    before_improvement = improvement_percent(base_median, before_median)
    after_improvement = improvement_percent(base_median, after_median)
    drift = a_to_a_drift_percent(before_median, after_median)

    if before_improvement < MINIMUM_IMPROVEMENT_PERCENT:
        violations.append(
            "optimized-before: expected at least "
            f"{MINIMUM_IMPROVEMENT_PERCENT:.0f}% improvement over base; "
            f"got {before_improvement:.1f}%"
        )
    if after_improvement < MINIMUM_IMPROVEMENT_PERCENT:
        violations.append(
            "optimized-after: expected at least "
            f"{MINIMUM_IMPROVEMENT_PERCENT:.0f}% improvement over base; "
            f"got {after_improvement:.1f}%"
        )
    if drift > MAXIMUM_A_TO_A_DRIFT_PERCENT:
        violations.append(
            "optimized A-to-A median drift "
            f"{drift:.1f}% exceeds {MAXIMUM_A_TO_A_DRIFT_PERCENT:.0f}%"
        )

    return {
        "schema_version": 1,
        "comparison": "same-job same-runner A/B/A",
        "scenario": "sequential_sibling_reconciliation_5000",
        "base_sha": base_sha,
        "head_sha": head_sha,
        "operation": (
            "move one keyed anchor from first to last across 5,000 unkeyed sibling elements"
        ),
        "minimum_samples_per_pass": MINIMUM_SAMPLES,
        "minimum_median_improvement_percent": MINIMUM_IMPROVEMENT_PERCENT,
        "maximum_optimized_a_to_a_median_drift_percent": MAXIMUM_A_TO_A_DRIFT_PERCENT,
        "optimized_before": {
            "samples": len(samples["optimized-before"]),
            "median_ms": before_median,
            "raw_ms": samples["optimized-before"],
        },
        "unoptimized": {
            "samples": len(samples["unoptimized"]),
            "median_ms": base_median,
            "raw_ms": samples["unoptimized"],
        },
        "optimized_after": {
            "samples": len(samples["optimized-after"]),
            "median_ms": after_median,
            "raw_ms": samples["optimized-after"],
        },
        "optimized_before_improvement_percent": before_improvement,
        "optimized_after_improvement_percent": after_improvement,
        "optimized_a_to_a_median_drift_percent": drift,
        "browser": optimized_before.get("browser"),
        "node_version": optimized_before.get("node_version"),
        "violations": violations,
        "accepted": not violations,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--optimized-before", type=Path, required=True)
    parser.add_argument("--unoptimized", type=Path, required=True)
    parser.add_argument("--optimized-after", type=Path, required=True)
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    report = compare_reports(
        load_report(args.optimized_before),
        load_report(args.unoptimized),
        load_report(args.optimized_after),
        base_sha=args.base_sha,
        head_sha=args.head_sha,
    )
    rendered = json.dumps(report, indent=2) + "\n"
    print(rendered, end="")
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    return 0 if report["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
