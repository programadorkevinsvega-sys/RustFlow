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

async function loadApp(page, initialListSize = 0, nestedDepth = 0, includeBenchmarkControls = false) {
  const parameters = [];
  if (initialListSize > 0) parameters.push(`bench_list=${initialListSize}`);
  if (nestedDepth > 0) parameters.push(`bench_depth=${nestedDepth}`);
  if (includeBenchmarkControls) parameters.push("bench_controls=1");
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
    const cleanupCounter = document.querySelector('[data-testid="todo-unmount-count"]');
    if (!list || !input || !add || !cleanupCounter) throw new Error("Todo controls not mounted");
    const readCleanupCount = () => Number(cleanupCounter.textContent.match(/[0-9]+/)?.[0] ?? 0);

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

    // One discarded insert/remove pair warms both mutation paths and restores
    // the steady-state list size without an additional cleanup mutation.
    prepare("isolated warmup insert");
    add.click();
    assertRows(initialRows + 1, "insert warmup");
    removeButton(rows()[0]).click();
    assertRows(initialRows, "remove warmup");

    const insertionSamples = [];
    const removalSamples = [];
    const targetIndexes = [];
    // Removal runs immediately after its paired insertion, so it starts with
    // N+1 rows and returns the fixture to N. The exact operation sizes are
    // recorded below rather than implying both operations start at N.
    const positions = [0, Math.floor((initialRows + 1) / 2), initialRows];
    for (let sample = 0; sample < repetitions; sample += 1) {
      const baselineNodes = rows();
      const cleanupBefore = readCleanupCount();
      prepare("isolated insert " + sample);
      const insertionStart = performance.now();
      add.click();
      insertionSamples.push(performance.now() - insertionStart);

      const afterInsert = rows();
      if (afterInsert.length !== initialRows + 1) {
        throw new Error("insert did not yield N+1 rows");
      }
      for (let index = 0; index < baselineNodes.length; index += 1) {
        if (afterInsert[index] !== baselineNodes[index]) {
          throw new Error("single insert changed the identity or order of an existing row");
        }
      }

      const index = positions[sample % positions.length];
      const target = afterInsert[index];
      if (!target) throw new Error("deletion target missing");
      const targetId = target.getAttribute("data-todo-id");
      const button = removeButton(target);
      const expectedSurvivors = afterInsert.filter((row) => row !== target);
      targetIndexes.push(index);

      const removalStart = performance.now();
      button.click();
      removalSamples.push(performance.now() - removalStart);

      const afterRemoval = rows();
      if (afterRemoval.length !== initialRows || target.isConnected ||
          afterRemoval.some((row) => row.getAttribute("data-todo-id") === targetId)) {
        throw new Error("deletion did not remove the selected keyed row or restore the baseline size");
      }
      if (afterRemoval.some((row, survivorIndex) => row !== expectedSurvivors[survivorIndex])) {
        throw new Error("single delete changed the identity or order of a surviving keyed row");
      }
      if (readCleanupCount() !== cleanupBefore + 1) {
        throw new Error("single delete did not run exactly one row component cleanup");
      }
    }

    return {
      insertion_samples_raw: insertionSamples,
      removal_samples_raw: removalSamples,
      deletion_target_indexes: targetIndexes,
      final_dom_rows: rows().length,
    };
  }, { initialRows: count, repetitions });

  if (result.final_dom_rows !== count) throw new Error("isolated benchmark did not restore N rows");

  // Gather DOM/JS boundary counts in a separate, non-timed pass. The same
  // insert/remove pair keeps the list at its steady-state size.
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
      add_event_listener: 0, remove_event_listener: 0,
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

      prepare("instrumented insert");
      reset();
      add.click();
      const insertion = snapshot();

      const target = rows()[Math.floor((initialRows + 1) / 2)];
      const button = removeButton(target);
      reset();
      button.click();
      const removal = snapshot();
      if (rows().length !== initialRows) {
        throw new Error("instrumented insert/remove pair did not restore the baseline row count");
      }
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
      baseline_dom_rows: count,
      operation_start_rows: count,
      operation_end_rows: count + 1,
      final_dom_rows: count,
      paired_operation: "single_remove",
      measured_scope:
        "Add click only; followed by the separately timed paired Remove click to restore the steady-state baseline",
      dom_operation_counts: domCounts.insertion,
    },
    {
      name: "keyed_list_single_remove_" + count,
      ...summarize(result.removal_samples_raw, 1, "ms"),
      warmup_samples: 1,
      measured_samples: result.removal_samples_raw.length,
      baseline_dom_rows: count,
      operation_start_rows: count + 1,
      operation_end_rows: count,
      final_dom_rows: count,
      deletion_target_indexes: result.deletion_target_indexes,
      paired_operation: "single_insert",
      measured_scope:
        "Selected Remove click only; one appended row is present at operation start and the paired removal returns the steady-state list to baseline",
      dom_operation_counts: domCounts.removal,
    },
  ];
}

