---
title: Escribir documentación
description: Criterios para mantener la documentación pública sincronizada con RustFlow.
---

La documentación pública es parte del producto y debe cambiar junto con la funcionalidad que describe.

## Qué incluir en cada funcionalidad

1. **Estado:** indica si la capacidad está planificada, en desarrollo o verificada.
2. **Propósito:** explica el problema que resuelve y cuándo usarla.
3. **Prerrequisitos:** especifica qué está disponible y qué debe instalarse.
4. **Ejemplo:** incluye un caso mínimo que corresponda a la API real.
5. **Resultado esperado:** explica qué debe observar el lector.
6. **Límites y errores:** documenta restricciones, limpieza, seguridad y casos no soportados.
7. **Validación:** indica qué tests, ejemplos o comandos permiten verificar la guía.

## Reglas

- No inventar APIs para completar un tutorial.
- No presentar planes del roadmap como funciones disponibles.
- Preferir ejemplos mínimos que puedan compilarse o probarse.
- Actualizar navegación y enlaces al añadir páginas.
- Mantener sincronizadas la documentación interna de docs/ y la documentación pública cuando ambas describan la misma funcionalidad.
- Revisar enlaces y compilar el sitio en CI antes de fusionar cambios.

## Documentación por fase

Cada PR que añade o cambia una capacidad debe considerar las explicaciones, casos de uso, ejemplos y limitaciones que correspondan. La cantidad de documentación depende del alcance: una corrección interna no necesita un tutorial nuevo, pero una API pública nueva sí necesita una guía adecuada.
