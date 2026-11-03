import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const wasmPath = resolve(process.cwd(), "pkg/rustflow_browser_bg.wasm");
const distAssetsPath = resolve(process.cwd(), "dist/assets");
const reportPath = resolve(process.cwd(), "performance-results/bundle-size.json");

if (!existsSync(wasmPath)) {
  console.error("Missing WASM output. Run npm run build:wasm first.");
  process.exit(1);
}
if (!existsSync(distAssetsPath)) {
  console.error("Missing Vite production assets. Run npm run build first.");
  process.exit(1);
}

function listFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return listFiles(path);
    if (!entry.isFile() || entry.name.endsWith(".map")) return [];
    return [path];
  });
}

function compress(bytes) {
  return {
    gzip_bytes: gzipSync(bytes, { level: 9 }).byteLength,
    brotli_bytes: brotliCompressSync(bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).byteLength,
  };
}

function summarize(files) {
  const summary = {
    files: files.length,
    raw_bytes: 0,
    gzip_bytes: 0,
    brotli_bytes: 0,
  };
  for (const file of files) {
    const bytes = readFileSync(file);
    const compressed = compress(bytes);
    summary.raw_bytes += bytes.byteLength;
    summary.gzip_bytes += compressed.gzip_bytes;
    summary.brotli_bytes += compressed.brotli_bytes;
  }
  return summary;
}

const assetFiles = listFiles(distAssetsPath);
const jsFiles = assetFiles.filter((path) => path.endsWith(".js"));
const cssFiles = assetFiles.filter((path) => path.endsWith(".css"));
const wasmFiles = assetFiles.filter((path) => path.endsWith(".wasm"));
const otherFiles = assetFiles.filter(
  (path) => !path.endsWith(".js") && !path.endsWith(".css") && !path.endsWith(".wasm"),
);
const javascriptSummary = summarize(jsFiles);
const cssSummary = summarize(cssFiles);
const wasmAssetSummary = summarize(wasmFiles);
const otherSummary = summarize(otherFiles);
const totalSummary = summarize(assetFiles);
const wasmBytes = readFileSync(wasmPath);
const rawWasmBytes = statSync(wasmPath).size;
const wasmCompressed = compress(wasmBytes);

// Initial hard bundle ceilings leave roughly 20–30% headroom above the first
// measured production artifact. They are deterministic byte limits, not timing
// thresholds. The historical 1.5 MiB WASM ceiling remains recorded for context.
const budgets = {
  wasmRawBytes: 600 * 1024,
  wasmGzipBytes: 220 * 1024,
  wasmBrotliBytes: 160 * 1024,
  javascriptRawBytes: 24 * 1024,
  javascriptGzipBytes: 7 * 1024,
  javascriptBrotliBytes: 6 * 1024,
  cssRawBytes: 32 * 1024,
  cssGzipBytes: 8 * 1024,
  cssBrotliBytes: 8 * 1024,
  totalGzipBytes: 225_280,
  totalBrotliBytes: 174_080,
};
const legacyWasmBudgetBytes = 1.5 * 1024 * 1024;

