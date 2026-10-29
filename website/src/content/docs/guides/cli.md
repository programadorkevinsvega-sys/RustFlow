---
title: CLI y generadores
description: Crea una aplicación de servidor RustFlow, ejecútala, compílala y genera archivos sin sobrescribir el trabajo existente.
---

El crate `rustflow-cli` proporciona el binario experimental `rustflow`. En esta fase crea proyectos de **servidor HTTP**, delega ejecución y compilación a Cargo y genera archivos estructurales. RustFlow es pre-1.0 y no publica todavía una versión estable en crates.io.

## Requisitos e instalación

Necesitas Git, Rust 1.85 o stable, Cargo y conexión de red para descargar la revisión Git fijada al crear el proyecto. El directorio padre pasado con `--path` debe existir.

Desde la raíz de un checkout de RustFlow:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow --help
rustflow --version
~~~

Para usar el binario sin instalarlo en el PATH, ejecuta `cargo run -p rustflow-cli -- --help`. No existe una publicación de crates.io que respalde `cargo install rustflow`.

## Crear una aplicación

~~~sh
rustflow new hello-rustflow
cd hello-rustflow
rustflow dev
~~~

Para elegir un directorio padre que ya exista:

~~~sh
rustflow new hello-rustflow --path /tmp
~~~

El destino no puede existir, sea archivo, directorio o enlace. El comando escribe en un directorio temporal y publica el proyecto solo después de escribir los archivos previstos. Si una escritura falla, intenta retirar el directorio temporal e informa la ruta del fallo.

La estructura principal es:

| Archivo | Responsabilidad |
|---|---|
| `Cargo.toml` | Manifiesto de aplicación y dependencias Git fijadas a la revisión fuente del CLI. |
| `src/main.rs` | Arranque del servidor HTTP y registro de la ruta raíz. |
| `src/pages/home.rs` | Vista estructural de la página inicial. |
| `src/controllers/home_controller.rs` | Handler que renderiza la página como documento HTML. |
| `src/components/mod.rs`, `src/pages/mod.rs`, `src/controllers/mod.rs`, `src/models/mod.rs` | Índices de módulos con una sección delimitada que administra el CLI. |
| `migrations/` | Directorio para plantillas SQL; no ejecuta migraciones. |

Abre <code>http://127.0.0.1:3000/</code>. La aplicación carga rustflow_server::AppConfig antes de escuchar. En desarrollo/test puedes copiar .env.example a .env; RUSTFLOW_HOST y RUSTFLOW_PORT controlan el bind. Producción exige ambas variables desde el entorno del proceso y no lee archivos dotenv. Consulta [Configuración por entornos](/guides/configuration/). La plantilla no incluye una app browser/WASM integrada ni crea automáticamente un bundle cliente.

## Desarrollo: `rustflow dev`

~~~sh
rustflow dev
rustflow dev --release
rustflow dev --offline
~~~

El comando ejecuta `cargo run` en el proyecto RustFlow más cercano y devuelve el código de salida de Cargo. Admite `--release`, `--offline` y `--locked`; puedes pasar argumentos de la aplicación tras `--`.

**No hay supervisión de archivos, reinicio automático, hot reload, preservación de estado ni proxy de desarrollo.** Cada invocación arranca Cargo una vez; después de editar código, detén y vuelve a ejecutar el comando.

## Build: `rustflow build`

~~~sh
rustflow build
rustflow build --release
rustflow build --target x86_64-unknown-linux-gnu
~~~

El comando delega a `cargo build`, transmite los mensajes del compilador y propaga su resultado. Acepta `--release`, `--offline`, `--locked`, `--target`, `--features`, `--all-features` y `--no-default-features`. Los artefactos siguen el layout habitual de Cargo, normalmente `target/debug` o `target/release`; Cargo informa la ruta efectiva.

