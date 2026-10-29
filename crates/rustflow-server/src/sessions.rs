//! Server-side sessions, cookie handling, identity, authorization, and password hashing.
//!
//! Session values and identities remain on the server. Cookies contain only opaque random IDs.
//! The in-memory adapter is volatile and is not a distributed or durable production store.

use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    fmt,
    num::NonZeroU32,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use http::{HeaderValue, StatusCode, header::SET_COOKIE};
use ring::{
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{HttpError, Middleware, Next, Request};

const SESSION_ID_BYTES: usize = 32;
const SESSION_ID_HEX_LEN: usize = SESSION_ID_BYTES * 2;
const IDENTITY_KEY: &str = "__rustflow.identity";
const PASSWORD_HASH_PREFIX: &str = "rf-pbkdf2-sha256";
const PASSWORD_ITERATIONS: u32 = 600_000;
const PASSWORD_SALT_BYTES: usize = 16;
const PASSWORD_OUTPUT_BYTES: usize = 32;

/// Opaque, cryptographically random session identifier. Debug output is redacted.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a cryptographically random identifier for a session store adapter.\n    pub fn generate() -> Result<Self, SessionError> {
        let mut bytes = [0_u8; SESSION_ID_BYTES];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| SessionError::RandomnessUnavailable)?;
        Ok(Self(encode_hex(&bytes)))
    }

    fn parse_cookie(value: &str) -> Option<Self> {
        if value.len() != SESSION_ID_HEX_LEN
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        Some(Self(value.to_owned()))
    }

    /// Expose the identifier to a storage adapter. Never use it as an authorization decision.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionId([REDACTED])")
    }
}

/// Required synchronous storage contract. Per-session operations must be atomic.
/// Rotation must install a fresh ID and invalidate the old ID as one transaction. Writes must
/// never recreate expired/revoked records, and invalidation must be linearizable.
pub trait SessionStore: Send + Sync + 'static {
    /// Create an empty session with an absolute Unix expiration time.
    fn create(
        &self,
        id: SessionId,
        expires_at_unix: u64,
        now_unix: u64,
    ) -> Result<(), SessionError>;
    /// Return expiry for an active session, removing an expired entry when necessary.
    fn expires_at(&self, id: &SessionId, now_unix: u64) -> Result<Option<u64>, SessionError>;
    /// Read one value only from an active session.
    fn get(&self, id: &SessionId, key: &str, now_unix: u64) -> Result<Option<Value>, SessionError>;
    /// Atomically update a value without extending the session expiry.
    fn set(
        &self,
        id: &SessionId,
        key: String,
        value: Value,
        now_unix: u64,
    ) -> Result<(), SessionError>;
    /// Atomically rotate an ID. When preserve_data is false, the new session is empty.
    /// A provided old ID must be active; missing/expired IDs fail closed.
    fn rotate(
        &self,
        old_id: Option<&SessionId>,
        new_id: SessionId,
        expires_at_unix: u64,
        now_unix: u64,
        preserve_data: bool,
    ) -> Result<(), SessionError>;
    /// Revoke an ID. Revoking a missing ID is idempotent.
    fn invalidate(&self, id: &SessionId) -> Result<(), SessionError>;
}

struct MemoryEntry {
    expires_at_unix: u64,
    values: HashMap<String, Value>,
}

/// Volatile, process-local storage for development and tests. It does not survive restarts
/// and does not coordinate multiple server processes. One lock makes each operation atomic.
#[derive(Default)]
pub struct InMemorySessionStore {
    entries: RwLock<HashMap<SessionId, MemoryEntry>>,
}

impl InMemorySessionStore {
    /// Create an empty process-local store.
    pub fn new() -> Self {
        Self::default()
    }

    fn active_entry<'a>(
        entries: &'a mut HashMap<SessionId, MemoryEntry>,
        id: &SessionId,
        now: u64,
    ) -> Result<&'a mut MemoryEntry, SessionError> {
        if entries
            .get(id)
            .is_some_and(|entry| entry.expires_at_unix <= now)
        {
            entries.remove(id);
        }
        entries.get_mut(id).ok_or(SessionError::InactiveSession)
    }
}

impl SessionStore for InMemorySessionStore {
    fn create(
        &self,
        id: SessionId,
        expires_at_unix: u64,
        now_unix: u64,
    ) -> Result<(), SessionError> {
        if expires_at_unix <= now_unix {
            return Err(SessionError::InvalidLifetime);
        }
        let mut entries = self
            .entries
            .write()
            .map_err(|_| SessionError::StoreUnavailable)?;
        entries.retain(|_, entry| entry.expires_at_unix > now_unix);
        if entries.contains_key(&id) {
            return Err(SessionError::IdentifierCollision);
        }
        entries.insert(
            id,
            MemoryEntry {
                expires_at_unix,
                values: HashMap::new(),
            },
        );
        Ok(())
    }