const report = {
  schema_version: 1,
  generated_at: new Date().toISOString(),
  node_version: process.version,
  commit: process.env.GITHUB_SHA || process.env.RUSTFLOW_BENCH_COMMIT || "unspecified",
  source_wasm: {
    path: "pkg/rustflow_browser_bg.wasm",
    raw_bytes: rawWasmBytes,
    gzip_bytes: wasmCompressed.gzip_bytes,
    brotli_bytes: wasmCompressed.brotli_bytes,
    legacy_raw_budget_bytes: legacyWasmBudgetBytes,
    active_raw_budget_bytes: budgets.wasmRawBytes,
    active_raw_budget_passed: rawWasmBytes <= budgets.wasmRawBytes,
    gzip_budget_bytes: budgets.wasmGzipBytes,
    gzip_budget_passed: wasmCompressed.gzip_bytes <= budgets.wasmGzipBytes,
    brotli_budget_bytes: budgets.wasmBrotliBytes,
    brotli_budget_passed: wasmCompressed.brotli_bytes <= budgets.wasmBrotliBytes,
  },
  vite_production_assets: {
    directory: "dist/assets",
    javascript: javascriptSummary,
    css: cssSummary,
    wasm: wasmAssetSummary,
    other: otherSummary,
    total: totalSummary,
    files: assetFiles.map((path) => ({
      path: path.replace(resolve(process.cwd(), "dist") + "/", "dist/"),
      bytes: statSync(path).size,
    })),
  },
  budgets: {
    ...budgets,
    javascript_raw_passed: javascriptSummary.raw_bytes <= budgets.javascriptRawBytes,
    javascript_gzip_passed: javascriptSummary.gzip_bytes <= budgets.javascriptGzipBytes,
    javascript_brotli_passed: javascriptSummary.brotli_bytes <= budgets.javascriptBrotliBytes,
    css_raw_passed: cssSummary.raw_bytes <= budgets.cssRawBytes,
    css_gzip_passed: cssSummary.gzip_bytes <= budgets.cssGzipBytes,
    css_brotli_passed: cssSummary.brotli_bytes <= budgets.cssBrotliBytes,
    total_gzip_passed: totalSummary.gzip_bytes <= budgets.totalGzipBytes,
    total_brotli_passed: totalSummary.brotli_bytes <= budgets.totalBrotliBytes,
  },
  notes: [
    "Compressed totals are per-file compression sums, not a simulated HTTP transfer with shared cross-file dictionaries.",
    "Vite production assets exclude source maps from the aggregate.",
    "Active hard ceilings are initial regression guards with measured baseline headroom; timing budgets remain informational because runner noise is hardware-dependent.",
  ],
};

mkdirSync(dirname(reportPath), { recursive: true });
writeFileSync(reportPath, JSON.stringify(report, null, 2) + "\n");

console.log(
  "RustFlow WASM: " +
    rawWasmBytes +
    " raw, " +
    wasmCompressed.gzip_bytes +
    " gzip, " +
    wasmCompressed.brotli_bytes +
    " Brotli bytes",
);
for (const [name, result] of Object.entries(report.vite_production_assets)) {
  if (name === "files" || !result || typeof result !== "object" || !("raw_bytes" in result)) continue;
  console.log(
    name +
      ": files=" +
      result.files +
      " raw=" +
      result.raw_bytes +
      " gzip=" +
      result.gzip_bytes +
      " brotli=" +
      result.brotli_bytes,
  );
}
console.log("Active raw WASM budget: " + budgets.wasmRawBytes + " bytes");
console.log("Legacy raw WASM budget: " + legacyWasmBudgetBytes + " bytes");
console.log("Bundle report: " + reportPath);

const checks = [
  ["WASM raw", rawWasmBytes, budgets.wasmRawBytes],
  ["WASM gzip", wasmCompressed.gzip_bytes, budgets.wasmGzipBytes],
  ["WASM Brotli", wasmCompressed.brotli_bytes, budgets.wasmBrotliBytes],
  ["JavaScript raw", report.vite_production_assets.javascript.raw_bytes, budgets.javascriptRawBytes],
  ["JavaScript gzip", report.vite_production_assets.javascript.gzip_bytes, budgets.javascriptGzipBytes],
  ["JavaScript Brotli", report.vite_production_assets.javascript.brotli_bytes, budgets.javascriptBrotliBytes],
  ["CSS raw", report.vite_production_assets.css.raw_bytes, budgets.cssRawBytes],
  ["CSS gzip", report.vite_production_assets.css.gzip_bytes, budgets.cssGzipBytes],
  ["CSS Brotli", report.vite_production_assets.css.brotli_bytes, budgets.cssBrotliBytes],
  ["total gzip", report.vite_production_assets.total.gzip_bytes, budgets.totalGzipBytes],
  ["total Brotli", report.vite_production_assets.total.brotli_bytes, budgets.totalBrotliBytes],
];
const violations = checks.filter(([, actual, budget]) => actual > budget);
if (violations.length > 0) {
  for (const [name, actual, budget] of violations) {
    console.error(name + " bundle budget exceeded: " + actual + " > " + budget + " bytes");
  }
  process.exit(1);
}
