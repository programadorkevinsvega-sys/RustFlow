#!/usr/bin/env python3
"""Fail-closed summary for the complete pinned js-framework-benchmark matrix."""
from __future__ import annotations
import json
import math
import os
import re
import statistics
import sys
from pathlib import Path
from typing import Any

UPSTREAM_COMMIT = "fa510a76ffe066af1d30f3e75fa51d3fcbc14009"
CPU_OPERATIONS = [
    "01_run1k", "02_replace1k", "03_update10th1k_x16", "04_select1k",
    "05_swap1k", "06_remove-one-1k", "07_create10k",
    "08_create1k-after1k_x2", "09_clear1k_x8",
]
EXTENDED_CPU_OPERATIONS = ["10_update10th10k", "11_append1k-after10k", "12_clear10k"]
MEMORY_OPERATIONS = [
    "21_ready-memory", "22_run-memory", "23_update5-memory",
    "24_run5-memory", "25_run-clear-memory",
]
SIZE_OPERATIONS = ["41_size-uncompressed", "42_size-compressed", "43_first-paint"]
FRAMEWORKS = {
    "react": ("react-hooks", "react"),
    "vue": ("vue",),
    "svelte": ("svelte",),
    "rustflow": ("rustflow",),
}
EXPECTED_SAMPLES = {"cpu": 15, "memory": 5, "size": 1}
STARTUP_WARMUPS = 3
STARTUP_SAMPLES = 15
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]


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


def read_environment(directory: Path, errors: list[str]) -> dict[str, str]:
    path = directory / "environment.txt"
    if not path.is_file():
        errors.append("missing environment.txt")
        return {}
    metadata: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            metadata[key.strip()] = value.strip()

    for key in ("rustflow_commit", "upstream_commit", "node", "npm", "rustc", "chromium_version"):
        if not metadata.get(key):
            errors.append(f"environment.txt is missing {key}")

    actual_sha = metadata.get("rustflow_commit", "").lower()
    if not SHA_RE.fullmatch(actual_sha):
        errors.append("environment.txt has an invalid rustflow_commit SHA")
    expected_sha = os.environ.get("RUSTFLOW_EXPECTED_COMMIT", "").lower()
    if not SHA_RE.fullmatch(expected_sha):
        errors.append("RUSTFLOW_EXPECTED_COMMIT must be the exact 40-character evaluated SHA")
    elif actual_sha != expected_sha:
        errors.append(f"evaluated RustFlow SHA mismatch: environment={actual_sha}, expected={expected_sha}")
    if metadata.get("upstream_commit") != UPSTREAM_COMMIT:
        errors.append(f"environment.txt upstream_commit must equal {UPSTREAM_COMMIT}")
    if not re.search(r"156\.0\.8078\.4", metadata.get("chromium_version", "")):
        errors.append("environment.txt does not record the required Chromium 156.0.8078.4")
    return metadata


def validate_samples(raw: object, expected: int, label: str) -> tuple[list[float] | None, str | None, int]:
    if not isinstance(raw, list):
        return None, f"{label}: samples are missing or not an array", 0
    actual = len(raw)
    if actual != expected:
        return None, f"{label}: expected exactly {expected} samples, got {actual}", actual
    try:
        values = [float(value) for value in raw]
    except (TypeError, ValueError):
        return None, f"{label}: contains a non-numeric sample", actual
    if any(not math.isfinite(value) for value in values):
        return None, f"{label}: contains a non-finite sample", actual
    return values, None, actual


def empty_results() -> dict[str, dict[str, dict[str, Any]]]:
    return {framework: {"cpu": {}, "memory": {}, "size": {}} for framework in FRAMEWORKS}


def empty_status() -> dict[str, dict[str, dict[str, Any]]]:
    required = (
        [(op, "cpu") for op in CPU_OPERATIONS + EXTENDED_CPU_OPERATIONS]
        + [(op, "memory") for op in MEMORY_OPERATIONS]
        + [(op, "size") for op in SIZE_OPERATIONS]
    )
    return {
        framework: {
            op: {
                "status": "missing",
                "kind": kind,
                "expected_samples": EXPECTED_SAMPLES[kind],
                "actual_samples": {},
                "source_file": None,
            }
            for op, kind in required
        }
        for framework in FRAMEWORKS
    }


