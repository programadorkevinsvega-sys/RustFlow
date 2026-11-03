#!/usr/bin/env python3
"""Enforce allocator lifecycle budgets from the dedicated stats_alloc benchmark."""

from __future__ import annotations

import json
import sys
from pathlib import Path

BUDGETS = {
    "component_owner.depth_1_states_3": {
        "max_allocations_per_lifecycle": 24,
        "max_bytes_allocated_per_lifecycle": 2304,
        "max_outstanding_requested_bytes_after_setup": 2304,
    },
    "component_owner.depth_5_states_3": {
        "max_allocations_per_lifecycle": 80,
        "max_bytes_allocated_per_lifecycle": 9472,
        "max_outstanding_requested_bytes_after_setup": 6912,
    },
    "component_owner.depth_20_states_3": {
        "max_allocations_per_lifecycle": 296,
        "max_bytes_allocated_per_lifecycle": 38912,
        "max_outstanding_requested_bytes_after_setup": 26112,
    },
    "component_owner.depth_80_states_3": {
        "max_allocations_per_lifecycle": 1120,
        "max_bytes_allocated_per_lifecycle": 156672,
        "max_outstanding_requested_bytes_after_setup": 102912,
    },
    "component_instance.depth_1_states_3": {
        "max_allocations_per_lifecycle": 32,
        "max_bytes_allocated_per_lifecycle": 3072,
        "max_outstanding_requested_bytes_after_setup": 3072,
    },
    "component_instance.depth_5_states_3": {
        "max_allocations_per_lifecycle": 120,
        "max_bytes_allocated_per_lifecycle": 13312,
        "max_outstanding_requested_bytes_after_setup": 10496,
    },
    "component_instance.depth_20_states_3": {
        "max_allocations_per_lifecycle": 440,
        "max_bytes_allocated_per_lifecycle": 54272,
        "max_outstanding_requested_bytes_after_setup": 40192,
    },
    "component_instance.depth_80_states_3": {
        "max_allocations_per_lifecycle": 1720,
        "max_bytes_allocated_per_lifecycle": 217856,
        "max_outstanding_requested_bytes_after_setup": 158976,
    },
}


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(
        "performance-results/native/owner-memory.jsonl"
    )
    records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]
    measurements = {
        item["name"]: item for item in records if item.get("type") == "memory_measurement"
    }
    errors: list[str] = []
    report: dict[str, object] = {
        "schema_version": 1,
        "source": str(path),
        "samples_required": 25,
        "scenario_budgets": {},
        "pending_budget_scenarios": [],
        "violations": [],
        "policy": "Hard bytes/alloc count ceilings for Scope owner trees and real ComponentInstance mount/unmount lifecycles; every sampled lifecycle must return outstanding requested bytes to zero after dispose/unmount.",
    }

    for name in sorted(BUDGETS):
        budget = BUDGETS[name]
        measurement = measurements.get(name)
        if measurement is None:
            errors.append(f"missing memory scenario: {name}")
            continue
        scenario = {
            "samples": measurement["samples"],
            "allocations_per_lifecycle": measurement["allocations_per_lifecycle"],
            "max_allocations_per_lifecycle": measurement.get("max_allocations_per_lifecycle"),
            "bytes_allocated_per_lifecycle": measurement["bytes_allocated_per_lifecycle"],
            "max_bytes_allocated_per_lifecycle": measurement.get("max_bytes_allocated_per_lifecycle"),
            "outstanding_requested_bytes_after_setup_max": measurement[
                "outstanding_requested_bytes_after_setup_max"
            ],
            "outstanding_requested_bytes_after_dispose_mean": measurement[
                "outstanding_requested_bytes_after_dispose_mean"
            ],
            "outstanding_requested_bytes_after_dispose_min": measurement.get(
                "outstanding_requested_bytes_after_dispose_min"
            ),
            "outstanding_requested_bytes_after_dispose_max": measurement.get(
                "outstanding_requested_bytes_after_dispose_max"
            ),
            "budgets": budget,
            "budget_status": "enforced",
        }
        report["scenario_budgets"][name] = scenario

        if int(measurement["samples"]) != 25:
            errors.append(f"{name}: expected exactly 25 samples; got {measurement['samples']}")

        if budget is not None:
            sample_maxima = [
                ("max_allocations_per_lifecycle", "max_allocations_per_lifecycle"),
                ("max_bytes_allocated_per_lifecycle", "max_bytes_allocated_per_lifecycle"),
            ]
            for metric, field in sample_maxima:
                actual = measurement.get(field)
                if actual is None:
                    errors.append(f"{name}: missing per-sample maximum field {field}")
                elif float(actual) > budget[metric]:
                    errors.append(f"{name}: {field} {actual} exceeds budget {budget[metric]}")

            checks = [
                (
                    "outstanding_requested_bytes_after_setup_max",
                    float(measurement["outstanding_requested_bytes_after_setup_max"]),
                    budget["max_outstanding_requested_bytes_after_setup"],
                ),
            ]
            for metric, actual, ceiling in checks:
                if actual > ceiling:
                    errors.append(f"{name}: {metric} {actual} exceeds budget {ceiling}")

        after_dispose = [
            measurement.get("outstanding_requested_bytes_after_dispose_min"),
            measurement.get("outstanding_requested_bytes_after_dispose_max"),
            measurement.get("outstanding_requested_bytes_after_dispose_mean"),
        ]
        if any(value is None for value in after_dispose):
            errors.append(f"{name}: missing per-sample after-dispose minimum/maximum")
        elif any(float(value) != 0.0 for value in after_dispose):
            errors.append(f"{name}: outstanding requested bytes after dispose must be zero; got {after_dispose}")

    expected_scenarios = set(BUDGETS)
    unexpected = sorted(set(measurements) - expected_scenarios)
    if unexpected:
        errors.append(f"unexpected memory scenarios require an explicit budget: {unexpected}")

    report["violations"] = errors
    output = path.with_name("memory-budget-review.json")
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for error in errors:
        print(f"::error::{error}")
    print(f"Wrote memory budget review to {output}")
    if errors:
        return 1
    print(f"Validated {len(BUDGETS)} Scope and ComponentInstance memory scenarios.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
