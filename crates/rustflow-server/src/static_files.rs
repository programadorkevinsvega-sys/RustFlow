use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
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
    length: u64,
    etag: HeaderValue,
    last_modified: HeaderValue,
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

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn etag(&self) -> &HeaderValue {
        &self.etag
    }

    pub fn last_modified(&self) -> &HeaderValue {
        &self.last_modified
    }
}

#[derive(Debug, Clone)]
pub struct StaticFiles {
    root: PathBuf,
    max_file_bytes: u64,
    cache_control: HeaderValue,
    index_file: Option<PathBuf>,
    spa_fallback: Option<PathBuf>,
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
            spa_fallback: None,
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

    pub fn with_spa_fallback(
        mut self,
        relative_path: impl AsRef<Path>,
    ) -> Result<Self, ServerError> {
        let relative_path = relative_path.as_ref();
        if relative_path.is_absolute() || contains_unsafe_components(relative_path) {
            return Err(ServerError::InvalidStaticIndex);
        }
        self.spa_fallback = Some(relative_path.to_path_buf());
        Ok(self)
    }

    pub fn resolve(&self, relative_path: &str) -> Option<StaticFile> {
        let relative_path = if relative_path.is_empty() {
            self.index_file.as_deref()
        } else {
            Some(Path::new(relative_path))
        };
        let relative_path = match relative_path {
            Some(path) => path,
            None => Path::new(""),
        };
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
        let canonical = match fs::canonicalize(&candidate) {
            Ok(canonical) => canonical,
            Err(_) => return self.resolve_spa_fallback(),
        };
        let canonical = if !canonical.is_file() {
            return self.resolve_spa_fallback();
        } else {
            canonical
        };
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
        let length = metadata.len();
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let modified_seconds = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let etag = format!("W/\"{length}-{modified_seconds}\"");
        let last_modified = httpdate::fmt_http_date(modified);

        Some(StaticFile {
            body: Bytes::from(body),
            content_type: content_type(canonical.extension().and_then(|value| value.to_str())),
            cache_control: self.cache_control.clone(),
            length,
            etag: HeaderValue::from_str(&etag).ok()?,
            last_modified: HeaderValue::from_str(&last_modified).ok()?,
        })
    }

    fn resolve_spa_fallback(&self) -> Option<StaticFile> {
        let relative = self.spa_fallback.as_deref()?;
        self.resolve_file(relative)
    }