    fn expires_at(&self, id: &SessionId, now_unix: u64) -> Result<Option<u64>, SessionError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| SessionError::StoreUnavailable)?;
        if entries
            .get(id)
            .is_some_and(|entry| entry.expires_at_unix <= now_unix)
        {
            entries.remove(id);
            return Ok(None);
        }
        Ok(entries.get(id).map(|entry| entry.expires_at_unix))
    }

    fn get(&self, id: &SessionId, key: &str, now_unix: u64) -> Result<Option<Value>, SessionError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| SessionError::StoreUnavailable)?;
        match Self::active_entry(&mut entries, id, now_unix) {
            Ok(entry) => Ok(entry.values.get(key).cloned()),
            Err(SessionError::InactiveSession) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn set(
        &self,
        id: &SessionId,
        key: String,
        value: Value,
        now_unix: u64,
    ) -> Result<(), SessionError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| SessionError::StoreUnavailable)?;
        Self::active_entry(&mut entries, id, now_unix)?
            .values
            .insert(key, value);
        Ok(())
    }

    fn rotate(
        &self,
        old_id: Option<&SessionId>,
        new_id: SessionId,
        expires_at_unix: u64,
        now_unix: u64,
        preserve_data: bool,
    ) -> Result<(), SessionError> {
        if expires_at_unix <= now_unix {
            return Err(SessionError::InvalidLifetime);
        }
        let mut entries = self
            .entries
            .write()
            .map_err(|_| SessionError::StoreUnavailable)?;
        entries.retain(|_, entry| entry.expires_at_unix > now_unix);
        if entries.contains_key(&new_id) {
            return Err(SessionError::IdentifierCollision);
        }
        let values = match old_id {
            Some(old_id) => {
                let old = entries.get(old_id).ok_or(SessionError::InactiveSession)?;
                if preserve_data {
                    old.values.clone()
                } else {
                    HashMap::new()
                }
            }
            None => HashMap::new(),
        };
        if let Some(old_id) = old_id {
            entries.remove(old_id);
        }
        entries.insert(
            new_id,
            MemoryEntry {
                expires_at_unix,
                values,
            },
        );
        Ok(())
    }

    fn invalidate(&self, id: &SessionId) -> Result<(), SessionError> {
        self.entries
            .write()
            .map_err(|_| SessionError::StoreUnavailable)?
            .remove(id);
        Ok(())
    }
}

/// Cookie SameSite setting for the session cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    /// Top-level safe navigations and same-site requests.
    Lax,
    /// Same-site requests only.
    Strict,
}

impl SameSite {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lax => "Lax",
            Self::Strict => "Strict",
        }
    }
}

/// Safe cookie settings. Cookies remain host-only; Domain cannot be configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCookieConfig {
    name: String,
    secure: bool,
    same_site: SameSite,
}

impl SessionCookieConfig {
    /// Production default: __Host- prefix, Secure, HttpOnly, SameSite=Lax, Path=/, no Domain.
    pub fn secure_default() -> Self {
        Self {
            name: "__Host-rustflow_session".to_owned(),
            secure: true,
            same_site: SameSite::Lax,
        }
    }

    /// Explicit plain-HTTP local-development policy. Never use it in production.
    pub fn development() -> Self {
        Self {
            name: "rustflow_session".to_owned(),
            secure: false,
            same_site: SameSite::Lax,
        }
    }

    /// Select the stricter SameSite policy.
    pub fn with_same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Configure a cookie name after validating its grammar and prefix requirements.
    pub fn with_name(mut self, name: impl Into<String>) -> Result<Self, SessionError> {
        let name = name.into();
        if !valid_cookie_name(&name) || (name.starts_with("__Host-") && !self.secure) {
            return Err(SessionError::InvalidCookieConfiguration);
        }
        self.name = name;
        Ok(self)
    }

    fn validate(&self) -> Result<(), SessionError> {
        if !valid_cookie_name(&self.name) || (self.name.starts_with("__Host-") && !self.secure) {
            return Err(SessionError::InvalidCookieConfiguration);
        }
        Ok(())
    }
}

fn valid_cookie_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'|'
                        | b'~'
                )
        })
}

/// Session middleware configuration and absolute expiry policy.
#[derive(Clone)]
pub struct SessionManager {
    store: Arc<dyn SessionStore>,
    lifetime: Duration,
    cookie: SessionCookieConfig,
}

impl SessionManager {
    /// Create a manager around a replaceable storage adapter.
    pub fn new(store: Arc<dyn SessionStore>, lifetime: Duration) -> Result<Self, SessionError> {
        if lifetime.as_secs() == 0 {
            return Err(SessionError::InvalidLifetime);
        }
        Ok(Self {
            store,
            lifetime,
            cookie: SessionCookieConfig::secure_default(),
        })
    }

