use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use bytes::Bytes;
use http::{HeaderValue, StatusCode};

use crate::{HttpError, Response, ServerError};

const DEFAULT_MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct StaticFile {
    body: Bytes,
    content_type: &'static str,
    cache_control: HeaderValue,
}

impl StaticFile {
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn content_type(&self) -> &'static str {
        self.content_type
    }

    pub fn cache_control(&self) -> &HeaderValue {
        &self.cache_control
    }
}

#[derive(Debug, Clone)]
pub struct StaticFiles {
    root: PathBuf,
    max_file_bytes: u64,
    cache_control: HeaderValue,
    index_file: Option<PathBuf>,
}

impl StaticFiles {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ServerError> {
        let root = fs::canonicalize(root.as_ref()).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => ServerError::StaticRootNotFound,
            io::ErrorKind::PermissionDenied => ServerError::StaticRootUnreadable,
            _ => ServerError::StaticRootInvalid,
        })?;
        let metadata = fs::metadata(&root).map_err(|_| ServerError::StaticRootInvalid)?;
        if !metadata.is_dir() {
            return Err(ServerError::StaticRootNotDirectory);
        }

        Ok(Self {
            root,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            cache_control: HeaderValue::from_static("public, max-age=3600"),
            index_file: None,
        })
    }

    pub fn with_max_file_bytes(mut self, max_file_bytes: u64) -> Result<Self, ServerError> {
        if max_file_bytes == 0 {
            return Err(ServerError::InvalidStaticFileLimit);
        }
        self.max_file_bytes = max_file_bytes;
        Ok(self)
    }

    pub fn with_cache_control(mut self, value: &str) -> Result<Self, ServerError> {
        self.cache_control =
            HeaderValue::from_str(value).map_err(|_| ServerError::InvalidStaticCacheControl)?;
        Ok(self)
    }

    pub fn with_index_file(mut self, relative_path: impl AsRef<Path>) -> Result<Self, ServerError> {
        let relative_path = relative_path.as_ref();
        if relative_path.is_absolute() || contains_unsafe_components(relative_path) {
            return Err(ServerError::InvalidStaticIndex);
        }
        self.index_file = Some(relative_path.to_path_buf());
        Ok(self)
    }

    pub fn resolve(&self, relative_path: &str) -> Option<StaticFile> {
        let relative_path = if relative_path.is_empty() {
            self.index_file.as_deref()
        } else {
            Some(Path::new(relative_path))
        }?;
        let relative_path = relative_path.to_string_lossy();
        let relative = Path::new(relative_path.as_ref());
        if relative.is_absolute() || contains_unsafe_components(relative) {
            return None;
        }
        if relative
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .any(|component| component.starts_with('.'))
        {
            return None;
        }

        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return None;
            };
            current.push(name);
            if fs::symlink_metadata(&current)
                .ok()
                .is_some_and(|metadata| metadata.file_type().is_symlink())
            {
                return None;
            }
        }

        let candidate = self.root.join(relative);
        let canonical = fs::canonicalize(&candidate).ok()?;
        if !canonical.starts_with(&self.root) {
            return None;
        }
        let symlink = fs::symlink_metadata(&candidate).ok()?;
        if symlink.file_type().is_symlink() || !canonical.is_file() {
            return None;
        }
        let metadata = fs::metadata(&canonical).ok()?;
        if metadata.len() > self.max_file_bytes {
            return None;
        }
        let body = fs::read(&canonical).ok()?;
        if body.len() as u64 > self.max_file_bytes {
            return None;
        }

        Some(StaticFile {
            body: Bytes::from(body),
            content_type: content_type(canonical.extension().and_then(|value| value.to_str())),
            cache_control: self.cache_control.clone(),
        })
    }
}

pub(crate) fn static_response(file: StaticFile, head: bool) -> Response {
    let body = if head { Bytes::new() } else { file.body };
    Response::new(StatusCode::OK, body)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(file.content_type),
        )
        .with_header(http::header::CACHE_CONTROL, file.cache_control)
}

pub(crate) fn static_not_found() -> HttpError {
    HttpError::not_found("Static file not found")
}

pub(crate) fn normalize_public_prefix(prefix: &str) -> Result<String, crate::ServerError> {
    if prefix.is_empty()
        || !prefix.starts_with('/')
        || prefix.contains(['?', '#', '\\'])
        || prefix.contains("//")
        || prefix.contains(['{', '}'])
    {
        return Err(crate::ServerError::InvalidStaticPrefix);
    }
    if prefix == "/" {
        return Ok(prefix.to_owned());
    }
    Ok(prefix.trim_end_matches('/').to_owned())
}

fn contains_unsafe_components(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) || {
        let normalized = path.to_string_lossy().to_ascii_lowercase();
        normalized.contains("%2e") || normalized.contains("%2f") || normalized.contains("%5c")
    }
}

fn content_type(extension: Option<&str>) -> &'static str {
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}
