---
title: "Tutorial: primera aplicación con el CLI"
description: Crea, modifica, prueba y compila un servidor RustFlow; genera archivos y recupérate de una colisión sin perder trabajo.
---

Este recorrido usa el binario de `crates/rustflow-cli`. El CLI y las APIs RustFlow son experimentales y pre-1.0. El scaffold inicial es un servidor HTTP nativo, no una aplicación SSR + browser/WASM integrada.

## 1. Requisitos e instalación

Clona el repositorio, entra en su raíz y compila/instala el CLI. Necesitas Rust 1.85 o stable, Cargo y Git.

~~~sh
git clone https://github.com/programadorkevinsvega-sys/RustFlow.git
cd RustFlow
cargo install --path crates/rustflow-cli --locked
rustflow --help
~~~

No hay una versión publicada del CLI en crates.io. La aplicación creada fija la dependencia Git de RustFlow a la revisión fuente desde la que compilaste el CLI.

## 2. Crear el proyecto y revisar el scaffold

~~~sh
rustflow new hello-rustflow
cd hello-rustflow
find . -maxdepth 3 -type f | sort
~~~

La estructura principal contiene `Cargo.toml`, `README.md`, `.gitignore`, `.env.example`, `src/main.rs`, las carpetas `src/components`, `src/pages`, `src/controllers`, `src/models` y `migrations`. La página inicial se construye en `src/pages/home.rs`; el handler está en `src/controllers/home_controller.rs` y se registra desde `src/main.rs`.

El manifiesto depende directamente de `rustflow` y `rustflow-server` en un commit Git concreto, además de Tokio para el runtime. Esto evita que una misma aplicación siga silenciosamente una rama móvil con APIs diferentes.

## 3. Ejecutar la aplicación

~~~sh
rustflow dev
~~~

Abre <code>http://127.0.0.1:3000/</code>. Para cambiar el host o el puerto en desarrollo, copia `.env.example` a `.env` y edita `RUSTFLOW_HOST`/`RUSTFLOW_PORT`, o sobreescríbelos en el entorno del proceso. Lee la [guía de configuración](/guides/configuration/) antes de definir ajustes de producción.

La ruta raíz transforma la vista tipada de `src/pages/home.rs` en un documento HTML mediante `html_document`. El comando ejecuta `cargo run` una vez; no hay supervisión de archivos, reinicio automático ni hot reload. Después de editar código, detén y vuelve a ejecutar el comando.

## 4. Modificar la página con una API existente

Edita `src/pages/home.rs` y reemplaza su función por esta composición estructural:

~~~rust
use rustflow::view::{View, ViewError};

pub fn render() -> Result<View, ViewError> {
    View::element(
        "main",
        [
            View::element("h1", [View::text("Mi primera página")])?,
            View::element(
                "p",
                [View::text("Esta vista se serializa en el servidor.")],
            )?,
        ],
    )
}
~~~

El código usa `View::element` y `View::text`, APIs existentes. La salida no es una cadena HTML interpolada: el serializador SSR aplica su política de escape. El handler llama a `render`, por lo que no necesitas cambiar la ruta.

## 5. Ejecutar pruebas y compilar

~~~sh
cargo test
rustflow build
rustflow build --release
~~~

Las pruebas propias de la aplicación se ejecutan mediante Cargo. `rustflow build` delega a `cargo build`, transmite el resultado del compilador y devuelve su código de salida. Los artefactos siguen el layout normal de Cargo, normalmente `target/debug` o `target/release`.

Para validar el CLI desde el checkout de RustFlow, ejecuta `cargo test -p rustflow-cli`. La suite crea proyectos temporales, compila un proyecto generado usando dependencias locales y realiza una petición HTTP real.

## 6. Generar archivos

Desde la raíz de `hello-rustflow`:

~~~sh
rustflow generate component ProductCard
rustflow generate page about
rustflow generate controller health
rustflow generate model User
rustflow generate migration create_users
~~~

| Salida | Qué contiene |
|---|---|
| `src/components/product_card.rs` | Función que devuelve una `View` estructural, no un `#[component]` reactivo. |
| `src/pages/about.rs` | Función `render` para una página. |
| `src/controllers/health_controller.rs` | Handler asíncrono que devuelve texto. |
| `src/models/user.rs` | Struct de dominio sin ORM ni conexión de datos. |
| `migrations/0001_create_users.sql` | Plantilla SQL que no se ejecuta automáticamente. |

Los generadores Rust actualizan el área delimitada de los archivos `mod.rs` para declarar cada archivo en orden lexicográfico. Mantén el código personalizado fuera de esa sección. El CLI no cambia rutas, no registra controllers en la aplicación ni inserta componentes en una página automáticamente.

## 7. Recuperarse de una colisión

Vuelve a generar el mismo archivo:

~~~sh
rustflow generate component ProductCard
~~~

El comando informa que el destino ya existe y termina con error; su contenido queda intacto. Inspecciona el archivo, conserva los cambios que necesites y elige un nombre nuevo o resuelve la colisión manualmente.

El mismo principio aplica a `rustflow new`: si el destino ya existe, el CLI no lo limpia ni lo reutiliza. Elige un nombre disponible o cambia el directorio padre con `--path`, que debe existir.

Si un índice `mod.rs` perdió los marcadores gestionados o contiene código personalizado dentro de esa región, el generador se detiene. Inspecciona el archivo y restaura los marcadores o registra el módulo manualmente.

## 8. Continuar con casos de uso reales

- Para un componente reactivo con estado y eventos, sigue [Crear componentes](/guides/components/) y el [tutorial browser/WASM de tareas](/tutorials/reactive-todo/). El scaffold no crea una app de navegador.
- Para SSR con catálogo compartido y layouts, ejecuta `cargo run -p rustflow-server --example basic` desde la raíz del repositorio y sigue el [tutorial de página de producto](/tutorials/product-page/).
- Para rutas HTTP y JSON, ejecuta `cargo run -p rustflow-server --example basic` y sigue el [tutorial de API JSON](/tutorials/json-api/).

## Configuración por entornos

El archivo .env.example contiene únicamente valores no sensibles. Los archivos .env y .env.* se ignoran mediante .gitignore. En desarrollo/test, el proceso puede sobreescribir .env.<perfil> y .env; en producción, los archivos dotenv no se leen y RUSTFLOW_HOST/RUSTFLOW_PORT deben definirse en el entorno del proceso. Consulta la [guía de configuración](/guides/configuration/) para ver errores y precedencia.

## Límites

La configuración de la Fase 6.2 cubre solo perfil, host y puerto; no proporciona un sistema general ni un gestor de secretos. El scaffold del CLI no conecta automáticamente validación tipada, `CsrfProtection`, `SessionManager` ni guards de autenticación/autorización; esas APIs experimentales están disponibles por separado en las guías de [formularios](/guides/forms-validation/) y [sesiones](/guides/sessions-and-access/). Siguen pendientes server actions, ORM, conexiones de base de datos y ejecución de migraciones. Tampoco incluye watcher, hot reload, proxy de desarrollo o bundler server/browser. `use_memo`, `use_ref` y `use_context` siguen pendientes. Consulta la [referencia de API](/reference/api-status/) y el [roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md).