    /// Create a volatile in-memory manager for tests and single-process development.
    pub fn in_memory(lifetime: Duration) -> Result<Self, SessionError> {
        Self::new(Arc::new(InMemorySessionStore::new()), lifetime)
    }

    /// Set the cookie policy. Plain HTTP requires SessionCookieConfig::development().
    pub fn with_cookie_config(mut self, cookie: SessionCookieConfig) -> Result<Self, SessionError> {
        cookie.validate()?;
        self.cookie = cookie;
        Ok(self)
    }

    /// Install session loading and cookie emission middleware before route guards.
    /// Storage failures return a generic 500 instead of allowing access.
    pub fn middleware(&self) -> Middleware {
        let manager = self.clone();
        Middleware::new(move |mut request, next| {
            let manager = manager.clone();
            async move {
                let now =
                    current_epoch_seconds().map_err(|_| HttpError::internal_server_error())?;
                let allow_cookie = !manager.cookie.secure || request.is_https();
                let parsed = request
                    .header("cookie")
                    .and_then(|v| v.to_str().ok())
                    .map_or(Ok(None), |value| find_cookie(value, &manager.cookie.name));
                let cookie_present = matches!(parsed, Ok(Some(_)) | Err(_));
                let mut id = None;
                let mut expires = None;
                let mut action = if allow_cookie && cookie_present {
                    CookieAction::Clear
                } else {
                    CookieAction::Keep
                };

                if allow_cookie {
                    if let Ok(Some(value)) = parsed {
                        if let Some(candidate) = SessionId::parse_cookie(value) {
                            if let Some(expiry) = manager
                                .store
                                .expires_at(&candidate, now)
                                .map_err(|_| HttpError::internal_server_error())?
                            {
                                id = Some(candidate);
                                expires = Some(expiry);
                                action = CookieAction::Keep;
                            }
                        }
                    }
                }

                let context = SessionContext {
                    inner: Arc::new(SessionInner {
                        store: manager.store.clone(),
                        lifetime_secs: manager.lifetime.as_secs(),
                        cookie: manager.cookie.clone(),
                        secure_request: request.is_https(),
                        state: Mutex::new(SessionState {
                            id,
                            expires_at_unix: expires,
                            cookie_action: action,
                        }),
                    }),
                };
                request.session = Some(context.clone());
                let mut response = next.run(request).await?;

                let action = {
                    let mut state = context
                        .inner
                        .state
                        .lock()
                        .map_err(|_| HttpError::internal_server_error())?;
                    let action = state.cookie_action.clone();
                    state.cookie_action = CookieAction::Keep;
                    action
                };
                match action {
                    CookieAction::Keep => {}
                    CookieAction::Clear => {
                        let header = clear_cookie_header(&manager.cookie)
                            .map_err(|_| HttpError::internal_server_error())?;
                        response = response.with_appended_header(SET_COOKIE, header);
                    }
                    CookieAction::Set => {
                        let (id, expiry) = {
                            let state = context
                                .inner
                                .state
                                .lock()
                                .map_err(|_| HttpError::internal_server_error())?;
                            match (state.id.as_ref(), state.expires_at_unix) {
                                (Some(id), Some(expiry)) => (id.clone(), expiry),
                                _ => return Err(HttpError::internal_server_error()),
                            }
                        };
                        if manager.cookie.secure && !context.inner.secure_request {
                            return Err(HttpError::internal_server_error());
                        }
                        let now = current_epoch_seconds()
                            .map_err(|_| HttpError::internal_server_error())?;
                        let header = session_cookie_header(&manager.cookie, &id, expiry, now)
                            .map_err(|_| HttpError::internal_server_error())?;
                        response = response.with_header(SET_COOKIE, header);
                    }
                }
                Ok(response)
            }
        })
    }
}

#[derive(Clone)]
enum CookieAction {
    Keep,
    Set,
    Clear,
}
struct SessionState {
    id: Option<SessionId>,
    expires_at_unix: Option<u64>,
    cookie_action: CookieAction,
}
struct SessionInner {
    store: Arc<dyn SessionStore>,
    lifetime_secs: u64,
    cookie: SessionCookieConfig,
    secure_request: bool,
    state: Mutex<SessionState>,
}

/// Cloneable handle to the session associated with one request.
#[derive(Clone)]
pub struct SessionContext {
    inner: Arc<SessionInner>,
}

impl fmt::Debug for SessionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionContext([REDACTED])")
    }
}

