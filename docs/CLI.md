# CLI de RustFlow (Fase 6.1)

El paquete `rustflow-cli` publica el ejecutable experimental `rustflow`. RustFlow es pre-1.0 y todavía no hay una publicación estable de los crates en crates.io. El método disponible es instalar el CLI desde un checkout:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow --help
rustflow --version
~~~

## Revisión fuente

El build script obtiene el SHA completo del checkout Git (o de la variable explícita `RUSTFLOW_GIT_REV`). `rustflow new` exige una revisión hexadecimal de 40 caracteres y usa ese SHA en las dependencias Git de `rustflow` y `rustflow-server`. Así, una aplicación no sigue silenciosamente la rama `main`. Si el CLI se compiló sin metadatos Git ni SHA explícito, `new` falla con un mensaje accionable.

## Comandos

- `rustflow new <nombre> [--path <directorio-padre>]`: crea un scaffold de servidor HTTP. El padre debe existir y el destino no puede existir previamente.
- `rustflow dev [--release] [--offline] [--locked] [-- <args>]`: ejecuta una vez `cargo run`; los argumentos tras `--` se pasan al ejecutable.
- `rustflow build [--release] [--target <triple>] [--features <lista>] [--all-features] [--no-default-features] [--offline] [--locked]`: delega a `cargo build`.
- `rustflow generate <component|page|controller|model|migration> <nombre>`: crea un archivo nuevo protegido contra colisiones. `rustflow g` es un alias.

Los comandos de ejecución y build requieren estar dentro de un paquete RustFlow: un manifiesto `Cargo.toml` con metadata de scaffold o dependencia directa de RustFlow. El CLI busca el manifiesto ascendiendo desde el directorio actual. La plantilla inicial es nativa; seleccionar un target WASM no crea automáticamente una aplicación de navegador.

## Scaffolding

El scaffold contiene una ruta HTTP, una página construida con `rustflow::view::View`, un controller y los directorios de componentes/modelos/migraciones. El helper público `html_document` convierte la vista en un documento HTML. La aplicación carga `rustflow_server::AppConfig` antes de escuchar: en desarrollo/test lee opcionalmente `.env` y `.env.<perfil>`, y acepta `RUSTFLOW_HOST`/`RUSTFLOW_PORT`. Consulta [Configuración por entornos](CONFIGURATION.md) para la precedencia y el comportamiento seguro de producción.

Las dependencias Git del proyecto se fijan al commit usado por el CLI. La primera compilación requiere acceso a ese repositorio salvo que el checkout esté disponible en la caché de Cargo. No se promete compatibilidad de API más allá de esa revisión.

## Protección frente a colisiones

- El proyecto se escribe en una carpeta temporal y después se publica mediante rename; los destinos existentes se rechazan antes de escribir.
- Los archivos generados se crean con semántica `create_new`; un archivo, directorio o enlace existente no se sobrescribe.
- Para componentes, páginas, controllers y modelos, el generador actualiza únicamente el segmento entre los marcadores `rustflow:managed-modules:start/end` en los índices `mod.rs`. Mantiene fuera de esa región los comentarios y cambios del usuario y ordena las declaraciones lexicográficamente.
- Si los marcadores no existen, se duplican, hay contenido que el CLI no reconoce dentro del área administrada o ya existe una declaración conflictiva, la generación falla sin reemplazar el índice ni dejar el archivo fuente nuevo.
- Si falla la escritura de un archivo, el CLI intenta retirar el archivo parcial; si falla el registro del módulo, elimina el archivo recién creado.

La protección no intenta fusionar automáticamente índices cuyo contrato gestionado haya sido alterado. Inspecciona el archivo y resuelve manualmente el cambio antes de reintentar.

## Semántica de los generadores

| Tipo | Destino | Contrato |
|---|---|---|
| component | `src/components/<snake_case>.rs` | Función de render estructural que devuelve `Result<View, ViewError>`. No usa `#[component]` ni estado reactivo. |
| page | `src/pages/<snake_case>.rs` | Función `render` que devuelve una vista estructural. |
| controller | `src/controllers/<snake_case>_controller.rs` | Handler asíncrono con `Request` y `Result<Response, HttpError>`. |
| model | `src/models/<snake_case>.rs` | Struct Rust de dominio; no hay ORM. |
| migration | `migrations/NNNN_<snake_case>.sql` | SQL ilustrativo con secuencia incremental; no se ejecuta. |

Los números de migración se asignan como uno más que el mayor prefijo numérico de cuatro dígitos encontrado. Si ya existe una migración con el mismo nombre, el comando falla; el SQL debe adaptarse al motor antes de cualquier uso real.

## Verificación

~~~sh
cargo test -p rustflow-cli
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Las pruebas de integración usan directorios temporales. Cubren proyectos válidos, nombres/rutas inválidas, conflictos, cinco tipos generados, preservación de archivos existentes, determinismo, build/run y una petición HTTP a un proyecto generado. CI también instala el CLI y prueba `--help` y versión.

## Límites explícitos

La configuración de la Fase 6.2 solo cubre perfil, host y puerto; no es un sistema general ni almacena secretos. El CLI y el scaffold no conectan automáticamente validación tipada, `CsrfProtection`, `SessionManager` ni guards de autenticación/autorización; esas APIs experimentales están disponibles por separado en `rustflow-server` (consulta [FORM_VALIDATION.md](FORM_VALIDATION.md) y [ADR 0015](adr/0015-sessions-and-access-control.md)). La Fase 6.6 está completa: MySQL/MariaDB y el adaptador MongoDB se integraron, y la matriz de motores quedó publicada en [DATABASE_SUPPORT_MATRIX.md](DATABASE_SUPPORT_MATRIX.md). La Fase 6.7.2 integra el ORM SQL experimental de SeaORM sobre pools SQLx tipados compartidos; la 6.7.3 integra el ODM documental MongoDB independiente (consulta [DATABASES.md](DATABASES.md)). Sin embargo, `rustflow generate model` continúa generando un struct de dominio ordinario: no crea entidades SeaORM, implementaciones `DocumentModel`, repositorios ni un esquema persistente automáticamente. El scaffold sí incluye el pool SQLx y el runner de migraciones descritos en `DATABASES.md`, pero no genera por sí solo una aplicación CRUD completa con autenticación y autorización de negocio. Siguen pendientes server actions y almacenamiento SQL durable para `SessionStore`; `InMemorySessionStore` permanece volátil y local al proceso. No hay file watcher, hot reload, preservación de estado, proxy de desarrollo ni bundler integrado server/browser. `use_memo`, `use_ref` y `use_context` siguen pendientes.
