import assert from "node:assert/strict";
import test from "node:test";
import { buildFrameworkEntryUrl, fetchFrameworkEntryUrls, resolveFrameworkEntryUrls } from "../framework-startup-helpers.mjs";
const baseUrl = "http://127.0.0.1:8080";
test("resolves standard keyed entry page", () => {
  assert.equal(buildFrameworkEntryUrl(baseUrl, { type: "keyed", directory: "react-hooks" }), "http://127.0.0.1:8080/frameworks/keyed/react-hooks/index.html");
});
test("uses upstream customURL for Vue", () => {
  assert.equal(buildFrameworkEntryUrl(baseUrl, { type: "keyed", directory: "vue", customURL: "/dist/" }), "http://127.0.0.1:8080/frameworks/keyed/vue/dist/index.html");
});
test("resolves all four selected keyed framework URLs from upstream metadata", () => {
  const catalog = [{ type: "keyed", directory: "react-hooks" }, { type: "keyed", directory: "vue", customURL: "/dist" }, { type: "keyed", directory: "svelte" }, { type: "keyed", directory: "rustflow" }, { type: "non-keyed", directory: "vue", customURL: "/wrong" }];
  assert.deepEqual(resolveFrameworkEntryUrls(baseUrl, catalog), { "react-hooks": "http://127.0.0.1:8080/frameworks/keyed/react-hooks/index.html", vue: "http://127.0.0.1:8080/frameworks/keyed/vue/dist/index.html", svelte: "http://127.0.0.1:8080/frameworks/keyed/svelte/index.html", rustflow: "http://127.0.0.1:8080/frameworks/keyed/rustflow/index.html" });
});
test("rejects missing metadata and unsafe customURL", () => {
  assert.throws(() => resolveFrameworkEntryUrls(baseUrl, [{ type: "keyed", directory: "react-hooks" }]), /Expected exactly one keyed metadata entry for vue/);
  assert.throws(() => buildFrameworkEntryUrl(baseUrl, { type: "keyed", directory: "vue", customURL: "/../other" }), /Unsafe customURL/);
});
test("fetches upstream metadata from /ls", async () => {
  let requestedUrl;
  const urls = await fetchFrameworkEntryUrls(baseUrl, ["vue"], async (url) => { requestedUrl = String(url); return { ok: true, json: async () => [{ type: "keyed", directory: "vue", customURL: "/dist" }] }; });
  assert.equal(requestedUrl, "http://127.0.0.1:8080/ls");
  assert.equal(urls.vue, "http://127.0.0.1:8080/frameworks/keyed/vue/dist/index.html");
});
test("fails closed on HTTP errors", async () => {
  await assert.rejects(fetchFrameworkEntryUrls(baseUrl, ["vue"], async () => ({ ok: false, status: 503 })), /HTTP 503/);
});