def build_comparisons(results: dict[str, dict[str, dict[str, Any]]], operations: list[str], kind: str, metric: str) -> dict[str, Any]:
    comparisons: dict[str, Any] = {}
    for competitor in ("react", "vue", "svelte"):
        per_operation: dict[str, Any] = {}
        ratios: list[float] = []
        for operation in operations:
            if kind == "cpu":
                ours = results["rustflow"][kind][operation]["metrics"][metric]
                theirs = results[competitor][kind][operation]["metrics"][metric]
            else:
                ours = results["rustflow"][kind][operation]["metrics"]
                theirs = results[competitor][kind][operation]["metrics"]
            ours_p50, theirs_p50 = float(ours["p50"]), float(theirs["p50"])
            ratio = ours_p50 / theirs_p50 if theirs_p50 else math.inf
            ratios.append(ratio)
            per_operation[operation] = {
                "rustflow_p50": ours_p50,
                f"{competitor}_p50": theirs_p50,
                "ratio_rustflow_over_competitor": ratio,
                "interpretation": "rustflow_lower" if ratio < 1 else "tie" if ratio == 1 else "rustflow_higher",
            }
        summary: dict[str, Any] = {"operations": per_operation}
        if kind == "cpu":
            geometric_mean = math.exp(sum(math.log(value) for value in ratios) / len(ratios))
            summary["geometric_mean_ratio_rustflow_over_competitor"] = geometric_mean
            summary["rustflow_aggregate_faster"] = geometric_mean < 1
        comparisons[competitor] = summary
    return comparisons


def validate_startup(directory: Path, errors: list[str]) -> dict[str, dict[str, Any]]:
    statuses = {
        name: {
            "status": "missing",
            "expected_warmup_rounds": STARTUP_WARMUPS,
            "expected_measured_samples": STARTUP_SAMPLES,
            "actual_measured_samples": 0,
        }
        for name in FRAMEWORKS
    }
    path = directory / "startup-comparison.json"
    if not path.is_file():
        errors.append("missing startup-comparison.json")
        return statuses
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"invalid startup-comparison.json: {error}")
        return statuses
    if data.get("warmup_rounds") != STARTUP_WARMUPS:
        errors.append(f"startup-comparison.json must declare {STARTUP_WARMUPS} discarded warmups")
    if data.get("measured_rounds") != STARTUP_SAMPLES:
        errors.append(f"startup-comparison.json must declare {STARTUP_SAMPLES} measured rounds")
    for framework in FRAMEWORKS:
        samples = data.get("frameworks", {}).get(framework, {}).get("samples_raw")
        count = len(samples) if isinstance(samples, list) else 0
        statuses[framework]["actual_measured_samples"] = count
        if count == STARTUP_SAMPLES:
            statuses[framework]["status"] = "complete"
        else:
            statuses[framework]["status"] = "invalid"
            errors.append(f"startup {framework}: expected exactly {STARTUP_SAMPLES} samples, got {count}")
    return statuses


