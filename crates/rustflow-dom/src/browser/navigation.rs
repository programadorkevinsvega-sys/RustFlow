//! Same-origin client navigation with conservative native fallbacks.
//!
//! Install this only after the initial view has mounted or hydrated. The callback must update
//! the application's route state synchronously and return Ok(false) without changing state for
//! paths the client does not own. The HTTP server remains authoritative: a transition is committed
//! only after a successful same-origin HTML response and a successful reactive view update.

use std::{cell::RefCell, rc::Rc};

use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Document, Element, Event, HtmlAnchorElement, HtmlFormElement, HtmlInputElement,
    HtmlSelectElement, HtmlTextAreaElement, MouseEvent, Node, Response, Url, UrlSearchParams,
    Window,
};

use super::{DomError, ReactiveMountedDom};

type RouteCallback = Box<dyn FnMut(&str) -> Result<bool, DomError> + 'static>;

struct NavigationState {
    window: Window,
    target: Element,
    mounted: ReactiveMountedDom,
    on_navigate: RouteCallback,
    current_url: String,
    generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NavigationKind {
    Push,
    Pop,
}

/// Opt-in same-origin client navigation tied to an existing RustFlow reactive mount.
///
/// Links are intercepted only for unmodified primary-button clicks to same-origin HTTP(S) URLs
/// with no fragment, download, alternate target, or rel=external. Forms are enhanced only when
/// they carry data-rustflow-enhance and use a supported same-origin GET submission with scalar
/// controls. Unsupported forms (including POST and file uploads) remain native HTML forms.
///
/// The route callback receives a path and query string such as /articles?page=2. It must return
/// Ok(true) after updating the reactive route state for a supported route, and Ok(false) for an
/// unsupported route without changing state. A failed request, incompatible response, unsupported
/// route, or failed render falls back to a full document navigation. Browser back/forward use the
/// same checks and reload the selected URL if client handling fails.
///
/// The initial mount should be created by DomRenderer::hydrate_reactive for marked SSR HTML or
/// DomRenderer::mount_reactive for a client-rendered entry point. Subsequent route updates use the
/// same mount/runtime and therefore retain RustFlow's reconciliation and cleanup semantics.
pub struct ClientNavigation {
    document: Document,
    window: Window,
    _state: Rc<RefCell<NavigationState>>,
    click_listener: Closure<dyn FnMut(Event)>,
    submit_listener: Closure<dyn FnMut(Event)>,
    popstate_listener: Closure<dyn FnMut(Event)>,
}

impl ClientNavigation {
    /// Install delegated navigation listeners after the initial reactive mount is ready.
    pub fn install<F>(
        target: Element,
        mounted: ReactiveMountedDom,
        on_navigate: F,
    ) -> Result<Self, DomError>
    where
        F: FnMut(&str) -> Result<bool, DomError> + 'static,
    {
        if !mounted.is_mounted() {
            return Err(DomError::JavaScript(
                "client navigation requires a successfully mounted RustFlow view".to_owned(),
            ));
        }

        let window = web_sys::window().ok_or(DomError::WindowUnavailable)?;
        let document = window.document().ok_or(DomError::DocumentUnavailable)?;
        let current_url = window.location().href().map_err(js_error)?;
        let state = Rc::new(RefCell::new(NavigationState {
            window: window.clone(),
            target,
            mounted,
            on_navigate: Box::new(on_navigate),
            current_url,
            generation: 0,
        }));

        let click_state = state.clone();
        let click_listener = Closure::wrap(Box::new(move |event: Event| {
            if event.default_prevented() {
                return;
            }
            let url = click_state
                .try_borrow()
                .ok()
                .and_then(|navigation| eligible_link(&event, &navigation));
            let Some(url) = url else {
                return;
            };

            event.prevent_default();
            spawn_navigation(click_state.clone(), url, NavigationKind::Push, None);
        }) as Box<dyn FnMut(Event)>);

        let submit_state = state.clone();
        let submit_listener = Closure::wrap(Box::new(move |event: Event| {
            if event.default_prevented() {
                return;
            }
            let form = submit_state
                .try_borrow()
                .ok()
                .and_then(|navigation| enhanced_form_from_event(&event, &navigation));
            let Some((form, url)) = form else {
                return;
            };

            if form.has_attribute("data-rustflow-pending") {
                event.prevent_default();
                return;
            }
            // If the event was raised by a formnovalidate submitter, preserve native behavior.
            if !form.check_validity() {
                return;
            }
            if form.set_attribute("data-rustflow-pending", "true").is_err() {
                return;
            }

            event.prevent_default();
            spawn_navigation(submit_state.clone(), url, NavigationKind::Push, Some(form));
        }) as Box<dyn FnMut(Event)>);

        let popstate_state = state.clone();
        let popstate_listener = Closure::wrap(Box::new(move |_event: Event| {
            let url = popstate_state
                .try_borrow()
                .ok()
                .and_then(|navigation| navigation.window.location().href().ok())
                .and_then(|href| Url::new(&href).ok());
            if let Some(url) = url {
                spawn_navigation(popstate_state.clone(), url, NavigationKind::Pop, None);
            } else if let Ok(navigation) = popstate_state.try_borrow() {
                let _ = navigation.window.location().reload();
            }
        }) as Box<dyn FnMut(Event)>);

        document
            .add_event_listener_with_callback("click", click_listener.as_ref().unchecked_ref())
            .map_err(js_error)?;
        if let Err(error) = document
            .add_event_listener_with_callback("submit", submit_listener.as_ref().unchecked_ref())
        {
            let _ = document.remove_event_listener_with_callback(
                "click",
                click_listener.as_ref().unchecked_ref(),
            );
            return Err(js_error(error));
        }
        if let Err(error) = window.add_event_listener_with_callback(
            "popstate",
            popstate_listener.as_ref().unchecked_ref(),
        ) {
            let _ = document.remove_event_listener_with_callback(
                "click",
                click_listener.as_ref().unchecked_ref(),
            );
            let _ = document.remove_event_listener_with_callback(
                "submit",
                submit_listener.as_ref().unchecked_ref(),
            );
            return Err(js_error(error));
        }

        Ok(Self {
            document,
            window,
            _state: state,
            click_listener,
            submit_listener,
            popstate_listener,
        })
    }
}

impl Drop for ClientNavigation {
    fn drop(&mut self) {
        let _ = self.document.remove_event_listener_with_callback(
            "click",
            self.click_listener.as_ref().unchecked_ref(),
        );
        let _ = self.document.remove_event_listener_with_callback(
            "submit",
            self.submit_listener.as_ref().unchecked_ref(),
        );
        let _ = self.window.remove_event_listener_with_callback(
            "popstate",
            self.popstate_listener.as_ref().unchecked_ref(),
        );
    }
}

fn eligible_link(event: &Event, navigation: &NavigationState) -> Option<Url> {
    let mouse = event.dyn_ref::<MouseEvent>()?;
    if mouse.button() != 0
        || mouse.alt_key()
        || mouse.ctrl_key()
        || mouse.meta_key()
        || mouse.shift_key()
    {
        return None;
    }

    let target = event.target()?.dyn_into::<Element>().ok()?;
    let anchor = target
        .closest("a[href]")
        .ok()?
        .and_then(|element| element.dyn_into::<HtmlAnchorElement>().ok())?;
    if !is_inside(&navigation.target, &anchor)
        || anchor.has_attribute("download")
        || anchor.has_attribute("data-rustflow-no-nav")
        || (!anchor.target().is_empty() && !anchor.target().eq_ignore_ascii_case("_self"))
        || anchor
            .rel()
            .split_ascii_whitespace()
            .any(|relation| relation.eq_ignore_ascii_case("external"))
    {
        return None;
    }

    let url = Url::new(&anchor.href()).ok()?;
    if !is_same_origin_http(&url, &navigation.window) || !url.hash().is_empty() {
        return None;
    }
    let current = Url::new(&navigation.window.location().href().ok()?).ok()?;
    if url.href() == current.href() {
        return None;
    }
    Some(url)
}

fn enhanced_form_from_event(
    event: &Event,
    navigation: &NavigationState,
) -> Option<(HtmlFormElement, Url)> {
    let target = event.target()?.dyn_into::<Element>().ok()?;
    let form = target
        .closest("form[data-rustflow-enhance]")
        .ok()?
        .and_then(|element| element.dyn_into::<HtmlFormElement>().ok())?;
    if !is_inside(&navigation.target, &form)
        || !form.method().eq_ignore_ascii_case("get")
        || !form
            .enctype()
            .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        || (!form.target().is_empty() && !form.target().eq_ignore_ascii_case("_self"))
        || form.has_attribute("novalidate")
        || form.has_attribute("data-rustflow-no-nav")
    {
        return None;
    }

    // Keep controls whose successful-control semantics are not implemented on the native path.
    if form
        .query_selector(
            "input[type='file'], input[type='image'], select[multiple], \
             button[name], input[type='submit'][name], input[type='button'][name], \
             input[type='reset'][name], [formaction], [formmethod], [formenctype], \
             [formtarget], [formnovalidate], [dirname]",
        )
        .ok()?
        .is_some()
    {
        return None;
    }

    let url = Url::new(&form.action()).ok()?;
    if !is_same_origin_http(&url, &navigation.window) || !url.hash().is_empty() {
        return None;
    }

    let params = UrlSearchParams::new_with_str(&url.search()).ok()?;
    let elements = form.elements();
    for index in 0..elements.length() {
        let Some(element) = elements.item(index) else {
            continue;
        };
        // Match the browser's effective disabled state, including disabled fieldsets.
        if element.matches(":disabled").ok()? {
            continue;
        }
        if element.tag_name().eq_ignore_ascii_case("BUTTON") {
            continue;
        }

        if let Ok(input) = element.clone().dyn_into::<HtmlInputElement>() {
            if input.disabled() {
                continue;
            }
            let name = input.name();
            let kind = input.type_().to_ascii_lowercase();
            if name.is_empty() || matches!(kind.as_str(), "button" | "reset" | "submit") {
                continue;
            }
            if matches!(kind.as_str(), "checkbox" | "radio") && !input.checked() {
                continue;
            }
            params.append(&name, &input.value());
            continue;
        }
        if let Ok(select) = element.clone().dyn_into::<HtmlSelectElement>() {
            if select.disabled() {
                continue;
            }
            if select.multiple() {
                return None;
            }
            if !select.name().is_empty() {
                params.append(&select.name(), &select.value());
            }
            continue;
        }
        if let Ok(textarea) = element.dyn_into::<HtmlTextAreaElement>() {
            if textarea.disabled() {
                continue;
            }
            if !textarea.name().is_empty() {
                params.append(&textarea.name(), &textarea.value());
            }
        }
    }

    let query = params.to_string().as_string()?;
    if query.is_empty() {
        url.set_search("");
    } else {
        url.set_search(&format!("?{query}"));
    }
    Some((form, url))
}

fn is_inside(root: &Element, candidate: &Element) -> bool {
    let root: Node = root.clone().unchecked_into();
    let candidate: Node = candidate.clone().unchecked_into();
    root.contains(Some(&candidate))
}

fn is_same_origin_http(url: &Url, window: &Window) -> bool {
    let Ok(origin) = window.location().origin() else {
        return false;
    };
    matches!(url.protocol().as_str(), "http:" | "https:") && url.origin() == origin
}

fn relative_url(url: &Url) -> String {
    format!("{}{}", url.pathname(), url.search())
}

fn js_error(value: JsValue) -> DomError {
    DomError::JavaScript(
        value
            .as_string()
            .unwrap_or_else(|| "browser navigation operation failed".to_owned()),
    )
}

fn spawn_navigation(
    state: Rc<RefCell<NavigationState>>,
    url: Url,
    kind: NavigationKind,
    pending_form: Option<HtmlFormElement>,
) {
    spawn_local(async move {
        navigate(state.clone(), url, kind).await;
        if let Some(form) = pending_form {
            let _ = form.remove_attribute("data-rustflow-pending");
        }
    });
}

fn is_latest_generation(state: &Rc<RefCell<NavigationState>>, generation: u64) -> bool {
    state
        .try_borrow()
        .is_ok_and(|navigation| navigation.generation == generation)
}

async fn navigate(state: Rc<RefCell<NavigationState>>, requested_url: Url, kind: NavigationKind) {
    let requested_href = requested_url.href();
    let (generation, previous_url, window) = {
        let mut navigation = state.borrow_mut();
        navigation.generation = navigation.generation.wrapping_add(1);
        (
            navigation.generation,
            navigation.current_url.clone(),
            navigation.window.clone(),
        )
    };

    let destination = match fetch_html_route(&window, &requested_url).await {
        Ok(url) => url,
        Err(error) => {
            if is_latest_generation(&state, generation) {
                web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                fallback_to_server(&window, &requested_href, kind);
            }
            return;
        }
    };
    if !is_latest_generation(&state, generation) {
        return;
    }

    let destination_path = relative_url(&destination);
    let route_result = {
        let mut navigation = state.borrow_mut();
        if navigation.generation != generation {
            return;
        }
        (navigation.on_navigate)(&destination_path)
    };
    let render_error = state.borrow_mut().mounted.take_error();
    match route_result {
        Err(error) => {
            web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
            restore_previous_route(&state, generation, &previous_url);
            fallback_to_server(&window, &requested_href, kind);
            return;
        }
        Ok(false) => {
            if let Some(error) = render_error {
                web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
            }
            restore_previous_route(&state, generation, &previous_url);
            fallback_to_server(&window, &requested_href, kind);
            return;
        }
        Ok(true) => {
            if let Some(error) = render_error {
                web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
                restore_previous_route(&state, generation, &previous_url);
                fallback_to_server(&window, &requested_href, kind);
                return;
            }
        }
    }

    let destination_href = destination.href();
    let history_result = match kind {
        NavigationKind::Push => window.history().map_err(js_error).and_then(|history| {
            history
                .push_state_with_url(&JsValue::NULL, "", Some(&destination_href))
                .map_err(js_error)
        }),
        NavigationKind::Pop if destination_href != requested_href => {
            window.history().map_err(js_error).and_then(|history| {
                history
                    .replace_state_with_url(&JsValue::NULL, "", Some(&destination_href))
                    .map_err(js_error)
            })
        }
        NavigationKind::Pop => Ok(()),
    };
    if let Err(error) = history_result {
        web_sys::console::error_1(&JsValue::from_str(&error.to_string()));
        restore_previous_route(&state, generation, &previous_url);
        fallback_to_server(&window, &requested_href, kind);
        return;
    }

    let mut navigation = state.borrow_mut();
    if navigation.generation == generation {
        navigation.current_url = destination_href;
    }
}

async fn fetch_html_route(window: &Window, requested_url: &Url) -> Result<Url, DomError> {
    let promise = window.fetch_with_str(&requested_url.href());
    let response = JsFuture::from(promise)
        .await
        .map_err(js_error)?
        .dyn_into::<Response>()
        .map_err(js_error)?;
    if !response.ok() {
        return Err(DomError::JavaScript(format!(
            "navigation request returned HTTP {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .map_err(js_error)?
        .unwrap_or_default();
    if !content_type.to_ascii_lowercase().starts_with("text/html") {
        return Err(DomError::JavaScript(
            "navigation response is not an HTML document".to_owned(),
        ));
    }

    let body = JsFuture::from(response.text().map_err(js_error)?)
        .await
        .map_err(js_error)?
        .as_string()
        .ok_or_else(|| DomError::JavaScript("navigation response body is not text".to_owned()))?;
    let lowercase_body = body.to_ascii_lowercase();
    if !lowercase_body.contains("<html") || !lowercase_body.contains("<body") {
        return Err(DomError::JavaScript(
            "navigation response is not a complete HTML document".to_owned(),
        ));
    }

    let response_url = response.url();
    let destination = if response_url.is_empty() {
        requested_url.clone()
    } else {
        Url::new(&response_url).map_err(js_error)?
    };
    if !is_same_origin_http(&destination, window) {
        return Err(DomError::JavaScript(
            "navigation response redirected outside the current origin".to_owned(),
        ));
    }
    Ok(destination)
}

fn restore_previous_route(
    state: &Rc<RefCell<NavigationState>>,
    generation: u64,
    previous_url: &str,
) {
    let Ok(previous) = Url::new(previous_url) else {
        return;
    };
    let path = relative_url(&previous);
    let mut navigation = state.borrow_mut();
    if navigation.generation == generation {
        let _ = (navigation.on_navigate)(&path);
        let _ = navigation.mounted.take_error();
    }
}

fn fallback_to_server(window: &Window, url: &str, kind: NavigationKind) {
    match kind {
        NavigationKind::Push => {
            let _ = window.location().set_href(url);
        }
        NavigationKind::Pop => {
            let _ = window.location().reload();
        }
    }
}
