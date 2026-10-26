---
title: CLI y comandos
description: Estado del CLI de RustFlow y comandos previstos en el roadmap.
---

> **Estado: planificado.** RustFlow todavía no incluye un ejecutable CLI propio. Los comandos de esta página no están disponibles para ejecutar.

La experiencia de desarrollo prevista está inspirada en herramientas como Laravel Artisan: comandos coherentes para crear proyectos, ejecutar el modo de desarrollo, compilar y generar archivos comunes. El diseño final dependerá de las fases de implementación y de sus pruebas.

## Capacidades previstas

La fase 6 del roadmap contempla:

- Crear un proyecto nuevo.
- Ejecutar el entorno de desarrollo y compilar el proyecto.
- Generar componentes, páginas, controladores, modelos y migraciones.
- Evitar que los generadores sobrescriban archivos del usuario sin autorización.
- Probar los comandos de forma determinista.

Los nombres propuestos en el roadmap, como rustflow new, rustflow dev y rustflow build, son objetivos de implementación, **no comandos disponibles en la versión actual**. Los generadores también siguen pendientes.

## Qué usar ahora

Para verificar el código que existe hoy, instala Rust y ejecuta las pruebas del crate desde el repositorio. Sigue la guía [Probar RustFlow hoy](/getting-started/setup/).

Cuando el CLI esté implementado, esta página deberá incluir requisitos de instalación, salida real de --help, ejemplos ejecutables y comportamiento documentado ante errores. No se marcará como disponible hasta que esos ejemplos se validen en CI.

Consulta la fase 6 del [roadmap público](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md).
