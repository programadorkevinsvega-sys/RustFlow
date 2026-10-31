---
title: Escribir documentación
description: Normas de calidad para que las guías públicas permanezcan fieles al código de RustFlow.
---

La documentación pública es parte del producto y debe cambiar junto con la funcionalidad que describe. Una API que funciona pero solo se entiende al leer el código interno todavía no ofrece una buena experiencia de aprendizaje.

## Plantilla mínima de una guía

1. **Estado y propósito:** qué existe, qué problema resuelve y para qué casos conviene usarlo.
2. **Prerrequisitos:** qué crates, runtime o configuración necesita el lector y qué debe tener instalado.
3. **Ejemplo real:** un ejemplo completo o una ruta clara al archivo ejecutable. No presentes pseudocódigo como si compilara.
4. **Explicación del ejemplo:** explica qué hace cada bloque, por qué existe, cómo se relaciona con las APIs y qué resultado produce.
5. **Casos de uso:** ilustra al menos un caso común y, cuando corresponda, una variante o un caso límite.
6. **Errores frecuentes:** explica los fallos más probables, las reglas de ciclo de vida, el comportamiento ante entradas inválidas y los fallbacks.
7. **Seguridad y límites:** documenta qué datos son públicos, qué no se debe hacer y qué no está implementado.
8. **Pruebas:** indica el comando reproducible y qué verifica realmente.
9. **Enlaces relacionados:** conecta conceptos, guía de uso, referencia de API y fuente técnica.

## Reglas que no se deben omitir

- No inventar APIs para completar un tutorial; comprobar la firma real en el código y en pruebas.
- No presentar planes del roadmap como funciones disponibles.
- No copiar un ejemplo que dependa de una API inexistente o de una configuración omitida.
- Para un tutorial que se anuncia como ejecutable, enlazar un archivo que compile o añadir una prueba automatizada apropiada.
- Si una muestra es un fragmento y omite contexto (por ejemplo, imports o función contenedora), etiquetarla claramente como fragmento.
- Explicar los conceptos para alguien que no conoce el diseño interno: propósito, entrada, salida, resultado visible y razones de cada paso.
- Incluir casos de uso reales sin prometer capacidades no cubiertas, como persistencia, autenticación, autorización, CSRF o server actions.
- Diferenciar explícitamente SSR, HTML completo, hidratación y navegación cliente.
- Evitar afirmar que el HTML estructural es un sanitizador universal o que el estado de una petición se puede compartir entre solicitudes.
- Actualizar la barra lateral, la página de estado y los enlaces al añadir una guía nueva.
- Mantener sincronizados README, roadmap, docs técnicos, ejemplos y website cuando se refieran a la misma capacidad.

## Validación antes de fusionar

En el PR:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
~~~

Para cambios de website:

~~~sh
cd website
npm install
npm run check
npm run build
~~~

Si se modifica el ejemplo de navegador, ejecuta además las instrucciones Playwright de [examples/browser/README.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/examples/browser/README.md). Los ejemplos de servidor deben compilarse por el CI de Rust (Clippy con todos los targets) y, cuando sea posible, ejecutarse como test smoke.

## Auditoría integral del website público

Antes de cerrar una fase o un cambio documental que afecte al sitio, revisa el árbol completo `website/src/content/docs/`, no solo las páginas que parecen relacionadas a primera vista:

1. **Inventario:** enumera todos los archivos `.md` y `.mdx`; comprueba que cada guía, tutorial, referencia y página de verificación siga teniendo un propósito vigente.
2. **Contrato real:** verifica nombres, firmas, tipos, límites, errores y disponibilidad de cada API contra el código de la revisión objetivo y sus pruebas. Distingue implementado, experimental, limitado, pendiente y retirado.
3. **Ejemplos:** comprueba que los ejemplos anunciados como ejecutables tengan una ruta y un comando reales. Marca como fragmentos los snippets que omitan imports, contexto o configuración; nunca presentes pseudocódigo como programa compilable.
4. **Navegación:** comprueba que cada página esté enlazada desde el lugar adecuado y que las páginas nuevas/renombradas aparezcan en `website/astro.config.mjs`. Revisa rutas internas y anchors, enlaces a fuentes y referencias cruzadas.
5. **Consistencia:** contrasta el estado de fase, los hashes/PRs y las limitaciones con `ROADMAP.md`, `README.md`, `docs/`, ADRs y pruebas. Identifica expresamente como históricas las decisiones o contratos de fases anteriores que hayan cambiado después.
6. **Experiencia de aprendizaje:** revisa que el recorrido permita pasar de la instalación a un caso funcional y que cada guía explique propósito, uso, casos apropiados, fallos comunes, seguridad y límites.

## Mantenimiento de la fuente de verdad

El roadmap describe hitos y criterios de aceptación; las ADR registran decisiones y su estado de revisión; los documentos de <code>docs/</code> describen los contratos técnicos; el website enseña a utilizar la API. No deben sustituirse entre sí. Cuando una capacidad cambia, actualiza los documentos afectados y conserva las notas históricas claramente identificadas como históricas.

Una guía no se considera completa porque enumere nombres de funciones. Debe ayudar al lector a construir algo, entender por qué funciona, saber cómo validarlo y reconocer cuándo el ejemplo no es suficiente para su aplicación.
