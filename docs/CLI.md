# CLI de RustFlow (Fase 6.1)

El paquete \`rustflow-cli\` publica el ejecutable experimental \`rustflow\`. RustFlow es pre-1.0 y todavía no hay una publicación estable de los crates en crates.io. El método disponible es instalar el CLI desde un checkout:

~~~sh
cargo install --path crates/rustflow-cli --locked
rustflow --help
rustflow --version
~~~

## Revisión fuente

El build script obtiene el SHA completo del checkout Git (o de la variable explícita \`RUSTFLOW_GIT_REV\`). \`rustflow new\` exige una revisión hexadecimal de 40 caracteres y usa ese SHA en las dependencias Git de \`rustflow\` y \`rustflow-server\`. Así, una aplicación no sigue silenciosamente la rama \`main\`. Si el CLI se compiló sin metadatos Git ni SHA explícito, \`new\` falla con un mensaje accionable.

## Comandos

- \`rustflow new <nombre> [--path <directorio-padre>]\`: crea un scaffold de servidor HTTP. El padre debe existir y el destino no puede existir previamente.
- \`rustflow dev [--release] [--offline] [--locked] [-- <args>]\`: ejecuta una vez \`cargo run\`; los argumentos tras \`--\` se pasan al ejecutable.
- \`rustflow build [--release] [--target <triple>] [--features <lista>] [--all-features] [--no-default-features] [--offline] [--locked]\`: delega a \`cargo build\`.
- \`rustflow generate <component|page|controller|model|migration> <nombre>\`: crea un archivo nuevo protegido contra colisiones. \`rustflow g\` es un alias.

Los comandos de ejecución y build requieren estar dentro de un paquete RustFlow: un manifiesto \`Cargo.toml\` con metadata de scaffold o dependencia directa de RustFlow. El CLI busca el manifiesto ascendiendo desde el directorio actual. La plantilla inicial es nativa; seleccionar un target WASM no crea automáticamente una aplicación de navegador.

## Scaffolding

El scaffold contiene una ruta HTTP, una página construida con \`rustflow::view::View\`, un controller y los directorios de componentes/modelos/migraciones. El helper público \`html_document\` convierte la vista en un documento HTML. \`RUSTFLOW_PORT\` permite elegir el puerto; si no se define, usa 3000.

Las dependencias Git del proyecto se fijan al commit usado por el CLI. La primera compilación requiere acceso a ese repositorio salvo que el checkout esté disponible en la caché de Cargo. No se promete compatibilidad de API más allá de esa revisión.

## Protección frente a colisiones

- El proyecto se escribe en una carpeta temporal y después se publica mediante rename; los destinos existentes se rechazan antes de escribir.
- Los archivos generados se crean con semántica \`create_new\`; un archivo, directorio o enlace existente no se sobrescribe.
- Para componentes, páginas, controllers y modelos, el generador actualiza únicamente el segmento entre los marcadores \`rustflow:managed-modules:start/end\` en los índices \`mod.rs\`. Mantiene fuera de esa región los comentarios y cambios del usuario y ordena las declaraciones lexicográficamente.
- Si los marcadores no existen, se duplican, hay contenido que el CLI no reconoce dentro del área administrada o ya existe una declaración conflictiva, la generación falla sin reemplazar el índice ni dejar el archivo fuente nuevo.
- Si falla la escritura de un archivo, el CLI intenta retirar el archivo parcial; si falla el registro del módulo, elimina el archivo recién creado.

La protección no intenta fusionar automáticamente índices cuyo contrato gestionado haya sido alterado. Inspecciona el archivo y resuelve manualmente el cambio antes de reintentar.

## Semántica de los generadores

| Tipo | Destino | Contrato |
|---|---|---|
| component | \`src/components/<snake_case>.rs\` | Función de render estructural que devuelve \`Result<View, ViewError>\`. No usa \`#[component]\` ni estado reactivo. |
| page | \`src/pages/<snake_case>.rs\` | Función \`render\` que devuelve una vista estructural. |
| controller | \`src/controllers/<snake_case>_controller.rs\` | Handler asíncrono con \`Request\` y \`Result<Response, HttpError>\`. |
| model | \`src/models/<snake_case>.rs\` | Struct Rust de dominio; no hay ORM. |
| migration | \`migrations/NNNN_<snake_case>.sql\` | SQL ilustrativo con secuencia incremental; no se ejecuta. |

Los números de migración se asignan como uno más que el mayor prefijo numérico de cuatro dígitos encontrado. Si ya existe una migración con el mismo nombre, el comando falla; el SQL debe adaptarse al motor antes de cualquier uso real.

## Verificación

~~~sh
cargo test -p rustflow-cli
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Las pruebas de integración usan directorios temporales. Cubren proyectos válidos, nombres/rutas inválidas, conflictos, cinco tipos generados, preservación de archivos existentes, determinismo, build/run y una petición HTTP a un proyecto generado. CI también instala el CLI y prueba \`--help\` y versión.

## Límites explícitos

No implementa configuración completa por entornos, validación de formularios, sesiones, autenticación/autorización, CSRF, server actions, base de datos, ORM ni runner de migraciones. No hay file watcher, hot reload, preservación de estado, proxy de desarrollo ni bundler integrado server/browser. \`use_memo\`, \`use_ref\` y \`use_context\` siguen pendientes.
