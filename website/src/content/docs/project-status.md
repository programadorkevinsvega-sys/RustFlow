---
title: Estado del proyecto
description: Capacidades implementadas, límites conocidos y estado verificable de RustFlow.
---

RustFlow es un prototipo pre-1.0. Esta página resume lo que puedes probar hoy y los límites que siguen abiertos. Una capacidad implementada no implica estabilidad de API ni preparación para producción.

## Límites actuales

| Limitación | Estado actual | Más información |
|---|---|---|
| Estilo inline en vistas | No disponible | [Estilos y assets](/guides/styling-assets/) |
| Servidor de archivos estáticos | Experimental disponible; sin streaming incremental | [Estilos y assets](/guides/styling-assets/) |
| Hot reload y bundler WASM integrado | No disponible | [CLI y generadores](/guides/cli/) |
| Server actions / server functions | No disponibles | [Referencia de API](/reference/api-status/) |
| Suspense, Resources y ErrorBoundary | No disponibles | [Errores en UI y SSR](/guides/ui-errors/) |
| Sesiones SQL durables | No disponibles | [Sesiones y control de acceso](/guides/sessions-and-access/) |
| Proveedor de usuarios | No disponible | [Sesiones y control de acceso](/guides/sessions-and-access/) |
| Rate limiting | No disponible | [Errores y seguridad](/guides/errors-and-security/) |
| MongoDB `mongodb+srv://` | Deshabilitado | [ODM MongoDB](/guides/mongodb-odm/) |
| Instalación fuera del checkout | No disponible | [Configuración inicial](/getting-started/setup/) |

El detalle histórico de PR y commits vive en [Historial del proyecto](/changelog/).

## Matriz rápida: qué puedes hacer

| Tarea | Disponibilidad | Referencia |
|---|---|---|
| Estado local y efectos owner-scoped | Experimental disponible | [Reactividad](/concepts/reactivity/) |
| Componentes, props, callbacks y listas keyed | Experimental disponible | [Componentes](/guides/components/) |
| Refs Rust y referencias DOM | Experimental; refs DOM solo en WASM/browser | [Composición de UI](/guides/ui-foundations/) |
| SSR y documentos HTML | Experimental disponible | [SSR](/guides/ssr/) |
| Hidratación DOM | Experimental, opt-in | [SSR e hidratación](/guides/ssr/) |
| Navegación cliente e historial | Experimental, opt-in | [Navegación](/guides/navigation/) |
| Servidor HTTP, rutas y middleware | Experimental disponible | [Servidor HTTP](/guides/server/) |
| Validación y respuestas 422 | Experimental disponible | [Formularios y validación](/guides/forms-validation/) |
| CSRF, sesiones y guards | Experimental; sin persistencia durable ni proveedor de usuarios | [Sesiones y control de acceso](/guides/sessions-and-access/) |
| SQLite, PostgreSQL, MySQL y MariaDB | Experimental disponible | [Base de datos](/guides/database/) |
| ORM SeaORM | Experimental disponible | [Base de datos y ORM](/guides/database/) |
| ODM MongoDB | Experimental disponible, sin SRV | [ODM MongoDB](/guides/mongodb-odm/) |
| CLI y scaffold server | Experimental disponible | [CLI y generadores](/guides/cli/) |
| Scaffold CRUD SQL | Demo experimental | [Primera aplicación con el CLI](/tutorials/first-cli-app/) |
| TestApp y fixtures de persistencia | Experimental disponible | [Testing](/guides/testing/) |

## Estado por fases

Las fases fundamentales se completaron dentro de alcances explícitos:

- Fase 0: gobernanza, licencia, arquitectura y proceso.
- Fases 1–3: reactividad, modelo de vistas y renderer DOM/WASM.
- Fases 4–5: servidor HTTP, SSR, hidratación y navegación progresiva.
- Fase 6.1–6.4: CLI, configuración, validación/CSRF y sesiones experimentales.
- Fase 6.5–6.7: SQLx, motores SQL, ODM MongoDB y ORM SeaORM experimentales.
- Fase 7.1: capacidades operativas de servidor.
- Fase 7.2: memos, refs, contextos tipados y referencias DOM.
- Fase 7.3: kit de testing y documentación.
- Phase 8.1: mediciones internas y benchmarks en progreso; no cerrada.
- Phase 8.2: hot reload con preservación de estado sigue pendiente; `rustflow dev` ya ofrece reconstrucción/reinicio nativo y regeneración WASM opt-in.
- Phase 8.3: cobertura de accesibilidad y compatibilidad pendiente.

Las fases no convierten las APIs en compatibles con producción. Consulta [ROADMAP.md](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md) y [Historial del proyecto](/changelog/) para criterios y entregas detalladas.

## Stack actual

- Rust estable y Rust 1.94 comprobados por CI cuando el cambio corresponde al alcance.
- SQLx 0.9.0, SeaORM 2.0.4 y SQLite 3.54.1.
- MongoDB 3.10.1 con conexiones `mongodb://`; SRV deshabilitado.
- Axum/Tokio/Tower para HTTP.
- WASM/browser con Chromium E2E y presupuesto de bundle.
- Instalación experimental desde checkout; no hay publicación estable en crates.io.

Estos datos describen el HEAD actual. La CI de un PR verifica el commit exacto y no garantiza compatibilidad futura ni preparación para producción.