async function measureSequentialSiblingReconciliation(page, count) {
  await page.goto(`/?bench_sequence=${count}`);
  await page.locator("#bench-sequence").waitFor({ state: "attached" });
  await page.locator("#bench-sequence-anchor").waitFor({ state: "attached" });

  const result = await page.evaluate(({ count, warmupSamples, repetitions }) => {
    const container = document.querySelector("#bench-sequence");
    const toggle = document.querySelector('[data-testid="bench-sequence-toggle"]');
    if (!container || !toggle) {
      throw new Error("Sequential sibling benchmark fixture was not mounted");
    }

    const childSpans = () => Array.from(container.querySelectorAll(":scope > span"));
    const originalAnchor = childSpans().find(
      (node) => node.id === "bench-sequence-anchor",
    );
    if (!originalAnchor) throw new Error("Initial keyed anchor is missing");

    const verify = (anchorAtEnd, phase) => {
      const nodes = childSpans();
      if (nodes.length !== count + 1) {
        throw new Error(
          phase + ": expected " + (count + 1) + " sequence children; got " + nodes.length,
        );
      }
      const expectedAnchorIndex = anchorAtEnd ? count : 0;
      if (nodes[expectedAnchorIndex]?.id !== "bench-sequence-anchor") {
        throw new Error(phase + ": keyed anchor is not at the expected position");
      }
      for (let index = 0; index < count; index += 1) {
        const node = nodes[anchorAtEnd ? index : index + 1];
        if (node?.getAttribute("data-sequence-index") !== String(index)) {
          throw new Error(phase + ": unkeyed sibling order changed at index " + index);
        }
      }
      // Unkeyed siblings intentionally follow positional identity. The stable
      // identity contract being measured here belongs to the keyed anchor.
      if (nodes[expectedAnchorIndex] !== originalAnchor) {
        throw new Error(phase + ": keyed anchor DOM identity changed");
      }
    };

    verify(false, "initial sequence");
    for (let sample = 0; sample < warmupSamples; sample += 1) {
      toggle.click();
      verify(true, "warmup move " + sample);
      toggle.click();
      verify(false, "warmup reset " + sample);
    }

    const samples = [];
    for (let sample = 0; sample < repetitions; sample += 1) {
      const started = performance.now();
      toggle.click();
      samples.push(performance.now() - started);
      verify(true, "measured move " + sample);
      // Restore the anchor to index zero outside the timed interval so every
      // measured operation exercises the same keyed-to-unkeyed fallback path.
      toggle.click();
      verify(false, "measured reset " + sample);
    }

    return {
      samples_raw: samples,
      warmup_samples: warmupSamples,
      measured_samples: samples.length,
      final_dom_children: childSpans().length,
      keyed_anchor_identity_preserved: true,
      final_anchor_position: "first",
    };
  }, { count, warmupSamples: 2, repetitions: 12 });

  if (
    result.final_dom_children !== count + 1 ||
    !result.keyed_anchor_identity_preserved
  ) {
    throw new Error("Sequential sibling benchmark did not preserve the original DOM");
  }

  return {
    size: count,
    operation: "move one keyed anchor from first to last across unkeyed siblings",
    ...summarize(result.samples_raw, 1, "ms"),
    warmup_samples: result.warmup_samples,
    measured_samples: result.measured_samples,
    final_dom_children: result.final_dom_children,
    keyed_anchor_identity_preserved: result.keyed_anchor_identity_preserved,
    final_anchor_position: result.final_anchor_position,
    measured_scope:
      "One dynamic fragment reconciliation moving a keyed anchor from index zero to the end of 5,000 unkeyed sibling elements; the reset move is excluded from timing and all DOM identities are checked",
  };
}

