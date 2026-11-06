import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";
import { buildFrameworkOrder, DEFAULT_STARTUP_FRAMEWORKS, fetchFrameworkEntryUrls } from "./framework-startup-helpers.mjs";

const BASE_URL = process.env.BENCHMARK_BASE_URL ?? "http://127.0.0.1:8080";
const CHROME_BIN = process.env.CHROME_BIN;
const FRAMEWORKS = DEFAULT_STARTUP_FRAMEWORKS;
const WARMUPS = 3;
const SAMPLES = 15;

if (!CHROME_BIN) {
  throw new Error("CHROME_BIN must point to the verified Chromium executable");
}

function percentile(values, fraction) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(fraction * sorted.length) - 1)];
}

function summarize(values) {
  const usable = values.filter((value) => typeof value === "number" && Number.isFinite(value));
  if (usable.length === 0) return null;
  return {
    samples: usable.length,
    p50: percentile(usable, 0.50),
    p90: percentile(usable, 0.90),
    p95: percentile(usable, 0.95),
    raw: usable,
  };
}

async function oneSample(browser, framework, entryUrl) {
  const page = await browser.newPage({
    viewport: { width: 1280, height: 720 },
    reducedMotion: "reduce",
  });
  try {
    await page.goto(entryUrl, {
      waitUntil: "networkidle",
    });
    await page.locator("#run").waitFor({ state: "visible" });

    const navigation = await page.evaluate(() => {
      const nav = performance.getEntriesByType("navigation")[0];
      const paints = Object.fromEntries(
        performance.getEntriesByType("paint").map((entry) => [entry.name, entry.startTime]),
      );
      const resources = performance.getEntriesByType("resource");
      const resourceGroups = {
        script: { transfer_bytes: 0, encoded_bytes: 0, duration_ms_sum: 0, count: 0 },
        wasm: { transfer_bytes: 0, encoded_bytes: 0, duration_ms_sum: 0, count: 0 },
        css: { transfer_bytes: 0, encoded_bytes: 0, duration_ms_sum: 0, count: 0 },
        other: { transfer_bytes: 0, encoded_bytes: 0, duration_ms_sum: 0, count: 0 },
      };
      for (const resource of resources) {
        const name = resource.name.toLowerCase();
        const kind = name.includes(".wasm")
          ? "wasm"
          : name.includes(".js")
            ? "script"
            : name.includes(".css")
              ? "css"
              : "other";
        const target = resourceGroups[kind];
        target.transfer_bytes += resource.transferSize || 0;
        target.encoded_bytes += resource.encodedBodySize || 0;
        target.duration_ms_sum += resource.duration || 0;
        target.count += 1;
      }
      const runtimeInit = performance.getEntriesByName("rustflow:runtime:init")[0];
      return {
        navigation: {
          response_start_ms: nav ? nav.responseStart : null,
          response_end_ms: nav ? nav.responseEnd : null,
          dom_interactive_ms: nav ? nav.domInteractive : null,
          dom_content_loaded_ms: nav ? nav.domContentLoadedEventEnd : null,
          load_event_end_ms: nav ? nav.loadEventEnd : null,
        },
        paint: {
          first_paint_ms: paints["first-paint"] ?? null,
          first_contentful_paint_ms: paints["first-contentful-paint"] ?? null,
        },
        resources: resourceGroups,
        total_transfer_bytes: Object.values(resourceGroups).reduce((sum, entry) => sum + entry.transfer_bytes, 0),
        total_encoded_bytes: Object.values(resourceGroups).reduce((sum, entry) => sum + entry.encoded_bytes, 0),
        rustflow_runtime_init_ms: runtimeInit ? runtimeInit.duration : null,
      };
    });

    const started = await page.evaluate(() => performance.now());
    await page.locator("#run").click();
    await page.waitForFunction(
      () => document.querySelectorAll("tbody > tr").length === 1_000,
      null,
      { timeout: 30_000 },
    );
    await page.evaluate(() => new Promise((resolveFrame) => requestAnimationFrame(() => requestAnimationFrame(resolveFrame))));
    const firstInteraction = await page.evaluate((start) => performance.now() - start, started);

    return {
      ...navigation,
      first_interaction_1000_rows_ms: firstInteraction,
      final_dom_rows: await page.locator("tbody > tr").count(),
    };
  } finally {
    await page.close();
  }
}

const frameworkEntryUrls = await fetchFrameworkEntryUrls(BASE_URL, FRAMEWORKS);
const browser = await chromium.launch({
  headless: true,
  executablePath: CHROME_BIN,
});
const results = {};
try {
  for (const framework of FRAMEWORKS) {
    results[framework] = [];
  }

  // Three discarded warmups per implementation; rotate the order in every
  // measured round so no framework always benefits from the first position.
  const order = [];
  for (let index = 0; index < WARMUPS + SAMPLES; index += 1) {
    const rotation = index % FRAMEWORKS.length;
    const round = FRAMEWORKS.map((_, offset) => FRAMEWORKS[(offset + rotation) % FRAMEWORKS.length]);
    order.push(round);
    for (const framework of round) {
      const measured = await oneSample(browser, framework, frameworkEntryUrls[framework]);
      if (measured.final_dom_rows !== 1_000) {
        throw new Error(`${framework}: startup benchmark did not render 1,000 rows`);
      }
      if (index >= WARMUPS) results[framework].push(measured);
    }
  }
} finally {
  await browser.close();
}

