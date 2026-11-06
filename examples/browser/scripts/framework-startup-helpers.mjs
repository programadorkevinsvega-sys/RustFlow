export const DEFAULT_STARTUP_FRAMEWORKS = ["react-hooks", "vue", "svelte", "rustflow"];

export function buildFrameworkEntryUrl(baseUrl, framework) {
  if (!framework || framework.type !== "keyed") throw new Error("Startup benchmark requires keyed framework metadata");
  if (typeof framework.directory !== "string" || framework.directory.trim() === "") throw new Error("Framework metadata is missing its directory");
  const customUrl = framework.customURL ?? "";
  if (typeof customUrl !== "string") throw new Error("Invalid customURL for " + framework.directory);
  const customSegments = customUrl.split("/").filter(Boolean);
  if (customSegments.some((segment) => segment === "." || segment === "..")) throw new Error("Unsafe customURL for " + framework.directory + ": " + customUrl);
  const pathSegments = ["frameworks", "keyed", framework.directory, ...customSegments, "index.html"];
  const encodedPath = pathSegments.map((segment) => encodeURIComponent(segment)).join("/");
  return new URL("/" + encodedPath, baseUrl).href;
}

export function resolveFrameworkEntryUrls(baseUrl, frameworkCatalog, frameworkDirectories = DEFAULT_STARTUP_FRAMEWORKS) {
  if (!Array.isArray(frameworkCatalog)) throw new Error("Upstream /ls response must be an array");
  return Object.fromEntries(frameworkDirectories.map((directory) => {
    const matches = frameworkCatalog.filter((item) => item?.type === "keyed" && item.directory === directory);
    if (matches.length !== 1) throw new Error("Expected exactly one keyed metadata entry for " + directory + ", found " + matches.length);
    return [directory, buildFrameworkEntryUrl(baseUrl, matches[0])];
  }));
}

export async function fetchFrameworkEntryUrls(baseUrl, frameworkDirectories = DEFAULT_STARTUP_FRAMEWORKS, fetchImpl = globalThis.fetch) {
  if (typeof fetchImpl !== "function") throw new Error("A fetch implementation is required to read upstream framework metadata");
  const response = await fetchImpl(new URL("/ls", baseUrl));
  if (!response.ok) throw new Error("Unable to read upstream framework metadata: HTTP " + response.status);
  return resolveFrameworkEntryUrls(baseUrl, await response.json(), frameworkDirectories);
}

export function buildFrameworkOrder(frameworks, rounds) {
  if (!Array.isArray(frameworks) || frameworks.length === 0) throw new Error("At least one framework is required");
  if (!Number.isInteger(rounds) || rounds < 0) throw new Error("Round count must be a non-negative integer");
  return Array.from({ length: rounds }, (_, index) => {
    const rotation = index % frameworks.length;
    return frameworks.map((_, offset) => frameworks[(offset + rotation) % frameworks.length]);
  });
}
