use std::{cell::RefCell, string::ToString};

use rustflow::components::ComponentRegistry;
use rustflow::prelude::*;
use rustflow::view::Element as ViewElement;
use rustflow_dom::{ClientNavigation, DomError, DomRenderer, ReactiveMountedDom};
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
    let todos = runtime.signal(Vec::<Todo>::new()).map_err(display_error)?;
    let draft = runtime.signal(String::new()).map_err(display_error)?;
    let next_id = runtime.signal(1_i64).map_err(display_error)?;
    let show_help = runtime.signal(true).map_err(display_error)?;
    let counter_label = runtime
        .signal(String::from("Initial label"))
        .map_err(display_error)?;
    let cleanup_count = runtime.signal(0_i32).map_err(display_error)?;
    let todo_identity = runtime.signal(0_i64).map_err(display_error)?;

    let mut registry = ComponentRegistry::new();
    register_counter(&mut registry).map_err(display_error)?;
    register_todorow(&mut registry).map_err(display_error)?;

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
                    let reverse_for_button = reverse_for_button.clone();
                    let show_for_button = show_for_button.clone();
                    rustflow::view! {
                        <main>
                            <form data-testid="hydration-form">
                                <label for="hydration-input">"Name"</label>
                                <input
                                    id="hydration-input"
                                    aria-label="Name"
                                    prop:value={value_for_input.get()}
                                    on:input={move |payload: EventPayload| {
                                        input_for_event.set(payload.value.unwrap_or_default());
                                    }}
                                />
                                <p data-testid="hydration-input-output">{value_for_output}</p>
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
            let document_for_add = document.clone();
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

            let show_for_region = show_for_region.clone();
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
                            if let Some(input) = document_for_add
                                .get_element_by_id("todo-input")
                                .and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
                            {
                                input.set_value("");
                            }
                        }}>
                            "Add todo"
                        </button>
                    </section>
                    <ul data-testid="todo-list">{todo_region}</ul>
                    <p data-testid="todo-unmount-count">"Unmounted rows: " {cleanup_count_for_display}</p>
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
