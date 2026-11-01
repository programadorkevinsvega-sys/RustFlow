---
title: CLI y generadores
description: Crea una aplicación de servidor RustFlow, ejecútala, compílala y genera archivos sin sobrescribir el trabajo existente.
---

El crate `rustflow-cli` proporciona el binario experimental `rustflow`. `rustflow new` crea por defecto un proyecto de **servidor HTTP**; `--template crud` selecciona una aplicación SQL CRUD de demostración que compone ORM, validación, transacciones, guards, CSRF y SSR. El CLI delega ejecución y compilación a Cargo. RustFlow es pre-1.0 y no publica todavía una versión estable en crates.io.

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
| `migrations/` | Directorio para migraciones SQLx; `generate migration` crea un archivo, pero no lo ejecuta. |

Abre <code>http://127.0.0.1:3000/</code>. La aplicación carga rustflow_server::AppConfig antes de escuchar. En desarrollo/test puedes copiar .env.example a .env; RUSTFLOW_HOST y RUSTFLOW_PORT controlan el bind. Producción exige ambas variables desde el entorno del proceso y no lee archivos dotenv. Consulta [Configuración por entornos](/guides/configuration/). La plantilla no incluye una app browser/WASM integrada ni crea automáticamente un bundle cliente.

## Aplicación CRUD generada (`--template crud`)

La plantilla predeterminada `server` no cambia. Para generar el ejemplo CRUD:

~~~sh
rustflow new products --template crud
cd products
cp .env.example .env
# Reemplaza RUSTFLOW_DEMO_PASSWORD y RUSTFLOW_CSRF_KEY por valores locales.
rustflow dev
~~~

Abre `http://127.0.0.1:3000/login`. La cuenta de demostración se toma de `RUSTFLOW_DEMO_USER`/`RUSTFLOW_DEMO_PASSWORD`; tras iniciar sesión puedes listar, crear, abrir, editar y borrar productos, asignarles una categoría y filtrar la lista por nombre/categoría con páginas acotadas.

La estructura añade `src/models/category.rs` y `src/models/product.rs` (SeaORM `DeriveEntityModel` + RustFlow `ModelDiagnostics`), `src/bin/migrate.rs` y migraciones separadas para SQLite, PostgreSQL y MySQL/MariaDB. `Product` declara una relación `belongs_to Category`, y `Category` declara `has_many Product`. La aplicación elige el directorio según `DatabaseBackend`; la URL de conexión por sí sola no traduce SQL ni DDL.

### Qué demuestra

- `Request::validated_form` y `ValidateInput` para formularios y errores estructurados 422.
- `DatabasePool::entity`/`EntityRepository` para listar, insertar, buscar, actualizar y borrar registros con el ORM respaldado por el pool compartido.
- Relación `Product`–`Category` con migraciones específicas por dialecto. El detalle carga su categoría mediante el helper explícito `load_one`; la lista carga las categorías de la página en un lote, no con una consulta por producto.
- `Request::validated_query::<ProductListQuery>` aplica validación al filtro de texto, categoría y número de página. El query builder usa valores enlazados, orden determinista por ID y `limit/offset` con tamaño de página máximo fijo.
- `DatabasePool::begin_transaction` y rollback explícito cuando una segunda escritura viola el índice único SKU.
- `SessionManager`, `AuthIdentity`, `route_protected`, `route_authorized` y una `RolePolicy` deny-by-default.
- `CsrfProtection` conectado explícitamente a rutas POST; los formularios incluyen `_csrf` oculto.
- `Element`, `View` y `html_document` para SSR con escape del contenido de usuario.

### Límites

Es una demostración educativa, no un starter de producción. Usa `InMemorySessionStore` (volátil y de un solo proceso), `SessionCookieConfig::development()` y una sola cuenta demo configurada por entorno. Rechaza `RUSTFLOW_ENV=production`. No aporta proveedor de usuarios, sesión durable, OAuth/OIDC, MFA, rate limiting ni gestión de credenciales. Sustituye explícitamente esas piezas y configura cookies/CSRF/secretos apropiados para HTTPS antes de cualquier despliegue.

