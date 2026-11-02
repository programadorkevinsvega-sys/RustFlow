import init from "../pkg/rustflow_browser.js";

const initStartMark = "rustflow:runtime:init:start";
const initReadyMark = "rustflow:runtime:init:ready";

if (typeof performance !== "undefined" && typeof performance.mark === "function") {
  performance.mark(initStartMark);
}

init()
  .then(() => {
    if (typeof performance !== "undefined" && typeof performance.mark === "function") {
      performance.mark(initReadyMark);
      performance.measure("rustflow:runtime:init", initStartMark, initReadyMark);
    }
  })
  .catch((error) => {
    console.error("RustFlow failed to initialize", error);
    const mount = document.getElementById("app");
    if (!mount) return;

    const message = document.createElement("pre");
    message.setAttribute("role", "alert");
    message.textContent = String(error);

    // A failed hydration must not destroy valid server-rendered content. Keep the
    // SSR boundary in place and append a text-only diagnostic instead.
    const hasSsrRoot = Array.from(mount.childNodes).some(
      (node) => node.nodeType === Node.COMMENT_NODE && node.nodeValue === "rustflow:root:start",
    );
    if (hasSsrRoot) {
      mount.append(message);
    } else {
      mount.replaceChildren(message);
    }
  });
