# ADR 0013: Contrato de configuración por entornos (Fase 6.2)

- **Estado:** Propuesto para revisión
- **Fecha:** 2026-10-27
- **Alcance:** perfil de ejecución y dirección de bind del servidor generado
- **Relacionado:** ROADMAP.md, docs/CONFIGURATION.md, crates/rustflow-server/src/config.rs

## Contexto

La Fase 6.1 ya permite generar un servidor nativo. La aplicación necesita una forma determinista de seleccionar ajustes para desarrollo, pruebas y producción sin añadir un sistema de configuración general, esconder fallos ni normalizar la inclusión de secretos en el repositorio.

## Decisión propuesta

1. Exponer AppConfig::load() y AppConfig::socket_addr() desde rustflow-server, la frontera de composición del servidor. La configuración no se introduce en el runtime reactivo ni en los componentes de navegador.
2. Admitir únicamente RUSTFLOW_ENV, RUSTFLOW_HOST y RUSTFLOW_PORT en esta fase. Validar el perfil permitido, una dirección IP literal y un puerto entre 1 y 65535.
3. En desarrollo/test, resolver cada valor con precedencia proceso > .env.<perfil> > .env > defaults. Leer archivos como datos sin mutar el entorno global ni expandir comandos/variables.
4. En producción, no leer archivos dotenv. Exigir que el perfil se seleccione desde el entorno de proceso y que host/puerto estén presentes; fallar antes de escuchar si la configuración es incompleta o inválida.
5. Generar un .env.example sin valores secretos y un .gitignore que excluya .env/.env.* pero mantenga versionado el ejemplo.
6. Mantener la dependencia solo en la biblioteca estándar. No se agrega un gestor de configuración o secretos antes de existir una necesidad funcional concreta.

## Alternativas consideradas

- Modificar variables globales con un cargador dotenv: descartado porque produce efectos laterales ocultos y dificulta las pruebas deterministas.
- Leer archivos dotenv en producción: descartado para no convertir accidentalmente un archivo local en la configuración autoritativa de despliegue.
- Añadir ahora TOML/YAML o un framework de configuración externo: descartado porque la fase necesita solo tres variables de arranque y más dependencias ampliarían el contrato sin un caso real.
- Incluir placeholders de credenciales en el ejemplo: descartado porque normaliza valores copiables y la aplicación generada todavía no consume secretos.

## Consecuencias y límites

El arranque falla pronto ante errores en el perfil, host o puerto. La precedencia es reproducible y la lectura de configuración puede probarse con directorios temporales sin alterar variables globales. Como coste, host acepta direcciones IP literales, no DNS; producción necesita configuración del proceso y esta API no proporciona almacenamiento/rotación de secretos.

## Verificación prevista

Los tests cubren defaults, precedencia, perfil test, configuración de producción, rechazo de entradas inválidas y sintaxis dotenv. El scaffold debe compilar y responder a una petición HTTP real. La PR debe pasar Rust 1.85/stable, Clippy, tests, rustdoc, target WASM, Browser E2E/bundle size, construcción del website, gobernanza y revisión de dependencias antes de cerrar formalmente la fase.
