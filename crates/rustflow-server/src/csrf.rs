//! Signed double-submit CSRF tokens for cookie-authenticated mutations.
use std::{
    error::Error,
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use http::{HeaderValue, Method};
use ring::{
    constant_time::verify_slices_are_equal,
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use serde::Deserialize;

use crate::{HttpError, Middleware, Request};

const COOKIE_NAME: &str = "rustflow_csrf";
const TOKEN_TTL_SECS: u64 = 8 * 60 * 60;
const TOKEN_DOMAIN: &[u8] = b"rustflow.csrf.v1\0";
const TOKEN_PART_BYTES: usize = 32;

/// Stateless CSRF verification using an application-supplied secret key.
///
/// Use one stable, randomly generated key per application/deployment and keep it
/// outside version control. Construct this type during application startup and
/// capture clones in the routes/middleware that protect cookie-authenticated
/// mutations. This does not create or identify a user session.
#[derive(Clone)]
pub struct CsrfProtection {
    secret: Arc<[u8]>,
}

impl fmt::Debug for CsrfProtection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CsrfProtection")
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

impl CsrfProtection {
    /// Create CSRF protection from a secret containing at least 32 bytes.
    ///
    /// Supply a cryptographically random secret from deployment configuration or
    /// a managed secret store. Do not derive a key from a public application name
    /// or commit it to source control.
    pub fn new(secret: impl AsRef<[u8]>) -> Result<Self, CsrfError> {
        let secret = secret.as_ref();
        if secret.len() < 32 {
            return Err(CsrfError::WeakSecret);
        }
        Ok(Self {
            secret: Arc::from(secret),
        })
    }

    /// Issue a fresh signed token.
    pub fn issue_token(&self) -> Result<CsrfToken, CsrfError> {
        let issued_at = current_epoch_seconds()?;
        let mut nonce = [0_u8; TOKEN_PART_BYTES];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| CsrfError::RandomnessUnavailable)?;
        let message = signing_message(issued_at, &nonce);
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.secret.as_ref());
        let signature = hmac::sign(&key, &message);
        Ok(CsrfToken(format!(
            "{issued_at}.{}.{}",
            encode_hex(&nonce),
            encode_hex(signature.as_ref())
        )))
    }

    /// Return the existing valid cookie token for this request, or issue a new one.
    ///
    /// Reusing an unexpired token avoids invalidating forms in other browser tabs.
    /// The caller should also set the returned value as a cookie with
    /// [`cookie_header`](Self::cookie_header).
    pub fn token_for(&self, request: &Request) -> Result<CsrfToken, CsrfError> {
        if let Some(token) = request
            .header("cookie")
            .and_then(|header| header.to_str().ok())
            .and_then(cookie_token)
            .map(str::to_owned)
            .filter(|token| self.verify_signed_token(token))
        {
            return Ok(CsrfToken(token));
        }
        self.issue_token()
    }

    /// Build a host-only CSRF cookie header for the token.
    ///
    /// The cookie is HttpOnly and SameSite=Lax. Render the token separately in a
    /// hidden form field or expose it to same-origin code for an
    /// `X-CSRF-Token` header. Pass `secure = true` for HTTPS deployments.
    pub fn cookie_header(
        &self,
        token: &CsrfToken,
        secure: bool,
    ) -> Result<HeaderValue, CsrfError> {
        if !self.verify_signed_token(token.as_str()) {
            return Err(CsrfError::InvalidToken);
        }
        let secure_attribute = if secure { "; Secure" } else { "" };
        let header = format!(
            "{COOKIE_NAME}={}; Path=/; SameSite=Lax; HttpOnly; Max-Age={TOKEN_TTL_SECS}{secure_attribute}",
            token.as_str()
        );
        HeaderValue::from_str(&header).map_err(|_| CsrfError::InvalidToken)
    }

    /// Middleware that rejects unsafe methods without a matching signed CSRF token.
    ///
    /// GET, HEAD, OPTIONS, and TRACE bypass token checks. Other methods require
    /// the valid `rustflow_csrf` cookie and a matching `X-CSRF-Token` header,
    /// or a matching `_csrf` field on an URL-encoded form. Register globally
    /// only if all matching mutating routes use cookie authentication, or attach
    /// it to selected routes/groups.
    pub fn middleware(&self) -> Middleware {
        let protection = self.clone();
        Middleware::new(move |request, next| {
            let protection = protection.clone();
            async move {
                if is_safe_method(request.method()) {
                    return next.run(request).await;
                }

                let cookie = request
                    .header("cookie")
                    .and_then(|header| header.to_str().ok())
                    .and_then(cookie_token);
                let submitted = submitted_token(&request);
                match (cookie, submitted) {
                    (Some(cookie), Some(submitted))
                        if protection.verify_pair(cookie, &submitted) =>
                    {
                        next.run(request).await
                    }
                    _ => Err(HttpError::forbidden("CSRF validation failed")),
                }
            }
        })
    }

    fn verify_pair(&self, cookie: &str, submitted: &str) -> bool {
        verify_slices_are_equal(cookie.as_bytes(), submitted.as_bytes()).is_ok()
            && self.verify_signed_token(cookie)
    }

    fn verify_signed_token(&self, token: &str) -> bool {
        let mut pieces = token.split('.');
        let Some(timestamp_text) = pieces.next() else {
            return false;
        };
        let Some(nonce_text) = pieces.next() else {
            return false;
        };
        let Some(signature_text) = pieces.next() else {
            return false;
        };
        if pieces.next().is_some() {
            return false;
        }

        let Ok(issued_at) = timestamp_text.parse::<u64>() else {
            return false;
        };
        let Some(nonce) = decode_hex::<TOKEN_PART_BYTES>(nonce_text) else {
            return false;
        };
        let Some(signature) = decode_hex::<TOKEN_PART_BYTES>(signature_text) else {
            return false;
        };
        let Ok(now) = current_epoch_seconds() else {
            return false;
        };
        if issued_at > now || now.saturating_sub(issued_at) > TOKEN_TTL_SECS {
            return false;
        }

        let message = signing_message(issued_at, &nonce);
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.secret.as_ref());
        hmac::verify(&key, &message, &signature).is_ok()
    }
}

