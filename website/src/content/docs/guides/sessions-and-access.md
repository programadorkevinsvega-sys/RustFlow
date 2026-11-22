---
title: Sesiones, autenticación y autorización
description: Usa sesiones del lado del servidor, guards de ruta y políticas de acceso con denegación predeterminada.
---

Estas APIs son experimentales y pre-1.0. El adaptador HTTP ofrece sesiones del lado del servidor, un port async `SessionStore`, un adapter `SqlSessionStore` sobre el pool compartido de la aplicación y guards integrados en el registro de rutas. `InMemorySessionStore` sigue disponible para pruebas y desarrollo de un solo proceso; no es durable. El proveedor de usuarios es un port independiente de la autorización y no incorpora una base de usuarios de producción. El rate limiting por ruta está disponible con un adapter en memoria de un solo proceso, no distribuido. Esto no equivale a una revisión de seguridad de producción y no incluye OAuth/OIDC, MFA ni recuperación de cuentas.

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

Para persistir sesiones entre reinicios y compartirlas entre procesos, configura el adapter SQL al iniciar la aplicación. La configuración de base de datos debe venir de una fuente de secretos del despliegue; no registres la URL ni la insertes en el repositorio.

~~~rust
use std::{sync::Arc, time::Duration};
use rustflow_server::{DatabaseConfig, DatabasePool, SessionManager, SqlSessionStore};

let database_url = std::env::var("DATABASE_URL")?;
let database = DatabasePool::connect(&DatabaseConfig::new(database_url)?).await?;
let store = Arc::new(SqlSessionStore::new(database));
store.ensure_schema().await?;
let sessions = SessionManager::new(store, Duration::from_secs(30 * 60))?;
app.middleware(sessions.middleware())?;
~~~

`SqlSessionStore` usa SQL parametrizado y expiración absoluta; no renueva el plazo al leer o escribir. Implementa SQLite, PostgreSQL y MySQL/MariaDB sobre el pool compartido. `ensure_schema()` crea las tablas propias de RustFlow si faltan y requiere permisos DDL en ese caso; si el despliegue administra el esquema con migraciones, aplícalas antes de aceptar tráfico. Las operaciones de almacenamiento son async y fallan cerradas ante errores de persistencia.

## 2. Crear, leer, rotar e invalidar

~~~rust
use rustflow_server::{HttpError, Request, Response};

