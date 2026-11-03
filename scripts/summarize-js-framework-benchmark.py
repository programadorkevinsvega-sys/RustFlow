#!/usr/bin/env python3
"""Summarize the pinned js-framework-benchmark raw JSON files without rewriting them."""
from __future__ import annotations

import json
import math
import statistics
import sys
from pathlib import Path

OPERATIONS = [
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


def identify_framework(name: str) -> str | None:
    lower = name.lower()
    for canonical, needles in FRAMEWORKS.items():
        if any(needle in lower for needle in needles):
            return canonical
    return None


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: summarize-js-framework-benchmark.py RESULT_DIRECTORY")
    directory = Path(sys.argv[1])
    results: dict[str, dict[str, dict[str, object]]] = {
        framework: {} for framework in FRAMEWORKS
    }

    for path in directory.glob("*.json"):
        if path.name in {"frameworks.json", "framework-comparison.json"}:
            continue
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        benchmark = payload.get("benchmark")
        framework = identify_framework(str(payload.get("framework", "")))
        if framework is None or benchmark not in OPERATIONS:
            continue
        total = payload.get("values", {}).get("total", {}).get("values")
        if not isinstance(total, list) or not total:
            raise SystemExit(f"missing CPU total samples in {path}")
        samples = [float(value) for value in total]
        values = payload.get("values", {})
        metrics = {}
        for metric in ("total", "script", "paint"):
            raw = values.get(metric, {}).get("values")
            if isinstance(raw, list) and raw:
                numeric = [float(value) for value in raw]
                metrics[metric] = {
                    "samples": len(numeric),
                    "p50": statistics.median(numeric),
                    "p90": percentile(numeric, 0.90),
                    "p95": percentile(numeric, 0.95),
                    "raw": numeric,
                }
        results[framework][benchmark] = {
            "source_file": path.name,
            "metrics": metrics,
        }

    missing = {
        framework: [operation for operation in OPERATIONS if operation not in values]
        for framework, values in results.items()
    }
    missing = {framework: operations for framework, operations in missing.items() if operations}
    if missing:
        raise SystemExit("incomplete official CPU comparison; missing: " + json.dumps(missing))

    comparisons = {}
    for competitor in ("react", "vue", "svelte"):
        per_operation = {}
        ratios = []
        for operation in OPERATIONS:
            rustflow = results["rustflow"][operation]["metrics"]["total"]["p50"]
            baseline = results[competitor][operation]["metrics"]["total"]["p50"]
            ratio = rustflow / baseline if baseline else math.inf
            ratios.append(ratio)
            per_operation[operation] = {
                "rustflow_p50_ms": rustflow,
                f"{competitor}_p50_ms": baseline,
                "ratio_rustflow_over_{competitor}": ratio,
                "interpretation": "rustflow_faster" if ratio < 1 else "tie" if ratio == 1 else "rustflow_slower",
            }
        geometric_mean = math.exp(sum(math.log(value) for value in ratios) / len(ratios))
        comparisons[competitor] = {
            "geometric_mean_ratio_rustflow_over_competitor": geometric_mean,
            "rustflow_aggregate_faster": geometric_mean < 1,
            "operations": per_operation,
        }

    metadata_path = directory / "environment.txt"
    report = {
        "schema_version": 1,
        "comparison_kind": "upstream-compatible keyed CPU benchmarks",
        "upstream_commit": "fa510a76ffe066af1d30f3e75fa51d3fcbc14009",
        "operations": OPERATIONS,
        "frameworks": results,
        "comparisons": comparisons,
        "competitive_superiority_claimed": False,
        "notes": [
            "CPU values use the pinned upstream Playwright/CDP trace pipeline, not performance.now around a click.",
            "Only exact same-job/same-runner samples are compared.",
            "The upstream README versus executable-runner discrepancies are documented in docs/performance/FRAMEWORK_BENCHMARK_REFERENCE.md.",
            "Memory and size files are retained raw alongside this summary; they are not mixed into the CPU geometric-mean index.",
        ],
    }
    (directory / "framework-comparison.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )

    lines = [
        "# Keyed framework comparison",
        "",
        "Upstream pin: `fa510a76ffe066af1d30f3e75fa51d3fcbc14009`",
        "",
        "| Operation | React p50 (ms) | Vue p50 (ms) | Svelte p50 (ms) | RustFlow p50 (ms) |",
        "|---|---:|---:|---:|---:|",
    ]
    for operation in OPERATIONS:
        row = [operation]
        for framework in ("react", "vue", "svelte", "rustflow"):
            row.append(f'{results[framework][operation]["metrics"]["total"]["p50"]:.3f}')
        lines.append("| " + " | ".join(row) + " |")
    lines += ["", "## Geometric mean ratios", ""]
    for framework, summary in comparisons.items():
        lines.append(
            f'- RustFlow / {framework}: **{summary["geometric_mean_ratio_rustflow_over_competitor"]:.4f}** '
            f'({"faster" if summary["rustflow_aggregate_faster"] else "not faster in aggregate"}).'
        )
    lines += [
        "",
        "This report contains same-runner measurements from the upstream trace runner. It does not assert broad superiority outside this pinned environment.",
        "",
    ]
    (directory / "framework-comparison.md").write_text("\n".join(lines), encoding="utf-8")
    print(json.dumps({"comparisons": comparisons, "report": str(directory / "framework-comparison.json")}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
