---
title: Probar RustFlow hoy
description: Prepara Rust y ejecuta las comprobaciones del runtime que RustFlow ya implementa.
---

Esta guía es para desarrolladores que quieren evaluar **el código real de RustFlow**. No es una guía para ejecutar el portal Astro.

> **Estado: prototipo de desarrollo.** RustFlow todavía no ofrece una instalación de producción, un paquete publicado y estable para aplicaciones, ni un CLI propio. La fase 1 implementa la base del runtime reactivo; consulta [Estado y fases](/project-status/) antes de adoptarlo.

## Requisitos

- Git.
- Rust estable con una versión compatible con el MSRV del repositorio (actualmente Rust 1.85).
- Cargo, incluido con Rust.

Comprueba que Rust está disponible:

~~~sh
rustc --version
cargo --version
~~~

## Descargar el código

Clona el repositorio y entra en él:

~~~sh
git clone https://github.com/programadorkevinsvega-sys/RustFlow.git
cd RustFlow
~~~

## Ejecutar las pruebas del crate

Desde la raíz del repositorio:

~~~sh
cargo test -p rustflow
~~~

Para ejecutar también las comprobaciones principales del workspace:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
~~~

Las pruebas validan el código implementado en el repositorio. No significan que RustFlow ya pueda compilar una aplicación web completa ni que esté listo para producción.

## ¿Cómo se integra en una aplicación?

Por ahora no hay un flujo de instalación recomendado para una aplicación nueva ni una versión estable publicada en crates.io. No uses cargo add rustflow suponiendo que el paquete está publicado, y no dependas de APIs no documentadas.

El runtime reactivo puede explorarse dentro del propio workspace. La API implementada está en [crates/rustflow/src/reactivity.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow/src/reactivity.rs); sus garantías y limitaciones se explican en [Reactividad disponible](/concepts/reactivity/).

## ¿Dónde están los componentes y los comandos?

El sistema de componentes y su sintaxis están en la fase 2 del [roadmap](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/ROADMAP.md). El runtime de navegador aparece en la fase 3 y el CLI —incluidos los futuros comandos para crear proyectos y generar código— en la fase 6.

Consulta las guías de [componentes](/guides/components/) y [CLI](/guides/cli/) para conocer el alcance previsto sin confundirlo con funcionalidades actuales.
