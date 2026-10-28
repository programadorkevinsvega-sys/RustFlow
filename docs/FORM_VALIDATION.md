# Validación de entradas y formularios (Fase 6.3)

Estas APIs son experimentales y forman parte del adaptador rustflow-server. Decodificar un valor no demuestra que sea válido para el dominio: primero se extraen tipos desde la petición, luego se ejecuta validación de aplicación y, en recursos protegidos, se aplica autorización por separado.

## Extracción de formularios tipados

Request::form<T>() solo acepta application/x-www-form-urlencoded. Un tipo no compatible produce 415; una carga malformada o que no coincida con el tipo produce un 400 genérico. El límite global de ServerConfig::with_max_body_bytes se aplica antes de que el handler lea el cuerpo.

~~~rust
use serde::Deserialize;
use rustflow_server::{Request, Response, ValidateInput, ValidationErrors};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignupInput {
    email: String,
    age: u8,
}

impl ValidateInput for SignupInput {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if !self.email.contains('@') {
            errors.add_field_error("email", "Enter a valid email address.");
        }
        if self.age < 18 {
            errors.add_field_error("age", "You must be at least 18 years old.");
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

async fn signup(request: Request) -> Result<Response, rustflow_server::HttpError> {
    let input = request.validated_form::<SignupInput>()?;
    Ok(Response::text(format!("Accepted: {}", input.email)))
}
~~~

Registra la función como ruta POST. Para JSON y query parameters hay helpers equivalentes: validated_json<T>() y validated_query<T>().

Usa #[serde(deny_unknown_fields)] cuando campos adicionales no deban ignorarse. Los tipos Rust siguen siendo el primer control de forma; por ejemplo, u8 no acepta un entero fuera de rango. No conviertas errores de parseo en valores predeterminados ni asumas que un campo oculto del navegador es confiable.

## Errores estructurados

Una entrada que se decodifica correctamente pero no pasa ValidateInput devuelve HTTP 422 y un cuerpo JSON con error: "validation_failed", errors.fieldErrors y errors.formErrors. Cada clave de campo puede tener varias explicaciones públicas.

~~~json
{
  "error": "validation_failed",
  "errors": {
    "fieldErrors": {
      "age": ["You must be at least 18 years old."],
      "email": ["Enter a valid email address."]
    },
    "formErrors": ["The submitted values could not be accepted."]
  }
}
~~~

Los mensajes son contenido visible al usuario. No incluyas el valor enviado, contraseñas, cookies, tokens, credenciales, consultas SQL ni detalles internos. Los errores de sintaxis/decodificación siguen siendo genéricos para no revelar detalles del parser.

## CSRF para mutaciones autenticadas mediante cookies

CsrfProtection proporciona un token aleatorio firmado por HMAC-SHA-256 usando la biblioteca mantenida ring. Debes proporcionar una clave aleatoria de al menos 32 bytes, almacenada fuera del repositorio y estable entre reinicios para cada despliegue. No uses una cadena pública, el nombre de la aplicación ni una clave nueva por petición.

El API no crea sesiones ni autentica usuarios. Conecta el middleware a las rutas o grupos que usan cookies para autenticar mutaciones. No lo uses como sustituto de la comprobación de identidad y autorización. El token tiene una duración de ocho horas. GET, HEAD, OPTIONS y TRACE omiten la comprobación; los demás métodos deben enviar la cookie rustflow_csrf y el mismo valor en X-CSRF-Token o en el campo URL-encoded _csrf.

~~~rust
use rustflow_server::{CsrfProtection, HttpError, HttpMethod, Request, Response};

let key = std::env::var("RUSTFLOW_CSRF_KEY")?;
let csrf = CsrfProtection::new(key.as_bytes())?; // La variable debe contener una clave aleatoria fuerte.
let token_protection = csrf.clone();

app.get("/signup/token", move |request: Request| {
    let csrf = token_protection.clone();
    async move {
        let token = csrf.token_for(&request)
            .map_err(|_| HttpError::internal_server_error())?;
        let cookie = csrf.cookie_header(&token, request.is_https())
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(Response::text(token.as_str().to_owned())
            .with_header(http::header::SET_COOKIE, cookie)
            .with_header(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("no-store"),
            ))
    }
})?;

// Registra primero la ruta POST; después conecta la defensa a esa ruta.
app.post("/signup", |request: Request| async move {
    let _input = request.validated_form::<SignupInput>()?;
    Ok(Response::redirect("/signup/success")?)
})?;
app.route_middleware(HttpMethod::Post, "/signup", csrf.middleware())?;
~~~

El endpoint de token ilustra el contrato y usa Cache-Control: no-store; no permitas que un CDN o caché compartido almacene su respuesta. En una página HTML, coloca el token dentro de un campo oculto escapado; para una petición fetch del mismo origen, envíalo en X-CSRF-Token. El navegador debe enviar la cookie rustflow_csrf; el middleware exige que el token presentado coincida y tenga una firma válida. Configura Secure para producción HTTPS; request.is_https() solo es confiable si TLS o la política de proxy del servidor están configurados correctamente. Usa una clave distinta por entorno y una estrategia administrada para su rotación.

El middleware debe limitarse a operaciones que dependan de cookies de navegador. No protege automáticamente rutas que no lo hayan registrado y no define política de autorización. Las sesiones, autenticación y autorización integradas pertenecen a una fase posterior.

## Redirección de retorno segura

Request::redirect_back("/signup") usa el path/query del Referer únicamente si su esquema, host y puerto efectivo coinciden con el origen de la petición. Si falta el Referer o apunta a otro origen, usa la ruta de fallback. El fallback debe ser una ruta relativa al origen, como /signup; se rechazan URLs externas y rutas protocol-relative.

~~~rust
app.post("/profile", |request: Request| async move {
    let _profile = request.validated_form::<SignupInput>()?;
    request.redirect_back("/profile")
})?;
~~~

No construyas un destino de redirección con campos proporcionados directamente por el cliente. Valida la entrada antes de cambiar estado y aplica autorización en la capa de aplicación antes de ejecutar la operación.

## Pruebas y límites

La suite HTTP/TCP debe cubrir entradas válidas e inválidas, campos inesperados, tipos y content-types incorrectos, 413 para cuerpos que excedan el límite, errores 422 sin eco de valores enviados, tokens CSRF ausentes/incorrectos/válidos y Referer externo. Revisa los resultados de CI en el HEAD final de la PR antes de marcar la fase como cerrada.

Esta fase no incluye sesiones, login, roles/policies, almacenamiento, ORM, server actions, cargas de archivos ni un generador automático de formularios. Las APIs son pre-1.0 y no convierten por sí mismas a RustFlow en una plataforma lista para producción.