El generador `rustflow generate model <Name>` sigue produciendo un struct de dominio plano por compatibilidad. El nombre no permite inferir tabla, tipos, claves o relaciones; consulta `src/models/product.rs` para la declaración explícita de una entidad SeaORM.

La relación, el filtro y la paginación forman parte de la aplicación CRUD aceptada en Phase 6.7.6. Las pruebas HTTP de esos recorridos pasaron contra SQLite, PostgreSQL, MySQL y MariaDB en el HEAD final `b049d96491e2b23bfe794ac3c0ed9abb68f7e493`, fusionado mediante PR #66 como `6046ae7ca60843d145476177e698162b57939c68`. La aceptación de CI no cambia el propósito educativo de la plantilla: no es un starter de producción.

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

El modelo generado sigue siendo un tipo Rust ordinario; no es un ORM. La migración generada es un archivo SQL y no se ejecuta en el momento de generarla. El scaffold incluye un pool SQLx para SQLite/PostgreSQL, helpers de transacción y runner de migraciones. Revisa el dialecto y el esquema antes de aplicar SQL. En producción, ejecuta `cargo run --bin migrate` como un paso único de despliegue.

La Fase 6.6 está completa: driver MySQL/MariaDB, adaptador MongoDB y matriz pública de versiones verificadas están integrados en `main`; consulta la [matriz de versiones](/guides/database/#matriz-de-versiones-verificadas). Phase 6.7.2 añade el ORM SQL experimental de SeaORM; Phase 6.7.3, el ODM documental MongoDB independiente; y Phase 6.7.4, filtros/orden por relación, proyecciones has-many, grafos de carga explícitos y helpers de asociación/desasociación/reemplazo uno-a-uno. Las tres subfases están integradas en `main` y se describen en [Base de datos](/guides/database/) y [ODM MongoDB](/guides/mongodb-odm/). Estos adaptadores no cambian el comportamiento del generador: `model` sigue creando structs de dominio sin persistencia generada.

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

La suite de integración usa directorios temporales aislados, prueba nombres y conflictos, genera los cinco tipos, compila un proyecto con dependencias locales y realiza peticiones HTTP. La prueba de `--template crud` compila y ejecuta la aplicación generada sin reemplazar `src/main.rs`, y recorre ORM, validación, autorización, CSRF, rollback y SSR. Revisa [la implementación](https://github.com/programadorkevinsvega-sys/RustFlow/tree/main/crates/rustflow-cli), la [nota técnica](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/docs/CLI.md) y la [referencia de API](/reference/api-status/).

El scaffold integra `AppConfig`, un pool SQLx configurable y una comprobación `/health/database`; por defecto, tanto el `.env.example` como las migraciones generadas están orientados a SQLite. En desarrollo/test aplica migraciones al iniciar y en producción expone el binario `migrate` para un paso de despliegue controlado. Cambiar `RUSTFLOW_DATABASE_URL` a PostgreSQL/MySQL/MariaDB no traduce automáticamente SQL, placeholders, tipos o DDL; adapta y prueba las migraciones contra el motor elegido. MongoDB usa `RUSTFLOW_MONGODB_URI` y `RUSTFLOW_MONGODB_DATABASE`, además de `MongoConfig`/`MongoDatabase`; el scaffold no inicializa ese adaptador ni reutiliza migraciones SQL. Consulta [Configuración](/guides/configuration/), [Formularios y validación](/guides/forms-validation/), [Sesiones y control de acceso](/guides/sessions-and-access/) y [Base de datos](/guides/database/). La [matriz de compatibilidad](/guides/database/#matriz-de-versiones-verificadas) enumera los targets verificados de Phase 6.6.3. El ORM SQL y el ODM MongoDB están disponibles como APIs experimentales. La plantilla `server` no genera entidades; la plantilla `crud` incluye una entidad SeaORM concreta. Ninguna plantilla genera tipos `DocumentModel` ni inicializa MongoDB. Siguen pendientes server actions, un proveedor de usuarios y almacenamiento SQL durable para `SessionStore`; `InMemorySessionStore` sigue siendo local al proceso. `use_memo`, `use_ref` y `use_context` continúan pendientes.
