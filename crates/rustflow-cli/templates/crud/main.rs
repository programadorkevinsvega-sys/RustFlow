use std::{error::Error, sync::Arc, time::Duration};

use http::{header::SET_COOKIE, StatusCode};
use rustflow::view::{Element, View};
use rustflow_server::{
    orm::load_one,
    AccessPolicy, AppConfig, AppEnvironment, Application, AuthIdentity, CsrfProtection,
    DatabaseBackend, DatabaseConfig, DatabasePool, HttpError, HttpMethod, PasswordHash,
    Request, Response, RolePolicy, SessionCookieConfig, SessionManager, ValidateInput,
    ValidationErrors, html_document,
};
use rustflow_server::sea_orm::{
    ActiveValue::{NotSet, Set},
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde::Deserialize;

use crate::models::category::{Column as CategoryColumn, Entity as CategoryEntity, Model as Category};
use crate::models::product::{
    ActiveModel as ProductActiveModel, Column as ProductColumn, Entity as ProductEntity,
    Model as Product,
};

pub mod components;
pub mod controllers;
pub mod models;
pub mod pages;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginForm {
    _csrf: Option<String>,
    username: String,
    password: String,
}

impl ValidateInput for LoginForm {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.username.trim().is_empty() {
            errors.add_field_error("username", "Enter your username.");
        }
        if self.password.is_empty() {
            errors.add_field_error("password", "Enter your password.");
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductForm {
    _csrf: Option<String>,
    name: String,
    sku: String,
    description: Option<String>,
    price_cents: i64,
    category_id: i64,
}

impl ValidateInput for ProductForm {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        let name = self.name.trim();
        let sku = self.sku.trim();

        if name.is_empty() {
            errors.add_field_error("name", "A product name is required.");
        } else if name.len() > 120 {
            errors.add_field_error("name", "The product name must be at most 120 bytes.");
        }
        if sku.is_empty() {
            errors.add_field_error("sku", "A SKU is required.");
        } else if sku.len() > 64 {
            errors.add_field_error("sku", "The SKU must be at most 64 bytes.");
        }
        if self.price_cents < 0 {
            errors.add_field_error("price_cents", "Price cannot be negative.");
        }
        if self.category_id <= 0 {
            errors.add_field_error("category_id", "Choose a valid category.");
        }
        if self.description.as_ref().is_some_and(|value| value.len() > 2_000) {
            errors.add_field_error("description", "Description must be at most 2000 bytes.");
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

fn view_element(
    tag: &str,
    attributes: &[(&str, String)],
    children: Vec<View>,
) -> Result<View, HttpError> {
    let mut element = Element::new(tag).map_err(|_| HttpError::internal_server_error())?;
    for (name, value) in attributes {
        element = element
            .with_attribute(*name, value.clone())
            .map_err(|_| HttpError::internal_server_error())?;
    }
    Ok(element.with_children(children).build())
}

fn text_input(
    label: &str,
    name: &str,
    value: &str,
    input_type: &str,
) -> Result<View, HttpError> {
    let input = view_element(
        "input",
        &[
            ("type", input_type.to_owned()),
            ("name", name.to_owned()),
            ("value", value.to_owned()),
        ],
        Vec::new(),
    )?;
    view_element("label", &[], vec![View::text(label), input])
}

fn csrf_field(token: &str) -> Result<View, HttpError> {
    view_element(
        "input",
        &[
            ("type", "hidden".to_owned()),
            ("name", "_csrf".to_owned()),
            ("value", token.to_owned()),
        ],
        Vec::new(),
    )
}

fn csrf_response_cookie(
    response: Response,
    csrf: &CsrfProtection,
    request: &Request,
) -> Result<Response, HttpError> {
    let token = csrf
        .token_for(request)
        .map_err(|_| HttpError::internal_server_error())?;
    let cookie = csrf
        .cookie_header(&token, request.is_https())
        .map_err(|_| HttpError::internal_server_error())?;
    Ok(response.with_header(SET_COOKIE, cookie))
}

fn page(title: &str, children: Vec<View>) -> Result<Response, HttpError> {
    let body = View::element("main", children)
        .map_err(|_| HttpError::internal_server_error())?;
    html_document(title, "en", &body)
}

fn parse_product_id(request: &Request) -> Result<i64, HttpError> {
    request
        .path_param("id")
        .ok_or_else(|| HttpError::bad_request("Missing product identifier"))?
        .parse::<i64>()
        .map_err(|_| HttpError::bad_request("Product identifier must be a positive integer"))
        .and_then(|id| {
            if id > 0 {
                Ok(id)
            } else {
                Err(HttpError::bad_request("Product identifier must be a positive integer"))
            }
        })
}

fn product_active_model(form: ProductForm) -> ProductActiveModel {
    ProductActiveModel {
        id: NotSet,
        name: Set(form.name.trim().to_owned()),
        sku: Set(form.sku.trim().to_owned()),
        description: Set(form.description.filter(|value| !value.trim().is_empty())),
        price_cents: Set(form.price_cents),
        category_id: Set(form.category_id),
    }
}

fn category_select(selected_id: i64, categories: &[Category]) -> Result<View, HttpError> {
    let mut options = Vec::with_capacity(categories.len());
    for category in categories {
        let mut attributes = vec![("value", category.id.to_string())];
        if category.id == selected_id {
            attributes.push(("selected", "selected".to_owned()));
        }
        options.push(view_element(
            "option",
            &attributes,
            vec![View::text(category.name.clone())],
        )?);
    }
    let select = view_element("select", &[("name", "category_id".to_owned())], options)?;
    view_element("label", &[], vec![View::text("Category: "), select])
}

fn category_filter_select(selected_id: i64, categories: &[Category]) -> Result<View, HttpError> {
    let mut all_attributes = vec![("value", "0".to_owned())];
    if selected_id == 0 {
        all_attributes.push(("selected", "selected".to_owned()));
    }
    let mut options = vec![view_element(
        "option",
        &all_attributes,
        vec![View::text("All categories")],
    )?];
    for category in categories {
        let mut attributes = vec![("value", category.id.to_string())];
        if category.id == selected_id {
            attributes.push(("selected", "selected".to_owned()));
        }
        options.push(view_element(
            "option",
            &attributes,
            vec![View::text(category.name.clone())],
        )?);
    }
    let select = view_element("select", &[("name", "category_id".to_owned())], options)?;
    view_element("label", &[], vec![View::text("Category filter: "), select])
}

fn hidden_input(name: &str, value: String) -> Result<View, HttpError> {
    view_element(
        "input",
        &[
            ("type", "hidden".to_owned()),
            ("name", name.to_owned()),
            ("value", value),
        ],
        Vec::new(),
    )
}

fn pagination_form(
    search: &str,
    category_id: i64,
    page: u64,
    label: &str,
) -> Result<View, HttpError> {
    let mut children = vec![hidden_input("page", page.to_string())?];
    if !search.is_empty() {
        children.push(hidden_input("q", search.to_owned())?);
    }
    if category_id > 0 {
        children.push(hidden_input("category_id", category_id.to_string())?);
    }
    children.push(view_element(
        "button",
        &[("type", "submit".to_owned())],
        vec![View::text(label)],
    )?);
    view_element(
        "form",
        &[
            ("method", "get".to_owned()),
            ("action", "/products".to_owned()),
        ],
        children,
    )
}

const PRODUCTS_PAGE_SIZE: u64 = 5;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ProductListQuery {
    q: Option<String>,
    #[serde(default)]
    category_id: i64,
    page: Option<u64>,
}

impl ValidateInput for ProductListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.q.as_ref().is_some_and(|value| value.len() > 120) {
            errors.add_field_error("q", "Search text must be at most 120 bytes.");
        }
        if self.category_id < 0 {
            errors.add_field_error("category_id", "Category id cannot be negative.");
        }
        if self.page.is_some_and(|page| page == 0 || page > 10_000) {
            errors.add_field_error("page", "Page must be between 1 and 10000.");
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

async fn load_categories(database: &DatabasePool) -> Result<Vec<Category>, HttpError> {
    database
        .entity::<CategoryEntity>()
        .all(CategoryEntity::find().order_by_asc(CategoryColumn::Name))
        .await
        .map_err(|error| error.to_http_error())
}

async fn load_product_category(
    database: &DatabasePool,
    product: &Product,
) -> Result<Category, HttpError> {
    let products = [product.clone()];
    load_one(&products, CategoryEntity::find(), database.orm())
        .await
        .map_err(|error| error.to_http_error())?
        .into_iter()
        .next()
        .flatten()
        .ok_or_else(|| HttpError::not_found("Product category not found"))
}

fn product_form_view(
    action: &str,
    token: &str,
    product: Option<&Product>,
    categories: &[Category],
) -> Result<View, HttpError> {
    let name = product.map_or("", |row| row.name.as_str());
    let sku = product.map_or("", |row| row.sku.as_str());
    let description = product
        .and_then(|row| row.description.as_deref())
        .unwrap_or("");
    let price_cents = product.map_or_else(|| "0".to_owned(), |row| row.price_cents.to_string());
    let category_id = product.map_or(1, |row| row.category_id);

    let description_input = view_element(
        "textarea",
        &[("name", "description".to_owned())],
        vec![View::text(description)],
    )?;
    let children = vec![
        csrf_field(token)?,
        text_input("Name: ", "name", name, "text")?,
        text_input("SKU: ", "sku", sku, "text")?,
        text_input("Price (cents): ", "price_cents", &price_cents, "number")?,
        category_select(category_id, categories)?,
        view_element("label", &[], vec![View::text("Description: "), description_input])?,
        view_element("button", &[("type", "submit".to_owned())], vec![View::text("Save product")])?,
    ];
    view_element(
        "form",
        &[
            ("method", "post".to_owned()),
            ("action", action.to_owned()),
        ],
        children,
    )
}

async fn load_product(database: &DatabasePool, id: i64) -> Result<Product, HttpError> {
    database
        .entity::<ProductEntity>()
        .find_by_id(id)
        .await
        .map_err(|error| error.to_http_error())?
        .ok_or_else(|| HttpError::not_found("Product not found"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::load()?;
    if config.environment == AppEnvironment::Production {
        return Err(
            "The generated CRUD demo uses InMemorySessionStore and is not a production template; configure a durable SessionStore before deployment."
                .into(),
        );
    }

    let address = config.socket_addr();
    let database_config = DatabaseConfig::from_app_config(&config)?;
    let database = Arc::new(DatabasePool::connect(&database_config).await?);
    let migration_directory = match database.backend() {
        DatabaseBackend::Sqlite => "migrations/sqlite",
        DatabaseBackend::Postgres => "migrations/postgres",
        DatabaseBackend::MySql => "migrations/mysql",
    };
    database.run_migrations(migration_directory).await?;

    let demo_user = config.setting("RUSTFLOW_DEMO_USER")?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "copy .env.example and set RUSTFLOW_DEMO_USER"))?;
    let demo_password = config.setting("RUSTFLOW_DEMO_PASSWORD")?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "copy .env.example and set RUSTFLOW_DEMO_PASSWORD"))?;
    let csrf_secret = config.setting("RUSTFLOW_CSRF_KEY")?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "copy .env.example and set RUSTFLOW_CSRF_KEY"))?;

    let sessions = SessionManager::in_memory(Duration::from_secs(60 * 60))?
        .with_cookie_config(SessionCookieConfig::development())?;
    let csrf = CsrfProtection::new(csrf_secret.as_bytes())?;
    let password_hash = Arc::new(PasswordHash::hash(&demo_password)?);
    let policy: Arc<dyn AccessPolicy> = Arc::new(
        RolePolicy::new().allow_role("products.write", "admin")?,
    );

    let mut app = Application::new();
    app.middleware(sessions.middleware())?;

    app.get("/", |_request: Request| async {
        page(
            "RustFlow generated CRUD",
            vec![
                View::element("h1", [View::text("Product manager")])
                    .map_err(|_| HttpError::internal_server_error())?,
                View::element("p", [View::text("A generated SQL ORM application.")])
                    .map_err(|_| HttpError::internal_server_error())?,
                view_element("a", &[("href", "/login".to_owned())], vec![View::text("Sign in")])?,
            ],
        )
    })?;

    let login_page_csrf = csrf.clone();
    app.get("/login", move |request: Request| {
        let csrf = login_page_csrf.clone();
        async move {
            let token = csrf
                .token_for(&request)
                .map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf
                .cookie_header(&token, request.is_https())
                .map_err(|_| HttpError::internal_server_error())?;
            let form = view_element(
                "form",
                &[("method", "post".to_owned()), ("action", "/login".to_owned())],
                vec![
                    csrf_field(token.as_str())?,
                    text_input("Username: ", "username", "", "text")?,
                    text_input("Password: ", "password", "", "password")?,
                    view_element("button", &[("type", "submit".to_owned())], vec![View::text("Sign in")])?,
                ],
            )?;
            let response = page(
                "Sign in",
                vec![View::element("h1", [View::text("Sign in")])
                    .map_err(|_| HttpError::internal_server_error())?, form],
            )?;
            Ok(response
                .with_header(SET_COOKIE, cookie)
                .with_header(http::header::CACHE_CONTROL, http::HeaderValue::from_static("no-store")))
        }
    })?;

    let login_password = Arc::clone(&password_hash);
    let login_user = demo_user.clone();
    let login_csrf = csrf.clone();
    app.post("/login", move |request: Request| {
        let password_hash = Arc::clone(&login_password);
        let demo_user = login_user.clone();
        async move {
            let input = request.validated_form::<LoginForm>()?;
            let accepted = input.username == demo_user
                && password_hash
                    .verify(&input.password)
                    .map_err(|_| HttpError::internal_server_error())?;
            if !accepted {
                return Err(HttpError::new(StatusCode::UNAUTHORIZED, "Invalid credentials"));
            }

            let identity = AuthIdentity::new(demo_user)
                .map_err(|_| HttpError::internal_server_error())?
                .with_role("admin")
                .map_err(|_| HttpError::internal_server_error())?;
            request
                .session()
                .ok_or_else(HttpError::internal_server_error)?
                .authenticate(identity)
                .map_err(|_| HttpError::internal_server_error())?;
            Response::redirect("/products")
        }
    })?;
    app.route_middleware(HttpMethod::Post, "/login", login_csrf.middleware())?;

    let list_database = Arc::clone(&database);
    let list_csrf = csrf.clone();
    app.route_protected(HttpMethod::Get, "/products", move |request: Request| {
        let database = Arc::clone(&list_database);
        let csrf = list_csrf.clone();
        async move {
            let query = request.validated_query::<ProductListQuery>()?;
            let search = query.q.unwrap_or_default();
            let current_page = query.page.unwrap_or(1);
            let offset = (current_page - 1) * PRODUCTS_PAGE_SIZE;
            let mut statement = ProductEntity::find();
            if !search.trim().is_empty() {
                statement = statement.filter(ProductColumn::Name.contains(search.trim().to_owned()));
            }
            if query.category_id > 0 {
                statement = statement.filter(ProductColumn::CategoryId.eq(query.category_id));
            }
            let repository = database.entity::<ProductEntity>();
            let mut rows = repository
                .all(
                    statement
                        .order_by_asc(ProductColumn::Id)
                        .limit(PRODUCTS_PAGE_SIZE + 1)
                        .offset(offset),
                )
                .await
                .map_err(|error| error.to_http_error())?;
            let has_next = rows.len() > PRODUCTS_PAGE_SIZE as usize;
            rows.truncate(PRODUCTS_PAGE_SIZE as usize);
            let related_categories = load_one(
                &rows,
                CategoryEntity::find(),
                database.orm(),
            )
            .await
            .map_err(|error| error.to_http_error())?;
            let categories = load_categories(&database).await?;

            let search_form = view_element(
                "form",
                &[("method", "get".to_owned()), ("action", "/products".to_owned())],
                vec![
                    text_input("Search name: ", "q", &search, "search")?,
                    category_filter_select(query.category_id, &categories)?,
                    view_element("button", &[("type", "submit".to_owned())], vec![View::text("Search")])?,
                ],
            )?;
            let mut children = vec![
                View::element("h1", [View::text("Products")])
                    .map_err(|_| HttpError::internal_server_error())?,
                view_element("a", &[("href", "/products/new".to_owned())], vec![View::text("Create product")])?,
                search_form,
            ];
            if rows.is_empty() {
                children.push(View::element("p", [View::text("No products match the current filters.")])
                    .map_err(|_| HttpError::internal_server_error())?);
            } else {
                let mut items = Vec::with_capacity(rows.len());
                for (product, category) in rows.into_iter().zip(related_categories) {
                    let category_name = category.map_or_else(
                        || "Unknown category".to_owned(),
                        |row| row.name,
                    );
                    let label = format!(
                        "{} ({}) — {} cents — {}",
                        product.name, product.sku, product.price_cents, category_name
                    );
                    items.push(view_element(
                        "li",
                        &[],
                        vec![view_element(
                            "a",
                            &[("href", format!("/products/{}", product.id))],
                            vec![View::text(label)],
                        )?],
                    )?);
                }
                children.push(View::element("ul", items)
                    .map_err(|_| HttpError::internal_server_error())?);
            }
            children.push(View::element(
                "p",
                [View::text(format!("Page {current_page}"))],
            ).map_err(|_| HttpError::internal_server_error())?);
            if current_page > 1 {
                children.push(pagination_form(
                    &search,
                    query.category_id,
                    current_page - 1,
                    "Previous",
                )?);
            }
            if has_next {
                children.push(pagination_form(
                    &search,
                    query.category_id,
                    current_page + 1,
                    "Next",
                )?);
            }
            let response = page("Products", children)?;
            csrf_response_cookie(response, &csrf, &request)
        }
    })?;

    let new_page_database = Arc::clone(&database);
    let new_page_csrf = csrf.clone();
    app.route_protected(HttpMethod::Get, "/products/new", move |request: Request| {
        let database = Arc::clone(&new_page_database);
        let csrf = new_page_csrf.clone();
        async move {
            let categories = load_categories(&database).await?;
            let token = csrf.token_for(&request).map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf.cookie_header(&token, request.is_https()).map_err(|_| HttpError::internal_server_error())?;
            let form = product_form_view("/products", token.as_str(), None, &categories)?;
            Ok(page("Create product", vec![
                View::element("h1", [View::text("Create product")]).map_err(|_| HttpError::internal_server_error())?,
                form,
            ])?.with_header(SET_COOKIE, cookie))
        }
    })?;

    let create_database = Arc::clone(&database);
    let create_policy = Arc::clone(&policy);
    app.route_authorized(
        HttpMethod::Post,
        "/products",
        "products.write",
        Some(create_policy),
        move |request: Request| {
            let database = Arc::clone(&create_database);
            async move {
                let input = request.validated_form::<ProductForm>()?;
                database
                    .entity::<ProductEntity>()
                    .insert(product_active_model(input))
                    .await
                    .map_err(|error| error.to_http_error())?;
                Response::redirect("/products")
            }
        },
    )?;
    app.route_middleware(HttpMethod::Post, "/products", csrf.clone().middleware())?;

    let detail_database = Arc::clone(&database);
    let detail_csrf = csrf.clone();
    app.route_protected(HttpMethod::Get, "/products/{id}", move |request: Request| {
        let database = Arc::clone(&detail_database);
        let csrf = detail_csrf.clone();
        async move {
            let id = parse_product_id(&request)?;
            let product = load_product(&database, id).await?;
            let category = load_product_category(&database, &product).await?;
            let token = csrf.token_for(&request).map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf.cookie_header(&token, request.is_https()).map_err(|_| HttpError::internal_server_error())?;
            let delete_form = view_element(
                "form",
                &[("method", "post".to_owned()), ("action", format!("/products/{id}/delete"))],
                vec![
                    csrf_field(token.as_str())?,
                    view_element("button", &[("type", "submit".to_owned())], vec![View::text("Delete")])?,
                ],
            )?;
            let body = vec![
                View::element("h1", [View::text(product.name.clone())]).map_err(|_| HttpError::internal_server_error())?,
                View::element("p", [View::text(format!("SKU: {}", product.sku))]).map_err(|_| HttpError::internal_server_error())?,
                View::element("p", [View::text(format!("Price: {} cents", product.price_cents))]).map_err(|_| HttpError::internal_server_error())?,
                View::element("p", [View::text(format!("Category: {}", category.name))]).map_err(|_| HttpError::internal_server_error())?,
                View::element("p", [View::text(product.description.clone().unwrap_or_default())]).map_err(|_| HttpError::internal_server_error())?,
                view_element("a", &[("href", format!("/products/{id}/edit"))], vec![View::text("Edit")])?,
                delete_form,
            ];
            Ok(page("Product details", body)?.with_header(SET_COOKIE, cookie))
        }
    })?;

    let edit_database = Arc::clone(&database);
    let edit_csrf = csrf.clone();
    app.route_protected(HttpMethod::Get, "/products/{id}/edit", move |request: Request| {
        let database = Arc::clone(&edit_database);
        let csrf = edit_csrf.clone();
        async move {
            let id = parse_product_id(&request)?;
            let product = load_product(&database, id).await?;
            let categories = load_categories(&database).await?;
            let token = csrf.token_for(&request).map_err(|_| HttpError::internal_server_error())?;
            let cookie = csrf.cookie_header(&token, request.is_https()).map_err(|_| HttpError::internal_server_error())?;
            let form = product_form_view(&format!("/products/{id}"), token.as_str(), Some(&product), &categories)?;
            Ok(page("Edit product", vec![
                View::element("h1", [View::text("Edit product")]).map_err(|_| HttpError::internal_server_error())?,
                form,
            ])?.with_header(SET_COOKIE, cookie))
        }
    })?;

    let update_database = Arc::clone(&database);
    let update_policy = Arc::clone(&policy);
    app.route_authorized(
        HttpMethod::Post,
        "/products/{id}",
        "products.write",
        Some(update_policy),
        move |request: Request| {
            let database = Arc::clone(&update_database);
            async move {
                let id = parse_product_id(&request)?;
                let input = request.validated_form::<ProductForm>()?;
                let active = ProductActiveModel {
                    id: Set(id),
                    name: Set(input.name.trim().to_owned()),
                    sku: Set(input.sku.trim().to_owned()),
                    description: Set(input.description.filter(|value| !value.trim().is_empty())),
                    price_cents: Set(input.price_cents),
                    category_id: Set(input.category_id),
                };
                database
                    .entity::<ProductEntity>()
                    .update(active)
                    .await
                    .map_err(|error| error.to_http_error())?;
                Response::redirect(&format!("/products/{id}"))
            }
        },
    )?;
    app.route_middleware(HttpMethod::Post, "/products/{id}", csrf.clone().middleware())?;

    let delete_database = Arc::clone(&database);
    let delete_policy = Arc::clone(&policy);
    app.route_authorized(
        HttpMethod::Post,
        "/products/{id}/delete",
        "products.write",
        Some(delete_policy),
        move |request: Request| {
            let database = Arc::clone(&delete_database);
            async move {
                let id = parse_product_id(&request)?;
                let result = database
                    .entity::<ProductEntity>()
                    .delete_by_id(id)
                    .await
                    .map_err(|error| error.to_http_error())?;
                if result.rows_affected == 0 {
                    return Err(HttpError::not_found("Product not found"));
                }
                Response::redirect("/products")
            }
        },
    )?;
    app.route_middleware(HttpMethod::Post, "/products/{id}/delete", csrf.clone().middleware())?;

    let transaction_database = Arc::clone(&database);
    let transaction_policy = Arc::clone(&policy);
    app.route_authorized(
        HttpMethod::Post,
        "/admin/transaction-rollback-probe",
        "products.write",
        Some(transaction_policy),
        move |_request: Request| {
            let database = Arc::clone(&transaction_database);
            async move {
                let transaction = database.begin_transaction().await
                    .map_err(|error| error.to_http_error())?;
                let first = {
                    let repository = transaction.entity::<ProductEntity>();
                    repository.insert(ProductActiveModel {
                        id: NotSet,
                        name: Set("Rollback probe A".to_owned()),
                        sku: Set(format!("ROLLBACK-PROBE-{}", std::process::id())),
                        description: Set(None),
                        price_cents: Set(0),
                        category_id: Set(1),
                    }).await
                };
                if first.is_err() {
                    transaction.rollback().await.map_err(|error| error.to_http_error())?;
                    return Err(HttpError::internal_server_error());
                }
                let second = {
                    let repository = transaction.entity::<ProductEntity>();
                    repository.insert(ProductActiveModel {
                        id: NotSet,
                        name: Set("Rollback probe B".to_owned()),
                        sku: Set(format!("ROLLBACK-PROBE-{}", std::process::id())),
                        description: Set(None),
                        price_cents: Set(0),
                        category_id: Set(1),
                    }).await
                };
                if second.is_err() {
                    transaction.rollback().await.map_err(|error| error.to_http_error())?;
                    return Ok(Response::new(StatusCode::CONFLICT, "Duplicate SKU; transaction rolled back"));
                }
                transaction.commit().await.map_err(|error| error.to_http_error())?;
                Ok(Response::text("Both transaction writes committed"))
            }
        },
    )?;
    app.route_middleware(
        HttpMethod::Post,
        "/admin/transaction-rollback-probe",
        csrf.clone().middleware(),
    )?;

    let denied_policy = Arc::clone(&policy);
    app.route_authorized(
        HttpMethod::Get,
        "/admin/denied-demo",
        "products.delete",
        Some(denied_policy),
        |_request: Request| async { Ok(Response::text("This route must remain denied.")) },
    )?;

    println!("RustFlow generated CRUD listening on http://{address}");
    app.serve(address).await?;
    Ok(())
}
