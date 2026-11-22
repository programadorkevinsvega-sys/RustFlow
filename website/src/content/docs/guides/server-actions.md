---
title: Server actions JSON
description: Registra acciones POST tipadas con validación, guards, CSRF y límites explícitos.
---

Las server actions de RustFlow son rutas `POST` experimentales para ejecutar una operación del servidor con una entrada JSON tipada y validada. Se ejecutan en el pipeline HTTP existente: no crean un segundo router ni confían en validación, identidad o roles enviados por el cliente.

## Primera acción verificable

El ejemplo ejecutable [`server_actions.rs`](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/server_actions.rs) expone `POST /api/actions/ping`. Inícialo desde la raíz del checkout:

```sh
cargo run -p rustflow-server --example server_actions
```

En otra terminal, envía JSON:

```sh
curl -i http://127.0.0.1:3001/api/actions/ping \
  -H 'Content-Type: application/json' \
  --data '{"nonce":42}'
```

El ejemplo valida `nonce` en el servidor y devuelve `{"nonce":42}` con HTTP 200. Las rutas pueden registrarse directamente en `Application` o dentro de un `RouteGroup`.

## Contrato de entrada y salida

Cada acción recibe `POST` y requiere `Content-Type: application/json`. Los parámetros de media type como `charset=utf-8` se aceptan. El tipo de entrada debe implementar `DeserializeOwned + ValidateInput`; RustFlow decodifica y ejecuta `ValidateInput` antes del callback. Usa `#[serde(deny_unknown_fields)]` si los campos adicionales deben rechazarse.

Este fragmento asume que `PingInput` y `PingOutput` están definidos como en el ejemplo ejecutable:

```rust
let config = ActionConfig::new(
    ActionAccess::Public,
    ActionCsrf::NotRequired,
)?;
app.action("/api/actions/ping", config, |_request, input: PingInput| async move {
    Ok(PingOutput { nonce: input.nonce })
})?;
```

`access` y `csrf` son argumentos obligatorios de `ActionConfig::new`; no existe un valor `Default`. Omitir uno produce un error de compilación. El `ActionAccess::Public` y `ActionCsrf::NotRequired` del ejemplo son declaraciones explícitas, no defaults. Para una acción que usa cookies, elige CSRF explícitamente.

Una respuesta correcta serializa directamente el tipo de salida como JSON con `Content-Type: application/json; charset=utf-8`, `Cache-Control: no-store` y `X-Content-Type-Options: nosniff`. El callback puede devolver `HttpError`; su mensaje público debe ser seguro y no contener datos privados.

## Acceso, sesiones y CSRF

`ActionAccess` requiere una decisión explícita:

- `Public`: no autentica; úsalo solo si la operación no depende de identidad en cookies.
- `Authenticated`: requiere identidad en una sesión del servidor.
- `ActionAccess::permission(name, policy)`: requiere una sesión y un permiso; las concesiones ausentes o los errores de policy no autorizan.

Los dos modos de sesión requieren `ActionCsrf::Required(csrf)`. También requieren instalar `SessionManager::middleware()` en la aplicación. La protección CSRF existente verifica token y origen; no autentica ni reemplaza la autorización.

```rust
let protected = ActionConfig::new(
    ActionAccess::Authenticated,
    ActionCsrf::Required(csrf.clone()),
)?;
app.action("/api/actions/profile", protected, handler)?;
```

Una acción pública puede declarar `ActionCsrf::Required(csrf)` cuando sus efectos dependan de cookies. La variante `NotRequired` es una declaración explícita de que la acción no usa cookies para autenticar ni decidir sus mutaciones. Para autenticación bearer o middleware personalizado, usa las rutas HTTP ordinarias; la primera API de acciones no modela esos esquemas.

## Límites, errores y cancelación

- Límite global: `ServerConfig::with_max_body_bytes`, por defecto 1 MiB, aplicado antes de invocar la acción.
- Límite por acción: 64 KiB por defecto; configurable hasta 1 MiB y nunca superior al global. Si el límite declarado supera el global, el registro falla con `ServerError::ActionRequestLimitExceedsGlobal`. Si el global es menor que 64 KiB, ajusta el límite de cada acción. Este middleware corre antes de la verificación CSRF y del callback; el middleware global y de grupo corre antes, pero el cuerpo ya está acotado por el límite global.
- Salida serializada: 256 KiB por defecto, configurable hasta 4 MiB. Si la serialización falla o excede el límite, se descarta la salida parcial y se devuelve HTTP 500 genérico. El callback ya se ejecutó: ese error no revierte efectos externos o cambios previamente confirmados.
- Errores de protocolo: 415 para media type incorrecto, 400 para JSON malformado o campos/tipos no decodificables, 422 para errores de `ValidateInput`, 413 si se supera el límite global o por acción, 401 sin identidad, 403 ante denegación de permiso o CSRF, y 504 si vence el timeout de petición.
- Las respuestas de errores siguen las páginas de error configuradas y el contrato HTTP existente. Los errores de validación usan `ValidationErrors`; no devuelvas el valor enviado, contraseñas, tokens ni diagnóstico interno en mensajes públicos.
- Se aplica el timeout global de la petición (30 segundos por defecto). Al vencer, el futuro de la acción se descarta y se devuelve 504. Al vencer el deadline de apagado, se cancelan los pipelines activos y el servidor puede devolver `ShutdownTimedOut`. El timeout no puede interrumpir una serialización síncrona o un callback que no cede al runtime; los límites de datos acotan trabajo de JSON, no son una cuota de CPU.
- Cancelar un future no revierte una escritura ni un efecto remoto. Usa operaciones async acotadas y transacciones explícitas; no asumas que un 504 significa que la operación no ocurrió.

## Carga de referencia con k6

La prueba usa el ejemplo TCP local y verifica status, JSON, `no-store` y latencia p95 inferior a 100 ms bajo 10 VUs durante 15 segundos:

```sh
# Terminal 1
cargo run -p rustflow-server --example server_actions

# Terminal 2
k6 run scripts/server-actions.k6.js
```

Puedes configurar `RUSTFLOW_ACTIONS_BASE_URL` y `RUSTFLOW_ACTIONS_PATH`. Esta carga smoke es una referencia reproducible para el ejemplo, no un compromiso de capacidad ni un benchmark de producción. Usa un entorno representativo y límites apropiados antes de dimensionar un despliegue.

## Alcance no incluido

La primera versión solo admite JSON POST y respuestas JSON 200. No ofrece formularios progresivos/redirects, proxy automático en el navegador, macros `use server`, uploads, streaming de respuesta, headers/status personalizados, reintentos, idempotencia ni rate limiting. El flujo de actions no incluye un proveedor de usuarios ni integra la persistencia SQL de sesiones. RustFlow continúa siendo pre-1.0; esta API experimental no implica preparación para producción.