impl SessionContext {
    /// Create a new anonymous session if the request has no active session.
    pub fn create(&self) -> Result<(), SessionError> {
        if self.inner.cookie.secure && !self.inner.secure_request {
            return Err(SessionError::SecureTransportRequired);
        }
        let now = current_epoch_seconds()?;
        if let Some(existing) = self.current_id()? {
            if self.inner.store.expires_at(&existing, now)?.is_some() {
                return Ok(());
            }
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| SessionError::StoreUnavailable)?;
            state.id = None;
            state.expires_at_unix = None;
            state.cookie_action = CookieAction::Clear;
            return Err(SessionError::InactiveSession);
        }
        self.rotate_inner(None, true, now)
    }

    /// Read and deserialize one typed value from the current session.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, SessionError> {
        validate_session_key(key)?;
        let Some(id) = self.current_id()? else {
            return Ok(None);
        };
        self.inner
            .store
            .get(&id, key, current_epoch_seconds()?)?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| SessionError::Serialization)
    }

    /// Serialize and store one typed value in an active session.
    pub fn insert<T: Serialize>(&self, key: &str, value: &T) -> Result<(), SessionError> {
        validate_session_key(key)?;
        let id = self.current_id()?.ok_or(SessionError::InactiveSession)?;
        let value = serde_json::to_value(value).map_err(|_| SessionError::Serialization)?;
        self.inner
            .store
            .set(&id, key.to_owned(), value, current_epoch_seconds()?)
    }

    /// Rotate the identifier, preserve application values, and invalidate the previous ID.
    pub fn rotate(&self) -> Result<(), SessionError> {
        let now = current_epoch_seconds()?;
        let old = self.current_id()?;
        self.rotate_inner(old.as_ref(), true, now)
    }

    /// Rotate to a fresh, empty session and save a server-verified identity.
    /// Call only after verifying credentials. Client-supplied role or identity fields are ignored.
    pub fn authenticate(&self, identity: AuthIdentity) -> Result<(), SessionError> {
        identity.validate()?;
        let now = current_epoch_seconds()?;
        let old = self.current_id()?;
        self.rotate_inner(old.as_ref(), false, now)?;
        let id = self.current_id()?.ok_or(SessionError::InactiveSession)?;
        let value = serde_json::to_value(StoredIdentity::from(identity))
            .map_err(|_| SessionError::Serialization)?;
        self.inner
            .store
            .set(&id, IDENTITY_KEY.to_owned(), value, now)
    }

    /// Extract identity only from server-side storage.
    pub fn identity(&self) -> Result<Option<AuthIdentity>, SessionError> {
        let Some(id) = self.current_id()? else {
            return Ok(None);
        };
        let value = self
            .inner
            .store
            .get(&id, IDENTITY_KEY, current_epoch_seconds()?)?;
        let Some(value) = value else {
            return Ok(None);
        };
        let stored: StoredIdentity =
            serde_json::from_value(value).map_err(|_| SessionError::InvalidIdentity)?;
        let identity = stored.into_identity();
        identity.validate()?;
        Ok(Some(identity))
    }

    /// Revoke the current session and expire its cookie on the response.
    pub fn invalidate(&self) -> Result<(), SessionError> {
        let id = {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| SessionError::StoreUnavailable)?;
            let id = state.id.take();
            state.expires_at_unix = None;
            state.cookie_action = CookieAction::Clear;
            id
        };
        if let Some(id) = id {
            self.inner.store.invalidate(&id)?;
        }
        Ok(())
    }

    fn current_id(&self) -> Result<Option<SessionId>, SessionError> {
        Ok(self
            .inner
            .state
            .lock()
            .map_err(|_| SessionError::StoreUnavailable)?
            .id
            .clone())
    }

    fn rotate_inner(
        &self,
        old_id: Option<&SessionId>,
        preserve_data: bool,
        now: u64,
    ) -> Result<(), SessionError> {
        if self.inner.cookie.secure && !self.inner.secure_request {
            return Err(SessionError::SecureTransportRequired);
        }
        let expiry = now
            .checked_add(self.inner.lifetime_secs)
            .ok_or(SessionError::InvalidLifetime)?;
        for _ in 0..3 {
            let new_id = SessionId::generate()?;
            match self
                .inner
                .store
                .rotate(old_id, new_id.clone(), expiry, now, preserve_data)
            {
                Ok(()) => {
                    let mut state = self
                        .inner
                        .state
                        .lock()
                        .map_err(|_| SessionError::StoreUnavailable)?;
                    state.id = Some(new_id);
                    state.expires_at_unix = Some(expiry);
                    state.cookie_action = CookieAction::Set;
                    return Ok(());
                }
                Err(SessionError::IdentifierCollision) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(SessionError::IdentifierCollision)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredIdentity {
    subject: String,
    roles: BTreeSet<String>,
    permissions: BTreeSet<String>,
}

impl From<AuthIdentity> for StoredIdentity {
    fn from(identity: AuthIdentity) -> Self {
        Self {
            subject: identity.subject,
            roles: identity.roles,
            permissions: identity.permissions,
        }
    }
}
impl StoredIdentity {
    fn into_identity(self) -> AuthIdentity {
        AuthIdentity {
            subject: self.subject,
            roles: self.roles,
            permissions: self.permissions,
        }
    }
}

/// Authenticated identity attached to a session by trusted application code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthIdentity {
    subject: String,
    roles: BTreeSet<String>,
    permissions: BTreeSet<String>,
}

impl AuthIdentity {
    /// Create an identity with a validated stable subject ID.
    pub fn new(subject: impl Into<String>) -> Result<Self, SessionError> {
        let identity = Self {
            subject: subject.into(),
            roles: BTreeSet::new(),
            permissions: BTreeSet::new(),
        };
        identity.validate()?;
        Ok(identity)
    }
    /// Assign a trusted role.
    pub fn with_role(mut self, role: impl Into<String>) -> Result<Self, SessionError> {
        let role = role.into();
        if !valid_policy_label(&role) {
            return Err(SessionError::InvalidIdentity);
        }
        self.roles.insert(role);
        Ok(self)
    }
    /// Assign a trusted permission.
    pub fn with_permission(mut self, permission: impl Into<String>) -> Result<Self, SessionError> {
        let permission = permission.into();
        if !valid_policy_label(&permission) {
            return Err(SessionError::InvalidIdentity);
        }
        self.permissions.insert(permission);
        Ok(self)
    }
    /// Return the authenticated subject.
    pub fn subject(&self) -> &str {
        &self.subject
    }
    /// Check a role assigned by server-side application logic.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }
    /// Check a permission assigned by server-side application logic.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
    fn validate(&self) -> Result<(), SessionError> {
        if self.subject.trim().is_empty()
            || self.subject.len() > 256
            || self.subject.chars().any(char::is_control)
            || self.roles.iter().any(|value| !valid_policy_label(value))
            || self
                .permissions
                .iter()
                .any(|value| !valid_policy_label(value))
        {
            return Err(SessionError::InvalidIdentity);
        }
        Ok(())
    }
}

impl Request {
    /// Return this request's session handle when SessionManager middleware is installed.
    pub fn session(&self) -> Option<SessionContext> {
        self.session.clone()
    }

    /// Extract identity from server-side session storage, if one is active.
    pub fn identity(&self) -> Result<Option<AuthIdentity>, SessionError> {
        match self.session() {
            Some(session) => session.identity(),
            None => Ok(None),
        }
    }
}

/// Authorization policy interface. Errors fail closed and produce generic HTTP 500 responses.
pub trait AccessPolicy: Send + Sync + 'static {
    /// Decide whether identity has a named permission on this request.
    fn allows(
        &self,
        identity: &AuthIdentity,
        permission: &str,
        request: &Request,
    ) -> Result<bool, AccessPolicyError>;
}

/// Authorization policy configuration or evaluation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessPolicyError {
    InvalidRule,
    Unavailable,
}

