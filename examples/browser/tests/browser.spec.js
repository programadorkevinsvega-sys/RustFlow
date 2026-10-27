import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }, testInfo) => {
  if (!testInfo.title.startsWith("SSR hydration")) {
    await page.goto("/");
  }
});

async function gotoWithHydrationFixture(page, corrupt = false, interactive = false, repeat = false) {
  const interactiveFixture = '<div id="hydration-fixture" data-hydration-case="interactive"><!--rustflow:root:start--><main><form data-testid="hydration-form"><label for="hydration-input">Name</label><input id="hydration-input" aria-label="Name" value="Ada"><p data-testid="hydration-input-output"><!--rustflow:dynamic:start:0-->Ada<!--rustflow:dynamic:end:0--></p><button type="button" data-testid="hydration-toggle">Toggle help</button><button type="button" data-testid="hydration-reverse">Reverse list</button></form><ul data-testid="hydration-list"><!--rustflow:dynamic:start:1--><li data-testid="hydration-item-first">First</li><li data-testid="hydration-item-second">Second</li><!--rustflow:dynamic:end:1--></ul><!--rustflow:dynamic:start:2--><p data-testid="hydration-help" role="status">Help</p><!--rustflow:dynamic:end:2--><a href="#hydration-details" data-testid="hydration-link">Go</a><div id="hydration-details">Target</div></main><!--rustflow:root:end--></div>';
  const fixture = interactive
    ? interactiveFixture
    : corrupt
      ? '<div id="hydration-fixture"><!--rustflow:root:start--><button data-testid="hydration-action">Corrupt: <!--rustflow:dynamic:start:0-->0<!--rustflow:dynamic:end:0--></button><!--rustflow:root:end--></div>'
      : repeat
        ? '<div id="hydration-fixture" data-hydration-repeat="true"><!--rustflow:root:start--><button data-testid="hydration-action">Count: <!--rustflow:dynamic:start:0-->0<!--rustflow:dynamic:end:0--></button><!--rustflow:root:end--></div>'
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

test("SSR hydration supports forms, conditional regions, keyed lists, and ordinary links", async ({ page }) => {
  await gotoWithHydrationFixture(page, false, true);

  const input = page.getByRole("textbox", { name: "Name" });
  const output = page.getByTestId("hydration-input-output");
  await expect(input).toHaveValue("Ada");
  await expect(output).toHaveText("Ada");

  await input.fill("Grace");
  await expect(output).toHaveText("Grace");

  const help = page.getByTestId("hydration-help");
  await expect(help).toHaveText("Help");
  await page.getByTestId("hydration-toggle").click();
  await expect(help).toHaveCount(0);
  await page.getByTestId("hydration-toggle").click();
  await expect(help).toHaveText("Help");

  const list = page.getByTestId("hydration-list");
  await expect(list.locator("li").nth(0)).toHaveText("First");
  await expect(list.locator("li").nth(1)).toHaveText("Second");
  await page.getByTestId("hydration-reverse").click();
  await expect(list.locator("li").nth(0)).toHaveText("Second");
  await expect(list.locator("li").nth(1)).toHaveText("First");

  await page.getByTestId("hydration-link").click();
  await expect(page).toHaveURL(/#hydration-details$/);
});

test("SSR hydration rejects a repeated mount without duplicating events", async ({ page }) => {
  const diagnostics = [];
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.push(message.text());
  });

  await gotoWithHydrationFixture(page, false, false, true);
  const button = page.getByTestId("hydration-action");
  await expect(button).toHaveText("Count: 0");
  await expect.poll(() => diagnostics.some((message) => message.includes("already been hydrated"))).toBe(true);

  await button.click();
  await expect(button).toHaveText("Count: 1");
  await button.click();
  await expect(button).toHaveText("Count: 2");
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


test("client navigation pushes history and supports back and forward", async ({ page }) => {
  const aboutRequests = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/about") {
      aboutRequests.push(request.isNavigationRequest());
    }
  });

  await page.getByTestId("about-link").click();
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByTestId("about-page")).toBeVisible();
  expect(aboutRequests).toContain(false);

  await page.goBack();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByTestId("home-page")).toBeVisible();

  await page.goForward();
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByTestId("about-page")).toBeVisible();
});

test("direct routes and full reloads work without a prior client navigation", async ({ page }) => {
  await page.goto("/about");
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByTestId("about-page")).toBeVisible();

  await page.reload();
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByTestId("about-page")).toBeVisible();
});

