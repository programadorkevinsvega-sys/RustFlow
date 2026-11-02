import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "./support/fixtures.js";

function summarize(values, operations, unit) {
  const sorted = [...values].sort((left, right) => left - right);
  if (sorted.length === 0) {
    return {
      unit,
      samples: 0,
      operations_per_sample: operations,
      min: null,
      median: null,
      p90: null,
      p95: null,
      p99: null,
      throughput_ops_per_second_at_median: null,
    };
  }
  const at = (fraction) => sorted[Math.ceil((sorted.length - 1) * fraction)];
  const median = at(0.5);
  return {
    unit,
    samples: sorted.length,
    operations_per_sample: operations,
    min: sorted[0],
    median,
    p90: at(0.9),
    p95: at(0.95),
    p99: sorted.length >= 100 ? at(0.99) : null,
    throughput_ops_per_second_at_median:
      unit === "ms" ? operations / (median / 1_000) : null,
    samples_raw: values,
  };
}

async function loadApp(page) {
  await page.goto("/");
  await page.getByTestId("counter-value").waitFor({ state: "visible" });
  return page.evaluate(() => {
    const entry = performance.getEntriesByName("rustflow:runtime:init")[0];
    const memory = performance.memory
      ? {
          source: "Chromium performance.memory",
          used_js_heap_bytes: performance.memory.usedJSHeapSize,
          total_js_heap_bytes: performance.memory.totalJSHeapSize,
        }
      : {
          source: "unavailable",
          used_js_heap_bytes: null,
          total_js_heap_bytes: null,
        };
    return {
      initialization_ms: entry ? entry.duration : null,
      memory,
    };
  });
}

async function addTodos(page, count) {
  return page.evaluate((total) => {
    const input = document.querySelector("#todo-input");
    const add = Array.from(document.querySelectorAll("#app button")).find((button) =>
      button.textContent.includes("Add todo"),
    );
    if (!input || !add) throw new Error("RustFlow todo editor was not mounted");

    const started = performance.now();
    for (let index = 0; index < total; index += 1) {
      input.value = "benchmark item " + index;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      add.click();
    }
    return {
      elapsed_ms: performance.now() - started,
      dom_rows: document.querySelectorAll('[data-testid="todo-list"] li').length,
    };
  }, count);
}

