import init from "../pkg/rustflow_browser.js";

init().catch((error) => {
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