async function measureExtendedRustFlowScenarios(page) {
  const report = [];

  for (const count of [100, 1_000, 5_000, 10_000]) {
    await loadApp(page, count);
    report.push(...(await measureIsolatedKeyedMutations(page, count)));

    const loaded = await loadApp(page, count, 0, true);
    if (loaded.dom_rows !== count) throw new Error(`Expected ${count} benchmark rows`);

    const updateSamples = await page.evaluate(() => {
      const list = document.querySelector('[data-testid="todo-list"]');
      if (!list) throw new Error("Todo list missing for extended update benchmark");
      const rows = () => Array.from(list.querySelectorAll(":scope > li"));
      const positions = [0, Math.floor(rows().length / 2), rows().length - 1];
      const values = [];
      for (let sample = 0; sample < 20; sample += 1) {
        const beforeNodes = rows();
        const position = positions[sample % positions.length];
        const row = beforeNodes[position];
        const checkbox = row?.querySelector('input[type="checkbox"]');
        if (!checkbox) throw new Error("Target row checkbox missing");
        const before = checkbox.checked;
        const started = performance.now();
        checkbox.checked = !before;
        checkbox.dispatchEvent(new Event("change", { bubbles: true }));
        values.push(performance.now() - started);
        if (checkbox.getAttribute("data-done") !== String(!before)) {
          throw new Error("Single-row update did not reach the DOM");
        }
        const afterNodes = rows();
        if (afterNodes.length !== beforeNodes.length) throw new Error("Single-row update changed list length");
        for (let index = 0; index < beforeNodes.length; index += 1) {
          if (index !== position && afterNodes[index] !== beforeNodes[index]) {
            throw new Error("Single-row update replaced an unaffected keyed row");
          }
        }
      }
      return values;
    });
    report.push({
      name: `keyed_list_single_row_update_${count}`,
      ...summarize(updateSamples, 1, "ms"),
      warmup_samples: 0,
      measured_samples: updateSamples.length,
      baseline_dom_rows: count,
      final_dom_rows: count,
      measured_scope: "One checkbox change event, with first/middle/last target rotation",
    });

    const bulkUpdate = await page.evaluate(() => {
      const button = document.querySelector('[data-testid="bench-update-every-10th"]');
      const list = document.querySelector('[data-testid="todo-list"]');
      if (!button || !list) throw new Error("Bulk update benchmark fixture missing");
      const rows = () => Array.from(list.querySelectorAll(":scope > li"));
      const initialNodes = rows();
      const samples = [];
      for (let sample = 0; sample < 20; sample += 1) {
        const started = performance.now();
        button.click();
        samples.push(performance.now() - started);
        const firstTitle = rows()[0]?.querySelector(":scope > span")?.textContent ?? "";
        if (firstTitle.split(" !!!").length - 1 !== sample + 1) {
          throw new Error("Bulk update did not update every-tenth-row content on the first row");
        }
        const afterNodes = rows();
        for (let index = 0; index < afterNodes.length; index += 1) {
          if (afterNodes[index] !== initialNodes[index]) {
            throw new Error("Bulk update replaced a keyed row instead of patching its contents");
          }
        }
      }
      return samples;
    });
    report.push({
      name: `keyed_list_update_every_tenth_${count}`,
      ...summarize(bulkUpdate, 1, "ms"),
      warmup_samples: 0,
      measured_samples: bulkUpdate.length,
      baseline_dom_rows: count,
      final_dom_rows: count,
      measured_scope: "One signal update mutating every tenth row and committing rendered keyed DOM",
    });

    const reorderButtons = [
      ["swap_rows", "bench-swap-rows"],
      ["move_first_to_end", "bench-move-first-to-end"],
      ["move_last_to_front", "bench-move-last-to-front"],
      ["reverse_first_section", "bench-reverse-first-section"],
    ];
    for (const [name, testId] of reorderButtons) {
      const values = await page.evaluate(({ testId }) => {
        const button = document.querySelector(`[data-testid="${testId}"]`);
        const list = document.querySelector('[data-testid="todo-list"]');
        if (!button || !list) throw new Error("Reorder control missing");
        const rows = () => Array.from(list.querySelectorAll(":scope > li"));
        let expected = rows();
        const initialSize = expected.length;
        const samples = [];
        for (let sample = 0; sample < 20; sample += 1) {
          const nextExpected = [...expected];
          if (testId === "bench-swap-rows" && nextExpected.length > 998) {
            [nextExpected[1], nextExpected[998]] = [nextExpected[998], nextExpected[1]];
          } else if (testId === "bench-move-first-to-end" && nextExpected.length > 1) {
            nextExpected.push(nextExpected.shift());
          } else if (testId === "bench-move-last-to-front" && nextExpected.length > 1) {
            nextExpected.unshift(nextExpected.pop());
          } else if (testId === "bench-reverse-first-section") {
            const section = nextExpected.slice(0, 100).reverse();
            nextExpected.splice(0, section.length, ...section);
          }
          const started = performance.now();
          button.click();
          samples.push(performance.now() - started);
          const actual = rows();
          if (actual.length !== initialSize) throw new Error("Reorder changed list length");
          if (actual.some((node, index) => node !== nextExpected[index])) {
            throw new Error("Reorder failed expected keyed DOM node placement or changed node identity");
          }
          expected = nextExpected;
        }
        return samples;
      }, { testId });
      report.push({
        name: `keyed_list_${name}_${count}`,
        ...summarize(values, 1, "ms"),
        warmup_samples: 0,
        measured_samples: values.length,
        baseline_dom_rows: count,
        final_dom_rows: count,
        measured_scope: "One reorder control click with real keyed DOM reconciliation and exact node-order verification",
      });
    }

    const appendSamples = [];
    for (let sample = 0; sample < 20; sample += 1) {
      const loadedForAppend = await loadApp(page, count, 0, true);
      if (loadedForAppend.dom_rows !== count) throw new Error("Append fixture has wrong initial size");
      const appendResult = await page.evaluate(() => {
        const button = document.querySelector('[data-testid="bench-append-1000"]');
        const cleanupButton = document.querySelector('[data-testid="bench-remove-last-1000"]');
        const list = document.querySelector('[data-testid="todo-list"]');
        if (!button || !cleanupButton || !list) throw new Error("Bulk append controls missing");
        const beforeRows = Array.from(list.querySelectorAll(":scope > li"));
        const started = performance.now();
        button.click();
        const elapsed = performance.now() - started;
        const afterRows = Array.from(list.querySelectorAll(":scope > li"));
        if (afterRows.length !== beforeRows.length + 1_000) throw new Error("Bulk append did not add exactly 1,000 rows");
        for (let index = 0; index < beforeRows.length; index += 1) {
          if (afterRows[index] !== beforeRows[index]) throw new Error("Bulk append changed an existing keyed row identity");
        }
        cleanupButton.click();
        const restored = list.querySelectorAll(":scope > li").length;
        if (restored !== beforeRows.length) throw new Error("Paired removal failed to restore the append fixture");
        return elapsed;
      });
      appendSamples.push(appendResult);
    }
    report.push({
      name: `keyed_list_append_1000_to_${count}`,
      ...summarize(appendSamples, 1_000, "ms"),
      warmup_samples: 0,
      measured_samples: appendSamples.length,
      baseline_dom_rows: count,
      operation_end_rows: count + 1_000,
      final_dom_rows: count,
      measured_scope: "One batch append of 1,000 rows; paired removal restores baseline outside timed interval",
    });

    const sectionRemovalSamples = [];
    for (let sample = 0; sample < 20; sample += 1) {
      await loadApp(page, count, 0, true);
      const measurement = await page.evaluate(() => {
        const button = document.querySelector('[data-testid="bench-remove-first-section"]');
        const list = document.querySelector('[data-testid="todo-list"]');
        const counter = document.querySelector('[data-testid="todo-unmount-count"]');
        if (!button || !list || !counter) throw new Error("Section removal controls missing");
        const initialRows = Array.from(list.querySelectorAll(":scope > li"));
        const initialCleanup = Number(counter.textContent.match(/\d+/)?.[0] ?? 0);
        const removed = Math.min(100, initialRows.length);
        const started = performance.now();
        button.click();
        const elapsed = performance.now() - started;
        const remaining = Array.from(list.querySelectorAll(":scope > li"));
        const cleanupDelta = Number(counter.textContent.match(/\d+/)?.[0] ?? 0) - initialCleanup;
        if (remaining.length !== initialRows.length - removed || cleanupDelta !== removed) {
          throw new Error("Section removal failed DOM count or component cleanup verification");
        }
        if (remaining.some((row, index) => row !== initialRows[index + removed])) {
          throw new Error("Section removal changed identity/order of surviving keyed rows");
        }
        return elapsed;
      });
      sectionRemovalSamples.push(measurement);
    }
    report.push({
      name: `keyed_list_remove_section_100_${count}`,
      ...summarize(sectionRemovalSamples, 100, "ms"),
      warmup_samples: 0,
      measured_samples: sectionRemovalSamples.length,
      baseline_dom_rows: count,
      operation_end_rows: count - Math.min(100, count),
      measured_scope: "Remove first 100 keyed row components and verify surviving identity plus owner cleanup",
    });

    const fullCleanupSamples = [];
    for (let sample = 0; sample < 20; sample += 1) {
      await loadApp(page, count, 0, true);
      const measurement = await page.evaluate(() => {
        const button = document.querySelector('[data-testid="bench-clear-all"]');
        const list = document.querySelector('[data-testid="todo-list"]');
        const counter = document.querySelector('[data-testid="todo-unmount-count"]');
        if (!button || !list || !counter) throw new Error("Cleanup controls missing");
        const initial = list.querySelectorAll(":scope > li").length;
        const startCount = Number(counter.textContent.match(/\d+/)?.[0] ?? 0);
        const started = performance.now();
        button.click();
        const elapsed = performance.now() - started;
        const remaining = list.querySelectorAll(":scope > li").length;
        const cleanupCount = Number(counter.textContent.match(/\d+/)?.[0] ?? 0) - startCount;
        if (remaining !== 0 || cleanupCount !== initial) {
          throw new Error("Full cleanup did not unmount every row component");
        }
        return { elapsed, initial, cleanupCount };
      });
      fullCleanupSamples.push(measurement.elapsed);
      if (measurement.initial !== count || measurement.cleanupCount !== count) {
        throw new Error("Full cleanup sample metadata mismatch");
      }
    }
    report.push({
      name: `keyed_list_clear_all_${count}`,
      ...summarize(fullCleanupSamples, count, "ms"),
      warmup_samples: 0,
      measured_samples: fullCleanupSamples.length,
      baseline_dom_rows: count,
      operation_end_rows: 0,
      measured_scope: "Clear all keyed rows and verify every row component owner cleanup",
    });
  }

  {
    const replaceSamples = [];
    for (let sample = 0; sample < 20; sample += 1) {
      await loadApp(page, 1_000, 0, true);
      const measurement = await page.evaluate(() => {
        const button = document.querySelector('[data-testid="bench-replace-1000"]');
        const list = document.querySelector('[data-testid="todo-list"]');
        const counter = document.querySelector('[data-testid="todo-unmount-count"]');
        if (!button || !list || !counter) throw new Error("Replace controls missing");
        const oldRows = Array.from(list.querySelectorAll(":scope > li"));
        const initialCleanup = Number(counter.textContent.match(/\d+/)?.[0] ?? 0);
        const started = performance.now();
        button.click();
        const elapsed = performance.now() - started;
        const newRows = Array.from(list.querySelectorAll(":scope > li"));
        const cleanupDelta = Number(counter.textContent.match(/\d+/)?.[0] ?? 0) - initialCleanup;
        if (newRows.length !== 1_000 || cleanupDelta !== 1_000) {
          throw new Error("Replace-all did not produce 1,000 new rows and clean up the old row components");
        }
        if (newRows.some((row, index) => row === oldRows[index])) {
          throw new Error("Replace-all unexpectedly preserved old row nodes");
        }
        return elapsed;
      });
      replaceSamples.push(measurement);
    }
    report.push({
      name: "keyed_list_replace_all_1000",
      ...summarize(replaceSamples, 1_000, "ms"),
      warmup_samples: 0,
      measured_samples: replaceSamples.length,
      baseline_dom_rows: 1_000,
      operation_end_rows: 1_000,
      measured_scope: "Replace all 1,000 rows with fresh keyed identities and verify old component cleanup",
    });
  }

  return report;
}