const metrics = [
  "response_start_ms",
  "response_end_ms",
  "dom_interactive_ms",
  "dom_content_loaded_ms",
  "load_event_end_ms",
  "first_paint_ms",
  "first_contentful_paint_ms",
  "total_transfer_bytes",
  "total_encoded_bytes",
  "first_interaction_1000_rows_ms",
];
const report = {
  schema_version: 1,
  measurement_kind: "same-runner keyed application startup and first interaction",
  browser: "Chromium executable passed by CI; exact version is recorded in environment.txt",
  upstream_commit: "fa510a76ffe066af1d30f3e75fa51d3fcbc14009",
  warmup_rounds: WARMUPS,
  measured_rounds: SAMPLES,
  framework_order_by_round: order,
  frameworks: {},
  limitations: [
    "Navigation timing and paint timings are browser performance entries.",
    "Resource transfer bytes are observed same-origin resource entries; resource duration sums are not a wall-clock download duration.",
    "RustFlow runtime init is reported only when its explicit rustflow:runtime:init mark exists; it is not directly comparable to a framework-specific internal runtime phase.",
    "Parse/compile/evaluate are not inferred from navigation timing. A separate trace/profiler protocol is required to split those phases reliably.",
    "First interaction is a real #run click that commits 1,000 keyed rows and waits two animation frames.",
    "This startup report is separate from the upstream CPU geometric-mean index.",
  ],
};

for (const framework of FRAMEWORKS) {
  const samples = results[framework];
  const allKeys = [
    "navigation",
    "paint",
    "resources",
    "total_transfer_bytes",
    "total_encoded_bytes",
    "rustflow_runtime_init_ms",
    "first_interaction_1000_rows_ms",
  ];
  const aggregate = {};
  for (const key of allKeys) {
    if (key === "navigation" || key === "paint" || key === "resources") continue;
    aggregate[key] = summarize(samples.map((sample) => sample[key]).filter((value) => value !== null));
  }
  for (const metric of [
    "response_start_ms",
    "response_end_ms",
    "dom_interactive_ms",
    "dom_content_loaded_ms",
    "load_event_end_ms",
    "first_paint_ms",
    "first_contentful_paint_ms",
  ]) {
    const [group, field] = metric.includes("paint") ? ["paint", metric] : ["navigation", metric];
    aggregate[metric] = summarize(samples.map((sample) => sample[group][field]).filter((value) => value !== null));
  }
  aggregate.resources = {};
  for (const resourceType of ["script", "wasm", "css", "other"]) {
    aggregate.resources[resourceType] = {};
    for (const field of ["transfer_bytes", "encoded_bytes", "duration_ms_sum", "count"]) {
      aggregate.resources[resourceType][field] = summarize(samples.map((sample) => sample.resources[resourceType][field]));
    }
  }
  aggregate.samples_raw = samples;
  report.frameworks[framework] = aggregate;
}

const outputDir = resolve(process.argv[2] ?? "performance-results/official");
mkdirSync(outputDir, { recursive: true });
writeFileSync(resolve(outputDir, "startup-comparison.json"), JSON.stringify(report, null, 2) + "\n");
const lines = [
  "# Keyed framework startup and first interaction",
  "",
  "| Metric | React p50 | Vue p50 | Svelte p50 | RustFlow p50 |",
  "|---|---:|---:|---:|---:|",
];
const tableMetrics = [
  ["response_start_ms", "Response start (ms)"],
  ["dom_content_loaded_ms", "DOMContentLoaded (ms)"],
  ["load_event_end_ms", "Load event end (ms)"],
  ["first_paint_ms", "First paint (ms)"],
  ["first_contentful_paint_ms", "First contentful paint (ms)"],
  ["first_interaction_1000_rows_ms", "First interaction + 1k rows (ms)"],
  ["total_transfer_bytes", "Resource transfer bytes"],
  ["total_encoded_bytes", "Encoded resource bytes"],
];
for (const [key, label] of tableMetrics) {
  const values = FRAMEWORKS.map((framework) => report.frameworks[framework][key]?.p50);
  lines.push("| " + [label, ...values.map((value) => value === undefined ? "n/a" : value.toFixed(2))].join(" | ") + " |");
}
lines.push(
  "",
  "This report uses same-runner, same-browser, sequential cold-page navigations with rotated framework order. It is not part of the official CPU geometric mean.",
  "",
);
writeFileSync(resolve(outputDir, "startup-comparison.md"), lines.join("\n"));
console.log(JSON.stringify({ output: resolve(outputDir, "startup-comparison.json"), frameworks: FRAMEWORKS }, null, 2));