impl fmt::Display for AccessPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRule => "authorization rule is invalid",
            Self::Unavailable => "authorization policy is unavailable",
        })
    }
}
impl Error for AccessPolicyError {}

/// Explicit role-to-permission policy. Permissions without a rule are denied.
#[derive(Debug, Clone, Default)]
pub struct RolePolicy {
    roles_by_permission: HashMap<String, BTreeSet<String>>,
}

impl RolePolicy {
    /// Create a policy with no grants.
    pub fn new() -> Self {
        Self::default()
    }
    /// Allow a role to perform the given permission.
    pub fn allow_role(
        mut self,
        permission: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<Self, AccessPolicyError> {
        let permission = permission.into();
        let role = role.into();
        if !valid_policy_label(&permission) || !valid_policy_label(&role) {
            return Err(AccessPolicyError::InvalidRule);
        }
        self.roles_by_permission
            .entry(permission)
            .or_default()
            .insert(role);
        Ok(self)
    }
}
impl AccessPolicy for RolePolicy {
    fn allows(
        &self,
        identity: &AuthIdentity,
        permission: &str,
        _request: &Request,
    ) -> Result<bool, AccessPolicyError> {
        let Some(roles) = self.roles_by_permission.get(permission) else {
            return Ok(false);
        };
        Ok(identity.has_permission(permission) || roles.iter().any(|role| identity.has_role(role)))
    }
}

/// Guard that returns 401 before reaching the handler when no identity is active.
pub fn require_authentication() -> Middleware {
    Middleware::new(|request, next: Next| async move {
        match request.identity() {
            Ok(Some(_)) => next.run(request).await,
            Ok(None) => Err(HttpError::new(
                StatusCode::UNAUTHORIZED,
                "Authentication required",
            )),
            Err(_) => Err(HttpError::internal_server_error()),
        }
    })
}

/// Guard with default-deny semantics. No policy, no rule, or policy error never grants access.
pub fn require_permission(
    permission: impl Into<String>,
    policy: Option<Arc<dyn AccessPolicy>>,
) -> Middleware {
    let permission = permission.into();
    Middleware::new(move |request, next: Next| {
        let permission = permission.clone();
        let policy = policy.clone();
        async move {
            let identity = match request.identity() {
                Ok(Some(identity)) => identity,
                Ok(None) => {
                    return Err(HttpError::new(
                        StatusCode::UNAUTHORIZED,
                        "Authentication required",
                    ));
                }
                Err(_) => return Err(HttpError::internal_server_error()),
            };
            let Some(policy) = policy else {
                return Err(HttpError::forbidden("Access denied"));
            };
            match policy.allows(&identity, &permission, &request) {
                Ok(true) => next.run(request).await,
                Ok(false) => Err(HttpError::forbidden("Access denied")),
                Err(_) => Err(HttpError::internal_server_error()),
            }
        }
    })
}

/// Password hash using ring PBKDF2-HMAC-SHA-256 (600,000 iterations, random 16-byte salt,
/// 32-byte derived key). The encoded value must be stored with trusted user credentials.
#[derive(Clone, PartialEq, Eq)]
pub struct PasswordHash(String);

impl PasswordHash {
    /// Hash a non-empty password.
    pub fn hash(password: &str) -> Result<Self, SessionError> {
        if password.is_empty() {
            return Err(SessionError::InvalidPassword);
        }
        let iterations =
            NonZeroU32::new(PASSWORD_ITERATIONS).ok_or(SessionError::InvalidPasswordHash)?;
        let mut salt = [0_u8; PASSWORD_SALT_BYTES];
        SystemRandom::new()
            .fill(&mut salt)
            .map_err(|_| SessionError::RandomnessUnavailable)?;
        let mut derived = [0_u8; PASSWORD_OUTPUT_BYTES];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            iterations,
            &salt,
            password.as_bytes(),
            &mut derived,
        );
        let mut encoded = String::new();
        encoded.push_str(PASSWORD_HASH_PREFIX);
        encoded.push('$');
        encoded.push_str(&PASSWORD_ITERATIONS.to_string());
        encoded.push('$');
        encoded.push_str(&encode_hex(&salt));
        encoded.push('$');
        encoded.push_str(&encode_hex(&derived));
        Ok(Self(encoded))
    }