test("RustFlow browser runtime performance measurements", async ({ page, browser, browserErrors }) => {
  test.setTimeout(process.env.RUSTFLOW_BENCH_BROWSER_EXTENDED === "1" ? 25 * 60 * 1000 : 10 * 60 * 1000);

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
  for (const [count, repetitions, warmupSamples] of [[100, 25, 10], [1_000, 7, 1]]) {
    // Discard ten startup samples so the 100-row scenario is not biased by
    // the browser/WASM cold-start settling period. Keep the 15% A/A gate intact.
    for (let warmupSample = 0; warmupSample < warmupSamples; warmupSample += 1) {
      const warmup = await loadApp(page, count);
      expect(warmup.dom_rows).toBe(count);
    }
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
      warmup_samples: warmupSamples,
      measured_samples: repetitions,
      final_dom_rows: count,
      measured_scope: "WASM initialization and initial keyed-list DOM mount",
    });
  }

  const nestedComponentScenarios = [];
  const nestedComponentWarmupCycles = 10;
  const nestedComponentMeasuredSamples = 25;
  const nestedComponentCyclesPerSample = 100;
  const nestedComponentCyclesPerTimingBatch = 20;
  const nestedComponentTimingBatchesPerSample =
    nestedComponentCyclesPerSample / nestedComponentCyclesPerTimingBatch;

  for (const depth of [5, 20, 80]) {
    const loaded = await loadApp(page, 0, depth);
    expect(loaded.nested_component_count).toBe(depth + 1);

    const measurements = await page.evaluate(
      ({ depth, warmupCycles, repetitions, cyclesPerSample, cyclesPerTimingBatch, timingBatchesPerSample }) => {
        const toggle = document.querySelector('[data-testid="bench-depth-toggle"]');
        if (!toggle) throw new Error("Nested benchmark toggle control was not mounted");
        const componentNodes = () => Array.from(document.querySelectorAll("[data-bench-depth]"));
        const verifyTree = (mounted, phase) => {
          const nodes = componentNodes();
          const expectedCount = mounted ? depth + 1 : 0;
          if (nodes.length !== expectedCount) {
            throw new Error(phase + ": expected " + expectedCount + " recursive component nodes; got " + nodes.length);
          }
          if (mounted) {
            for (let index = 0; index < nodes.length; index += 1) {
              const expectedDepth = depth - index;
              if (Number(nodes[index].getAttribute("data-bench-depth")) !== expectedDepth) {
                throw new Error(phase + ": recursive component depth sequence is invalid");
              }
            }
          }
          if (document.querySelector('[data-testid="bench-depth-toggle"]') !== toggle) {
            throw new Error(phase + ": benchmark control was replaced during subtree updates");
          }
        };
        const toggleAndVerify = (mounted, phase) => {
          toggle.click();
          verifyTree(mounted, phase);
        };

        verifyTree(true, "initial mount");
        for (let cycle = 0; cycle < warmupCycles; cycle += 1) {
          toggleAndVerify(false, "warmup unmount " + cycle);
          toggleAndVerify(true, "warmup mount " + cycle);
        }

        const lifecycleSamples = [];
        for (let sample = 0; sample < repetitions; sample += 1) {
          let sampleElapsed = 0;
          // Twenty complete unmount+mount pairs per timing window are long
          // enough to avoid Chromium timer quantization dominating sub-ms work.
          // Verify the DOM after each timed batch, outside the measured interval.
          for (let batch = 0; batch < timingBatchesPerSample; batch += 1) {
            const started = performance.now();
            for (let cycle = 0; cycle < cyclesPerTimingBatch; cycle += 1) {
              toggle.click();
              toggle.click();
            }
            sampleElapsed += performance.now() - started;
            verifyTree(true, "measured lifecycle batch " + sample + "/" + batch);
          }
          lifecycleSamples.push(sampleElapsed / cyclesPerSample);
        }

        return {
          lifecycle_samples_raw: lifecycleSamples,
          verified_mounts: warmupCycles + repetitions * cyclesPerSample,
          verified_unmounts: warmupCycles + repetitions * cyclesPerSample,
          final_component_count: componentNodes().length,
        };
      },
      {
        depth,
        warmupCycles: nestedComponentWarmupCycles,
        repetitions: nestedComponentMeasuredSamples,
        cyclesPerSample: nestedComponentCyclesPerSample,
        cyclesPerTimingBatch: nestedComponentCyclesPerTimingBatch,
        timingBatchesPerSample: nestedComponentTimingBatchesPerSample,
      },
    );

    nestedComponentScenarios.push({
      depth,
      component_instances: depth + 1,
      warmup_cycles: nestedComponentWarmupCycles,
      measured_samples: nestedComponentMeasuredSamples,
      cycles_per_sample: nestedComponentCyclesPerSample,
      cycles_per_timing_batch: nestedComponentCyclesPerTimingBatch,
      timing_batches_per_sample: nestedComponentTimingBatchesPerSample,
      verified_mounts: measurements.verified_mounts,
      verified_unmounts: measurements.verified_unmounts,
      final_component_count: measurements.final_component_count,
      ...summarize(measurements.lifecycle_samples_raw, 1, "ms"),
      measured_scope:
        "Average complete recursive subtree unmount+mount lifecycle cycle over five timed batches of twenty cycles in one initialized WASM runtime; DOM count, depth order, and stable control identity are verified after every batch",
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
    // A 10k-row mount has higher startup and host-scheduling cost than the
    // smaller lists. Keep it as a single optimized informational stress run;
    // the controlled A/B/A comparison intentionally excludes this workload.
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

  const extendedScenarios =
    process.env.RUSTFLOW_BENCH_BROWSER_EXTENDED === "1"
      ? await measureExtendedRustFlowScenarios(page)
      : [];
  const sequentialSiblingReconciliation =
    process.env.RUSTFLOW_BENCH_BROWSER_SEQUENCE === "1"
      ? await measureSequentialSiblingReconciliation(page, 5_000)
      : null;

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
    nested_component_depth_mount_cycles: nestedComponentScenarios,
    keyed_list_incremental_insertions: listScenarios,
    keyed_list_isolated_mutations: isolatedKeyedMutationScenarios,
    extended_scenarios: extendedScenarios,
    sequential_sibling_reconciliation: sequentialSiblingReconciliation,
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
      "Extended 100/1,000/5,000/10,000-row scenarios are opt-in via RUSTFLOW_BENCH_BROWSER_EXTENDED=1 and are not part of the upstream-compatible suite or current A/B/A acceptance aggregate.",
      "The mixed sequential-sibling A/B/A workload is opt-in via RUSTFLOW_BENCH_BROWSER_SEQUENCE=1 and moves a keyed anchor across 5,000 unkeyed siblings; it is distinct from KeyedList workloads.",
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
    const scenario = report.nested_component_depth_mount_cycles.find(
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
