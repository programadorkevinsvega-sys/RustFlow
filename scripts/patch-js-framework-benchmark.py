#!/usr/bin/env python3
"""Apply the Phase 8.1 comparative extension to the exact pinned upstream source."""
from __future__ import annotations

import sys
from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    source = path.read_text(encoding="utf-8")
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one patch anchor, got {count}: {old[:100]!r}")
    path.write_text(source.replace(old, new, 1), encoding="utf-8")


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: patch-js-framework-benchmark.py UPSTREAM_CHECKOUT")
    root = Path(sys.argv[1])
    common = root / "webdriver-ts/src/benchmarksCommon.ts"
    playwright = root / "webdriver-ts/src/benchmarksPlaywright.ts"
    playwright_access = root / "webdriver-ts/src/playwrightAccess.ts"
    puppeteer_access = root / "webdriver-ts/src/puppeteerAccess.ts"

    replace_once(
        common,
        '  _09 = "09_clear1k_x8",\n  _21 = "21_ready-memory",',
        '  _09 = "09_clear1k_x8",\n'
        '  _10 = "10_update10th10k",\n'
        '  _11 = "11_append1k-after10k",\n'
        '  _12 = "12_clear10k",\n'
        '  _21 = "21_ready-memory",',
    )

    replace_once(
        common,
        '  },\n];\n\nexport const memBenchmarkInfosArray: Array<MemBenchmarkInfo> = [',
        '  },\n'
        '  {\n'
        '    id: Benchmark._10,\n'
        '    label: "partial update 10,000 rows",\n'
        '    warmupCount: 5,\n'
        '    description: "updating every 10th row in a 10,000-row table.",\n'
        '    type: BenchmarkType.CPU,\n'
        '    allowBatching: true,\n'
        '    layoutEventRequired: true,\n'
        '    additionalNumberOfRuns: 0,\n'
        '  },\n'
        '  {\n'
        '    id: Benchmark._11,\n'
        '    label: "append 1,000 rows to 10,000 rows",\n'
        '    warmupCount: 5,\n'
        '    description: "appending 1,000 rows to a 10,000-row table.",\n'
        '    type: BenchmarkType.CPU,\n'
        '    allowBatching: true,\n'
        '    layoutEventRequired: true,\n'
        '    additionalNumberOfRuns: 0,\n'
        '  },\n'
        '  {\n'
        '    id: Benchmark._12,\n'
        '    label: "clear 10,000 rows",\n'
        '    warmupCount: 5,\n'
        '    description: "clearing a table with 10,000 rows.",\n'
        '    type: BenchmarkType.CPU,\n'
        '    allowBatching: true,\n'
        '    layoutEventRequired: true,\n'
        '    additionalNumberOfRuns: 0,\n'
        '  },\n'
        '];\n\nexport const memBenchmarkInfosArray: Array<MemBenchmarkInfo> = [',
    )

    replace_once(
        common,
        '  | typeof Benchmark._09\n  | typeof Benchmark._30',
        '  | typeof Benchmark._09\n'
        '  | typeof Benchmark._10\n'
        '  | typeof Benchmark._11\n'
        '  | typeof Benchmark._12\n'
        '  | typeof Benchmark._30',
    )

    replace_once(
        common,
        '  [Benchmark._09]: 4,\n};',
        '  [Benchmark._09]: 4,\n'
        '  [Benchmark._10]: 4,\n'
        '  [Benchmark._12]: 4,\n'
        '};',
    )

    replace_once(
        common,
        '  // {\n  //   id: Benchmark._23,\n  //   label: "update every 10th row for 1k rows (5 cycles)",\n  //   description: "Memory usage after clicking update every 10th row 5 times",\n  //   type: BenchmarkType.MEM,\n  // },\n'
        '  // {\n  //   id: Benchmark._24,\n  //   label: "replace 1k rows (5 cycles)",\n  //   description: "Memory usage after clicking create 1000 rows 5 times",\n  //   type: BenchmarkType.MEM,\n  // },',
        '  {\n'
        '    id: Benchmark._23,\n'
        '    label: "update every 10th row for 1k rows (5 cycles)",\n'
        '    description: "Memory usage after clicking update every 10th row 5 times",\n'
        '    type: BenchmarkType.MEM,\n'
        '  },\n'
        '  {\n'
        '    id: Benchmark._24,\n'
        '    label: "replace 1k rows (5 cycles)",\n'
        '    description: "Memory usage after clicking create 1000 rows 5 times",\n'
        '    type: BenchmarkType.MEM,\n'
        '  },',
    )

    replace_once(
        playwright,
        'export const benchCreateClear5Memory = new (class extends MemBenchmarkPlaywright {',
        'export const benchReplace5Memory = new (class extends MemBenchmarkPlaywright {\n'
        '  constructor() {\n'
        '    super(memBenchmarkInfos[Benchmark._24]);\n'
        '  }\n'
        '  async init(browser: Browser, page: Page) {\n'
        '    await checkElementExists(page, "#run");\n'
        '  }\n'
        '  async run(browser: Browser, page: Page) {\n'
        '    for (let i = 0; i < 5; i++) {\n'
        '      await clickElement(page, "#run");\n'
        '      await checkElementContainsText(page, "tbody>tr:nth-of-type(1000)>td:nth-of-type(1)", (1000 * (i + 1)).toFixed());\n'
        '    }\n'
        '  }\n'
        '})();\n\n'
        'export const benchCreateClear5Memory = new (class extends MemBenchmarkPlaywright {',
    )

    replace_once(
        playwright,
        'await checkElementContainsText(page, "tbody>tr:nth-of-type(1)>td:nth-of-type(2)>a", " !!!".repeat(i));',
        'await checkElementContainsText(page, "tbody>tr:nth-of-type(1)>td:nth-of-type(2)>a", " !!!".repeat(i + 1));',
    )

    replace_once(
        playwright,
        'export const benchmarks = [\n',
        'export const benchUpdate10K = new (class extends CPUBenchmarkPlaywright {\n'
        '  constructor() {\n'
        '    super(cpuBenchmarkInfos[Benchmark._10]);\n'
        '  }\n'
        '  async init(browser: Browser, page: Page) {\n'
        '    await checkElementExists(page, "#runlots");\n'
        '    await clickElement(page, "#runlots");\n'
        '    await checkElementExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '    for (let i = 0; i < this.benchmarkInfo.warmupCount; i++) {\n'
        '      await clickElement(page, "#update");\n'
        '      await checkElementContainsText(page, "tbody>tr:nth-of-type(9991)>td:nth-of-type(2)>a", " !!!".repeat(i + 1));\n'
        '    }\n'
        '  }\n'
        '  async run(browser: Browser, page: Page) {\n'
        '    await clickElement(page, "#update");\n'
        '    await checkElementContainsText(page, "tbody>tr:nth-of-type(9991)>td:nth-of-type(2)>a", " !!!".repeat(this.benchmarkInfo.warmupCount + 1));\n'
        '  }\n'
        '})();\n\n'
        'export const benchAppendTo10K = new (class extends CPUBenchmarkPlaywright {\n'
        '  constructor() {\n'
        '    super(cpuBenchmarkInfos[Benchmark._11]);\n'
        '  }\n'
        '  async init(browser: Browser, page: Page) {\n'
        '    await checkElementExists(page, "#runlots");\n'
        '    for (let i = 0; i < this.benchmarkInfo.warmupCount; i++) {\n'
        '      await clickElement(page, "#runlots");\n'
        '      await checkElementExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '      await clickElement(page, "#add");\n'
        '      await checkElementExists(page, "tbody>tr:nth-of-type(11000)>td:nth-of-type(1)");\n'
        '    }\n'
        '    await clickElement(page, "#runlots");\n'
        '    await checkElementExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '  }\n'
        '  async run(browser: Browser, page: Page) {\n'
        '    await clickElement(page, "#add");\n'
        '    await checkElementExists(page, "tbody>tr:nth-of-type(11000)>td:nth-of-type(1)");\n'
        '  }\n'
        '})();\n\n'
        'export const benchClear10K = new (class extends CPUBenchmarkPlaywright {\n'
        '  constructor() {\n'
        '    super(cpuBenchmarkInfos[Benchmark._12]);\n'
        '  }\n'
        '  async init(browser: Browser, page: Page) {\n'
        '    await checkElementExists(page, "#runlots");\n'
        '    for (let i = 0; i < this.benchmarkInfo.warmupCount; i++) {\n'
        '      await clickElement(page, "#runlots");\n'
        '      await checkElementExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '      await clickElement(page, "#clear");\n'
        '      await checkElementNotExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '    }\n'
        '    await clickElement(page, "#runlots");\n'
        '    await checkElementExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '  }\n'
        '  async run(browser: Browser, page: Page) {\n'
        '    await clickElement(page, "#clear");\n'
        '    await checkElementNotExists(page, "tbody>tr:nth-of-type(10000)>td:nth-of-type(1)");\n'
        '  }\n'
        '})();\n\n'
        'export const benchmarks = [\n',
    )

    replace_once(
        playwright,
        '   benchClear, \n   benchReadyMemory, \n   benchRunMemory, \n  //  benchRun10KMemory,\n  //  benchUpdate5Memory,\n  //  benchReplace5Memory, \n   benchCreateClear5Memory,',
        '   benchClear,\n'
        '   benchUpdate10K,\n'
        '   benchAppendTo10K,\n'
        '   benchClear10K,\n'
        '   benchReadyMemory,\n'
        '   benchRunMemory,\n'
        '   benchUpdate5Memory,\n'
        '   benchReplace5Memory,\n'
        '   benchCreateClear5Memory,',
    )
    replace_once(
        playwright_access,
        'function browserPath(benchmarkOptions: BenchmarkOptions) {\n  if (benchmarkOptions.chromeBinaryPath) return benchmarkOptions.chromeBinaryPath;',
        'function browserPath(benchmarkOptions: BenchmarkOptions) {\n'
        '  if (benchmarkOptions.chromeBinaryPath) return benchmarkOptions.chromeBinaryPath;\n'
        '  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;',
    )
    replace_once(
        puppeteer_access,
        'function browserPath(benchmarkOptions: BenchmarkOptions) {\n  if (benchmarkOptions.chromeBinaryPath) return benchmarkOptions.chromeBinaryPath;',
        'function browserPath(benchmarkOptions: BenchmarkOptions) {\n'
        '  if (benchmarkOptions.chromeBinaryPath) return benchmarkOptions.chromeBinaryPath;\n'
        '  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;',
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