app.post("/preferences", |request: Request| async move {
    let session = request.session().ok_or_else(HttpError::internal_server_error)?;
    session.insert("theme", &"dark").await
        .map_err(|_| HttpError::internal_server_error())?;
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

La aplicación puede implementar `UserProvider` para buscar y verificar usuarios en su propio adapter. `login` verifica con el proveedor, rota el ID y guarda solo la identidad de servidor; `logout` invalida la sesión. No asigna roles desde datos del cliente ni reemplaza CSRF.

~~~rust
use rustflow_server::{AuthIdentity, Credentials, StaticUserProvider, login};

let identity = AuthIdentity::new("ada")?.with_role("analyst")?;
let provider = StaticUserProvider::new("ada", configured_password, identity)?;
let credentials = Credentials::new(submitted_subject, submitted_password)?;
let authenticated = login(&session, &provider, &credentials).await?;
if !authenticated {
    // Devuelve una respuesta genérica; no reveles si existe la cuenta.
}
~~~

`StaticUserProvider` es un adapter de demostración/pruebas almacenado en memoria, no una base durable. `Credentials` limita el identificador a 256 bytes y el secreto a 1.024 bytes. Los adapters personalizados deben devolver identidad/roles únicamente desde datos confiables; provider o almacenamiento fallidos deben convertirse en respuestas genéricas y nunca en acceso concedido. Protege las rutas POST de login y logout con CSRF tal como se muestra abajo.

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

Para nuevas credenciales, `PasswordHash::hash_argon2id` usa la biblioteca mantenida RustCrypto Argon2id v=19 con 19 MiB, dos iteraciones, una lane, salt aleatorio de 16 bytes y salida de 32 bytes. El formato PHC se valida con esos parámetros fijos; no se aceptan work factors suministrados por quien presenta el hash. Las contraseñas y las codificaciones tienen límites de tamaño.

~~~rust
use rustflow_server::PasswordHash;

let stored = PasswordHash::hash_argon2id("a long passphrase")?;
assert!(stored.verify("a long passphrase")?);
~~~

`verify` sigue aceptando el formato PBKDF2-HMAC-SHA-256 anterior de `PasswordHash::hash`, para no invalidar cuentas existentes. `needs_rehash()` marca esos hashes legacy, pero no actualiza la base de datos. Migra solo después de verificar una contraseña válida y de persistir explícitamente el nuevo hash; no hay migración silenciosa. Llama a las operaciones de hashing síncronas al iniciar o desde un worker bloqueante, nunca directamente dentro de una tarea async.

`StaticUserProvider` genera Argon2id para los usuarios de demo, ejecuta la verificación fuera del worker async y limita a cuatro las verificaciones simultáneas por provider; si todos los espacios están ocupados, falla con un error genérico. Continúa siendo un adapter de tests/demo, no un almacén durable ni un provider de producción. Los providers propios deben limitar entradas, concurrencia y frecuencia de login.

### Algoritmo legacy

`PasswordHash::hash` conserva explícitamente PBKDF2-HMAC-SHA-256 de ring con 600.000 iteraciones, salt aleatorio de 16 bytes y salida de 32 bytes. Se mantiene para compatibilidad con hashes existentes; usa `hash_argon2id` para nuevas credenciales. Guarda el valor codificado en el almacén confiable del usuario, nunca la contraseña.

## 7. Limitar solicitudes por ruta

Configura un scope estable por ruta y un adapter compartido por los endpoints que deban compartir el contador:

~~~rust
use std::{sync::Arc, time::Duration};
use rustflow_server::{HttpMethod, InMemoryRateLimitStore, RateLimit};

let rate_store = Arc::new(InMemoryRateLimitStore::new());
let login_limit =
    RateLimit::new(rate_store, "auth.login", 5, Duration::from_secs(60))?;
app.route_middleware(HttpMethod::Post, "/login", login_limit.middleware())?;
~~~

La clave combina ese identificador constante con `Request::client_ip()`, ya resuelta por `ClientAddressPolicy`. No se leen directamente `Forwarded` ni `X-Forwarded-For`; configura proxies confiables en el servidor si termina TLS o recibe tráfico detrás de un proxy. La ventana es fija, alineada a epoch, de 1 a 86.400 segundos y permite de 1 a 1.000.000 solicitudes.

Al exceder el límite, el middleware devuelve 429 antes del handler, con `Retry-After`, metadatos de límite y `Cache-Control: no-store`. Si falta una identidad IP, el store falla o se agota su capacidad, devuelve 503 y no ejecuta el handler. No hay fallback a permitir el acceso.

`InMemoryRateLimitStore` tiene un máximo de 100.000 claves por defecto (configurable hasta 1.000.000), usa una operación atómica por proceso y elimina ventanas expiradas solo al necesitar espacio. Sus contadores se pierden al reiniciar y no se comparten entre instancias; no lo uses como control distribuido en un despliegue multi-proceso. El contrato `RateLimitStore` permite conectar un adapter compartido, pero RustFlow aún no incluye uno. Las ventanas fijas también pueden permitir una ráfaga mayor cerca del límite entre ventanas.

Para repetir la medición local del ejemplo y sus thresholds:

~~~sh
cargo run -p rustflow-server --example rate_limited
k6 run scripts/rate-limit.k6.js
~~~

## 8. Integración CSRF en login/logout

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

## 9. Errores y límites

- 401: no hay identidad autenticada activa.
- 403: la política deniega, no existe política o no hay regla para ese permiso.
- 500 genérico: falla almacenamiento o política; no se concede acceso.
- Los IDs manipulados, caducados o revocados no reconstruyen identidad.
- `InMemorySessionStore` es volátil, local al proceso y no se comparte entre instancias. Usa `SqlSessionStore` cuando necesites persistencia entre reinicios/procesos.
- Las operaciones del port `SessionStore` son async. Rotación e invalidación usan transacciones; las escrituras no extienden la expiración y los fallos de almacenamiento no conceden acceso.
- El framework no ofrece migraciones de usuarios, bloqueo de cuentas, recuperación de cuentas ni una política completa de protección contra fuerza bruta. El rate limiting en memoria no sobrevive reinicios ni coordina instancias.
- OAuth/OIDC, MFA y auditoría integral del despliegue quedan fuera del alcance actual.

## La plantilla CRUD y sus límites

La plantilla `rustflow new products --template crud` usa `SessionManager::in_memory` y `SessionCookieConfig::development` solo para ejecutar el ejemplo local por HTTP. Verifica una cuenta demo configurada mediante `RUSTFLOW_DEMO_USER` y `RUSTFLOW_DEMO_PASSWORD`, calcula `PasswordHash` y llama `SessionContext::authenticate` después de verificar las credenciales. Las rutas de lectura usan `route_protected`; las mutaciones usan `route_authorized` con una `RolePolicy` explícita; un permiso sin regla queda denegado.

El ejemplo rechaza `RUSTFLOW_ENV=production`. `InMemorySessionStore` no sobrevive a reinicios ni coordina procesos; la cuenta única de demostración no es un proveedor de usuarios durable. No copies este flujo directamente a producción: configura un store SQL, implementa un provider adecuado a tu sistema de usuarios, usa cookies seguras HTTPS y protege con CSRF cada mutación autenticada por cookies.

## 10. Ejecutar pruebas

~~~sh
cargo test -p rustflow-server --test session_access
cargo test -p rustflow-server
cargo test --workspace --all-features
~~~

Las pruebas de regresión cubren expiración, rotación e invalidación, guards previos al handler, verificación legacy/Argon2id, CSRF y replay tras cerrar sesión. La Fase 6.4 se integró en `main` mediante la PR #49; los checks requeridos se verificaron antes de la fusión y la CI post-merge quedó en verde. La API sigue siendo experimental: esta cobertura no equivale a una auditoría de seguridad ni a una garantía de preparación para producción.