def write_markdown(directory: Path, report: dict[str, Any]) -> None:
    lines = [
        "# Keyed framework comparison", "",
        f"Status: **{report['status'].upper()}**", "",
        f"RustFlow commit: {report.get('rustflow_commit') or 'unavailable'}",
        f"Upstream commit: {report.get('upstream_commit') or 'unavailable'}", "",
    ]
    if not report["valid"]:
        lines += ["## Validation errors", ""]
        lines.extend(f"- {message}" for message in report["validation_errors"])
        lines += ["", "This is a partial diagnostic artifact, not an accepted official comparison.", ""]
    else:
        lines += [
            "## Per-benchmark status", "",
            "All required CPU, memory, size and startup scenarios passed exact sample-count validation.", "",
            "## CPU geometric means", "",
        ]
        for key, competitors in report["comparisons"].items():
            if not key.startswith("cpu_"):
                continue
            lines.append(f"### {key}")
            for competitor, item in competitors.items():
                value = item["geometric_mean_ratio_rustflow_over_competitor"]
                lines.append(f"- RustFlow / {competitor}: {value:.4f}")
            lines.append("")
        lines += [
            "Raw samples and p50/p90/p95 values are retained in framework-comparison.json.",
            "Memory, size and startup metrics remain separate from CPU geometric means.", "",
        ]
    (directory / "framework-comparison.md").write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: summarize-js-framework-benchmark.py RESULT_DIRECTORY")
    directory = Path(sys.argv[1])
    errors: list[str] = []
    environment = read_environment(directory, errors)
    results = empty_results()
    statuses = empty_status()
    operations = CPU_OPERATIONS + EXTENDED_CPU_OPERATIONS + MEMORY_OPERATIONS + SIZE_OPERATIONS
    kinds = {
        **{op: "cpu" for op in CPU_OPERATIONS + EXTENDED_CPU_OPERATIONS},
        **{op: "memory" for op in MEMORY_OPERATIONS},
        **{op: "size" for op in SIZE_OPERATIONS},
    }

    for path in sorted(directory.rglob("*.json")):
        if path.name in {"frameworks.json", "framework-comparison.json", "startup-comparison.json"}:
            continue
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            errors.append(f"invalid raw JSON {path.relative_to(directory)}: {error}")
            continue
        framework = identify_framework(str(payload.get("framework", "")))
        operation = payload.get("benchmark")
        if framework is None or operation not in kinds:
            continue
        kind = kinds[operation]
        status = statuses[framework][operation]
        if status["status"] != "missing":
            status["status"] = "duplicate"
            errors.append(f"{framework}/{operation}: duplicate raw result")
            continue

        status["source_file"] = str(path.relative_to(directory))
        metric_values: dict[str, list[float]] = {}
        actual: dict[str, int] = {}
        failures: list[str] = []
        expected = EXPECTED_SAMPLES[kind]
        metrics = ("total", "script", "paint") if kind == "cpu" else ("DEFAULT",)
        for metric in metrics:
            raw = payload.get("values", {}).get(metric, {}).get("values")
            values, failure, count = validate_samples(raw, expected, f"{framework}/{operation}/{metric}")
            actual[metric] = count
            if failure:
                failures.append(failure)
            elif values is not None:
                metric_values[metric] = values
        status["actual_samples"] = actual
        if failures:
            status["status"] = "invalid"
            status["errors"] = failures
            errors.extend(failures)
            continue

        status["status"] = "complete"
        if kind == "cpu":
            results[framework][kind][operation] = {
                "source_file": status["source_file"],
                "metrics": {metric: summarize(metric_values[metric]) for metric in metrics},
            }
        else:
            results[framework][kind][operation] = {
                "source_file": status["source_file"],
                "unit": "MiB" if kind == "memory" else "KiB" if "size-" in operation else "ms",
                "metrics": summarize(metric_values["DEFAULT"]),
            }

    for framework in FRAMEWORKS:
        for operation in operations:
            if statuses[framework][operation]["status"] == "missing":
                errors.append(f"missing {framework}/{operation} raw result")

    startup_status = validate_startup(directory, errors)
    valid = not errors
    comparisons: dict[str, Any] = {}
    if valid:
        comparisons = {
            "cpu_upstream_9": build_comparisons(results, CPU_OPERATIONS, "cpu", "total"),
            "cpu_phase81_extended_3": build_comparisons(results, EXTENDED_CPU_OPERATIONS, "cpu", "total"),
            "cpu_combined_12": build_comparisons(results, CPU_OPERATIONS + EXTENDED_CPU_OPERATIONS, "cpu", "total"),
            "memory": build_comparisons(results, MEMORY_OPERATIONS, "memory", "metrics"),
            "size": build_comparisons(results, SIZE_OPERATIONS, "size", "metrics"),
        }

    report = {
        "schema_version": 3,
        "status": "valid" if valid else "incomplete",
        "valid": valid,
        "accepted": valid,
        "comparison_kind": "pinned upstream keyed benchmarks",
        "rustflow_commit": environment.get("rustflow_commit"),
        "expected_rustflow_commit": os.environ.get("RUSTFLOW_EXPECTED_COMMIT"),
        "upstream_commit": environment.get("upstream_commit"),
        "expected_upstream_commit": UPSTREAM_COMMIT,
        "environment": environment,
        "expected_sample_counts": EXPECTED_SAMPLES,
        "startup_sampling": {
            "discarded_warmup_rounds": STARTUP_WARMUPS,
            "measured_samples_per_framework": STARTUP_SAMPLES,
        },
        "benchmark_status": statuses,
        "startup_status": startup_status,
        "frameworks": results,
        "comparisons": comparisons,
        "competitive_superiority_claimed": False,
        "validation_errors": errors,
        "notes": [
            "All 12 CPU scenarios require exactly 15 samples for total/script/paint for all four frameworks.",
            "All five memory scenarios require exactly five samples per framework; each size/first-paint metric requires one result.",
            "The three 10k CPU extensions retain the five warm-up actions in the deterministic upstream patch.",
            "Startup requires three discarded warm-up rounds and 15 measured navigations per framework.",
            "Incomplete runs emit diagnostic JSON and Markdown but are never accepted as official comparisons.",
        ],
    }
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "framework-comparison.json").write_text(
        json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8"
    )
    write_markdown(directory, report)
    print(json.dumps({
        "status": report["status"],
        "valid": report["valid"],
        "rustflow_commit": report["rustflow_commit"],
        "validation_errors": errors,
        "report": str(directory / "framework-comparison.json"),
    }, indent=2))
    if errors:
        for message in errors:
            print(f"::error::{message}")
        return 1
    print("Pinned four-framework comparison is complete and sample-validated.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
