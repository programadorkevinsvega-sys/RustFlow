use std::{cell::RefCell, string::ToString};

use rustflow::components::ComponentRegistry;
use rustflow::prelude::*;
use rustflow::view::Element as ViewElement;
use rustflow_dom::{ClientNavigation, DomError, DomRenderer, ElementRef, ReactiveMountedDom};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::HtmlInputElement;

thread_local! {
    static ACTIVE_NAVIGATION: RefCell<Option<ClientNavigation>> = const { RefCell::new(None) };
    static ACTIVE_HYDRATION_MOUNT: RefCell<Option<ReactiveMountedDom>> = const { RefCell::new(None) };
}

#[derive(Clone, PartialEq)]
struct Todo {
    id: i64,
    title: String,
    done: bool,
}

#[rustflow::component]
fn Counter(initial: i32, label: String) -> impl IntoView {
    let count = signal(initial);
    let count_for_button = count.clone();

    rustflow::view! {
        <section data-testid="counter">
            <p data-testid="counter-label">{label}</p>
            <p>
                "Count: "
                <span data-testid="counter-value">{count}</span>
            </p>
            <button on:click={move |_payload| count_for_button.update(|value| *value += 1)}>
                <span data-testid="increment-label">"Increment"</span>
            </button>
        </section>
    }
}

#[rustflow::component]
fn NestedBenchmarkNode(depth: i32) -> impl IntoView {
    if depth == 0 {
        rustflow::view! {
            <div data-bench-depth={depth.to_string()}>
                "Nested benchmark leaf"
            </div>
        }
    } else {
        rustflow::view! {
            <div data-bench-depth={depth.to_string()}>
                <NestedBenchmarkNode depth={depth - 1} />
            </div>
        }
    }
}

#[rustflow::component]
fn TodoRow(
    id: i64,
    title: String,
    done: bool,
    on_toggle: EventHandler,
    on_remove: EventHandler,
    on_unmount: EventHandler,
) -> impl IntoView {
    let row_touches = signal(0_i32);
    let touch_for_button = row_touches.clone();
    let _cleanup = on_cleanup(move || on_unmount.invoke(EventPayload::default()));
    let shown_title = if done {
        format!("✓ {}", title.clone())
    } else {
        title.clone()
    };

    rustflow::view! {
        <li data-testid={format!("todo-{}", id)} data-todo-id={id.to_string()}>
            <input
                type="checkbox"
                aria-label={format!("Mark {} complete", title)}
                data-done={done.to_string()}
                prop:checked={done}
                on:change={move |payload: EventPayload| on_toggle.invoke(payload)}
            />
            <span>{shown_title}</span>
            <button
                data-testid={format!("todo-touch-{}", id)}
                aria-label={format!("Touch {}", title)}
                on:click={move |_payload| touch_for_button.update(|touches| *touches += 1)}
            >
                "Touch count: " {row_touches}
            </button>
            <button
                aria-label={format!("Remove {}", title)}
                on:click={move |payload| on_remove.invoke(payload)}
            >
                "Remove"
            </button>
        </li>
    }
}

fn todo_component(
    todo: Todo,
    todos: Signal<Vec<Todo>>,
    cleanup_count: Signal<i32>,
) -> Result<View, rustflow::view::ViewError> {
    let toggle_todos = todos.clone();
    let toggle_id = todo.id;
    let on_toggle = EventHandler::new(move |payload: EventPayload| {
        if let Some(checked) = payload.checked {
            toggle_todos.update(|items| {
                if let Some(item) = items.iter_mut().find(|item| item.id == toggle_id) {
                    item.done = checked;
                }
            });
        }
    });

    let remove_todos = todos;
    let remove_id = todo.id;
    let on_remove = EventHandler::new(move |_payload: EventPayload| {
        remove_todos.update(|items| items.retain(|item| item.id != remove_id));
    });
    let on_unmount = EventHandler::new(move |_payload: EventPayload| {
        cleanup_count.update(|count| *count += 1);
    });

    View::component(
        "TodoRow",
        [
            Property::new("id", todo.id)?,
            Property::new("title", todo.title)?,
            Property::new("done", todo.done)?,
            Property::new("on_toggle", PropertyValue::Callback(on_toggle))?,
            Property::new("on_remove", PropertyValue::Callback(on_remove))?,
            Property::new("on_unmount", PropertyValue::Callback(on_unmount))?,
        ],
        [],
        None,
    )
}

