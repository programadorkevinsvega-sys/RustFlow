import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }, testInfo) => {
  if (!testInfo.title.startsWith("SSR hydration")) {
    await page.goto("/");
  }
});

async function gotoWithHydrationFixture(page, corrupt = false) {
  const fixture = corrupt
    ? '<div id="hydration-fixture"><!--rustflow:root:start--><button data-testid="hydration-action">Corrupt: <!--rustflow:dynamic:start:0-->0<!--rustflow:dynamic:end:0--></button><!--rustflow:root:end--></div>'
    : '<div id="hydration-fixture"><!--rustflow:root:start--><button data-testid="hydration-action">Count: <!--rustflow:dynamic:start:0-->0<!--rustflow:dynamic:end:0--></button><!--rustflow:root:end--></div>';

  await page.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname !== "/") {
      await route.continue();
      return;
    }

    const response = await route.fetch();
    const html = await response.text();
    const appMount = '<div id="app" aria-live="polite"></div>';
    if (!html.includes(appMount)) {
      throw new Error("could not inject SSR fixture into the browser example");
    }
    await route.fulfill({
      response,
      body: html.replace(appMount, appMount + fixture),
    });
  });
  await page.goto("/");
}

test("counter increments by patching its existing DOM node", async ({ page }) => {
  const counterValue = page.getByTestId("counter-value");
  const originalNode = await counterValue.elementHandle();

  await expect(counterValue).toHaveText("0");
  await page.getByTestId("increment-label").click();
  await expect(counterValue).toHaveText("1");
  await page.getByRole("button", { name: "Increment" }).click();
  await expect(counterValue).toHaveText("2");

  expect(
    await originalNode.evaluate((node) => node === document.querySelector('[data-testid="counter-value"]')),
  ).toBe(true);
});

test("component prop updates preserve local state and compatible DOM identity", async ({ page }) => {
  const label = page.getByTestId("counter-label");
  const value = page.getByTestId("counter-value");
  const originalLabel = await label.elementHandle();
  const originalValue = await value.elementHandle();

  await expect(label).toHaveText("Initial label");
  await expect(value).toHaveText("0");
  await page.getByRole("button", { name: "Increment" }).click();
  await expect(value).toHaveText("1");

  await page.getByTestId("change-counter-label").click();
  await expect(label).toHaveText("Updated label");
  await expect(value).toHaveText("1");

  expect(
    await originalLabel.evaluate((node) => node === document.querySelector('[data-testid="counter-label"]')),
  ).toBe(true);
  expect(
    await originalValue.evaluate((node) => node === document.querySelector('[data-testid="counter-value"]')),
  ).toBe(true);

  await page.getByRole("button", { name: "Increment" }).click();
  await expect(value).toHaveText("2");
});

test("todo CRUD and keyed reorder preserve existing row identity", async ({ page }) => {
  const input = page.getByRole("textbox", { name: "New todo" });
  await input.fill("First task");
  await page.getByRole("button", { name: "Add todo" }).click();
  await input.fill("Second task");
  await page.getByRole("button", { name: "Add todo" }).click();

  const firstRow = page.getByTestId("todo-1");
  const originalNode = await firstRow.elementHandle();
  await expect(firstRow).toContainText("First task");
  const touches = page.getByTestId("todo-touch-1");
  await touches.click();
  await expect(touches).toHaveText("Touch count: 1");

  await page.getByRole("button", { name: "Reverse order" }).click();

  await expect(page.locator('[data-testid="todo-list"] li').first()).toHaveAttribute("data-todo-id", "2");
  expect(
    await originalNode.evaluate((node) => node === document.querySelector('[data-testid="todo-1"]')),
  ).toBe(true);
  await expect(page.getByTestId("todo-touch-1")).toHaveText("Touch count: 1");

  await page.getByTestId("change-todo-key").click();
  await expect(page.getByTestId("todo-touch-1")).toHaveText("Touch count: 0");
  expect(
    await originalNode.evaluate((node) => node === document.querySelector('[data-testid="todo-1"]')),
  ).toBe(false);
  await expect(page.getByTestId("todo-unmount-count")).toHaveText("Unmounted rows: 1");

  await page.getByRole("checkbox", { name: "Mark First task complete" }).check();
  await expect(firstRow).toContainText("✓ First task");
  await page.getByRole("button", { name: "Remove First task" }).click();
  await expect(page.getByTestId("todo-1")).toHaveCount(0);
  await expect(page.getByTestId("todo-unmount-count")).toHaveText("Unmounted rows: 2");
  await expect(page.getByTestId("todo-2")).toContainText("Second task");
  await page.getByRole("checkbox", { name: "Mark Second task complete" }).check();
  await expect(page.getByTestId("todo-2")).toContainText("✓ Second task");
});

test("conditional child mounts and unmounts without replacing the application", async ({ page }) => {
  const heading = page.getByRole("heading", { name: "RustFlow Phase 3 browser demo" });
  const headingNode = await heading.elementHandle();
  await expect(page.getByTestId("help-message")).toBeVisible();

  await page.getByRole("button", { name: "Toggle help" }).click();
  await expect(page.getByTestId("help-message")).toHaveCount(0);

  await page.getByRole("button", { name: "Toggle help" }).click();
  await expect(page.getByTestId("help-message")).toBeVisible();
  expect(
    await headingNode.evaluate((node) => node === document.querySelector("main > h1")),
  ).toBe(true);
});


test("SSR hydration reuses server DOM and attaches one interactive handler", async ({ page }) => {
  await gotoWithHydrationFixture(page);
  const button = page.getByTestId("hydration-action");
  const originalNode = await button.elementHandle();

  await expect(button).toHaveText("Count: 0");
  await originalNode.evaluate((node) => {
    window.__rustflowHydrationOriginal = node;
  });

  await button.click();
  await expect(button).toHaveText("Count: 1");
  expect(
    await page.evaluate(() =>
      window.__rustflowHydrationOriginal === document.querySelector('[data-testid="hydration-action"]'),
    ),
  ).toBe(true);

  await button.click();
  await expect(button).toHaveText("Count: 2");
  expect(
    await page.evaluate(() =>
      window.__rustflowHydrationOriginal === document.querySelector('[data-testid="hydration-action"]'),
    ),
  ).toBe(true);
});

test("SSR hydration mismatch reports a diagnostic and preserves static content", async ({ page }) => {
  const diagnostics = [];
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.push(message.text());
  });

  await gotoWithHydrationFixture(page, true);
  const button = page.getByTestId("hydration-action");
  const originalNode = await button.elementHandle();

  await expect(button).toHaveText("Corrupt: 0");
  await button.click();
  await expect(button).toHaveText("Corrupt: 0");
  expect(
    await originalNode.evaluate((node) => node === document.querySelector('[data-testid="hydration-action"]')),
  ).toBe(true);
  expect(diagnostics.some((message) => message.includes("hydration mismatch"))).toBe(true);
});
