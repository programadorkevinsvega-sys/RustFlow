use std::future::Future;

use crate::{
    Application, HttpError, HttpMethod, Middleware, Request, Response, ServerError,
    validate_route_name, validate_route_path,
};

/// A nested route collection that shares a path prefix, optional name prefix,
/// and middleware stack.
///
/// Group middleware runs after global middleware and before any middleware later
/// attached to an individual route. Nested groups inherit parent prefix and
/// middleware.
pub struct RouteGroup<'a> {
    app: &'a mut Application,
    prefix: String,
    name_prefix: String,
    middleware: Vec<Middleware>,
}

impl<'a> RouteGroup<'a> {
    pub(crate) fn new(
        app: &'a mut Application,
        prefix: String,
        name_prefix: String,
        middleware: Vec<Middleware>,
    ) -> Self {
        Self {
            app,
            prefix,
            name_prefix,
            middleware,
        }
    }

    /// Append middleware for every route registered on this group afterwards.
    pub fn middleware(&mut self, middleware: Middleware) -> &mut Self {
        self.middleware.push(middleware);
        self
    }

    /// Set or extend the route-name prefix joined with `.`.
    pub fn name_prefix(&mut self, prefix: impl Into<String>) -> Result<&mut Self, ServerError> {
        self.name_prefix = join_route_names(&self.name_prefix, &prefix.into())?;
        Ok(self)
    }

    /// Nest another group under this group's prefix and middleware.
    pub fn group<F>(
        &mut self,
        prefix: impl Into<String>,
        configure: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: FnOnce(&mut RouteGroup<'_>) -> Result<(), ServerError>,
    {
        let prefix = join_route_paths(&self.prefix, &prefix.into())?;
        {
            let mut nested = RouteGroup::new(
                self.app,
                prefix,
                self.name_prefix.clone(),
                self.middleware.clone(),
            );
            configure(&mut nested)?;
        }
        Ok(self)
    }

    /// Register a GET handler under the group prefix.
    pub fn get<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Get, path, handler)
    }

    /// Register a POST handler under the group prefix.
    pub fn post<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Post, path, handler)
    }

    /// Register a PUT handler under the group prefix.
    pub fn put<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Put, path, handler)
    }

    /// Register a PATCH handler under the group prefix.
    pub fn patch<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Patch, path, handler)
    }

    /// Register a DELETE handler under the group prefix.
    pub fn delete<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Delete, path, handler)
    }

    /// Register a HEAD handler under the group prefix.
    pub fn head<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Head, path, handler)
    }

    /// Register an OPTIONS handler under the group prefix.
    pub fn options<F, Fut>(
        &mut self,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        self.route(HttpMethod::Options, path, handler)
    }

    /// Register a handler under the group prefix.
    pub fn route<F, Fut>(
        &mut self,
        method: HttpMethod,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        let path = join_route_paths(&self.prefix, &path.into())?;
        self.app
            .register_route(method, path, None, self.middleware.clone(), handler)?;
        Ok(self)
    }

    /// Register a uniquely named handler, joining the group name prefix.
    pub fn route_named<F, Fut>(
        &mut self,
        name: impl Into<String>,
        method: HttpMethod,
        path: impl Into<String>,
        handler: F,
    ) -> Result<&mut Self, ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Response, HttpError>> + Send + 'static,
    {
        let path = join_route_paths(&self.prefix, &path.into())?;
        let name = join_route_names(&self.name_prefix, &name.into())?;
        self.app
            .register_route(method, path, Some(name), self.middleware.clone(), handler)?;
        Ok(self)
    }
}

pub(crate) fn join_route_paths(prefix: &str, path: &str) -> Result<String, ServerError> {
    if prefix.is_empty() || prefix == "/" {
        validate_route_path(path)?;
        return Ok(path.to_owned());
    }
    validate_route_path(prefix)?;
    if path.is_empty() || path == "/" {
        return Ok(prefix.trim_end_matches('/').to_owned());
    }
    validate_route_path(path)?;
    Ok(format!("{}{}", prefix.trim_end_matches('/'), path))
}

pub(crate) fn join_route_names(prefix: &str, name: &str) -> Result<String, ServerError> {
    validate_route_name(name)?;
    if prefix.is_empty() {
        return Ok(name.to_owned());
    }
    validate_route_name(prefix)?;
    let combined = if prefix.ends_with('.') {
        format!("{prefix}{name}")
    } else {
        format!("{prefix}.{name}")
    };
    validate_route_name(&combined)?;
    Ok(combined)
}

pub(crate) fn normalize_group_prefix(prefix: &str) -> Result<String, ServerError> {
    if prefix.is_empty() || prefix == "/" {
        return Ok(String::new());
    }
    validate_route_path(prefix)?;
    Ok(prefix.trim_end_matches('/').to_owned())
}