#[derive(Clone, PartialEq)]
struct OfficialBenchmarkRowData {
    id: i64,
    label: String,
}

fn official_random(max: usize) -> usize {
    ((js_sys::Math::random() * 1_000.0).round() as usize) % max
}

fn official_benchmark_rows(count: usize, first_id: i64) -> Vec<OfficialBenchmarkRowData> {
    const ADJECTIVES: [&str; 25] = [
        "pretty",
        "large",
        "big",
        "small",
        "tall",
        "short",
        "long",
        "handsome",
        "plain",
        "quaint",
        "clean",
        "elegant",
        "easy",
        "angry",
        "crazy",
        "helpful",
        "mushy",
        "odd",
        "unsightly",
        "adorable",
        "important",
        "inexpensive",
        "cheap",
        "expensive",
        "fancy",
    ];
    const COLORS: [&str; 11] = [
        "red", "yellow", "blue", "green", "pink", "brown", "purple", "brown", "white", "black",
        "orange",
    ];
    const NOUNS: [&str; 13] = [
        "table", "chair", "house", "bbq", "desk", "car", "pony", "cookie", "sandwich", "burger",
        "pizza", "mouse", "keyboard",
    ];

    (0..count)
        .map(|offset| OfficialBenchmarkRowData {
            id: first_id + offset as i64,
            label: format!(
                "{} {} {}",
                ADJECTIVES[official_random(ADJECTIVES.len())],
                COLORS[official_random(COLORS.len())],
                NOUNS[official_random(NOUNS.len())]
            ),
        })
        .collect()
}

#[rustflow::component]
fn OfficialBenchmarkRow(
    id: i64,
    label: String,
    selected: bool,
    on_select: EventHandler,
    on_remove: EventHandler,
) -> impl IntoView {
    rustflow::view! {
        <tr class={if selected { "danger" } else { "" }}>
            <td class="col-md-1">{id}</td>
            <td class="col-md-4">
                <a on:click={move |_payload| on_select.invoke(EventPayload::default())}>
                    {label}
                </a>
            </td>
            <td class="col-md-1">
                <a on:click={move |_payload| on_remove.invoke(EventPayload::default())}>
                    <span class="glyphicon glyphicon-remove" aria-hidden="true"></span>
                </a>
            </td>
            <td class="col-md-6"></td>
        </tr>
    }
}

fn official_benchmark_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(EventPayload) + 'static,
) -> Result<View, rustflow::view::ViewError> {
    Ok(ViewElement::new("div")?
        .with_attribute("class", "col-sm-6 smallpad")?
        .with_children([ViewElement::new("button")?
            .with_attribute("type", "button")?
            .with_attribute("class", "btn btn-primary btn-block")?
            .with_attribute("id", id)?
            .with_event("click", on_click)?
            .with_children([View::text(label)])
            .build()])
        .build())
}