test("opt-in GET forms use client navigation and suppress duplicate submissions", async ({ page }) => {
  const requests = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.pathname === "/" && url.searchParams.has("q")) {
      requests.push(request.isNavigationRequest());
    }
  });

  await page.getByRole("textbox", { name: "Search query" }).fill("rust flow");
  await page.evaluate(() => {
    const form = document.querySelector('[data-testid="search-form"]');
    form.requestSubmit();
    form.requestSubmit();
  });

  await expect(page).toHaveURL(/\/\?q=rust\+flow$/);
  await expect(page.getByTestId("home-page")).toBeVisible();
  await expect.poll(() => requests.length).toBe(1);
  expect(requests).toEqual([false]);
});

test("forms without explicit enhancement keep native browser submission", async ({ page }) => {
  const navigationRequests = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.pathname === "/about" && url.searchParams.has("native")) {
      navigationRequests.push(request.isNavigationRequest());
    }
  });

  await page.evaluate(() => {
    const form = document.createElement("form");
    form.method = "get";
    form.action = "/about";
    form.innerHTML =
      '<input type="hidden" name="native" value="ok"><button type="submit">Native submit</button>';
    document.getElementById("app").append(form);
  });

  await page.getByRole("button", { name: "Native submit" }).click();
  await expect(page).toHaveURL(/\/about\?native=ok$/);
  await expect(page.getByTestId("about-page")).toBeVisible();
  expect(navigationRequests).toContain(true);
});

test("HTTP failures fall back to a full server navigation", async ({ page }) => {
  await page.route("**/about", async (route) => {
    await route.fulfill({
      status: 503,
      contentType: "text/html; charset=utf-8",
      body: "<!doctype html><html><body><p>Upstream unavailable</p></body></html>",
    });
  });

  await page.getByTestId("about-link").click();
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByText("Upstream unavailable")).toBeVisible();
});

test("modified, alternate-target, download, and external links are not intercepted", async ({ page }) => {
  const prevented = await page.evaluate(() => {
    const root = document.getElementById("app");
    const inspect = (attributes, modifiers = {}) => {
      const anchor = document.createElement("a");
      for (const [name, value] of Object.entries(attributes)) {
        anchor.setAttribute(name, value);
      }
      anchor.textContent = "temporary link";
      root.append(anchor);

      let wasPrevented = null;
      const observe = (event) => {
        if (event.target === anchor) {
          wasPrevented = event.defaultPrevented;
          event.preventDefault();
          document.removeEventListener("click", observe);
        }
      };
      document.addEventListener("click", observe);
      anchor.dispatchEvent(
        new MouseEvent("click", {
          bubbles: true,
          cancelable: true,
          button: 0,
          ...modifiers,
        }),
      );
      anchor.remove();
      return wasPrevented;
    };

    return [
      inspect({ href: "/about" }, { ctrlKey: true }),
      inspect({ href: "/about", target: "_blank" }),
      inspect({ href: "/about", download: "about.html" }),
      inspect({ href: "https://example.com/" }),
    ];
  });

  expect(prevented).toEqual([false, false, false, false]);
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByTestId("home-page")).toBeVisible();
});


test("native HTML forms work when JavaScript is disabled", async ({ browser, page }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const noJsPage = await context.newPage();
  const origin = new URL(page.url()).origin;

  await noJsPage.route("**/native-form", (route) =>
    route.fulfill({
      status: 200,
      contentType: "text/html; charset=utf-8",
      body:
        '<!doctype html><html><body><form method="get" action="/native-result">' +
        '<label for="native-query">Query</label><input id="native-query" name="q" required>' +
        '<button type="submit">Send</button></form></body></html>',
    }),
  );
  await noJsPage.route("**/native-result**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "text/html; charset=utf-8",
      body: "<!doctype html><html><body><p>Native submission: bare html</p></body></html>",
    }),
  );

  await noJsPage.goto(origin + "/native-form");
  await noJsPage.getByRole("textbox", { name: "Query" }).fill("bare html");
  await noJsPage.getByRole("button", { name: "Send" }).click();
  await expect(noJsPage).toHaveURL(/\\/native-result\\?q=bare\\+html$/);
  await expect(noJsPage.getByText("Native submission: bare html")).toBeVisible();

  await context.close();
});