El scaffold inicial es nativo. `rustflow build --target wasm32-unknown-unknown` no convierte ese servidor automáticamente en una aplicación browser. Para el flujo browser/WASM actual, sigue el [README del ejemplo de navegador](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md) y utiliza el build separado con wasm-pack/Vite.

## Generadores

La sintaxis es `rustflow generate <tipo> <nombre>`; también existe el alias `rustflow g`. Los nombres usan letras ASCII, números, guiones y guiones bajos, empiezan por una letra y se normalizan a `snake_case`.

| Comando | Destino | Resultado |
|---|---|---|
| `rustflow generate component ProductCard` | `src/components/product_card.rs` | Función reutilizable que construye una `View`; no es un componente reactivo `#[component]`. |
| `rustflow generate page about` | `src/pages/about.rs` | Función `render` para una página estructural. |
| `rustflow generate controller health` | `src/controllers/health_controller.rs` | Handler HTTP asíncrono que devuelve texto. |
| `rustflow generate model User` | `src/models/user.rs` | Struct de dominio, sin ORM. |
| `rustflow generate migration create_users` | `migrations/0001_create_users.sql` | Plantilla SQL que no se ejecuta. |

Para ver el proceso de principio a fin, sigue el [tutorial de primera aplicación](/tutorials/first-cli-app/). Las APIs reactivas de navegador se explican por separado en [Crear componentes](/guides/components/) y en el ejemplo `examples/browser/src/app.rs`.

### Registro de módulos y archivos existentes

Para componentes, páginas, controllers y modelos, el generador añade una declaración `pub mod` en orden lexicográfico dentro de la sección entre `rustflow:managed-modules:start` y `rustflow:managed-modules:end` del `mod.rs` generado. Conserva esos marcadores y mantén el código personalizado fuera de la sección. Si faltan marcadores, aparece contenido no reconocido dentro del segmento gestionado o hay una declaración conflictiva, el comando se detiene sin reemplazar el índice ni dejar el archivo nuevo.

Si un destino ya existe, el generador no lo sobrescribe. Inspecciona y conserva el archivo, elige otro nombre o resuelve la colisión de forma manual.

### Modelos y migraciones no son persistencia

El modelo generado es un tipo Rust ordinario. La migración es un archivo de texto; el CLI no ejecuta SQL ni valida el dialecto. RustFlow no incluye todavía pool de conexiones, ORM, transacciones ni runner de migraciones.

## Errores frecuentes

- **El destino de `rustflow new` existe:** el comando no lo vacía ni lo reutiliza. Escoge otro nombre/directorio o inspecciona y mueve manualmente el destino.
- **El archivo generado existe:** no se sobrescribe. Usa otro nombre o resuelve la colisión con conocimiento del contenido.
- **Faltan los marcadores de `mod.rs`:** inspecciona el archivo y restaura los marcadores, o registra manualmente el módulo. El CLI no reescribe un índice personalizado.
- **No se detecta un proyecto:** ejecuta el comando desde el proyecto o revisa que `Cargo.toml` declare directamente RustFlow.
- **Cargo falla:** el error del compilador se transmite; comprueba red, revisión fijada, target y toolchain.

## Pruebas y referencias

~~~sh
cargo test -p rustflow-cli
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

La suite de integración usa directorios temporales aislados, prueba nombres y conflictos, genera los cinco tipos, compila un proyecto con dependencias locales y realiza una petición HTTP. Revisa [la implementación](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/crates/rustflow-cli), la [nota técnica](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/CLI.md) y la [referencia de API](/reference/api-status/).

El scaffold no conecta automáticamente `AppConfig`, validación tipada, `CsrfProtection` ni `SessionManager`/guards. Estas APIs experimentales están disponibles por separado; consulta [Configuración](/guides/configuration/), [Formularios y validación](/guides/forms-validation/) y [Sesiones y control de acceso](/guides/sessions-and-access/). Siguen pendientes server actions y persistencia integrada (SQLx, transacciones y runner de migraciones). `use_memo`, `use_ref` y `use_context` continúan pendientes.