    /// Parse the fixed supported encoding and reject unrecognized algorithms/work factors.
    pub fn parse(encoded: &str) -> Result<Self, SessionError> {
        let mut parts = encoded.split('$');
        if parts.next() != Some(PASSWORD_HASH_PREFIX)
            || parts.next().and_then(|value| value.parse::<u32>().ok()) != Some(PASSWORD_ITERATIONS)
        {
            return Err(SessionError::InvalidPasswordHash);
        }
        let salt = parts.next().and_then(decode_hex::<PASSWORD_SALT_BYTES>);
        let derived = parts.next().and_then(decode_hex::<PASSWORD_OUTPUT_BYTES>);
        if parts.next().is_some() || salt.is_none() || derived.is_none() {
            return Err(SessionError::InvalidPasswordHash);
        }
        Ok(Self(encoded.to_owned()))
    }

    /// Verify a candidate password with ring's constant-time PBKDF2 verifier.
    pub fn verify(&self, password: &str) -> Result<bool, SessionError> {
        if password.is_empty() {
            return Ok(false);
        }
        let parsed = Self::parse(&self.0)?;
        let mut parts = parsed.0.split('$');
        let _label = parts.next();
        let iterations = NonZeroU32::new(
            parts
                .next()
                .ok_or(SessionError::InvalidPasswordHash)?
                .parse::<u32>()
                .map_err(|_| SessionError::InvalidPasswordHash)?,
        )
        .ok_or(SessionError::InvalidPasswordHash)?;
        let salt = decode_hex::<PASSWORD_SALT_BYTES>(
            parts.next().ok_or(SessionError::InvalidPasswordHash)?,
        )
        .ok_or(SessionError::InvalidPasswordHash)?;
        let expected = decode_hex::<PASSWORD_OUTPUT_BYTES>(
            parts.next().ok_or(SessionError::InvalidPasswordHash)?,
        )
        .ok_or(SessionError::InvalidPasswordHash)?;
        Ok(pbkdf2::verify(
            pbkdf2::PBKDF2_HMAC_SHA256,
            iterations,
            &salt,
            password.as_bytes(),
            &expected,
        )
        .is_ok())
    }

    /// Return the encoded value for trusted persistence. Never log it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for PasswordHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordHash([REDACTED])")
    }
}

/// Session operations fail with generic, non-secret messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    InvalidLifetime,
    RandomnessUnavailable,
    ClockUnavailable,
    StoreUnavailable,
    InactiveSession,
    IdentifierCollision,
    Serialization,
    InvalidKey,
    InvalidIdentity,
    InvalidCookieConfiguration,
    SecureTransportRequired,
    InvalidPasswordHash,
    InvalidPassword,
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLifetime => "session lifetime must be a positive number of seconds",
            Self::RandomnessUnavailable => "secure randomness is unavailable",
            Self::ClockUnavailable => "system clock is unavailable",
            Self::StoreUnavailable => "session storage is unavailable",
            Self::InactiveSession => "session is missing, expired, or invalidated",
            Self::IdentifierCollision => "could not allocate a unique session identifier",
            Self::Serialization => "session value could not be serialized",
            Self::InvalidKey => "session key is invalid or reserved",
            Self::InvalidIdentity => "identity is invalid",
            Self::InvalidCookieConfiguration => "session cookie configuration is invalid",
            Self::SecureTransportRequired => "HTTPS is required for this session cookie policy",
            Self::InvalidPasswordHash => "password hash encoding is invalid or unsupported",
            Self::InvalidPassword => "password must not be empty",
        })
    }
}
impl Error for SessionError {}

