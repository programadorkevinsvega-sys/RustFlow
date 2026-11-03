#!/usr/bin/env python3
"""Compare optimized and base keyed DOM reconciliation on the same CI runner."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
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

    optimized_insert = combine_samples(
        [opt_before, opt_after], "keyed_list_incremental_insertions",
        "keyed_list_incremental_insert_100",
    )
    control_insert = combine_samples(
        [control], "keyed_list_incremental_insertions",
        "keyed_list_incremental_insert_100",
    )
    optimized_reverse = (
        scenario(opt_before, "keyed_list_reverse_100", "keyed_list_reverse_100")["samples_raw"]
        + scenario(opt_after, "keyed_list_reverse_100", "keyed_list_reverse_100")["samples_raw"]
    )
    control_reverse = scenario(
        control, "keyed_list_reverse_100", "keyed_list_reverse_100"
    )["samples_raw"]

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
    for name, data in comparisons.items():
        before = data["unoptimized"]["median_ms"]
        after = data["optimized"]["median_ms"]
        gain = improvement(before, after)
        data["median_improvement_percent"] = gain
        data["p95_improvement_percent"] = improvement(
            data["unoptimized"]["p95_ms"], data["optimized"]["p95_ms"]
        )
        if gain < 20.0:
            violations.append(
                f"{name}: expected at least 20% median improvement on same-runner A/B; got {gain:.1f}%"
            )

    # Ensure the stress scenario was actually exercised with 10,000 mounted rows
    # in both variants; timings remain informational due the small sample count.
    large_name = "keyed_list_initial_mount_10000"
    for label, report in [("optimized-before", opt_before), ("unoptimized", control), ("optimized-after", opt_after)]:
        large = scenario(report, "keyed_list_initial_mounts", large_name)
        if large.get("final_dom_rows") != 10_000 or len(large.get("samples_raw", [])) < 3:
            violations.append(f"{label}: expected three verified 10,000-row mount samples")

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
        "policy": {
            "critical_scenarios": [
                "keyed_list_incremental_insert_100",
                "keyed_list_reverse_100",
            ],
            "minimum_median_improvement_percent": 20.0,
            "timing_budgets": "warning-only; this script gates only the controlled relative improvement and 10k stress-scenario integrity",
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
