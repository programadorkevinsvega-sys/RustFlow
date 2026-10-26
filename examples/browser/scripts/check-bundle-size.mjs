import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { gzipSync } from "node:zlib";

const wasmPath = resolve(process.cwd(), "pkg/rustflow_browser_bg.wasm");
if (!existsSync(wasmPath)) {
  console.error("Missing WASM output. Run npm run build:wasm first.");
  process.exit(1);
}

const rawBytes = statSync(wasmPath).size;
const gzippedBytes = gzipSync(readFileSync(wasmPath), { level: 9 }).byteLength;
const budgetBytes = 1.5 * 1024 * 1024;

console.log(`RustFlow browser WASM: ${rawBytes} bytes raw, ${gzippedBytes} bytes gzip`);
console.log(`Raw WASM budget: ${budgetBytes} bytes`);

if (rawBytes > budgetBytes) {
  console.error("WASM bundle exceeds the Phase 3 size budget.");
  process.exit(1);
}
