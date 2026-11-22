# Roadmap de brechas verificadas de RustFlow

**Estado:** propuestas y entregas verificadas. Este documento registra qué subfases ya están integradas y cuáles siguen propuestas o pendientes de aprobación.

**Base de contraste:** checkout sincronizado con `origin/main`, revisado el 2026-11-20.

**Regla de entrega:** una subfase por rama y PR, salvo la excepción aprobada para agrupar 2.8–2.12 en `feat/2-8-durable-sql-sessions`. Las cinco subfases están implementadas y verificadas localmente; se publicaron en una sola PR (#111) con un commit y aún no están fusionadas. Cada entrega debe tener criterios de aceptación verificables y evidencia de sus pruebas.

## Estado de entrega

| Subfase | Estado | Evidencia integrada |
|---|---|---|
| 1.1 Sincronizar dependencias, rendimiento y fases | Integrada | PR #88, merge `abcbf46f3541fba9a8a232b6d990c983c140bc60` |
| 1.2 CRUD y reactividad en guías | Integrada | PR #89, merge `542daf0df87921b4849a90b670fe6080f50d6857` |
| 1.3 Numeración, conteos y navegación | Integrada | PR #91, merge `a2d4ae53794bda54575aa1f56cf7716083db8728` |
| 1.4 Tipografía inteligente en código | Integrada | PR #92, merge `9455cd6ed716864de8214eb617619178157fc095` |
| 1.5 Tutorial JSON validado | Integrada | PR #93, merge `e50e3d5d2a7b13e24e38c36b840ece73cd4cfd5e` |
| 1.6 Instalación desde checkout | Integrada | PR #94, merge `36aca362f3b2702dc0a6d354b339f2e840f40c6d` |
| 1.7 Limitación MongoDB SRV | Integrada | PR #95, merge `8f6d4bf1df13cd3a3f33438ba5f2fea11b8d4047` |
| 1.8 Matriz de capacidades, límites e historial | Integrada | PR #99, merge `84c73ab` |
| 1.9 About y excepción de gobernanza | Integrada | PR #100, merge `1c80b21d818e469d72e420ce3d4c6400aede589f`; conserva los archivos de gobernanza en raíz y verifica Description, Website y topics |
| Fase 2 | En curso | 2.7 integrada por PR #110; 2.8–2.12 implementadas y verificadas localmente como excepción de agrupación aprobada; PR #111 abierta con un commit, aún sin fusionar |

### Entregas adicionales posteriores al roadmap

- PR #96: CI diferencia PR Draft de Ready para revisar y la validación final antes de fusionar. Merge `19f0486caa64a4d2790d5d39aac94bd0d257bbfb`.
- PR #97: target compartido para el CLI, benchmark manual/programado/por comentario, aislamiento de migraciones de prueba y eliminación de dependencias WASM mal ubicadas. Merge `e5d00e8d9761d5faa00f9905525432e4473eb381`.

Las marcas Integrada requieren que el PR haya sido fusionado y que sus criterios hayan quedado respaldados por código, documentación o evidencia de CI. Las mejoras posteriores de CI son entregas adicionales y no reabren ni cierran por sí solas las subfases 1.8 o 1.9.

## 1. Alcance y reglas de ejecución

- Las subfases de la Fase 1 son correcciones documentales y de presentación de bajo riesgo. Se pueden ejecutar en PRs pequeños.
- Las Fases 2 y 3 son propuestas. No empezar su implementación ni publicar cambios hasta que el autor apruebe el alcance de cada fase o subfase.
- No afirmar capacidades, compatibilidad, seguridad, rendimiento ni preparación para producción sin código, pruebas o mediciones que lo respalden.
- Mantener documentación pública en español. Los ejemplos y comandos deben coincidir con la API disponible.
- No mover `AGENTS.md` ni `ARCHITECTURE_RULES.md` fuera de la raíz: el [workflow de gobernanza](../.github/workflows/governance.yml) los exige allí. Cualquier cambio futuro a esa política debe ser un PR independiente y aprobado.
- Este roadmap no establece un límite de líneas por archivo. El tamaño y la división de módulos se podrán revisar como trabajo separado antes de una eventual preparación para producción.
- No publicar en crates.io, cambiar el nombre del proyecto ni anunciar compatibilidad o producción sin aprobación explícita.

### Criterios de pruebas compartidos

Cada subfase debe registrar pruebas acordes a su alcance:

- **Solo documentación o metadatos:** `cd website && npm run build` cuando cambien páginas, navegación o configuración del sitio; comprobar enlaces y fragmentos de código modificados.
- **Código Rust:** pruebas unitarias, de integración y de fallos pertinentes al contrato; cubrir límites, entradas inválidas y vacías, y Unicode cuando se acepten cadenas. Ejecutar formato, Clippy y pruebas de los crates afectados.
- **Navegador/WASM:** compilar el ejemplo con la API documentada y ejecutar E2E para interacciones, identidad DOM y limpieza cuando el cambio afecte esas conductas.
- **Servidor:** añadir pruebas HTTP/E2E para éxito, rechazo, autorización y límites. Añadir carga k6 cuando el cambio afecte capacidad, concurrencia o latencia de una ruta; documentar entorno y umbrales reproducibles.
- **Cierre de fase:** CI verde para el HEAD del PR y checks de las fases previas aplicables. No sustituir pruebas funcionales por compilación solamente.

Comandos de referencia del workspace, cuando sean aplicables:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cd website && npm run build
```

El PR debe incluir un checklist de aceptación marcado criterio por criterio, con comandos exactos, resultado observado y enlaces a las pruebas o al run de CI. No declarar un test pasado si no se ejecutó y observó.

## 2. Estado verificado que condiciona el plan

- Las subfases 1.1–1.7 están integradas en `main`; sus PRs se indican en la matriz anterior.
- La comprobación automática de tipografía inteligente está integrada junto con sus tests y CI.
- `validated_json` y las respuestas 422 existen en el servidor y el tutorial JSON las distingue del endpoint de eco.
- La documentación de instalación adelanta la instalación desde checkout y muestra `cargo install --path crates/rustflow-cli --locked`.
- La guía MongoDB presenta al inicio la limitación SRV y la alternativa `mongodb://`.
- La matriz rápida existe dentro de `project-status`; no reemplaza una tabla única de límites ni elimina el historial extenso de esa página.
- La actualización SQLite/dependencias está integrada en `main` y verificada en los HEADs de PR #84/#85.
- El workflow oficial de comparación de frameworks terminó correctamente el 2026-11-20; su paso de validación confirmó un informe completo y válido y subió un artifact ([run](https://github.com/programadorkevinsvega-sys/RustFlow/actions/runs/44677034080)). Esto corrige la afirmación de que no existe ninguna ejecución oficial aceptada. **No cierra Phase 8.1:** siguen pendientes otros criterios, incluida evidencia A/B/A de escenarios de 10.000 filas y revisión de resultados. No se afirma superioridad competitiva sin revisar datos y limitaciones.
- La auditoría externa no examinó el código. Las propuestas funcionales de la Fase 2 deben volver a verificarse contra código, pruebas y dependencias antes de implementarse.

## 3. Fase 1 - Correcciones documentales y de repositorio

### 1.1 Sincronizar estado de dependencias, rendimiento y fases

**Estado actual:** integrada mediante PR #88.

**Cubre:** A5, B9 y afirmaciones afectadas por PRs ya integrados.
**Un PR:** actualizar estado público y enlaces obsoletos en README, `project-status`, guía de rendimiento, referencia de API y documentación de base de datos.

**Aceptación**

- Los textos de actualización SQL/SQLite coinciden con `Cargo.toml`, `Cargo.lock`, `.cargo/audit.toml` y la evidencia de CI actual.
- Se retiran enlaces a ramas temporales como evidencia vigente.
- La guía separa claramente: evidencia interna A/B/A, ejecución oficial aceptada, evidencia histórica rechazada y trabajo que sigue pendiente.
- Se identifica la ejecución oficial completa y sus limitaciones; no se publican razones de rendimiento ni comparaciones ganadoras sin revisar el artifact.
- Phase 8.1 continúa abierta mientras queden criterios de su roadmap sin evidencia aceptada.

**Pruebas/evidencia:** revisar el run oficial y su artifact; `cd website && npm run build`; buscar enlaces a ramas eliminadas y afirmaciones de validación pendientes que contradigan el estado actual.

### 1.2 Corregir plantilla CRUD y reactividad en las guías

**Estado actual:** integrada mediante PR #89.

**Cubre:** A1 y A2.
**Un PR:** corregir las tres afirmaciones obsoletas de CRUD y las contradicciones sobre contexto y `ElementRef`.

**Aceptación**

- Se distingue que `server` crea un servidor base y `crud` (`rustflow new products --template crud`) crea una demo CRUD SQL.
- Las páginas de errores/seguridad, servidor y tutorial de tareas ya no niegan la capacidad que documentan otras guías.
- La guía de reactividad explica que existe contexto tipado, sin insinuar un provider global ni garantías no implementadas.
- `ElementRef` tiene un único estado coherente en los conceptos, el estado del proyecto y la referencia de API.

**Pruebas/evidencia:** confirmar los templates y exports en código; comprobar los ejemplos públicos existentes; `cd website && npm run build`.

### 1.3 Corregir numeración, conteos y labels de navegación

**Estado actual:** integrada mediante PR #91.

**Cubre:** A3 y A10.
**Un PR:** corregir la numeración en testing/configuración, el conteo del tutorial de producto y el nombre de la guía de base de datos en la navegación.

**Aceptación**

- Las secciones numeradas son secuenciales y no repiten números.
- El texto del tutorial coincide con el número de responsabilidades de su tabla.
- El título y el label de navegación de la guía de base de datos usan el mismo nombre.

**Pruebas/evidencia:** inspección de títulos y tabla; `cd website && npm run build`.

### 1.4 Proteger ejemplos de código frente a tipografía inteligente

**Estado actual:** integrada mediante PR #92.

**Cubre:** A4.
**Un PR:** añadir una comprobación automatizada acotada al contenido de código Markdown/MDX.

**Aceptación**

- Se revisan y corrigen los ejemplos concretos indicados en el encargo.
- La comprobación detecta raya larga y comillas curvas dentro de bloques y spans de código, pero no rechaza esos caracteres en prosa normal.
- Hay pruebas de la comprobación para casos válidos e inválidos y una ejecución en CI.
- Los comandos copiados conservan `--` ASCII.

**Pruebas/evidencia:** ejecutar los tests del checker, demostrar que los casos de código con caracteres prohibidos fallan y que la prosa tipográfica pasa; `cd website && npm run build`.

### 1.5 Mejorar el tutorial de API JSON sin ocultar el ejemplo de eco

**Estado actual:** integrada mediante PR #93.

**Cubre:** A6.
**Un PR:** ampliar el tutorial para mostrar validación con `validated_json`/`ValidateInput` y el error 422, conservando explícitamente la distinción con un endpoint de eco si sigue siendo didáctico.

**Aceptación**

- El ejemplo validado compila con la API pública documentada.
- Un cuerpo válido produce la respuesta esperada y uno inválido produce 422 con una forma comprobada por test.
- Se cubren cuerpo vacío, formato JSON inválido y errores de validación pertinentes.
- El texto diferencia parsing/eco de validación de negocio.

**Pruebas/evidencia:** test de integración HTTP para éxito y 422; tests de errores de parsing aplicables; ejecutar el test del crate afectado y `cd website && npm run build`.

### 1.6 Adelantar los requisitos de instalación

**Estado actual:** integrada mediante PR #94.

**Cubre:** A8.
**Un PR:** reordenar el comienzo de `getting-started/setup` para decir que hoy se instala desde checkout y mostrar el comando real.

**Aceptación**

- La primera sección indica que no hay release estable en crates.io.
- El comando documentado coincide con el manifiesto y la ruta real del CLI.
- No se promete una instalación desde crates.io.

**Pruebas/evidencia:** comprobar la ruta del crate y el comando desde un checkout; `cd website && npm run build`.

### 1.7 Hacer visible la limitación de MongoDB SRV

**Estado actual:** integrada mediante PR #95.

**Cubre:** A9.
**Un PR:** poner al inicio de la guía la limitación verificada, su motivo técnico/de seguridad actual y la alternativa admitida.

**Aceptación**

- El texto y los ejemplos reflejan la configuración real del driver y sus features.
- Se presenta `mongodb://` con hosts explícitos como alternativa solo si está respaldada por la API.
- Se enlaza al análisis B6 de este roadmap sin prometer que SRV será habilitado.

**Pruebas/evidencia:** inspeccionar feature flags/dependencias y pruebas de configuración; `cd website && npm run build`.

### 1.8 Unificar matriz de capacidades, límites e historial

**Estado actual:** integrada en el contenido de PR #99; pendiente de fusión.

**Cubre:** A7 y A11.
**Un PR:** subir la matriz rápida, consolidar los límites solicitados y separar el historial de fases de la explicación de capacidades actuales.

**Aceptación**

- La matriz rápida aparece cerca del inicio de `project-status`.
- Hay una tabla única de límites, con enlaces a explicación y a la subfase correspondiente de este roadmap.
- Los hashes/PRs que no ayuden a decidir qué se puede usar se trasladan a un historial apropiado o a docs internas.
- README y portada priorizan capacidades actuales y límites relevantes, no cronología interna repetida.

**Pruebas/evidencia:** revisar que cada fila enlaza a contenido existente; `cd website && npm run build`; comprobar navegación y enlaces.

### 1.9 Actualizar About y documentar la excepción de gobernanza

**Estado actual:** integrada mediante PR #100 y actualización verificable de metadatos.

**Cubre:** A12 y A13.
**Un PR de documentación** para registrar la decisión; el cambio de About/topics es una operación del mantenedor fuera del build del sitio.

**Aceptación**

- No se mueven `AGENTS.md` ni `ARCHITECTURE_RULES.md` mientras el workflow de gobernanza los requiera en raíz.
- Si se decide cambiar esa política, primero se aprueba y prueba un PR independiente de gobernanza.
- El About usa una descripción que declara pre-1.0; Website y topics se actualizan con los valores aprobados por el autor.
- El cambio manual de GitHub se verifica mediante la API y se registra sin fingir que lo hizo una CI de código.

**Evidencia actual:**

- About: `Framework web full-stack en Rust: reactividad, SSR, servidor y CLI inspirados en Laravel (pre-1.0).`
- Website: `https://rustflow.pages.dev`
- Topics: `rust`, `web-framework`, `wasm`, `ssr`, `axum`, `orm`
- Gobernanza: `AGENTS.md` y `ARCHITECTURE_RULES.md` permanecen en raíz porque el workflow los exige; no se moverán sin un PR independiente que pruebe y aprueba la política.

**Pruebas/evidencia:** comprobar el workflow de gobernanza; verificar About, Website y topics en GitHub después de la operación.

## 4. Fase 2 - Brechas funcionales propuestas

**Gate:** cada subfase requiere aprobación explícita, análisis de arquitectura/seguridad y pruebas del contrato nuevo. El autor aprobó agrupar 2.8–2.12 en `feat/2-8-durable-sql-sessions`; están completadas y verificadas localmente y publicadas en la PR #111, aún sin fusionar. Las subfases de servidor incluyen carga k6 cuando el comportamiento de carga sea un criterio relevante.

### 2.1 Decidir contrato de estilos y archivos estáticos

**Estado actual:** integrada mediante PR #102, merge `bfc795af3214d34e108f7df59e0d891ddf60395b`; ADR 0027 conserva el rechazo de `style` y deja la implementación de `StaticFiles` para 2.2.

**Cubre:** B1, decisión previa.
**Un PR:** ADR que compare mantener el rechazo de `style` con permitir estilos validados y defina el contrato de archivos estáticos.

**Aceptación**

- Decisión explícita sobre `style`; no se debilita la política actual sin análisis de sanitización y XSS.
- Se define si `StaticFiles` pertenece a un adapter del servidor y los límites de tamaño, MIME, caché y traversal.
- Si se conserva el rechazo de `style`, se documenta una alternativa compatible; si se permite, se define y prueba el contrato seguro.
- Se documentan amenazas, configuración, errores y comportamiento de rutas.

**Pruebas/evidencia:** revisión de seguridad y ADR aprobado; no requiere k6 porque no implementa el servidor.

### 2.2 Implementar servidor de archivos estáticos

**Estado actual:** implementación incluida en PR #104; pendiente de revisión y CI. Incluye Range, ETag/conditional requests y SPA fallback dentro de 2.2. Range usa seek y lee solo el segmento solicitado, pero el streaming incremental completo queda fuera del alcance inicial.

**Cubre:** B1, sujeto a 2.1.
**Un PR:** implementar adapter con el contrato aprobado.

**Aceptación**

- Path traversal, symlinks según política, rutas absolutas, archivos ausentes, MIME, límites y caché se prueban explícitamente.
- No se filtran rutas locales ni se sirve contenido fuera del directorio configurado.
- Los controles de `style` siguen la decisión del ADR.

**Pruebas/evidencia:** unitarias, HTTP/integración y seguridad; E2E de acceso si aplica; k6 con límites reproducibles para servir archivos.

### 2.3 Añadir flujo de desarrollo con watcher y build WASM

**Estado actual:** PR #105 está fusionada en `main`. Sus checks de CI de Rust 1.94/stable, WebAssembly, Browser E2E/bundle, documentación, gobernanza, dependencias y Pages pasaron; los jobs de stress cross-browser y performance tooling quedaron omitidos. El watcher implementa rebuild/restart, `--once`, cancelación y reconstrucción WASM opt-in. La aceptación de recuperación tras build fallido y supervisión completa de procesos descendientes aún requiere evidencia focalizada; el merge no sustituye esas pruebas.

**Cubre:** B2.
**Un PR:** diseñar e implementar `rustflow dev` con observación, cancelación y reconstrucción.

**Aceptación**

- Cambios relevantes reinician/reconstruyen sin procesos huérfanos ni estados de éxito falsos.
- Los errores de build se muestran y una corrección permite continuar.
- Se documentan herramientas WASM requeridas, alcance y limitaciones.

**Pruebas/evidencia:** tests de lógica del watcher/proceso, integración de CLI y E2E de desarrollo; verificar cancelación, archivo eliminado/renombrado y fallo de build.

### 2.4 Implementar server actions/functions

**Estado actual:** contrato JSON inicial aprobado en ADR 0029 e implementación experimental local completa en `feat/2-4-server-actions`. El primer alcance es `POST` JSON tipado, sin formularios progresivos ni proxy cliente. Pasaron las pruebas focalizadas, doctests/compile-fail, integración HTTP, formato, Clippy y rustdoc del workspace. La suite completa del workspace también pasó en serie, con los targets de proyectos generados del CLI en almacenamiento de disco; el primer intento paralelo agotó el tmpfs de `/tmp` y no se toma como resultado de tests de producto. El build público Astro pasó (33 páginas; mantiene las advertencias existentes de la colección `i18n` vacía y la referencia `docs → 404`).

**Carga local:** `k6 run scripts/server-actions.k6.js` pasó con 10 VUs durante 15 s: 2.920 solicitudes, cero fallos, checks al 100 % y p95 de 1,04 ms. Es una prueba smoke del ejemplo local, no una garantía de capacidad de producción.

**Pendiente:** abrir y validar el PR con CI requerido y fusionarlo. La subfase no se cierra antes de que los checks aplicables estén verdes y la implementación esté integrada.

**Cubre:** B3.
**Un PR:** ADR y primera implementación/contrato sobre las protecciones existentes.

**Aceptación**

- Cada acción aplica validación en servidor, autorización/guards, límites de request y CSRF cuando corresponda.
- No se confía en estado serializado ni validación del cliente.
- Se definen errores, serialización, cancelación y límites de datos.

**Pruebas/evidencia:** unitarias, HTTP/E2E para éxito, 422, CSRF, no autenticado/no autorizado, request excesivo y errores; k6 para cargas previstas.

### 2.5 Diseñar y añadir recursos asíncronos

**Cubre:** B4, recursos y seguimiento de async.
**Un PR:** fijar primero semántica y después implementar una API pequeña, sin asumir seguimiento reactivo a través de `await`.

**Aceptación**

- Estados, cancelación, resultados obsoletos, errores, SSR e hidratación están definidos.
- Se define si los recursos incluyen Suspense/loading boundaries y cómo se comportan sus fallbacks.
- Se documentan restricciones del runtime de un hilo.
- No se incorpora scheduler/hook-cell global implícito.

**Pruebas/evidencia:** tests deterministas de estados, cancelación, carreras y cleanup; integración SSR/hidratación si se incluye en el alcance.

**Estado actual:** implementado y fusionado mediante la PR #108, commit `9106416c`.

### 2.6 Añadir límites de error UI

**Cubre:** B4, `ErrorBoundary`.
**Un PR:** aislar la captura y presentación de errores de componentes bajo contrato explícito.

**Aceptación**

- Se especifica qué errores captura y cuáles propaga.
- Unmount elimina efectos y listeners; no se ocultan errores del servidor.
- La salida es segura y accesible.

**Pruebas/evidencia:** unitarias y browser E2E para fallo, recuperación, anidamiento y cleanup.

**Estado actual:** implementación y ADR 0031 en revisión en la PR #109.

### 2.7 Definir router unificado

**Estado actual:** ADR 0032 integrada mediante PR #110; define la separación entre navegación cliente y rutas HTTP. No implementa un router unificado.

**Cubre:** B4, router.
**Un PR:** ADR de alcance y compatibilidad entre navegación cliente y rutas HTTP; implementación solo con aprobación posterior.

**Aceptación**

- Contrato de rutas, parámetros, fallback, SSR, navegación, errores y seguridad definido.
- Se explicita si el router se integra con los adaptadores actuales o mantiene límites separados.

**Pruebas/evidencia:** test de matching y límites; E2E de navegación/historial y pruebas de rutas servidoras para cualquier implementación.

### 2.8 Añadir sesiones durables

**Estado actual:** implementación y verificación local completas en `feat/2-8-durable-sql-sessions`; no se ha creado ni fusionado una PR. ADR 0033 define el port async y el adapter SQL; `InMemorySessionStore` permanece disponible. Pasaron los tests SQLite de expiración/rotación/invalidation, colisión, fallo de persistencia y rotaciones concurrentes; `session_store_contract` pasó contra servicios aislados PostgreSQL 16.15, MySQL 8.4.13 y MariaDB 11.4.13 (3/3). También pasaron la suite del servidor con los tres servicios y el test aislado de migraciones PostgreSQL repetibles y reversibles.

**Impacto en documentación pública:** cumplido en la guía de sesiones, referencia API, estado del proyecto y README; el build estático de documentación pasó con 33 páginas.

**Cubre:** B5, almacenamiento de sesiones.
**Un PR:** port de almacenamiento y primer adapter SQL, sin mezclarlo con un proveedor de usuarios.

**Aceptación**

- Expiración, rotación, invalidación, concurrencia y fallos de persistencia tienen contrato.
- Las transacciones y consultas son parametrizadas; los tokens no se registran.
- `InMemorySessionStore` sigue disponible para pruebas.

**Pruebas/evidencia:** unitarias, contrato de adapter, integración con base real/aislada, expiración e invalidación; k6 si el almacenamiento forma parte de una ruta de alta concurrencia.

### 2.9 Definir proveedor de usuarios

**Estado actual:** implementación y verificación local completas en `feat/2-8-durable-sql-sessions`; no se ha creado ni fusionado una PR. Los ports `UserProvider`, `Credentials`, `login` y `logout` están definidos sin OAuth/OIDC/MFA. El ejemplo autenticado y las pruebas cubren credenciales correctas/incorrectas, CSRF, guard/autorización y logout. Pasaron la suite del servidor y las pruebas de integración CLI; la revisión de seguridad independiente no encontró vulnerabilidades en el alcance revisado.

**Impacto en documentación pública:** cumplido en la guía de sesiones/autenticación, referencia API, estado del proyecto y README; `StaticUserProvider` se identifica como demostración y el build estático pasó.

**Cubre:** B5, identidad.
**Un PR:** definir puertos y un flujo inicial, sin incluir OAuth/OIDC/MFA salvo aprobación separada.

**Aceptación**

- Autenticación y autorización permanecen separadas y deny-by-default.
- Las credenciales y errores no se filtran ni se registran.
- La aplicación puede integrar usuarios sin acoplar dominio a una base concreta.

**Pruebas/evidencia:** unitarias, integración de login/logout y autorización, entradas inválidas y límites; revisión de seguridad.

### 2.10 Añadir rate limiting

**Estado actual:** implementación y verificación local completas en `feat/2-8-durable-sql-sessions`. El port y middleware aplican ventanas fijas por IP confiable y scope estático; el adapter aprobado para esta primera entrega es memoria acotada, atómica por proceso y volátil. El escenario k6 de 10 VUs/15 s pasó: checks 100%, errores HTTP 0%, p95 1.01 ms y 12,760 respuestas 429 esperadas. La suite del servidor pasó y la revisión de seguridad independiente no encontró vulnerabilidades en el alcance revisado. No implica límites compartidos entre instancias ni preparación para producción.

**Impacto en documentación pública:** cumplido en la guía de seguridad, referencia API, README y estado del proyecto, incluidos confianza de proxies, respuestas 429/503, límites del store en memoria y comando reproducible k6.

**Cubre:** B5, rate limiting.
**Un PR:** port/política acotada para límites por IP/ruta y un adapter aprobado.

**Aceptación**

- Se define clave, ventana, respuesta de límite, proxies confiables, almacenamiento, concurrencia y configuración.
- No se confía en headers de proxy no confiables.
- El comportamiento bajo concurrencia y reinicio es explícito.

**Pruebas/evidencia:** unitarias de ventana/límites, integración HTTP, spoofing de IP y límites de borde; k6 con thresholds y entorno reproducibles.

### 2.11 Evaluar MongoDB SRV

**Estado actual:** evaluación documentada en ADR 0036. El feature sigue deshabilitado y los paquetes Hickory no están en `Cargo.lock`; la prueba de URI estándar MongoDB pasa sin resolver SRV. El driver 3.10.1 declara resolver Hickory opcional 0.26; `RUSTSEC-2026-0120` afecta `hickory-net` hasta 0.26.0 y está corregido desde 0.26.1. `cargo audit` pasó sobre las 377 dependencias bloqueadas. Se difiere la habilitación a una PR separada que audite la resolución futura y pruebe DNS, sin Atlas ni secretos.

**Impacto en documentación pública:** cumplido en las guías MongoDB/base de datos y el estado del proyecto; se conserva `mongodb://` como única URI anunciada y se documenta el follow-up requerido.

**Cubre:** B6.
**Un PR de evaluación:** revisar feature flags/dependencias, mantenimiento, advisory, MSRV, DNS y licencia; no habilitar SRV durante la evaluación.

**Aceptación**

- ADR con riesgos, alternativas y decisión: habilitar, mantener desactivado o diferir.
- Las pruebas no dependen de secretos ni de Atlas externo.
- La documentación refleja la decisión.

**Pruebas/evidencia:** auditoría de dependencias y tests locales/falsables; cualquier habilitación requiere un PR de implementación posterior aprobado.

### 2.12 Añadir Argon2id como opción de hashing

**Estado actual:** implementación y verificación local completas. `PasswordHash` añade Argon2id PHC v=19 con perfil fijo, mantiene PBKDF2 para verificación legacy y expone `needs_rehash()` sin migración automática; `StaticUserProvider` usa Argon2id fuera de workers async con máximo cuatro verificaciones concurrentes. Pasaron las pruebas de hashes Argon2id/legacy, la suite del servidor, la integración CLI que compila y ejecuta la plantilla CRUD y `cargo audit` sobre el lockfile. La revisión de seguridad independiente no encontró vulnerabilidades en el alcance revisado.

**Impacto en documentación pública:** cumplido en la guía de sesiones/autenticación, referencia API, guía/plantilla CRUD y estado del proyecto; se documentan algoritmo, work factors, límites de concurrencia, actualización explícita y soporte legacy. El build estático pasó.

**Cubre:** B7.
**Un PR:** evaluar dependencia y formato versionado de hashes; mantener verificación de hashes existentes.

**Aceptación**

- Se usa una biblioteca mantenida; no se implementa criptografía propia.
- Verificación distingue hashes nuevos y legacy sin migración silenciosa ni pérdida de acceso.
- Parámetros, coste, actualización de hashes y límites de concurrencia quedan documentados.

**Pruebas/evidencia:** tests de hash correcto/incorrecto, legacy, encoding malformado y actualización; auditoría de dependencia y revisión de seguridad.

### 2.13 Completar medición de Phase 8.1

**Cubre:** B9, trabajo restante tras la ejecución oficial válida.
**Un PR por cada conjunto de medición**, si cambia código; registrar resultados de benchmark como evidencia separada.

**Aceptación**

- Revisar el artifact oficial ya válido sin extrapolar sus resultados a una afirmación de superioridad.
- Completar la comparación A/B/A contra base para los escenarios de 10.000 filas que siguen pendientes.
- Registrar runner, commits, warmups, muestras, variación, resultados desfavorables y límites.
- Phase 8.1 solo se marca completa cuando cada criterio de `ROADMAP.md` esté respaldado y revisado.

**Pruebas/evidencia:** repetir el benchmark controlado; preservar artefactos crudos; verificar CI y actualizar guía/roadmap en el mismo PR de evidencia.

### 2.14 Preparar empaquetado y versiones

**Cubre:** B8.
**Un PR de preparación:** resolver versionado, empaquetado, licencia, notas, MSRV y matriz de publicación. No publicar.

**Aceptación**

- Lista de crates/binarios, nombres, versiones, dependencias, features y notas de versión revisada.
- Instrucciones reproducibles de build/instalación y criterios de release.
- Sin publicar en crates.io hasta que C4 tenga decisión explícita de nombre y autorización del autor.

**Pruebas/evidencia:** package dry-run local, validación de manifests y CI de release en modo no-publicación. Nunca incluir tokens reales.

## 5. Fase 3 - Presentación y publicación pública propuestas

**Gate:** contenido de portada/comparación y cambios de identidad requieren aprobación del autor. La disponibilidad de nombres se investiga; no se reserva ni publica un crate.

### 3.1 Rediseñar la portada

**Cubre:** C1.
**Un PR:** reorganizar `index.mdx` siguiendo la propuesta de producto.

**Aceptación**

- El titular aprobado comunica "Backend completo estilo Laravel y UI reactiva en Rust. Prototipo pre-1.0." sin insinuar que está listo para producción.
- Las acciones llevan a `/getting-started/setup/` y `/project-status/`.
- El bloque Counter tiene como máximo 12 líneas; el Counter y la ruta se copian de las guías vigentes y se compilan.
- El bloque CRUD usa `rustflow new products --template crud`, copia `.env.example` a `.env` y ejecuta `rustflow dev`; advierte que es una demo, no producción.
- Hay cuatro tarjetas para servidor, datos, UI/SSR y testing, enlazadas a documentación real.
- Los límites apuntan a la tabla A11.

**Pruebas/evidencia:** compilar los ejemplos con la API documentada; `cd website && npm run build`; verificar los enlaces.

### 3.2 Crear comparación con otros frameworks

**Cubre:** C2.
**Un PR:** nueva página solo después de comprobar documentación oficial de Leptos 0.8.x y Dioxus 0.7.x.

**Aceptación**

- Registrar fecha de revisión y fuentes oficiales por afirmación.
- Usar "no verificado" donde la fuente no permita confirmar una capacidad.
- Comparar RustFlow según implementación y evidencia, no según metas.
- Cubrir UI reactiva, SSR/hidratación, server functions/actions, hot reload/bundler, backend, ORM/ODM, operación, testing, plataformas y madurez.
- Incluir la nota de posicionamiento aprobada: RustFlow no busca reemplazar Leptos/Dioxus en UI, sino dar a Rust una experiencia de backend completo estilo Laravel con una capa reactiva.
- Evitar claims de rendimiento sin resultados revisados.
- Colocar esas comparaciones en el website en una sesion diferente

**Pruebas/evidencia:** revisar enlaces y cada fila contra fuentes; `cd website && npm run build`.

### 3.3 Acortar README y completar metadatos del repositorio

**Cubre:** C3 y el enlace del sitio en About.
**Un PR de contenido:** reducir el README; actualizar About, Website y topics tras aprobación.

**Aceptación**

- README de 40 líneas o menos con descripción, estado, instalación actual, ejemplo verificado y enlaces a docs/comparación/roadmap.
- Incluye `https://rustflow.pages.dev`.
- About describe un framework Rust pre-1.0 sin sugerir madurez/producción.
- About usa la descripción aprobada: "Framework web full-stack en Rust: reactividad, SSR, servidor y CLI inspirados en Laravel (pre-1.0)."
- Website es `https://rustflow.pages.dev`; los topics propuestos son `rust`, `web-framework`, `wasm`, `ssr`, `axum` y `orm`, sujetos a aprobación.

**Pruebas/evidencia:** contar líneas; verificar links y metadata en GitHub; `cd website && npm run build` si se cambian enlaces de docs.

### 3.4 Investigar alternativas de nombre

**Cubre:** C4, investigación solamente.
**Un PR o informe separado:** proponer tres nombres y comprobar su disponibilidad en crates.io el día de la revisión.

**Aceptación**

- Registrar fecha, método y resultado para cada nombre, incluyendo nombres similares y riesgos de confusión.
- No registrar, renombrar ni publicar nada.
- El autor escoge o rechaza las opciones antes de cualquier trabajo de release.

**Pruebas/evidencia:** enlaces/consultas de disponibilidad reproducibles y revisión del autor; no se ejecuta `cargo publish`.

## 6. Checklist de cierre para cada PR

- [ ] Esta subfase tiene un único PR y respeta sus dependencias aprobadas.
- [ ] Los criterios de aceptación tienen evidencia, no solo afirmaciones.
- [ ] Tests unitarios, integración, E2E y k6 se ejecutaron cuando corresponden.
- [ ] El comando exacto y el resultado observado están en el PR.
- [ ] CI del HEAD final está verde y los checks previos aplicables siguen verdes.
- [ ] Documentación, ejemplos, limitaciones y roadmap coinciden con el código.
- [ ] No se añadieron capacidades ni dependencias fuera del alcance aprobado.
- [ ] El autor revisó los criterios antes de cerrar el PR.

## 7. Fuera de alcance de este roadmap

- Implementar automáticamente todas las propuestas de las Fases 2 y 3.
- Publicar paquetes o hacer un cambio de nombre sin autorización.
- Mover archivos de gobernanza que CI exige en la raíz.
- Declarar el proyecto listo para producción antes de cerrar sus gates de seguridad, compatibilidad, documentación, pruebas y release.
