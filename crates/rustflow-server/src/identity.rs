//! Application-owned user/provider ports for authentication.
//!
//! This module separates credential verification from authorization. The server
//! never derives identity from client-supplied role claims or couples provider
//! implementation to a concrete SQL, LDAP, or remote service.

use std::{error::Error, fmt, future::Future, pin::Pin};

use tokio::sync::Semaphore;

use crate::{AuthIdentity, PasswordHash, SessionContext, SessionError};

const MAX_CREDENTIAL_SUBJECT_BYTES: usize = 256;
const MAX_CREDENTIAL_SECRET_BYTES: usize = 1024;
const MAX_CONCURRENT_PASSWORD_VERIFICATIONS: usize = 4;

/// Boxed future returned by an asynchronous user-provider port.
pub type UserProviderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, UserProviderError>> + Send + 'a>>;

/// User-provided identifier and secret, with a bounded secret size.
pub struct Credentials {
    subject: String,
    secret: String,
}

impl Credentials {
    /// Validate and bound credentials before passing them to a provider.
    pub fn new(
        subject: impl Into<String>,
        secret: impl Into<String>,
    ) -> Result<Self, UserProviderError> {
        let subject = subject.into();
        let secret = secret.into();
        if subject.trim().is_empty()
            || subject.len() > MAX_CREDENTIAL_SUBJECT_BYTES
            || subject.chars().any(char::is_control)
            || secret.is_empty()
            || secret.len() > MAX_CREDENTIAL_SECRET_BYTES
        {
            return Err(UserProviderError::InvalidCredentials);
        }
        Ok(Self { subject, secret })
    }

    /// Return the account identifier, which is not a credential secret.
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Return the secret for verification. Do not log or persist it.
    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Credentials")
            .field("subject", &self.subject)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserProviderError {
    InvalidCredentials,
    ProviderUnavailable,
}

impl fmt::Display for UserProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCredentials => "invalid credentials",
            Self::ProviderUnavailable => "user provider unavailable",
        })
    }
}

impl Error for UserProviderError {}

/// Verifies credentials and returns identity data derived from trusted server policy.
pub trait UserProvider: Send + Sync + 'static {
    /// Return `None` for invalid credentials and an error for provider failure.
    fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> UserProviderFuture<'_, Option<AuthIdentity>>;
}

/// Safe provider default that never authenticates a user.
#[derive(Debug, Default)]
pub struct DenyAllUserProvider;

impl UserProvider for DenyAllUserProvider {
    fn authenticate(
        &self,
        _credentials: &Credentials,
    ) -> UserProviderFuture<'_, Option<AuthIdentity>> {
        Box::pin(async { Ok(None) })
    }
}

#[derive(Clone)]
pub struct StaticUserProvider {
    users: Vec<(String, PasswordHash, AuthIdentity)>,
    dummy_hash: PasswordHash,
    verification_slots: std::sync::Arc<Semaphore>,
}

impl StaticUserProvider {
    /// Build a test/demo provider. This is not a durable user database.
    pub fn new(
        subject: impl Into<String>,
        password: &str,
        identity: AuthIdentity,
    ) -> Result<Self, UserProviderError> {
        let subject = subject.into();
        validate_static_user(&subject, password)?;
        let hash = PasswordHash::hash_argon2id(password)
            .map_err(|_| UserProviderError::InvalidCredentials)?;
        let dummy_hash = PasswordHash::hash_argon2id("rustflow-static-provider-dummy")
            .map_err(|_| UserProviderError::ProviderUnavailable)?;
        Ok(Self {
            users: vec![(subject, hash, identity)],
            dummy_hash,
            verification_slots: std::sync::Arc::new(Semaphore::new(
                MAX_CONCURRENT_PASSWORD_VERIFICATIONS,
            )),
        })
    }

    /// Add another test/demo user with a unique subject.
    pub fn add(
        mut self,
        subject: impl Into<String>,
        password: &str,
        identity: AuthIdentity,
    ) -> Result<Self, UserProviderError> {
        let subject = subject.into();
        validate_static_user(&subject, password)?;
        if self
            .users
            .iter()
            .any(|(existing, _, _)| existing == &subject)
        {
            return Err(UserProviderError::InvalidCredentials);
        }
        let hash = PasswordHash::hash_argon2id(password)
            .map_err(|_| UserProviderError::InvalidCredentials)?;
        self.users.push((subject, hash, identity));
        Ok(self)
    }
}

fn validate_static_user(subject: &str, password: &str) -> Result<(), UserProviderError> {
    if subject.trim().is_empty()
        || subject.len() > MAX_CREDENTIAL_SUBJECT_BYTES
        || subject.chars().any(char::is_control)
        || password.is_empty()
        || password.len() > MAX_CREDENTIAL_SECRET_BYTES
    {
        return Err(UserProviderError::InvalidCredentials);
    }
    Ok(())
}

