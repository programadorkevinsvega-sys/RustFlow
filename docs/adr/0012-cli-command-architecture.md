# ADR 0012: CLI y generadores para la Fase 6.1

- **Estado:** Propuesto para revisión
- **Fecha:** 2026-10-27
- **Alcance:** binario \`rustflow\`, scaffolding de servidor, comandos Cargo y generación de archivos
- **Relacionado:** \`ROADMAP.md\`, \`docs/CLI.md\`, \`docs/ENGINEERING_BASELINE.md\`

## Contexto

El workspace contiene crates de vistas, reactividad, SSR y servidor HTTP, pero no publica una plantilla instalable desde crates.io ni incluía un comando para iniciar aplicaciones. La Fase 6.1 debe cerrar esa brecha sin adelantar la configuración por entornos, la validación, las sesiones/autenticación o la persistencia de fases posteriores.

## Decisión propuesta

1. Introducir un crate independiente \`rustflow-cli\` con un binario \`rustflow\`, sin dependencia de un parser CLI para mantener pequeño el grafo.
2. Delegar build y ejecución a Cargo. El CLI valida su contrato, selecciona el paquete RustFlow y devuelve el código de salida informado por Cargo.
3. Hacer que \`rustflow new\` genere una aplicación de servidor que usa las firmas existentes de \`rustflow-server\` y vistas tipadas. La plantilla fija dependencias Git al SHA completo del checkout desde el que se compiló el CLI.
4. Mantener los cinco generadores como plantillas estructurales. Componentes y páginas devuelven \`View\`; el controller usa el contrato HTTP actual; los modelos son structs; las migraciones son SQL sin ejecutor.
5. Publicar solo archivos inexistentes. El scaffolding escribe en un directorio temporal y luego hace rename; los generadores usan creación exclusiva de archivo y actualizan solo el segmento delimitado de \`mod.rs\` que pertenece al CLI.
6. Rechazar un índice si faltan los marcadores o hay modificaciones no interpretables dentro del área administrada. No intentar una fusión heurística de código del usuario.
7. Mantener desarrollo como una invocación de \`cargo run\`; no anunciar file watching, hot reload o proxy hasta que exista una implementación y pruebas separadas.

## Alternativas consideradas

- **Dependencias a crates.io:** descartado porque RustFlow todavía no ha publicado estos crates.
- **Dependencias a rama main:** descartado porque una rama móvil hace que un mismo proyecto resuelva APIs distintas con el tiempo.
- **Copiar el framework dentro de cada app:** descartado porque duplica código y confunde el scaffold con una release.
- **Integrar un watcher/hot reload en esta fase:** descartado porque su semántica de reinicio y estado va más allá de 6.1.
- **Editar libremente cualquier \`mod.rs\`:** descartado porque la actualización podría pisar declaraciones o comentarios del usuario.

## Consecuencias

Ventajas: creación repetible, revisión fuente fijada, errores de colisión explícitos y límites claros. Costes: la primera compilación requiere acceso al repositorio Git o una caché; las actualizaciones de framework requieren decisión explícita; el desarrollador aún compone rutas y componentes.

## Verificación propuesta

La suite \`crates/rustflow-cli/tests/cli_integration.rs\` verifica scaffolding, colisiones, los cinco generadores, reproducibilidad de plantillas, build/run y una petición HTTP. CI debe probar Rust 1.85/stable, instalar el CLI y construir el website. La fase no se marca cerrada hasta que esas verificaciones pasen para el HEAD final de la PR.
