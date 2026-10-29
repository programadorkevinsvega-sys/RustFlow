---
title: Formularios y validación
description: Extrae entradas tipadas, devuelve errores de campo, protege mutaciones con cookies mediante CSRF y vuelve a una ruta local segura.
---

Esta guía cubre las APIs experimentales de la Fase 6.3 del servidor RustFlow. Un dato recibido es siempre no confiable: primero decodifícalo a un tipo, después valida las reglas de aplicación y realiza la comprobación de permisos por separado. La Fase 6.4 incorpora APIs experimentales de sesiones y guards que están en revisión. Esta guía describe la validación y el middleware CSRF, que deben integrarse deliberadamente en las rutas con cookies. No hay persistencia de base de datos.

## 1. Extrae un formulario tipado

El siguiente ejemplo define una entrada de registro y rechaza tipos o campos inesperados:

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
            errors.add_field_error("email", "Introduce un correo válido.");
        }
        if self.age < 18 {
            errors.add_field_error("age", "Debes tener al menos 18 años.");
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

async fn signup(request: Request) -> Result<Response, rustflow_server::HttpError> {
    let input = request.validated_form::<SignupInput>()?;
    Ok(Response::text(format!("Registro válido: {}", input.email)))
}
~~~

Registra la función como un handler POST. Request::form<T>() acepta application/x-www-form-urlencoded; validated_form<T>() además ejecuta ValidateInput. Para JSON y parámetros de query puedes usar validated_json<T>() y validated_query<T>().

Los content-types no admitidos producen 415. El JSON inválido, los valores que no pueden convertirse al tipo y los campos adicionales rechazados por Serde producen un 400 genérico. El límite de ServerConfig se aplica primero y devuelve 413 cuando la carga supera el máximo.

## 2. Muestra errores por campo

Una entrada que se decodifica bien pero no supera la validación produce HTTP 422:

~~~json
{
  "error": "validation_failed",
  "errors": {
    "fieldErrors": {
      "age": ["Debes tener al menos 18 años."],
      "email": ["Introduce un correo válido."]
    },
    "formErrors": []
  }
}
~~~

ValidationErrors::add_field_error admite varios mensajes por campo. add_form_error registra errores que afectan al formulario completo. Las claves se serializan de forma determinista. No incluyas valores enviados, contraseñas, cookies, tokens, SQL ni detalles internos en mensajes públicos.

La validación de ejemplo es deliberadamente pequeña: ajusta rangos y reglas a tu dominio. La API no valida automáticamente formatos de correo completos, permisos, unicidad en una base de datos ni reglas de negocio específicas.

## 3. Añade una defensa CSRF en rutas con cookies

Si una mutación se autentica mediante cookies del navegador, conecta el middleware CSRF a esas rutas. La clave debe ser aleatoria, tener al menos 32 bytes y provenir del entorno de despliegue o de un almacén de secretos, no del código fuente.

~~~rust
use rustflow_server::{CsrfProtection, HttpError, HttpMethod, Request, Response};

let key = std::env::var("RUSTFLOW_CSRF_KEY")?;
let csrf = CsrfProtection::new(key.as_bytes())?;
let for_token_route = csrf.clone();

app.get("/profile/csrf-token", move |request: Request| {
    let csrf = for_token_route.clone();
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

app.post("/profile", |request: Request| async move {
    let _profile = request.validated_form::<SignupInput>()?;
    request.redirect_back("/profile")
})?;
app.route_middleware(HttpMethod::Post, "/profile", csrf.middleware())?;
~~~

Registra la ruta POST antes de llamar a route_middleware. La respuesta que entrega el token debe incluir Cache-Control: no-store y no debe almacenarse en una caché compartida. Para una página HTML, coloca el token dentro de un campo oculto correctamente escapado; para una petición same-origin, envíalo en X-CSRF-Token. El navegador debe enviar la cookie rustflow_csrf; el middleware exige que el token presentado coincida, tenga firma válida y que Origin (o Referer cuando Origin falta) coincida con el mismo origen; las solicitudes sin origen verificable o con origen cruzado se rechazan. Los métodos GET, HEAD, OPTIONS y TRACE no requieren token; los demás métodos lo requieren si el middleware está conectado.

El token firmado expira después de ocho horas. cookie_header configura HttpOnly y SameSite=Lax; en producción HTTPS, usa Secure. request.is_https() solo refleja TLS o un proxy expresamente confiable. No registres el token ni la clave. Este mecanismo no identifica al usuario ni reemplaza sesiones, autenticación o autorización.

Aplica el middleware a los endpoints que dependen de cookies de autenticación, no indiscriminadamente a rutas ajenas a ese modelo. La autorización del recurso debe verificarse en el handler/caso de uso protegido.

Para un formulario HTML tradicional, el campo `_csrf` debe contener el token que emitió el servidor y debe insertarse con escape de atributo HTML. Este fragmento es la forma del formulario, no una plantilla RustFlow lista para ejecutarse; reemplaza el valor ilustrativo en el renderizado de tu aplicación:

~~~html
<form method="post" action="/profile">
  <input type="hidden" name="_csrf" value="TOKEN_EMITIDO_POR_EL_SERVIDOR">
  <label for="email">Correo</label>
  <input id="email" name="email" type="email" required>
  <label for="age">Edad</label>
  <input id="age" name="age" type="number" min="0" max="255" required>
  <button type="submit">Guardar</button>
</form>
~~~

El envío debe llevar también la cookie `rustflow_csrf` configurada por el servidor. No dejes el texto de ejemplo como token real ni aceptes un token proveniente de entrada de usuario sin la verificación del middleware.

## 4. Redirige de vuelta sin abrir redirecciones externas

Tras aceptar una mutación, Request::redirect_back("/profile") usa la ruta de Referer solo cuando el origen coincide con la petición actual. Si falta, es inválido o apunta a otro origen, redirige al fallback local. El fallback debe empezar por una sola barra, por ejemplo /profile; no uses valores del cuerpo del usuario como destino de redirección.

## 5. Qué comprobar

Las pruebas deben cubrir entradas válidas, errores de campo/formulario, content-types no admitidos, datos inesperados, payloads demasiado grandes, tokens ausentes/incompatibles, tokens firmados válidos y Referer externo. La suite de integración TCP canónica se ejecuta desde la raíz con `cargo test -p rustflow-server --test http_integration`; cubre el contrato de red real del servidor. Para toda la verificación del workspace, consulta [Probar y depurar aplicaciones](/guides/testing/). Consulta también el contrato técnico en [HTTP_SERVER.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/HTTP_SERVER.md) y el [estado del proyecto](/project-status/).

Sesiones, login, autorización integrada, almacenamiento, bases de datos, uploads y server actions no forman parte de esta fase. RustFlow sigue siendo un prototipo pre-1.0.


## CSRF cuando se usan sesiones

Una sesión no reemplaza CSRF. Añade CsrfProtection::middleware() a cada ruta de mutación que dependa de cookies de navegador, incluido login y logout. Conserva las comprobaciones de token y Origin/Referer; la rotación de sesión no desactiva esas comprobaciones. Consulta la [guía de sesiones y control de acceso](/guides/sessions-and-access/).