    fn resolve_file(&self, relative: &Path) -> Option<StaticFile> {
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
        if !canonical.starts_with(&self.root) || !canonical.is_file() {
            return None;
        }
        let symlink = fs::symlink_metadata(&candidate).ok()?;
        if symlink.file_type().is_symlink() {
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
        let length = metadata.len();
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let modified_seconds = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let etag = format!("W/\"{length}-{modified_seconds}\"");
        let last_modified = httpdate::fmt_http_date(modified);
        Some(StaticFile {
            body: Bytes::from(body),
            content_type: content_type(canonical.extension().and_then(|value| value.to_str())),
            cache_control: self.cache_control.clone(),
            length,
            etag: HeaderValue::from_str(&etag).ok()?,
            last_modified: HeaderValue::from_str(&last_modified).ok()?,
        })
    }
}

pub(crate) fn static_response(file: StaticFile, head: bool, request: &crate::Request) -> Response {
    let length = file.length;
    let response_headers = vec![
        (
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(file.content_type),
        ),
        (http::header::CACHE_CONTROL, file.cache_control.clone()),
        (http::header::ETAG, file.etag.clone()),
        (http::header::LAST_MODIFIED, file.last_modified.clone()),
        (
            http::header::ACCEPT_RANGES,
            HeaderValue::from_static("bytes"),
        ),
    ];

    if conditional_not_modified(request, &file) {
        let mut response = Response::status(StatusCode::NOT_MODIFIED);
        for (name, value) in response_headers {
            response = response.with_header(name, value);
        }
        return if head { strip_body(response) } else { response };
    }

    let range = request
        .header("range")
        .and_then(|value| value.to_str().ok());
    if let Some(range) = range {
        let if_range = request
            .header("if-range")
            .and_then(|value| value.to_str().ok());
        let range_allowed = if_range.is_none_or(|value| {
            value == file.etag.to_str().unwrap_or("")
                || value == file.last_modified.to_str().unwrap_or("")
        });
        if range_allowed {
            match parse_single_range(range, length) {
                Ok(Some((start, end))) => {
                    let body = if head {
                        Bytes::new()
                    } else {
                        file.body.slice(start as usize..(end + 1) as usize)
                    };
                    let mut response = Response::new(StatusCode::PARTIAL_CONTENT, body);
                    for (name, value) in response_headers {
                        response = response.with_header(name, value);
                    }
                    response = response.with_header(
                        http::header::CONTENT_RANGE,
                        HeaderValue::from_str(&format!("bytes {start}-{end}/{length}"))
                            .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
                    );
                    response = response.with_header(
                        http::header::CONTENT_LENGTH,
                        HeaderValue::from_str(&(end - start + 1).to_string())
                            .unwrap_or_else(|_| HeaderValue::from_static("0")),
                    );
                    return response;
                }
                Ok(None) => {}
                Err(_) => {
                    let mut response = Response::status(StatusCode::RANGE_NOT_SATISFIABLE);
                    response = response.with_header(
                        http::header::CONTENT_RANGE,
                        HeaderValue::from_str(&format!("bytes */{length}"))
                            .unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
                    );
                    for (name, value) in response_headers {
                        response = response.with_header(name, value);
                    }
                    return response;
                }
            }
        }
    }

    let body = if head { Bytes::new() } else { file.body };
    let mut response = Response::new(StatusCode::OK, body);
    for (name, value) in response_headers {
        response = response.with_header(name, value);
    }
    response = response.with_header(
        http::header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    response
}

fn strip_body(mut response: Response) -> Response {
    response.body = Bytes::new();
    response
}

fn conditional_not_modified(request: &crate::Request, file: &StaticFile) -> bool {
    if let Some(value) = request
        .header("if-none-match")
        .and_then(|value| value.to_str().ok())
    {
        let matches = value.split(',').any(|candidate| {
            candidate.trim() == "*"
                || candidate.trim() == file.etag.to_str().unwrap_or("")
                || candidate.trim().trim_start_matches("W/").trim_matches('"')
                    == file
                        .etag
                        .to_str()
                        .unwrap_or("")
                        .trim_start_matches("W/")
                        .trim_matches('"')
        });
        if matches {
            return true;
        }
    }

    if let Some(value) = request
        .header("if-modified-since")
        .and_then(|value| value.to_str().ok())
        && let Ok(since) = httpdate::parse_http_date(value)
        && let Ok(modified) = httpdate::parse_http_date(file.last_modified.to_str().unwrap_or(""))
        && modified <= since
    {
        return true;
    }

    false
}

fn parse_single_range(value: &str, length: u64) -> Result<Option<(u64, u64)>, ()> {
    let value = value.trim();
    let spec = value.strip_prefix("bytes=").ok_or(())?.trim();
    if spec.is_empty() || spec.contains(',') {
        return Err(());
    }
    if length == 0 {
        return Err(());
    }
    let (start, end) = spec.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        let start = length.saturating_sub(suffix);
        return Ok(Some((start, length - 1)));
    }
    let start = start.parse::<u64>().map_err(|_| ())?;
    if start >= length {
        return Err(());
    }
    let end = if end.is_empty() {
        length - 1
    } else {
        end.parse::<u64>().map_err(|_| ())?.min(length - 1)
    };
    if start > end {
        return Err(());
    }
    Ok(Some((start, end)))
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
