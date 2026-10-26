use std::{cell::RefCell, string::ToString};

use rustflow::components::ComponentRegistry;
use rustflow::prelude::*;
use rustflow::view::Element as ViewElement;
use rustflow_dom::{DomError, DomRenderer, ReactiveMountedDom};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::HtmlInputElement;

thread_local! {
    static ACTIVE_MOUNT: RefCell<Option<ReactiveMountedDom>> = const { RefCell::new(None) };
}

#[derive(Clone, PartialEq)]
struct Todo {
    id: i64,
    title: String,
    done: bool,
}

#[rustflow::component]
fn Counter(initial: i32) -> impl IntoView {
    let count = signal(initial);
    let count_for_button = count.clone();

    rustflow::view! {
        <section data-testid="counter">
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

fn todo_row(todo: Todo, todos: Signal<Vec<Todo>>) -> Result<View, rustflow::view::ViewError> {
    let toggle_todos = todos.clone();
    let toggle_id = todo.id;
    let checkbox = ViewElement::new("input")?
        .with_attribute("type", "checkbox")?
        .with_attribute("aria-label", format!("Mark {} complete", todo.title))?
        .with_property("checked", todo.done)?
        .with_event("change", move |payload: EventPayload| {
            if let Some(checked) = payload.checked {
                toggle_todos.update(|items| {
                    if let Some(item) = items.iter_mut().find(|item| item.id == toggle_id) {
                        item.done = checked;
                    }
                });
            }
        })?
        .build();

    let title = if todo.done {
        format!("✓ {}", todo.title)
    } else {
        todo.title.clone()
    };
    let title = View::element("span", [View::text(title)])?;

    let remove_todos = todos;
    let remove_id = todo.id;
    let remove = ViewElement::new("button")?
        .with_attribute("aria-label", format!("Remove {}", todo.title))?
        .with_event("click", move |_payload| {
            remove_todos.update(|items| items.retain(|item| item.id != remove_id));
        })?
        .with_children([View::text("Remove")])
        .build();

    Ok(ViewElement::new("li")?
        .with_attribute("data-testid", format!("todo-{}", todo.id))?
        .with_attribute("data-todo-id", todo.id.to_string())?
        .with_children([checkbox, title, remove])
        .build())
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
    let todos = runtime.signal(Vec::<Todo>::new()).map_err(display_error)?;
    let draft = runtime.signal(String::new()).map_err(display_error)?;
    let next_id = runtime.signal(1_i64).map_err(display_error)?;
    let show_help = runtime.signal(true).map_err(display_error)?;

    let mut registry = ComponentRegistry::new();
    register_counter(&mut registry).map_err(display_error)?;

    let mut renderer = DomRenderer::new(document.clone(), runtime, registry);
    let mounted = renderer
        .mount_reactive(&target, &root_scope, move || {
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

            let todo_region = View::dynamic(move || {
                let snapshot = todos_for_region.get();
                let mut children = Vec::with_capacity(snapshot.len());
                for todo in snapshot {
                    let id = todo.id;
                    children.push(KeyedChild::new(id, todo_row(todo, todos_for_rows.clone())?));
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
                <main>
                    <h1>"RustFlow Phase 3 browser demo"</h1>
                    <Counter initial={0} />
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
                    <button on:click={move |_payload| {
                        todos_for_reverse.update(|items| items.reverse());
                    }}>
                        "Reverse order"
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

    ACTIVE_MOUNT.with(|slot| *slot.borrow_mut() = Some(mounted));
    Ok(())
}

fn display_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