#[rustflow::component]
fn OfficialBenchmark() -> impl IntoView {
    let rows = signal(Vec::<OfficialBenchmarkRowData>::new());
    let selected = signal(0_i64);
    let next_id = signal(1_i64);

    let run_rows = rows.clone();
    let run_selected = selected.clone();
    let run_next_id = next_id.clone();
    let run_button = official_benchmark_button("run", "Create 1,000 rows", move |_payload| {
        let first_id = run_next_id.get();
        run_next_id.update(|next| *next += 1_000);
        run_rows.set(official_benchmark_rows(1_000, first_id));
        run_selected.set(0);
    })
    .expect("valid official benchmark run button");

    let run_lots_rows = rows.clone();
    let run_lots_selected = selected.clone();
    let run_lots_next_id = next_id.clone();
    let run_lots_button =
        official_benchmark_button("runlots", "Create 10,000 rows", move |_payload| {
            let first_id = run_lots_next_id.get();
            run_lots_next_id.update(|next| *next += 10_000);
            run_lots_rows.set(official_benchmark_rows(10_000, first_id));
            run_lots_selected.set(0);
        })
        .expect("valid official benchmark runlots button");

    let append_rows = rows.clone();
    let append_next_id = next_id.clone();
    let append_button = official_benchmark_button("add", "Append 1,000 rows", move |_payload| {
        let first_id = append_next_id.get();
        append_next_id.update(|next| *next += 1_000);
        append_rows.update(|items| items.extend(official_benchmark_rows(1_000, first_id)));
    })
    .expect("valid official benchmark append button");

    let update_rows = rows.clone();
    let update_button =
        official_benchmark_button("update", "Update every 10th row", move |_payload| {
            update_rows.update(|items| {
                for item in items.iter_mut().step_by(10) {
                    item.label.push_str(" !!!");
                }
            });
        })
        .expect("valid official benchmark update button");

    let clear_rows = rows.clone();
    let clear_selected = selected.clone();
    let clear_button = official_benchmark_button("clear", "Clear", move |_payload| {
        clear_rows.set(Vec::new());
        clear_selected.set(0);
    })
    .expect("valid official benchmark clear button");

    let swap_rows = rows.clone();
    let swap_button = official_benchmark_button("swaprows", "Swap Rows", move |_payload| {
        swap_rows.update(|items| {
            if items.len() > 998 {
                items.swap(1, 998);
            }
        });
    })
    .expect("valid official benchmark swap button");

    let rows_for_region = rows.clone();
    let rows_for_actions = rows.clone();
    let selected_for_region = selected.clone();
    let selected_for_actions = selected.clone();
    let rows_region = View::dynamic(move || {
        let snapshot = rows_for_region.get();
        let selected_id = selected_for_region.get();
        let mut children = Vec::with_capacity(snapshot.len());
        for item in snapshot {
            let id = item.id;
            let select_state = selected_for_actions.clone();
            let select_handler = EventHandler::new(move |_payload: EventPayload| {
                select_state.set(id);
            });
            let remove_state = rows_for_actions.clone();
            let remove_handler = EventHandler::new(move |_payload: EventPayload| {
                remove_state.update(|items| items.retain(|candidate| candidate.id != id));
            });
            children.push(KeyedChild::new(
                id.to_string(),
                View::component(
                    "OfficialBenchmarkRow",
                    [
                        Property::new("id", item.id)?,
                        Property::new("label", item.label)?,
                        Property::new("selected", selected_id == id)?,
                        Property::new("on_select", PropertyValue::Callback(select_handler))?,
                        Property::new("on_remove", PropertyValue::Callback(remove_handler))?,
                    ],
                    [],
                    None,
                )?,
            ));
        }
        View::keyed_list(children)
    });

    let controls = View::fragment([
        run_button,
        run_lots_button,
        append_button,
        update_button,
        clear_button,
        swap_button,
    ]);

    rustflow::view! {
        <div id="main" class="container">
            <div class="jumbotron">
                <div class="row">
                    <div class="col-md-6">
                        <h1>"RustFlow (keyed)"</h1>
                    </div>
                    <div class="col-md-6">
                        <div class="row">{controls}</div>
                    </div>
                </div>
            </div>
            <table class="table table-hover table-striped test-data">
                <tbody>{rows_region}</tbody>
            </table>
            <span class="preloadicon glyphicon glyphicon-remove" aria-hidden="true"></span>
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window =
        web_sys::window().ok_or_else(|| JsValue::from_str("browser window unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("browser document unavailable"))?;
    let target = document
        .get_element_by_id("app")
        .ok_or_else(|| JsValue::from_str("missing #app mount element"))?;

    let runtime = Runtime::new();
    let root_scope = runtime.root_scope();
    let current_route = runtime
        .signal(format!(
            "{}{}",
            window.location().pathname()?,
            window.location().search()?
        ))
        .map_err(display_error)?;
    // The browser performance suite can seed a large keyed list in one initial render.
    // This keeps list-mount measurements separate from repeated incremental insertions.
    let benchmark_list_size = window
        .location()
        .search()?
        .trim_start_matches('?')
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("bench_list="))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count <= 10_000)
        .unwrap_or(0);
    // The browser performance suite can request an isolated nested component tree.
    // The limit protects the demo route from unbounded recursive component setup.
    let benchmark_component_depth = window
        .location()
        .search()?
        .trim_start_matches('?')
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("bench_depth="))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|depth| (1..=80).contains(depth))
        .unwrap_or(0);
    let benchmark_controls_enabled = window
        .location()
        .search()?
        .trim_start_matches('?')
        .split('&')
        .any(|parameter| parameter == "bench_controls=1");
    let official_benchmark_mode = window
        .location()
        .pathname()?
        .contains("/frameworks/keyed/rustflow/");
    let initial_todos = (0..benchmark_list_size)
        .map(|index| Todo {
            id: index as i64 + 1,
            title: format!("benchmark item {index}"),
            done: false,
        })
        .collect::<Vec<_>>();
    let todos = runtime.signal(initial_todos).map_err(display_error)?;
    let draft = runtime.signal(String::new()).map_err(display_error)?;
    let next_id = runtime
        .signal(benchmark_list_size as i64 + 1)
        .map_err(display_error)?;
    let show_help = runtime.signal(true).map_err(display_error)?;
    let counter_label = runtime
        .signal(String::from("Initial label"))
        .map_err(display_error)?;
    let cleanup_count = runtime.signal(0_i32).map_err(display_error)?;
    let todo_identity = runtime.signal(0_i64).map_err(display_error)?;
    let show_dom_ref = runtime.signal(true).map_err(display_error)?;
    let dom_ref_status = runtime
        .signal(String::from("not checked"))
        .map_err(display_error)?;
    let dom_ref_handle = ElementRef::new();
    let dom_ref_for_region = dom_ref_handle.clone();
    let show_dom_ref_for_region = show_dom_ref.clone();
    let dom_ref_region = View::dynamic(move || {
        if show_dom_ref_for_region.get() {
            Ok(ViewElement::new("input")?
                .with_attribute("data-testid", "dom-ref-target")?
                .with_attribute("aria-label", "DOM ref target")?
                .with_node_ref(dom_ref_for_region.clone())
                .build())
        } else {
            Ok(View::fragment([]))
        }
    });

    let todo_input_ref = ElementRef::new();

    let mut registry = ComponentRegistry::new();
    register_counter(&mut registry).map_err(display_error)?;
    register_todorow(&mut registry).map_err(display_error)?;
    register_nestedbenchmarknode(&mut registry).map_err(display_error)?;
    register_officialbenchmark(&mut registry).map_err(display_error)?;
    register_officialbenchmarkrow(&mut registry).map_err(display_error)?;

    // This optional fixture is injected only by the browser E2E tests. It exercises the
    // same public hydration API against server-shaped HTML without affecting the demo page.
    if let Some(hydration_target) = document.get_element_by_id("hydration-fixture") {
        let mut hydration_renderer =
            DomRenderer::new(document.clone(), runtime.clone(), ComponentRegistry::new());
        let interactive_fixture = hydration_target
            .get_attribute("data-hydration-case")
            .as_deref()
            == Some("interactive");

        let result = if interactive_fixture {
            let input_value = runtime.signal(String::from("Ada")).map_err(display_error)?;
            let input_for_event = input_value.clone();
            let value_for_output = input_value.clone();
            let value_for_input = input_value.clone();
            let hydration_input_ref = ElementRef::new();
            let hydration_input_ref_for_render = hydration_input_ref.clone();
            let hydration_input_ref_for_probe = hydration_input_ref.clone();
            let reverse_order = runtime.signal(false).map_err(display_error)?;
            let reverse_for_list = reverse_order.clone();
            let reverse_for_button = reverse_order.clone();
            let show_fixture_help = runtime.signal(true).map_err(display_error)?;
            let show_for_region = show_fixture_help.clone();
            let show_for_button = show_fixture_help.clone();

            let list_region = View::dynamic(move || {
                let reversed = reverse_for_list.get();
                let items = if reversed {
                    vec![("second", "Second"), ("first", "First")]
                } else {
                    vec![("first", "First"), ("second", "Second")]
                };
                let mut children = Vec::with_capacity(items.len());
                for (key, label) in items {
                    let item = ViewElement::new("li")?
                        .with_attribute("data-testid", format!("hydration-item-{key}"))?
                        .with_children([View::text(label)])
                        .build();
                    children.push(KeyedChild::new(key, item));
                }
                View::keyed_list(children)
            });

            let help_region = View::dynamic(move || {
                if show_for_region.get() {
                    Ok(ViewElement::new("p")?
                        .with_attribute("data-testid", "hydration-help")?
                        .with_attribute("role", "status")?
                        .with_children([View::text("Help")])
                        .build())
                } else {
                    Ok(View::fragment([]))
                }
            });

            hydration_renderer.hydrate_reactive(
                &hydration_target,
                &root_scope,
                move || {
                    let input_for_event = input_for_event.clone();
                    let value_for_output = value_for_output.clone();
                    let value_for_input = value_for_input.clone();
                    let hydration_input_ref_for_render = hydration_input_ref_for_render.clone();
                    let hydration_input_ref_for_probe = hydration_input_ref_for_probe.clone();
                    let reverse_for_button = reverse_for_button.clone();
                    let show_for_button = show_for_button.clone();
                    rustflow::view! {
                        <main>
                            <form data-testid="hydration-form">
                                <label for="hydration-input">"Name"</label>
                                <input
                                    id="hydration-input"
                                    aria-label="Name"
                                    ref={hydration_input_ref_for_render}
                                    prop:value={value_for_input.get()}
                                    on:input={move |payload: EventPayload| {
                                        input_for_event.set(payload.value.unwrap_or_default());
                                    }}
                                />
                                <p data-testid="hydration-input-output">{value_for_output}</p>
                                <button type="button" data-testid="hydration-ref-probe" on:click={move |_payload| {
                                    if let Some(element) = hydration_input_ref_for_probe.get() {
                                        let _ = element.set_attribute("data-ref-probe", "ready");
                                    }
                                }}>
                                    "Probe hydrated ref"
                                </button>
                                <button type="button" data-testid="hydration-toggle" on:click={move |_payload| {
                                    show_for_button.update(|visible| *visible = !*visible);
                                }}>
                                    "Toggle help"
                                </button>
                                <button type="button" data-testid="hydration-reverse" on:click={move |_payload| {
                                    reverse_for_button.update(|reversed| *reversed = !*reversed);
                                }}>
                                    "Reverse list"
                                </button>
                            </form>
                            <ul data-testid="hydration-list">{list_region}</ul>
                            {help_region}
                            <a href="#hydration-details" data-testid="hydration-link">"Go"</a>
                            <div id="hydration-details">"Target"</div>
                        </main>
                    }
                    .map_err(DomError::from)
                },
            )
        } else {
            let hydration_count = runtime.signal(0_i32).map_err(display_error)?;
            let count_for_render = hydration_count.clone();
            let count_for_button = hydration_count.clone();
            hydration_renderer.hydrate_reactive(&hydration_target, &root_scope, move || {
                let count_for_button = count_for_button.clone();
                let count_for_render = count_for_render.clone();
                rustflow::view! {
                    <button data-testid="hydration-action" on:click={move |_payload| {
                        count_for_button.update(|value| *value += 1);
                    }}>
                        "Count: " {count_for_render}
                    </button>
                }
                .map_err(DomError::from)
            })
        };

        let hydration_succeeded = match result {
            Ok(mounted) => {
                ACTIVE_HYDRATION_MOUNT.with(|slot| *slot.borrow_mut() = Some(mounted));
                true
            }
            Err(error) => {
                // A mismatch is diagnosed but the server DOM remains in place as static HTML.
                web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                false
            }
        };

        if hydration_succeeded
            && !interactive_fixture
            && hydration_target
                .get_attribute("data-hydration-repeat")
                .as_deref()
                == Some("true")
        {
            // Exercise the duplicate-hydration guard: a second attempt must fail before
            // installing any additional listeners, leaving the first mount active.
            let retry_count = runtime.signal(0_i32).map_err(display_error)?;
            let retry_for_render = retry_count.clone();
            let retry_for_button = retry_count.clone();
            let repeated =
                hydration_renderer.hydrate_reactive(&hydration_target, &root_scope, move || {
                    let retry_for_render = retry_for_render.clone();
                    let retry_for_button = retry_for_button.clone();
                    rustflow::view! {
                        <button data-testid="hydration-action" on:click={move |_payload| {
                            retry_for_button.update(|value| *value += 1);
                        }}>
                            "Count: " {retry_for_render}
                        </button>
                    }
                    .map_err(DomError::from)
                });
            match repeated {
                Ok(_mounted) => web_sys::console::error_1(&JsValue::from_str(
                    "hydration unexpectedly succeeded more than once",
                )),
                Err(error) => {
                    web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                }
            }
        }
    }

    let mut renderer = DomRenderer::new(document.clone(), runtime, registry);
    let route_for_render = current_route.clone();
    let mounted = renderer
        .mount_reactive(&target, &root_scope, move || {
            let route = route_for_render.get();
            if official_benchmark_mode {
                return View::component("OfficialBenchmark", [], [], None).map_err(DomError::from);
            }
            // This route contains only the recursively nested component tree so its
            // initialization measurement is not dominated by the normal demo UI.
            if benchmark_component_depth > 0 {
                return View::component(
                    "NestedBenchmarkNode",
                    [Property::new("depth", benchmark_component_depth as i32)?],
                    [],
                    None,
                )
                .map_err(DomError::from);
            }
            match route.split('?').next().unwrap_or("/") {
                "/" => {}
                "/about" => {
                    return rustflow::view! {
                        <main data-testid="about-page">
                            <h1>"About RustFlow"</h1>
                            <p>"This page was rendered by the RustFlow client navigation example."</p>
                            <a href="/" data-testid="about-home-link">"Home"</a>
                        </main>
                    }
                    .map_err(DomError::from);
                }
                _ => {
                    return rustflow::view! {
                        <main data-testid="not-found-page">
                            <h1>"Route unavailable"</h1>
                            <p>"The client does not own this route; the server fallback handles it."</p>
                            <a href="/" data-testid="not-found-home-link">"Home"</a>
                        </main>
                    }
                    .map_err(DomError::from);
                }
            }
            // Clone captures within each render pass so the render closure remains Fn.
            let todos_for_region = todos.clone();
            let todos_for_rows = todos.clone();
            let draft_for_input = draft.clone();
            let draft_for_add = draft.clone();
            let todos_for_add = todos.clone();
            let todos_for_reverse = todos.clone();
            let next_id_for_add = next_id.clone();
            let show_for_region = show_help.clone();
            let show_for_button = show_help.clone();
            let counter_label_for_component = counter_label.clone();
            let counter_label_for_button = counter_label.clone();
            let cleanup_count_for_rows = cleanup_count.clone();
            let cleanup_count_for_display = cleanup_count.clone();
            let todo_identity_for_region = todo_identity.clone();
            let todo_identity_for_button = todo_identity.clone();

            let todo_region = View::dynamic(move || {
                let snapshot = todos_for_region.get();
                let identity = todo_identity_for_region.get();
                let mut children = Vec::with_capacity(snapshot.len());
                for todo in snapshot {
                    let id = todo.id;
                    let key = if id == 1 {
                        format!("todo-{id}-{identity}")
                    } else {
                        format!("todo-{id}")
                    };
                    children.push(KeyedChild::new(
                        key,
                        todo_component(
                            todo,
                            todos_for_rows.clone(),
                            cleanup_count_for_rows.clone(),
                        )?,
                    ));
                }
                View::keyed_list(children)
            });

            let benchmark_controls_enabled = benchmark_controls_enabled;
            let benchmark_controls = if benchmark_controls_enabled {
                let update_todos = todos.clone();
                let update_every_tenth = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-update-every-10th")?
                    .with_event("click", move |_payload| {
                        update_todos.update(|items| {
                            for item in items.iter_mut().step_by(10) {
                                item.title.push_str(" !!!");
                            }
                        });
                    })?
                    .with_children([View::text("Update every tenth row")])
                    .build();

                let swap_todos = todos.clone();
                let swap_rows = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-swap-rows")?
                    .with_event("click", move |_payload| {
                        swap_todos.update(|items| {
                            if items.len() > 998 {
                                items.swap(1, 998);
                            }
                        });
                    })?
                    .with_children([View::text("Swap rows 2 and 999")])
                    .build();

                let move_first_todos = todos.clone();
                let move_first_to_end = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-move-first-to-end")?
                    .with_event("click", move |_payload| {
                        move_first_todos.update(|items| {
                            if items.len() > 1 {
                                items.rotate_left(1);
                            }
                        });
                    })?
                    .with_children([View::text("Move first row to end")])
                    .build();

                let move_last_todos = todos.clone();
                let move_last_to_front = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-move-last-to-front")?
                    .with_event("click", move |_payload| {
                        move_last_todos.update(|items| {
                            if items.len() > 1 {
                                items.rotate_right(1);
                            }
                        });
                    })?
                    .with_children([View::text("Move last row to front")])
                    .build();

                let reverse_section_todos = todos.clone();
                let reverse_first_section = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-reverse-first-section")?
                    .with_event("click", move |_payload| {
                        reverse_section_todos.update(|items| {
                            let count = items.len().min(100);
                            items[..count].reverse();
                        });
                    })?
                    .with_children([View::text("Reverse first 100 rows")])
                    .build();

                let append_todos = todos.clone();
                let append_next_id = next_id.clone();
                let append_one_thousand = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-append-1000")?
                    .with_event("click", move |_payload| {
                        let first_id = append_next_id.get();
                        append_next_id.update(|next| *next += 1_000);
                        append_todos.update(|items| {
                            items.extend((0..1_000).map(|offset| {
                                let id = first_id + offset;
                                Todo {
                                    id,
                                    title: format!("benchmark item {id}"),
                                    done: false,
                                }
                            }));
                        });
                    })?
                    .with_children([View::text("Append 1,000 rows")])
                    .build();

                let remove_last_todos = todos.clone();
                let remove_last_one_thousand = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-remove-last-1000")?
                    .with_event("click", move |_payload| {
                        remove_last_todos.update(|items| {
                            let start = items.len().saturating_sub(1_000);
                            items.truncate(start);
                        });
                    })?
                    .with_children([View::text("Remove last 1,000 rows")])
                    .build();

                let replace_todos = todos.clone();
                let replace_next_id = next_id.clone();
                let replace_one_thousand = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-replace-1000")?
                    .with_event("click", move |_payload| {
                        let first_id = replace_next_id.get();
                        replace_next_id.update(|next| *next += 1_000);
                        replace_todos.update(|items| {
                            *items = (0..1_000)
                                .map(|offset| {
                                    let id = first_id + offset;
                                    Todo {
                                        id,
                                        title: format!("benchmark item {id}"),
                                        done: false,
                                    }
                                })
                                .collect();
                        });
                    })?
                    .with_children([View::text("Replace with 1,000 rows")])
                    .build();

                let remove_section_todos = todos.clone();
                let remove_first_section = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-remove-first-section")?
                    .with_event("click", move |_payload| {
                        remove_section_todos.update(|items| {
                            let count = items.len().min(100);
                            items.drain(..count);
                        });
                    })?
                    .with_children([View::text("Remove first 100 rows")])
                    .build();

                let clear_todos = todos.clone();
                let clear_all_rows = ViewElement::new("button")?
                    .with_attribute("type", "button")?
                    .with_attribute("data-testid", "bench-clear-all")?
                    .with_event("click", move |_payload| {
                        clear_todos.update(|items| items.clear());
                    })?
                    .with_children([View::text("Clear all rows")])
                    .build();

                View::fragment([
                    update_every_tenth,
                    swap_rows,
                    move_first_to_end,
                    move_last_to_front,
                    reverse_first_section,
                    append_one_thousand,
                    remove_last_one_thousand,
                    replace_one_thousand,
                    remove_first_section,
                    clear_all_rows,
                ])
            } else {
                View::fragment([])
            };

            let show_for_region = show_for_region.clone();
            let todo_input_ref_for_add = todo_input_ref.clone();
            let dom_ref_for_probe = dom_ref_handle.clone();
            let dom_ref_status_for_probe = dom_ref_status.clone();
            let dom_ref_status_for_display = dom_ref_status.clone();
            let show_dom_ref_for_button = show_dom_ref.clone();
            let help_region = View::dynamic(move || {
                if show_for_region.get() {
                    Ok(ViewElement::new("p")?
                        .with_attribute("data-testid", "help-message")?
                        .with_attribute("role", "status")?
                        .with_children([View::text(
                            "Help: add, complete, remove, and reverse tasks.",
                        )])
                        .build())
                } else {
                    Ok(View::fragment([]))
                }
            });

            rustflow::view! {
                <main data-testid="home-page">
                    <h1>"RustFlow Phase 3 browser demo"</h1>
                    <nav aria-label="Main navigation">
                        <a href="/" data-testid="home-link">"Home"</a>
                        <a href="/about" data-testid="about-link">"About"</a>
                    </nav>
                    <form method="get" action="/" data-rustflow-enhance="true" data-testid="search-form">
                        <label for="navigation-query">"Search query"</label>
                        <input id="navigation-query" type="search" name="q" aria-label="Search query" />
                        <button type="submit">"Search"</button>
                    </form>
                    <Counter initial={0} label={counter_label_for_component.get()} />
                    <button data-testid="change-counter-label" on:click={move |_payload| {
                        counter_label_for_button.set(String::from("Updated label"));
                    }}>
                        "Change counter label"
                    </button>
                    <section aria-label="Todo editor">
                        <label for="todo-input">"New task"</label>
                        <input
                            id="todo-input"
                            aria-label="New todo"
                            ref={todo_input_ref}
                            prop:value={""}
                            on:input={move |payload: EventPayload| {
                                draft_for_input.set(payload.value.unwrap_or_default());
                            }}
                        />
                        <button on:click={move |_payload| {
                            let title = draft_for_add.get().trim().to_owned();
                            if title.is_empty() {
                                return;
                            }
                            let id = next_id_for_add.get();
                            next_id_for_add.update(|next| *next += 1);
                            todos_for_add.update(|items| items.push(Todo {
                                id,
                                title,
                                done: false,
                            }));
                            draft_for_add.set(String::new());
                            if let Some(input) = todo_input_ref_for_add
                                .get()
                                .and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
                            {
                                input.set_value("");
                            }
                        }}>
                            "Add todo"
                        </button>
                    </section>
                    <ul data-testid="todo-list">{todo_region}</ul>
                    {benchmark_controls}
                    <p data-testid="todo-unmount-count">"Unmounted rows: " {cleanup_count_for_display}</p>
                    <section aria-label="DOM element reference demo">
                        {dom_ref_region}
                        <p data-testid="dom-ref-status">{dom_ref_status_for_display}</p>
                        <button type="button" data-testid="dom-ref-probe" on:click={move |_payload| {
                            if let Some(element) = dom_ref_for_probe.get() {
                                let _ = element.set_attribute("data-ref-probe", "ready");
                                dom_ref_status_for_probe.set(String::from("mounted"));
                            } else {
                                dom_ref_status_for_probe.set(String::from("unmounted"));
                            }
                        }}>
                            "Probe DOM ref"
                        </button>
                        <button type="button" data-testid="dom-ref-toggle" on:click={move |_payload| {
                            show_dom_ref_for_button.update(|visible| *visible = !*visible);
                        }}>
                            "Toggle DOM ref"
                        </button>
                    </section>
                    <button on:click={move |_payload| {
                        todos_for_reverse.update(|items| items.reverse());
                    }}>
                        "Reverse order"
                    </button>
                    <button data-testid="change-todo-key" on:click={move |_payload| {
                        todo_identity_for_button.update(|identity| *identity += 1);
                    }}>
                        "Replace first todo instance"
                    </button>
                    <button on:click={move |_payload| {
                        show_for_button.update(|visible| *visible = !*visible);
                    }}>
                        "Toggle help"
                    </button>
                    {help_region}
                </main>
            }
            .map_err(DomError::from)
        })
        .map_err(display_error)?;

    let route_for_navigation = current_route.clone();
    let navigation = ClientNavigation::install(target, mounted, move |path| {
        let pathname = path.split('?').next().unwrap_or("/");
        match pathname {
            "/" | "/about" => {
                route_for_navigation.set(path.to_owned());
                Ok(true)
            }
            _ => Ok(false),
        }
    })
    .map_err(display_error)?;
    ACTIVE_NAVIGATION.with(|slot| *slot.borrow_mut() = Some(navigation));
    Ok(())
}

fn display_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
