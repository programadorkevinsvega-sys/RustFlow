#!/usr/bin/env python3
"""Summarize raw JSON from the pinned js-framework-benchmark runner."""
from __future__ import annotations

import json
import math
import statistics
import sys
from pathlib import Path

UPSTREAM_COMMIT = "fa510a76ffe066af1d30f3e75fa51d3fcbc14009"
CPU_OPERATIONS = [
    "01_run1k",
    "02_replace1k",
    "03_update10th1k_x16",
    "04_select1k",
    "05_swap1k",
    "06_remove-one-1k",
    "07_create10k",
    "08_create1k-after1k_x2",
    "09_clear1k_x8",
]
EXTENDED_CPU_OPERATIONS = [
    "10_update10th10k",
    "11_append1k-after10k",
    "12_clear10k",
]
MEMORY_OPERATIONS = [
    "21_ready-memory",
    "22_run-memory",
    "23_update5-memory",
    "24_run5-memory",
    "25_run-clear-memory",
]
SIZE_OPERATIONS = [
    "41_size-uncompressed",
    "42_size-compressed",
    "43_first-paint",
]
FRAMEWORKS = {
    "react": ("react-hooks", "react"),
    "vue": ("vue",),
    "svelte": ("svelte",),
    "rustflow": ("rustflow",),
}


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    index = max(0, math.ceil(fraction * len(ordered)) - 1)
    return ordered[index]


def summarize(values: list[float]) -> dict[str, object]:
    if not values:
        raise ValueError("cannot summarize empty sample set")
    return {
        "samples": len(values),
        "p50": statistics.median(values),
        "p90": percentile(values, 0.90),
        "p95": percentile(values, 0.95),
        "raw": values,
    }


def identify_framework(name: str) -> str | None:
    lower = name.lower()
    for canonical, needles in FRAMEWORKS.items():
        if any(needle in lower for needle in needles):
            return canonical
    return None


def require_complete(results: dict[str, dict[str, dict[str, object]]], operations: list[str], kind: str) -> None:
    missing = {
        framework: [operation for operation in operations if operation not in values[kind]]
        for framework, values in results.items()
    }
    missing = {framework: operations for framework, operations in missing.items() if operations}
    if missing:
        raise SystemExit(f"incomplete upstream {kind} comparison; missing: " + json.dumps(missing))


