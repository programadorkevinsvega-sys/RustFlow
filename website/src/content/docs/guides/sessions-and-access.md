---
title: Sesiones, autenticación y autorización
description: Usa sesiones del lado del servidor, guards de ruta y políticas de acceso con denegación predeterminada.
---

Estas APIs son experimentales y pre-1.0. El adaptador HTTP ofrece sesiones del lado del servidor y guards integrados en el registro de rutas. La integración SQLx de la Fase 6.5 es un contrato independiente de persistencia de la aplicación; no convierte `InMemorySessionStore` en un almacén durable ni integra las sesiones con la base de datos. No equivale a una revisión de seguridad de producción y no incluye OAuth/OIDC, MFA, recuperación de cuentas ni rate limiting.

## 1. Instalar el middleware

~~~rust
use std::{sync::Arc, time::Duration};
use rustflow_server::{Application, InMemorySessionStore, SessionCookieConfig, SessionManager};

let store = Arc::new(InMemorySessionStore::new());
let sessions = SessionManager::new(store, Duration::from_secs(30 * 60))?;
let mut app = Application::new();
// En HTTPS, usa los valores predeterminados seguros.
app.middleware(sessions.middleware())?;
~~~

La cookie predeterminada es __Host-rustflow_session con Secure, HttpOnly, SameSite=Lax, Path=/ y sin Domain. Su Max-Age deriva de la expiración guardada en el servidor. En desarrollo local por HTTP configura explícitamente SessionCookieConfig::development() mediante with_cookie_config; nunca uses ese perfil en producción.

## 2. Crear, leer, rotar e invalidar

~~~rust
use rustflow_server::{HttpError, Request, Response};

app.post("/preferences", |request: Request| async move {
    let session = request.session().ok_or_else(HttpError::internal_server_error)?;
    session.insert("theme", &"dark").map_err(|_| HttpError::internal_server_error())?;
    Ok(Response::text("saved"))
})?;
~~~

- create() crea una sesión anónima cuando no existe una activa.
- get::<T>(key) e insert(key, &value) leen y escriben valores serializables vía Serde.
- rotate() genera un ID nuevo, invalida el anterior y preserva los valores de aplicación.
- authenticate(identity) rota el ID, descarta valores preautenticación y guarda AuthIdentity en el servidor. Úsalo solo después de verificar credenciales.
- invalidate() revoca la sesión y solicita que la respuesta expire la cookie.

Los errores de almacenamiento y las sesiones inexistentes/caducadas son resultados explícitos. No conviertas un error en un permiso concedido, no recrees una sesión escribiendo en un ID antiguo y no registres cookies, IDs o credenciales.

## 3. Guard antes del handler

route_protected registra el middleware de autenticación antes de ejecutar el handler. Una petición anónima recibe HTTP 401. Si no se instaló SessionManager::middleware(), el guard también falla cerrado.

~~~rust
use http::StatusCode;
use rustflow_server::{HttpError, HttpMethod, Request, Response};

app.route_protected(HttpMethod::Get, "/profile", |request: Request| async move {
    let identity = request.identity().map_err(|_| HttpError::internal_server_error())?
        .ok_or_else(|| HttpError::new(StatusCode::UNAUTHORIZED, "Authentication required"))?;
    Ok(Response::text(identity.subject().to_owned()))
})?;
~~~

La aplicación debe verificar credenciales mediante su propio flujo y solo después llamar session.authenticate(identity). RustFlow no incluye un proveedor de usuarios.

## 4. Autorización con denegación predeterminada

RolePolicy asocia un permiso explícito con uno o más roles. Una política ausente o un permiso no configurado deniega.

~~~rust
use std::sync::Arc;
use rustflow_server::{AccessPolicy, HttpMethod, Request, Response, RolePolicy};

let policy: Arc<dyn AccessPolicy> = Arc::new(
    RolePolicy::new().allow_role("reports.read", "analyst")?
);
app.route_authorized(
    HttpMethod::Get, "/reports", "reports.read", Some(policy),
    |_request: Request| async { Ok(Response::text("report")) },
)?;
~~~

Construye la identidad desde datos confiables del servidor:

~~~rust
let identity = AuthIdentity::new("user-42")?.with_role("analyst")?;
session.authenticate(identity)?;
~~~

No copies roles ni permisos directamente desde JSON, formularios, query strings o cookies. Los valores enviados por el cliente no deben convertirse en AuthIdentity.

## 5. Hashing de contraseñas

PasswordHash::hash y verify usan PBKDF2-HMAC-SHA-256 de ring, con 600.000 iteraciones, salt aleatorio de 16 bytes y salida de 32 bytes. Guarda el resultado con el registro del usuario, nunca la contraseña. La verificación consume CPU: la aplicación debe aplicar rate limiting y mantener su política de contraseñas. No se incluye login completo, búsqueda de usuarios ni recuperación de contraseña.

~~~rust
use rustflow_server::PasswordHash;

let stored = PasswordHash::hash("a long passphrase")?;
// Persiste stored.as_str() en un almacén de credenciales confiable.
assert!(stored.verify("a long passphrase")?);
~~~

## 6. Integración CSRF en login/logout

Una sesión no sustituye CsrfProtection. Conecta el middleware CSRF a todas las rutas mutables que dependan de cookies, incluido login y logout. Las peticiones deben enviar la cookie CSRF, el mismo token en X-CSRF-Token o _csrf y un contexto Origin/Referer del mismo origen.

~~~rust
use rustflow_server::{CsrfProtection, HttpMethod};

let csrf_key = std::env::var("RUSTFLOW_CSRF_KEY")?;
let csrf = CsrfProtection::new(csrf_key.as_bytes())?;
app.post("/login", login_handler)?;
app.route_middleware(HttpMethod::Post, "/login", csrf.clone().middleware())?;
app.post("/logout", logout_handler)?;
app.route_middleware(HttpMethod::Post, "/logout", csrf.middleware())?;
~~~

El secreto debe ser aleatorio, estable para el despliegue y almacenado fuera del repositorio. La sesión y CSRF son mecanismos independientes. No retires la validación de origen ni sirvas tokens desde una caché compartida.

## 7. Errores y límites

- 401: no hay identidad autenticada activa.
- 403: la política deniega, no existe política o no hay regla para ese permiso.
- 500 genérico: falla almacenamiento o política; no se concede acceso.
- Los IDs manipulados, caducados o revocados no reconstruyen identidad.
- InMemorySessionStore es volátil, local al proceso y no se comparte entre instancias.
- SessionStore exige operaciones atómicas. La PR de la Fase 6.5 añade SQLx, pools, transacciones y migraciones para datos de aplicación, pero todavía no implementa un adaptador SQL para SessionStore; `InMemorySessionStore` sigue siendo volátil y local al proceso.
- No se incluyen OAuth/OIDC, MFA, recuperación de cuentas, rate limiting, protección de fuerza bruta ni auditoría completa del despliegue.

## 8. Ejecutar pruebas

~~~sh
cargo test -p rustflow-server --test session_access
cargo test -p rustflow-server
cargo test --workspace --all-features
~~~

Las pruebas de regresión cubren expiración, rotación e invalidación, guards previos al handler, verificación de contraseña, CSRF y replay tras cerrar sesión. La fase permanece abierta hasta que pase toda la CI obligatoria.
