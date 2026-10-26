import init from "../pkg/rustflow_browser.js";

init().catch((error) => {
  console.error("RustFlow failed to initialize", error);
  const mount = document.getElementById("app");
  if (mount) {
    const message = document.createElement("pre");
    message.setAttribute("role", "alert");
    message.textContent = String(error);
    mount.replaceChildren(message);
  }
});
