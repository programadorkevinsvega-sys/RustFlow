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

async function loadApp(page, initialListSize = 0, nestedDepth = 0) {
  const parameters = [];
  if (initialListSize > 0) parameters.push(`bench_list=${initialListSize}`);
  if (nestedDepth > 0) parameters.push(`bench_depth=${nestedDepth}`);
  const path = parameters.length > 0 ? `/?${parameters.join("&")}` : "/";
  await page.goto(path);
  if (nestedDepth > 0) {
    await page.locator('[data-bench-depth="0"]').waitFor({ state: "attached" });
  } else {
    await page.getByTestId("counter-value").waitFor({ state: "visible" });
  }
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
      dom_rows: document.querySelectorAll('[data-testid="todo-list"] li').length,
      nested_component_count: document.querySelectorAll("[data-bench-depth]").length,
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

async function measureIsolatedKeyedMutations(page, count) {
  const loaded = await loadApp(page, count);
  if (loaded.dom_rows !== count) throw new Error("Expected " + count + " pre-existing rows");

  const repetitions = 20;
  const result = await page.evaluate(({ initialRows, repetitions }) => {
    const list = document.querySelector('[data-testid="todo-list"]');
    const input = document.querySelector("#todo-input");
    const add = Array.from(document.querySelectorAll("#app button")).find((button) =>
      button.textContent.includes("Add todo"),
    );
    if (!list || !input || !add) throw new Error("Todo controls not mounted");

    const rows = () => Array.from(list.querySelectorAll(":scope > li"));
    const assertRows = (expected, context) => {
      const actual = rows().length;
      if (actual !== expected) throw new Error(context + ": expected " + expected + ", got " + actual);
    };
    const prepare = (value) => {
      input.value = value;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    };
    const removeButton = (row) => {
      const button = row.querySelector('button[aria-label^="Remove "]');
      if (!button) throw new Error("Remove button missing");
      return button;
    };

    // Unmeasured warm-up for each operation.
    prepare("isolated warmup insert");
    add.click();
    assertRows(initialRows + 1, "insert warmup");
    removeButton(rows()[rows().length - 1]).click();
    assertRows(initialRows, "insert warmup cleanup");

    removeButton(rows()[0]).click();
    assertRows(initialRows - 1, "remove warmup");
    prepare("isolated warmup restore");
    add.click();
    assertRows(initialRows, "remove warmup restore");

    const insertionSamples = [];
    for (let sample = 0; sample < repetitions; sample += 1) {
      prepare("isolated insert " + sample);
      const start = performance.now();
      add.click();
      insertionSamples.push(performance.now() - start);
      const current = rows();
      if (current.length !== initialRows + 1) throw new Error("insert did not yield N+1 rows");
      removeButton(current[current.length - 1]).click();
      assertRows(initialRows, "insert cleanup");
    }

    const removalSamples = [];
    const targetIndexes = [];
    const positions = [0, Math.floor(initialRows / 2), initialRows - 1];
    for (let sample = 0; sample < repetitions; sample += 1) {
      const before = rows();
      const index = positions[sample % positions.length];
      const target = before[index];
      if (!target) throw new Error("deletion target missing");
      const targetId = target.getAttribute("data-todo-id");
      const button = removeButton(target);
      targetIndexes.push(index);

      const start = performance.now();
      button.click();
      removalSamples.push(performance.now() - start);

      const after = rows();
      if (after.length !== initialRows - 1 || target.isConnected ||
          after.some((row) => row.getAttribute("data-todo-id") === targetId)) {
        throw new Error("deletion did not remove the selected keyed row");
      }
      prepare("isolated remove restore " + sample);
      add.click();
      assertRows(initialRows, "remove restore");
    }

    return {
      insertion_samples_raw: insertionSamples,
      removal_samples_raw: removalSamples,
      deletion_target_indexes: targetIndexes,
      final_dom_rows: rows().length,
    };
  }, { initialRows: count, repetitions });

  if (result.final_dom_rows !== count) throw new Error("isolated benchmark did not restore N rows");

  // Gather DOM/JS boundary counts in a separate, non-timed pass.
  const domCounts = await page.evaluate((initialRows) => {
    const list = document.querySelector('[data-testid="todo-list"]');
    const input = document.querySelector("#todo-input");
    const add = Array.from(document.querySelectorAll("#app button")).find((button) =>
      button.textContent.includes("Add todo"),
    );
    if (!list || !input || !add) throw new Error("Todo controls missing for instrumentation");
    const counts = {
      set_attribute: 0, remove_attribute: 0, insert_before: 0, remove_child: 0,
      create_element: 0, create_text_node: 0, create_comment: 0,
      add_event_listener: 0, remove_event_listener: 0, reflect_get: 0, reflect_set: 0,
    };
    const restores = [];
    const wrap = (target, method, key) => {
      const original = target[method];
      if (typeof original !== "function") return;
      target[method] = function (...args) {
        counts[key] += 1;
        return original.apply(this, args);
      };
      restores.push(() => { target[method] = original; });
    };
    const reset = () => Object.keys(counts).forEach((key) => { counts[key] = 0; });
    const snapshot = () => ({ ...counts });
    const rows = () => Array.from(list.querySelectorAll(":scope > li"));
    const prepare = (value) => {
      input.value = value;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    };
    const removeButton = (row) => {
      const button = row.querySelector('button[aria-label^="Remove "]');
      if (!button) throw new Error("Remove button missing");
      return button;
    };
    try {
      wrap(Element.prototype, "setAttribute", "set_attribute");
      wrap(Element.prototype, "removeAttribute", "remove_attribute");
      wrap(Node.prototype, "insertBefore", "insert_before");
      wrap(Node.prototype, "removeChild", "remove_child");
      wrap(Document.prototype, "createElement", "create_element");
      wrap(Document.prototype, "createTextNode", "create_text_node");
      wrap(Document.prototype, "createComment", "create_comment");
      wrap(EventTarget.prototype, "addEventListener", "add_event_listener");
      wrap(EventTarget.prototype, "removeEventListener", "remove_event_listener");
      wrap(Reflect, "get", "reflect_get");
      wrap(Reflect, "set", "reflect_set");

      prepare("instrumented insert");
      reset();
      add.click();
      const insertion = snapshot();
      removeButton(rows()[rows().length - 1]).click();

      const target = rows()[Math.floor(initialRows / 2)];
      const button = removeButton(target);
      reset();
      button.click();
      const removal = snapshot();

      prepare("instrumented restore");
      add.click();
      return { insertion, removal };
    } finally {
      for (const restore of restores.reverse()) restore();
    }
  }, count);

  return [
    {
      name: "keyed_list_single_insert_" + count,
      ...summarize(result.insertion_samples_raw, 1, "ms"),
      warmup_samples: 1,
      measured_samples: result.insertion_samples_raw.length,
      preexisting_rows: count,
      final_dom_rows: count,
      measured_scope: "Add click only; input preparation, selector lookup, verification and cleanup excluded",
      dom_operation_counts: domCounts.insertion,
    },
    {
      name: "keyed_list_single_remove_" + count,
      ...summarize(result.removal_samples_raw, 1, "ms"),
      warmup_samples: 1,
      measured_samples: result.removal_samples_raw.length,
      preexisting_rows: count,
      final_dom_rows: count,
      deletion_target_indexes: result.deletion_target_indexes,
      measured_scope: "Selected Remove click only; selector lookup, verification and restoration excluded",
      dom_operation_counts: domCounts.removal,
    },
  ];
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

  const initialListScenarios = [];
  for (const [count, repetitions] of [[100, 7], [1_000, 7]]) {
    // Discard one untimed navigation/mount to warm the browser/WASM path before
    // collecting samples. The warm-up is not included in the reported statistics.
    const warmup = await loadApp(page, count);
    expect(warmup.dom_rows).toBe(count);
    const measurements = [];
    for (let sample = 0; sample < repetitions; sample += 1) {
      const loaded = await loadApp(page, count);
      expect(loaded.dom_rows).toBe(count);
      if (loaded.initialization_ms === null) {
        throw new Error("RustFlow initialization timing mark was not recorded");
      }
      measurements.push(loaded.initialization_ms);
    }
    initialListScenarios.push({
      name: "keyed_list_initial_mount_" + count,
      ...summarize(measurements, count, "ms"),
      warmup_samples: 1,
      measured_samples: repetitions,
      final_dom_rows: count,
      measured_scope: "WASM initialization and initial keyed-list DOM mount",
    });
  }

  const nestedComponentScenarios = [];
  for (const { depth, warmupRepetitions, repetitions } of [
    { depth: 5, warmupRepetitions: 1, repetitions: 7 },
    { depth: 20, warmupRepetitions: 3, repetitions: 25 },
    { depth: 80, warmupRepetitions: 1, repetitions: 7 },
  ]) {
    // Depth 20 has a short (~5 ms) setup path, so it gets additional discarded
    // warm-ups and measured repetitions to stabilize its A/A median.
    for (let sample = 0; sample < warmupRepetitions; sample += 1) {
      const warmup = await loadApp(page, 0, depth);
      expect(warmup.nested_component_count).toBe(depth + 1);
    }
    const measurements = [];
    for (let sample = 0; sample < repetitions; sample += 1) {
      const loaded = await loadApp(page, 0, depth);
      expect(loaded.nested_component_count).toBe(depth + 1);
      if (loaded.initialization_ms === null) {
        throw new Error("RustFlow nested component initialization timing mark was not recorded");
      }
      measurements.push(loaded.initialization_ms);
    }
    nestedComponentScenarios.push({
      depth,
      component_instances: depth + 1,
      warmup_samples: warmupRepetitions,
      measured_samples: repetitions,
      ...summarize(measurements, depth + 1, "ms"),
      measured_scope:
        "WASM initialization, recursive component setup, component rendering, and real DOM mount",
    });
  }

  const listScenarios = [];
  for (const [count, repetitions] of [[100, 5]]) {
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

  const isolatedKeyedMutationScenarios = [];
  if (process.env.RUSTFLOW_BENCH_BROWSER_KEYED_OPERATIONS === "1") {
    for (const count of [100, 1_000, 5_000]) {
      isolatedKeyedMutationScenarios.push(...(await measureIsolatedKeyedMutations(page, count)));
    }
  }

  if (process.env.RUSTFLOW_BENCH_BROWSER_LARGE_LIST === "1") {
    // A 10k-row mount has higher startup and host-scheduling variance than the
    // smaller lists. Warm the exact workload once, then use the same seven
    // measured samples per A/B/A variant as the other initial-mount scenarios.
    const warmup = await loadApp(page, 10_000);
    expect(warmup.dom_rows).toBe(10_000);
    if (warmup.initialization_ms === null) {
      throw new Error("RustFlow 10k warm-up initialization timing mark was not recorded");
    }

    const repetitions = 7;
    const measurements = [];
    for (let sample = 0; sample < repetitions; sample += 1) {
      const large = await loadApp(page, 10_000);
      expect(large.dom_rows).toBe(10_000);
      if (large.initialization_ms === null) {
        throw new Error("RustFlow initialization timing mark was not recorded");
      }
      measurements.push(large.initialization_ms);
    }
    initialListScenarios.push({
      name: "keyed_list_initial_mount_10000",
      ...summarize(measurements, 10_000, "ms"),
      warmup_samples: 1,
      measured_samples: repetitions,
      final_dom_rows: 10_000,
      measured_scope: "WASM initialization and initial keyed-list DOM mount",
      note: "CI stress scenario; one warm-up is excluded; timing remains informational pending equivalent runner baselines",
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
    keyed_list_initial_mounts: initialListScenarios,
    nested_component_depth_initial_mounts: nestedComponentScenarios,
    keyed_list_incremental_insertions: listScenarios,
    keyed_list_isolated_mutations: isolatedKeyedMutationScenarios,
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
      "Incremental insertion measures repeated reactive reconciliation at 100 rows; the opt-in isolated suite measures one insert/remove on pre-existing lists of 100, 1,000, and 5,000 rows.",
      "Timing values are informational and must not fail CI because browser hardware and scheduling vary.",
      "Nested component depth is exercised at depths 5, 20, and 80 in Chromium; Firefox/WebKit profiling remains follow-up work.",
    ],
  };

  // Timing budgets are warning-only on hosted runners. The exact values and
  // baseline provenance are checked into docs/performance/baseline-2026-11-03.json.
  const timingBudgetWarnings = [];
  const checkTimingBudget = (name, measured, budget) => {
    if (measured !== null && measured !== undefined && measured > budget) {
      timingBudgetWarnings.push({
        scenario: name,
        measured_p95_ms: measured,
        warning_budget_ms: budget,
      });
    }
  };
  checkTimingBudget("runtime_initialization", report.initialization.p95, 30);
  checkTimingBudget(
    "counter_100_updates",
    report.counter_100_synchronous_dom_updates.p95,
    10,
  );
  checkTimingBudget(
    "todo_row_update",
    report.todo_row_attribute_and_checked_property_update.p95,
    0.3,
  );
  checkTimingBudget(
    "keyed_list_incremental_insert_100",
    report.keyed_list_incremental_insertions.find(
      (scenario) => scenario.name === "keyed_list_incremental_insert_100",
    )?.p95,
    400,
  );
  checkTimingBudget(
    "keyed_list_initial_mount_1000",
    report.keyed_list_initial_mounts.find(
      (scenario) => scenario.name === "keyed_list_initial_mount_1000",
    )?.p95,
    70,
  );
  checkTimingBudget("keyed_list_reverse_100", report.keyed_list_reverse_100.p95, 14);
  for (const [depth, budget] of [[5, 16], [20, 16], [80, 16]]) {
    const scenario = report.nested_component_depth_initial_mounts.find(
      (item) => item.depth === depth,
    );
    checkTimingBudget(
      "nested_component_depth_" + depth,
      scenario?.p95,
      budget,
    );
  }
  report.timing_budget_warnings = timingBudgetWarnings;
  report.timing_budget_policy =
    "Warning-only on GitHub-hosted runners; compare repeated samples on the same runner class before changing budgets.";

  for (const warning of timingBudgetWarnings) {
    console.warn(
      "RustFlow performance warning: " +
        warning.scenario +
        " p95 " +
        warning.measured_p95_ms +
        " ms exceeds " +
        warning.warning_budget_ms +
        " ms",
    );
  }

  const outputDirectory = resolve(process.cwd(), "performance-results");
  mkdirSync(outputDirectory, { recursive: true });
  writeFileSync(
    resolve(outputDirectory, "browser.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report, null, 2));
  expect(browserErrors).toEqual([]);
});
