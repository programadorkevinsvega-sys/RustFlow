import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
});

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
  await page.getByRole("button", { name: "Reverse order" }).click();

  await expect(page.locator('[data-testid="todo-list"] li').first()).toHaveAttribute("data-todo-id", "2");
  expect(
    await originalNode.evaluate((node) => node === document.querySelector('[data-testid="todo-1"]')),
  ).toBe(true);

  await page.getByRole("checkbox", { name: "Mark First task complete" }).check();
  await expect(firstRow).toContainText("✓ First task");
  await page.getByRole("button", { name: "Remove First task" }).click();
  await expect(page.getByTestId("todo-1")).toHaveCount(0);
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
