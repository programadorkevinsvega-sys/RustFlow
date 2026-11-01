import { expect, test as base } from "@playwright/test";

// Playwright's built-in page/context fixtures create a fresh BrowserContext per
// test and close it after the test, including when the test fails. This fixture
// adds consistent console/page error capture and deterministic app mounting.
export const test = base.extend({
  browserErrors: async ({ page }, use) => {
    const errors = [];
    const onConsole = (message) => {
      if (message.type() === "error") {
        errors.push({ source: "console", message: message.text() });
      }
    };
    const onPageError = (error) => {
      errors.push({ source: "page", message: error.message });
    };

    page.on("console", onConsole);
    page.on("pageerror", onPageError);
    try {
      await use(errors);
    } finally {
      page.off("console", onConsole);
      page.off("pageerror", onPageError);
    }
  },
});

export { expect };

export async function mountApplication(page, path = "/") {
  await page.goto(path);
  await page.locator("#app").waitFor({ state: "visible" });
}