test("RustFlow browser runtime performance measurements", async ({ page, browser, browserErrors }) => {
  test.setTimeout(10 * 60 * 1000);

  const initializationSamples = [];
  const memorySamples = [];
  const counterSamples = [];

  for (let sample = 0; sample < 5; sample += 1) {
    const loaded = await loadApp(page);
    if (loaded.initialization_ms !== null) initializationSamples.push(loaded.initialization_ms);
    memorySamples.push(loaded.memory);

    const elapsed = await page.evaluate(() => {
      const increment = document.querySelector('[data-testid="increment-label"]');
      if (!increment) throw new Error("RustFlow counter was not mounted");
      const started = performance.now();
      for (let index = 0; index < 100; index += 1) increment.click();
      return performance.now() - started;
    });
    counterSamples.push(elapsed);
  }

  const todoRowUpdateSamples = [];
  for (let sample = 0; sample < 5; sample += 1) {
    await loadApp(page);
    const created = await addTodos(page, 1);
    expect(created.dom_rows).toBe(1);
    const measurements = await page.evaluate(() => {
      const values = [];
      for (let index = 0; index < 20; index += 1) {
        const checkbox = document.querySelector(
          '[data-testid="todo-1"] input[type="checkbox"]',
        );
        if (!checkbox) throw new Error("RustFlow todo row checkbox was not mounted");
        const next = !checkbox.checked;
        const started = performance.now();
        checkbox.checked = next;
        checkbox.dispatchEvent(new Event("change", { bubbles: true }));
        values.push(performance.now() - started);
        if (checkbox.getAttribute("data-done") !== String(next)) {
          throw new Error("RustFlow did not patch the checkbox data-done attribute");
        }
      }
      return values;
    });
    todoRowUpdateSamples.push(...measurements);
  }

  const listScenarios = [];
  for (const [count, repetitions] of [[100, 5], [1_000, 3]]) {
    const measurements = [];
    for (let sample = 0; sample < repetitions; sample += 1) {
      await loadApp(page);
      const measured = await addTodos(page, count);
      expect(measured.dom_rows).toBe(count);
      measurements.push(measured.elapsed_ms);
    }
    listScenarios.push({
      name: "keyed_list_incremental_insert_" + count,
      ...summarize(measurements, count, "ms"),
      median_ms_per_operation:
        summarize(measurements, count, "ms").median / count,
      final_dom_rows: count,
    });
  }

  await loadApp(page);
  const prepared = await addTodos(page, 100);
  expect(prepared.dom_rows).toBe(100);
  const reorderSamples = await page.evaluate(() => {
    const button = Array.from(document.querySelectorAll("#app button")).find((item) =>
      item.textContent.includes("Reverse order"),
    );
    if (!button) throw new Error("RustFlow list reorder control was not mounted");
    const values = [];
    for (let sample = 0; sample < 10; sample += 1) {
      const started = performance.now();
      button.click();
      values.push(performance.now() - started);
    }
    return values;
  });

  await loadApp(page);
  const removalPrepared = await addTodos(page, 100);
  expect(removalPrepared.dom_rows).toBe(100);
  const removalElapsed = await page.evaluate(() => {
    const list = document.querySelector('[data-testid="todo-list"]');
    if (!list) throw new Error("RustFlow todo list was not mounted");
    const started = performance.now();
    let removed = 0;
    while (true) {
      const button = list.querySelector('button[aria-label^="Remove "]');
      if (!button) break;
      button.click();
      removed += 1;
    }
    return {
      elapsed_ms: performance.now() - started,
      removed,
      remaining_rows: list.querySelectorAll("li").length,
    };
  });
  expect(removalElapsed.removed).toBe(100);
  expect(removalElapsed.remaining_rows).toBe(0);

  if (process.env.RUSTFLOW_BENCH_BROWSER_LARGE_LIST === "1") {
    await loadApp(page);
    const large = await addTodos(page, 10_000);
    listScenarios.push({
      name: "keyed_list_incremental_insert_10000",
      ...summarize([large.elapsed_ms], 10_000, "ms"),
      final_dom_rows: large.dom_rows,
      warning: "single opt-in stress sample; not a CI threshold",
    });
  }

  const report = {
    schema_version: 1,
    suite: "rustflow-browser-dom",
    generated_at: new Date().toISOString(),
    commit: process.env.GITHUB_SHA || "unspecified",
    node_version: process.version,
    browser: {
      engine: "Chromium",
      version: browser.version(),
    },
    initialization: summarize(initializationSamples, 1, "ms"),
    counter_100_synchronous_dom_updates: summarize(counterSamples, 100, "ms"),
    todo_row_attribute_and_checked_property_update: summarize(todoRowUpdateSamples, 1, "ms"),
    keyed_lists: listScenarios,
    keyed_list_reverse_100: summarize(reorderSamples, 1, "ms"),
    keyed_list_remove_100: {
      elapsed_ms: removalElapsed.elapsed_ms,
      removed: removalElapsed.removed,
      initial_rows: 100,
      remaining_rows: removalElapsed.remaining_rows,
      throughput_ops_per_second:
        removalElapsed.removed / (removalElapsed.elapsed_ms / 1_000),
    },
    memory: {
      observations: memorySamples,
      note:
        "performance.memory is a Chromium-specific JS heap observation when exposed; it is not process RSS, retained Rust memory, allocation count, or a cross-browser guarantee.",
    },
    limitations: [
      "These measurements exercise the shipped RustFlow WASM runtime in Chromium and synchronous interactions with its real DOM.",
      "List insertion is incremental and includes repeated reactive list reconciliation; it is not a single batched list construction.",
      "Timing values are informational and must not fail CI because browser hardware and scheduling vary.",
      "Deep component-tree and multi-browser profiling remain follow-up work; the current demo fixture covers Counter and TodoRow.",
    ],
  };

  const outputDirectory = resolve(process.cwd(), "performance-results");
  mkdirSync(outputDirectory, { recursive: true });
  writeFileSync(
    resolve(outputDirectory, "browser.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report, null, 2));
  expect(browserErrors).toEqual([]);
});