def build_comparisons(
    results: dict[str, dict[str, dict[str, object]]],
    operations: list[str],
    kind: str,
    metric: str,
) -> dict[str, dict[str, object]]:
    comparisons: dict[str, dict[str, object]] = {}
    for competitor in ("react", "vue", "svelte"):
        per_operation: dict[str, object] = {}
        ratios: list[float] = []
        for operation in operations:
            rustflow = float(results["rustflow"][kind][operation][metric]["p50"])
            baseline = float(results[competitor][kind][operation][metric]["p50"])
            ratio = rustflow / baseline if baseline else math.inf
            ratios.append(ratio)
            per_operation[operation] = {
                "rustflow_p50": rustflow,
                f"{competitor}_p50": baseline,
                "ratio_rustflow_over_{competitor}": ratio,
                "interpretation": (
                    "rustflow_lower" if ratio < 1 else "tie" if ratio == 1 else "rustflow_higher"
                ),
            }
        comparison: dict[str, object] = {"operations": per_operation}
        if kind == "cpu":
            geometric_mean = math.exp(sum(math.log(value) for value in ratios) / len(ratios))
            comparison["geometric_mean_ratio_rustflow_over_competitor"] = geometric_mean
            comparison["rustflow_aggregate_faster"] = geometric_mean < 1
        comparisons[competitor] = comparison
    return comparisons


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: summarize-js-framework-benchmark.py RESULT_DIRECTORY")
    directory = Path(sys.argv[1])
    results: dict[str, dict[str, dict[str, object]]] = {
        framework: {"cpu": {}, "memory": {}, "size": {}}
        for framework in FRAMEWORKS
    }

    for path in directory.glob("*.json"):
        if path.name in {"frameworks.json", "framework-comparison.json"}:
            continue
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        framework = identify_framework(str(payload.get("framework", "")))
        benchmark = payload.get("benchmark")
        if framework is None:
            continue

        if benchmark in CPU_OPERATIONS:
            values = payload.get("values", {})
            metrics: dict[str, object] = {}
            for metric in ("total", "script", "paint"):
                raw = values.get(metric, {}).get("values")
                if not isinstance(raw, list) or not raw:
                    raise SystemExit(f"missing CPU {metric} samples in {path}")
                numeric = [float(value) for value in raw]
                metrics[metric] = summarize(numeric)
            results[framework]["cpu"][benchmark] = {
                "source_file": path.name,
                "metrics": metrics,
            }
        elif benchmark in MEMORY_OPERATIONS or benchmark in SIZE_OPERATIONS:
            raw = payload.get("values", {}).get("DEFAULT", {}).get("values")
            if not isinstance(raw, list) or not raw:
                raise SystemExit(f"missing {payload.get('type')} samples in {path}")
            numeric = [float(value) for value in raw]
            kind = "memory" if benchmark in MEMORY_OPERATIONS else "size"
            results[framework][kind][benchmark] = {
                "source_file": path.name,
                "unit": "MiB" if kind == "memory" else "KiB" if "size-" in benchmark else "ms",
                "metrics": summarize(numeric),
            }

    require_complete(results, CPU_OPERATIONS, "cpu")
    require_complete(results, EXTENDED_CPU_OPERATIONS, "cpu")
    require_complete(results, MEMORY_OPERATIONS, "memory")
    require_complete(results, SIZE_OPERATIONS, "size")

    comparisons = {
        "cpu_upstream_9": build_comparisons(results, CPU_OPERATIONS, "cpu", "total"),
        "cpu_phase81_extended_3": build_comparisons(results, EXTENDED_CPU_OPERATIONS, "cpu", "total"),
        "cpu_combined_12": build_comparisons(results, CPU_OPERATIONS + EXTENDED_CPU_OPERATIONS, "cpu", "total"),
        "memory": build_comparisons(results, MEMORY_OPERATIONS, "memory", "metrics"),
        "size": build_comparisons(results, SIZE_OPERATIONS, "size", "metrics"),
    }
    report = {
        "schema_version": 2,
        "comparison_kind": "upstream-compatible keyed benchmarks",
        "upstream_commit": UPSTREAM_COMMIT,
        "frameworks": results,
        "comparisons": comparisons,
        "competitive_superiority_claimed": False,
        "notes": [
            "CPU values use the pinned upstream Playwright/CDP trace pipeline and retain total/script/paint metrics.",
            "Memory values use performance.measureUserAgentSpecificMemory after the upstream forced-GC protocol.",
            "Size values use the upstream server response-size hook: uncompressed KiB, Brotli-compressed KiB and first paint.",
            "All four frameworks are measured sequentially in the same CI job and browser job.",
            "The upstream CPU geometric-mean covers the nine original upstream operations; a separate Phase 8.1 extension geometric mean covers the three required 10k scenarios; a combined 12-operation geometric mean is also reported. Memory and size are never blended into CPU indexes.",
            "The pinned upstream runner does not include Lighthouse startup in its ordinary benchmark list; a separate same-runner startup/first-interaction pass is published. The deterministic Phase 8.1 patcher re-enables memory IDs 23 and 24.",
            "The upstream README and executable-runner row-count differences are documented in docs/performance/FRAMEWORK_BENCHMARK_REFERENCE.md.",
        ],
    }
    (directory / "framework-comparison.json").write_text(
        json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8"
    )

    lines = [
        "# Keyed framework comparison",
        "",
        f"Upstream pin: `{UPSTREAM_COMMIT}`",
        "",
        "## CPU — upstream-compatible nine operations",
        "",
        "| Operation | React p50 ms | Vue p50 ms | Svelte p50 ms | RustFlow p50 ms |",
        "|---|---:|---:|---:|---:|",
    ]
    for operation in CPU_OPERATIONS:
        row = [operation]
        for framework in ("react", "vue", "svelte", "rustflow"):
            row.append(f'{results[framework]["cpu"][operation]["metrics"]["total"]["p50"]:.3f}')
        lines.append("| " + " | ".join(row) + " |")
    lines += [
        "",
        "## CPU — Phase 8.1 extension, three 10k operations",
        "",
        "| Operation | React p50 ms | Vue p50 ms | Svelte p50 ms | RustFlow p50 ms |",
        "|---|---:|---:|---:|---:|",
    ]
    for operation in EXTENDED_CPU_OPERATIONS:
        row = [operation]
        for framework in ("react", "vue", "svelte", "rustflow"):
            row.append(f'{results[framework]["cpu"][operation]["metrics"]["total"]["p50"]:.3f}')
        lines.append("| " + " | ".join(row) + " |")
    lines += [
        "",
        "## Memory",
        "",
        "| Scenario | React p50 MiB | Vue p50 MiB | Svelte p50 MiB | RustFlow p50 MiB |",
        "|---|---:|---:|---:|---:|",
    ]
    for operation in MEMORY_OPERATIONS:
        row = [operation]
        for framework in ("react", "vue", "svelte", "rustflow"):
            row.append(f'{results[framework]["memory"][operation]["metrics"]["p50"]:.3f}')
        lines.append("| " + " | ".join(row) + " |")
    lines += [
        "",
        "## Bundle and first paint",
        "",
        "| Metric | React p50 | Vue p50 | Svelte p50 | RustFlow p50 |",
        "|---|---:|---:|---:|---:|",
    ]
    for operation in SIZE_OPERATIONS:
        row = [operation]
        for framework in ("react", "vue", "svelte", "rustflow"):
            item = results[framework]["size"][operation]
            row.append(f'{item["metrics"]["p50"]:.3f} {item["unit"]}')
        lines.append("| " + " | ".join(row) + " |")
    lines += ["", "## Geometric mean CPU ratios", ""]
    for index_name in ("cpu_upstream_9", "cpu_phase81_extended_3", "cpu_combined_12"):
        lines.append(f"### {index_name}")
        for framework, summary in comparisons[index_name].items():
            gm = float(summary["geometric_mean_ratio_rustflow_over_competitor"])
            lines.append(
                f'- RustFlow / {framework}: **{gm:.4f}** '
                f'({"faster in aggregate" if gm < 1 else "not faster in aggregate"}).'
            )
        lines.append("")
    lines += [
        "The combined CPU geometric mean covers the nine upstream-compatible operations plus the three Phase 8.1 10k extensions. Memory, bundle size and first paint are reported separately.",
        "The JSON artifact additionally contains p50/p90/p95 and raw samples for total/script/paint, memory and size.",
        "This report does not assert broad superiority outside this pinned runner/browser environment.",
        "",
    ]
    (directory / "framework-comparison.md").write_text("\n".join(lines), encoding="utf-8")
    print(json.dumps({"comparisons": comparisons, "report": str(directory / "framework-comparison.json")}, indent=2, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