fn current_epoch_seconds() -> Result<u64, SessionError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SessionError::ClockUnavailable)?
        .as_secs())
}
fn validate_session_key(key: &str) -> Result<(), SessionError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        || key.starts_with("__rustflow.")
        || key == IDENTITY_KEY
    {
        return Err(SessionError::InvalidKey);
    }
    Ok(())
}
fn valid_policy_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 128
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}
#[derive(Debug, Clone, Copy)]\nstruct CookieParseError;\n\nfn find_cookie<'a>(header: &'a str, name: &str) -> Result<Option<&'a str>, CookieParseError> {
    let mut found = None;
    for part in header.split(';') {
        let Some((cookie_name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if cookie_name.trim() == name {
            let value = value.trim();
            if found.is_some()
                || value.is_empty()
                || value
                    .bytes()
                    .any(|byte| byte.is_ascii_control() || byte == b';' || byte == b',')
            {
                return Err(CookieParseError);
            }
            found = Some(value);
        }
    }
    Ok(found)
}
fn session_cookie_header(
    config: &SessionCookieConfig,
    id: &SessionId,
    expires: u64,
    now: u64,
) -> Result<HeaderValue, SessionError> {
    let secure = if config.secure { "; Secure" } else { "" };
    let header = format!(
        "{}={}; Path=/; HttpOnly; SameSite={}; Max-Age={}{}",
        config.name,
        id.as_str(),
        config.same_site.as_str(),
        expires.saturating_sub(now),
        secure
    );
    HeaderValue::from_str(&header).map_err(|_| SessionError::InvalidCookieConfiguration)
}
fn clear_cookie_header(config: &SessionCookieConfig) -> Result<HeaderValue, SessionError> {
    let secure = if config.secure { "; Secure" } else { "" };
    let header = format!(
        "{}=; Path=/; HttpOnly; SameSite={}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT{}",
        config.name,
        config.same_site.as_str(),
        secure
    );
    HeaderValue::from_str(&header).map_err(|_| SessionError::InvalidCookieConfiguration)
}
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[(byte >> 4) as usize]));
        output.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    output
}
fn decode_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut output = [0_u8; N];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}
fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Response;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn request_with_session(session: Option<SessionContext>) -> Request {
        Request {
            method: http::Method::GET,
            path: "/private".to_owned(),
            query_string: None,
            path_params: HashMap::new(),
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::new(),
            request_id: "test".to_owned(),
            peer_addr: None,
            client_ip: None,
            https: true,
            session,
        }
    }
    fn next_with_flag(flag: Arc<AtomicBool>) -> Next {
        let terminal: crate::SharedHandler = Arc::new(move |_request: Request| {
            let flag = flag.clone();
            Box::pin(async move {
                flag.store(true, Ordering::SeqCst);
                Ok(Response::text("handler"))
            })
        });
        Next {
            index: 0,
            middleware: Arc::new(Vec::new()),
            terminal,
        }
    }

    struct FailingStore;

    impl SessionStore for FailingStore {
        fn create(
            &self,
            _id: SessionId,
            _expires_at_unix: u64,
            _now_unix: u64,
        ) -> Result<(), SessionError> {
            Err(SessionError::StoreUnavailable)
        }
        fn expires_at(&self, _id: &SessionId, _now_unix: u64) -> Result<Option<u64>, SessionError> {
            Err(SessionError::StoreUnavailable)
        }
        fn get(
            &self,
            _id: &SessionId,
            _key: &str,
            _now_unix: u64,
        ) -> Result<Option<Value>, SessionError> {
            Err(SessionError::StoreUnavailable)
        }
        fn set(
            &self,
            _id: &SessionId,
            _key: String,
            _value: Value,
            _now_unix: u64,
        ) -> Result<(), SessionError> {
            Err(SessionError::StoreUnavailable)
        }
        fn rotate(
            &self,
            _old_id: Option<&SessionId>,
            _new_id: SessionId,
            _expires_at_unix: u64,
            _now_unix: u64,
            _preserve_data: bool,
        ) -> Result<(), SessionError> {
            Err(SessionError::StoreUnavailable)
        }
        fn invalidate(&self, _id: &SessionId) -> Result<(), SessionError> {
            Err(SessionError::StoreUnavailable)
        }
    }

    #[tokio::test]
    async fn storage_failure_returns_500_without_reaching_the_handler() {
        let manager = SessionManager::new(Arc::new(FailingStore), Duration::from_secs(60))
            .expect("manager")
            .with_cookie_config(SessionCookieConfig::development())
            .expect("development cookie");
        let mut request = request_with_session(None);
        let cookie = format!("rustflow_session={}", "a".repeat(SESSION_ID_HEX_LEN));
        request.headers.insert(
            http::header::COOKIE,
            http::HeaderValue::from_str(&cookie).expect("cookie header"),
        );
        let called = Arc::new(AtomicBool::new(false));
        let result = manager
            .middleware()
            .call(request, next_with_flag(called.clone()))
            .await;
        assert_eq!(
            result
                .expect_err("storage failure must fail closed")
                .into_response()
                .status_code(),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn expiry_and_invalidation_are_enforced_by_storage() {
        let store = InMemorySessionStore::new();
        let id = SessionId::parse_cookie(&"a".repeat(SESSION_ID_HEX_LEN)).expect("id");
        store.create(id.clone(), 20, 10).expect("create");
        store
            .set(
                &id,
                "profile".to_owned(),
                serde_json::json!({"name":"A"}),
                11,
            )
            .expect("set");
        assert_eq!(
            store.get(&id, "profile", 19).expect("get"),
            Some(serde_json::json!({"name":"A"}))
        );
        assert_eq!(store.expires_at(&id, 20).expect("expiry"), None);
        store.create(id.clone(), 30, 21).expect("recreate");
        store.invalidate(&id).expect("invalidate");
        assert_eq!(store.expires_at(&id, 22).expect("revoked"), None);
    }

    #[test]
    fn rotation_invalidates_old_id_and_can_drop_prior_values() {
        let store = InMemorySessionStore::new();
        let old = SessionId::parse_cookie(&"b".repeat(SESSION_ID_HEX_LEN)).expect("old id");
        let new = SessionId::parse_cookie(&"c".repeat(SESSION_ID_HEX_LEN)).expect("new id");
        store.create(old.clone(), 100, 1).expect("create");
        store
            .set(&old, "private".to_owned(), serde_json::json!("old"), 2)
            .expect("set");
        store
            .rotate(Some(&old), new.clone(), 200, 3, false)
            .expect("rotate");
        assert_eq!(store.expires_at(&old, 4).expect("old invalid"), None);
        assert_eq!(store.get(&new, "private", 4).expect("new record"), None);
    }

    #[test]
    fn password_hash_checks_password_and_redacts_debug() {
        let hash = PasswordHash::hash("correct horse battery staple").expect("hash");
        assert!(hash.verify("correct horse battery staple").expect("verify"));
        assert!(!hash.verify("incorrect").expect("wrong password"));
        assert_eq!(format!("{hash:?}"), "PasswordHash([REDACTED])");
        assert!(PasswordHash::parse("sha256$1$invalid$invalid").is_err());
    }

    #[tokio::test]
    async fn anonymous_guard_does_not_run_protected_handler() {
        let called = Arc::new(AtomicBool::new(false));
        let result = require_authentication()
            .call(request_with_session(None), next_with_flag(called.clone()))
            .await;
        assert_eq!(
            result
                .expect_err("deny anonymous")
                .into_response()
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn absent_policy_denies_and_does_not_run_handler() {
        let store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let manager = SessionManager::new(store, Duration::from_secs(60))
            .expect("manager")
            .with_cookie_config(SessionCookieConfig::development())
            .expect("cookie");
        let context = SessionContext {
            inner: Arc::new(SessionInner {
                store: manager.store.clone(),
                lifetime_secs: manager.lifetime.as_secs(),
                cookie: manager.cookie.clone(),
                secure_request: true,
                state: Mutex::new(SessionState {
                    id: None,
                    expires_at_unix: None,
                    cookie_action: CookieAction::Keep,
                }),
            }),
        };
        context.create().expect("create");
        context
            .authenticate(AuthIdentity::new("user-1").expect("identity"))
            .expect("authenticate");
        let called = Arc::new(AtomicBool::new(false));
        let result = require_permission("private.read", None)
            .call(
                request_with_session(Some(context)),
                next_with_flag(called.clone()),
            )
            .await;
        assert_eq!(
            result
                .expect_err("deny missing policy")
                .into_response()
                .status_code(),
            StatusCode::FORBIDDEN
        );
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn cookie_defaults_are_secure_and_host_only() {
        let cookie = SessionCookieConfig::secure_default();
        assert!(cookie.secure);
        assert_eq!(cookie.name, "__Host-rustflow_session");
        assert_eq!(cookie.same_site, SameSite::Lax);
        assert!(SessionCookieConfig::development().validate().is_ok());
        assert!(
            SessionCookieConfig::development()
                .with_name("__Host-invalid")
                .is_err()
        );
    }
}