impl fmt::Debug for StaticUserProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StaticUserProvider")
            .field("user_count", &self.users.len())
            .finish()
    }
}

impl UserProvider for StaticUserProvider {
    fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> UserProviderFuture<'_, Option<AuthIdentity>> {
        let user = self
            .users
            .iter()
            .find(|(expected_subject, _, _)| expected_subject == credentials.subject())
            .map(|(_, hash, identity)| (hash.clone(), identity.clone()));
        let hash = user
            .as_ref()
            .map_or_else(|| self.dummy_hash.clone(), |(hash, _)| hash.clone());
        let secret = credentials.secret().to_owned();
        let permit = match std::sync::Arc::clone(&self.verification_slots).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                return Box::pin(async { Err(UserProviderError::ProviderUnavailable) });
            }
        };
        Box::pin(async move {
            let valid = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                hash.verify(&secret)
            })
            .await
            .map_err(|_| UserProviderError::ProviderUnavailable)?
            .map_err(|_| UserProviderError::ProviderUnavailable)?;
            Ok(if valid {
                user.map(|(_, identity)| identity)
            } else {
                None
            })
        })
    }
}

/// Internal login failure category. Map invalid credentials to a generic public response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginError {
    InvalidCredentials,
    ProviderUnavailable,
    StorageUnavailable,
}

impl fmt::Display for LoginError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCredentials => "invalid credentials",
            Self::ProviderUnavailable => "authentication provider unavailable",
            Self::StorageUnavailable => "session storage unavailable",
        })
    }
}

impl Error for LoginError {}

/// Verify credentials through a provider and rotate the session on success.
///
/// This does not install HTTP routes or CSRF middleware; cookie-authenticated login handlers
/// must apply the application's CSRF protection.
pub async fn login(
    session: &SessionContext,
    provider: &dyn UserProvider,
    credentials: &Credentials,
) -> Result<bool, LoginError> {
    let identity = provider
        .authenticate(credentials)
        .await
        .map_err(|error| match error {
            UserProviderError::InvalidCredentials => LoginError::InvalidCredentials,
            UserProviderError::ProviderUnavailable => LoginError::ProviderUnavailable,
        })?;
    let Some(identity) = identity else {
        return Ok(false);
    };
    session
        .authenticate(identity)
        .await
        .map_err(|_| LoginError::StorageUnavailable)?;
    Ok(true)
}

/// Revoke the current session. Cookie-authenticated logout handlers must apply CSRF protection.
pub async fn logout(session: &SessionContext) -> Result<(), SessionError> {
    session.invalidate().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn credentials_and_provider_fail_closed() {
        assert!(Credentials::new("", "secret").is_err());
        assert!(Credentials::new("ada", "x".repeat(MAX_CREDENTIAL_SECRET_BYTES + 1)).is_err());
        let credentials = Credentials::new("ada", "secret").expect("credentials");
        assert_eq!(
            format!("{credentials:?}"),
            "Credentials { subject: \"ada\", secret: \"[REDACTED]\" }"
        );

        let provider = DenyAllUserProvider;
        assert_eq!(
            provider.authenticate(&credentials).await.expect("provider"),
            None
        );
    }

    #[tokio::test]
    async fn static_provider_verifies_password_and_returns_server_identity() {
        let identity = AuthIdentity::new("ada")
            .expect("identity")
            .with_role("member")
            .expect("role")
            .with_permission("profile.read")
            .expect("permission");
        let provider =
            StaticUserProvider::new("ada", "correct horse", identity.clone()).expect("provider");

        let valid = Credentials::new("ada", "correct horse").expect("valid");
        assert_eq!(
            provider.authenticate(&valid).await.expect("authenticate"),
            Some(identity.clone())
        );

        let invalid = Credentials::new("ada", "wrong").expect("invalid");
        assert_eq!(
            provider.authenticate(&invalid).await.expect("authenticate"),
            None
        );
        let unknown = Credentials::new("grace", "correct horse").expect("unknown subject");
        assert_eq!(
            provider.authenticate(&unknown).await.expect("authenticate"),
            None
        );
        assert!(
            provider
                .clone()
                .add("ada", "another password", identity)
                .is_err()
        );
    }

    #[tokio::test]
    async fn static_provider_bounds_password_verification_concurrency() {
        let provider = StaticUserProvider::new(
            "ada",
            "correct horse",
            AuthIdentity::new("ada").expect("identity"),
        )
        .expect("provider");
        let _permits = std::sync::Arc::clone(&provider.verification_slots)
            .try_acquire_many_owned(MAX_CONCURRENT_PASSWORD_VERIFICATIONS as u32)
            .expect("hold all verification slots");
        let credentials = Credentials::new("ada", "correct horse").expect("credentials");

        assert_eq!(
            provider.authenticate(&credentials).await,
            Err(UserProviderError::ProviderUnavailable)
        );
    }
}
