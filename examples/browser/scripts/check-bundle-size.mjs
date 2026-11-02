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
const wasmBytes = readFileSync(wasmPath);
const rawWasmBytes = statSync(wasmPath).size;
const wasmCompressed = compress(wasmBytes);
const budgetBytes = 1.5 * 1024 * 1024;

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
    existing_raw_budget_bytes: budgetBytes,
    existing_raw_budget_passed: rawWasmBytes <= budgetBytes,
  },
  vite_production_assets: {
    directory: "dist/assets",
    javascript: summarize(jsFiles),
    css: summarize(cssFiles),
    wasm: summarize(wasmFiles),
    other: summarize(otherFiles),
    total: summarize(assetFiles),
    files: assetFiles.map((path) => ({
      path: path.replace(resolve(process.cwd(), "dist") + "/", "dist/"),
      bytes: statSync(path).size,
    })),
  },
  notes: [
    "Compressed totals are per-file compression sums, not a simulated HTTP transfer with shared cross-file dictionaries.",
    "Vite production assets exclude source maps from the aggregate.",
    "Only the pre-existing raw WASM budget is enforced; JavaScript and CSS thresholds remain informational until a stable baseline is approved.",
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
console.log("Existing raw WASM budget: " + budgetBytes + " bytes");
console.log("Bundle report: " + reportPath);

if (rawWasmBytes > budgetBytes) {
  console.error("WASM bundle exceeds the existing raw WASM size budget.");
  process.exit(1);
}