/// Opaque signed CSRF token; its string representation should not be logged.
#[derive(Clone, PartialEq, Eq)]
pub struct CsrfToken(String);

impl CsrfToken {
    /// Return the token for a hidden field or request header.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CsrfToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CsrfToken([REDACTED])")
    }
}

/// CSRF setup or token-generation failure. Messages never include secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsrfError {
    /// The supplied application key is shorter than 32 bytes.
    WeakSecret,
    /// The operating system random source could not provide a nonce.
    RandomnessUnavailable,
    /// The system clock is before the Unix epoch or unavailable.
    ClockUnavailable,
    /// A token was malformed, expired, or could not be represented as a cookie.
    InvalidToken,
}

impl fmt::Display for CsrfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WeakSecret => "CSRF secret must contain at least 32 bytes",
            Self::RandomnessUnavailable => "secure randomness is unavailable",
            Self::ClockUnavailable => "system time is unavailable",
            Self::InvalidToken => "CSRF token is invalid or expired",
        })
    }
}

impl Error for CsrfError {}

#[derive(Deserialize)]
struct FormToken {
    #[serde(rename = "_csrf")]
    token: Option<String>,
}

fn submitted_token(request: &Request) -> Option<String> {
    if let Some(header) = request.header("x-csrf-token") {
        return header.to_str().ok().map(str::to_owned);
    }

    let content_type = request.header("content-type")?.to_str().ok()?;
    if !content_type
        .split(';')
        .next()
        .is_some_and(|media_type| {
            media_type
                .trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
    {
        return None;
    }
    let body = std::str::from_utf8(request.body()).ok()?;
    serde_urlencoded::from_str::<FormToken>(body).ok()?.token
}

fn cookie_token(cookie_header: &str) -> Option<&str> {
    for part in cookie_header.split(';') {
        if let Some((name, value)) = part.trim().split_once('=')
            && name.trim() == COOKIE_NAME
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn is_safe_method(method: &Method) -> bool {
    *method == Method::GET
        || *method == Method::HEAD
        || *method == Method::OPTIONS
        || *method == Method::TRACE
}

fn current_epoch_seconds() -> Result<u64, CsrfError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| CsrfError::ClockUnavailable)
}

fn signing_message(issued_at: u64, nonce: &[u8; TOKEN_PART_BYTES]) -> Vec<u8> {
    let mut message = Vec::with_capacity(TOKEN_DOMAIN.len() + 8 + nonce.len());
    message.extend_from_slice(TOKEN_DOMAIN);
    message.extend_from_slice(&issued_at.to_be_bytes());
    message.extend_from_slice(nonce);
    message
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_hex<const N: usize>(encoded: &str) -> Option<[u8; N]> {
    if encoded.len() != N * 2 {
        return None;
    }
    let mut decoded = [0_u8; N];
    for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        decoded[index] = (high << 4) | low;
    }
    Some(decoded)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{CsrfError, CsrfProtection};

    #[test]
    fn rejects_short_secrets_and_redacts_secret_debug_output() {
        assert!(matches!(
            CsrfProtection::new("too-short"),
            Err(CsrfError::WeakSecret)
        ));
        let protection = CsrfProtection::new([7_u8; 32]).expect("32 byte key");
        assert!(!format!("{protection:?}").contains("777777"));
        assert!(format!("{protection:?}").contains("[REDACTED]"));
    }

    #[test]
    fn issues_verifiable_tokens_and_detects_tampering_and_wrong_keys() {
        let protection = CsrfProtection::new([7_u8; 32]).expect("key");
        let other = CsrfProtection::new([8_u8; 32]).expect("key");
        let token = protection.issue_token().expect("secure token");
        assert!(protection.verify_signed_token(token.as_str()));
        assert!(!other.verify_signed_token(token.as_str()));

        let mut tampered = token.as_str().to_owned();
        tampered.push('0');
        assert!(!protection.verify_signed_token(&tampered));
        assert_eq!(
            format!("{token:?}"),
            "CsrfToken([REDACTED])"
        );
    }
}
