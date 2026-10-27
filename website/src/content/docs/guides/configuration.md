---
title: Configuración por entornos
description: Configura perfil, host y puerto en desarrollo, pruebas y producción con precedencia explícita y sin incluir secretos.
---

La configuración del servidor de una aplicación creada con el CLI se carga mediante rustflow_server::AppConfig. Es una API experimental que valida el perfil y la dirección de bind antes de que el servidor empiece a escuchar. No configura todavía bases de datos, sesiones, autenticación ni secretos.

## 1. Valores iniciales

El proyecto creado incluye .env.example. Es una referencia segura, no un archivo que se cargue por sí mismo.

~~~sh
cp .env.example .env
rustflow dev
~~~

El contenido inicial es deliberadamente pequeño:

~~~dotenv
RUSTFLOW_ENV=development
RUSTFLOW_HOST=127.0.0.1
RUSTFLOW_PORT=3000
~~~

El host usa una IP literal; el puerto debe estar entre 1 y 65535. Sin valores ni archivos personalizados, el servidor escucha en 127.0.0.1:3000.

## 2. Precedencia

En desarrollo y pruebas, la aplicación combina las fuentes de configuración en este orden (de mayor a menor prioridad):

1. Variables del proceso.
2. .env.<perfil>, como .env.test.
3. .env.
4. Valores predeterminados.

El perfil proviene de RUSTFLOW_ENV; si no se define en el proceso, se lee de .env, y si tampoco existe, el perfil es development. Por ejemplo, un ajuste en el proceso reemplaza el valor de cualquiera de los archivos:

~~~sh
RUSTFLOW_PORT=3100 rustflow dev
~~~

Los archivos se leen como datos; el cargador no exporta valores al entorno global, ejecuta comandos ni expande $VARIABLE. Las líneas malformadas provocan un error con el archivo y la línea que deben revisarse.

## 3. Desarrollo, test y producción

**Desarrollo.** Usa .env y opcionalmente .env.development para valores locales no sensibles.

**Pruebas.** Establece el perfil en el proceso para activar los ajustes de .env.test:

~~~sh
RUSTFLOW_ENV=test cargo test
~~~

**Producción.** No se leen .env, .env.production ni otros archivos dotenv. El perfil debe seleccionarse en el entorno del proceso y tanto host como puerto son obligatorios:

~~~sh
RUSTFLOW_ENV=production RUSTFLOW_HOST=0.0.0.0 RUSTFLOW_PORT=8080 ./target/release/hello_rustflow
~~~

Ajusta el nombre del binario al paquete de tu aplicación. En un contenedor o plataforma de despliegue, define estas variables en la configuración del servicio. Un perfil de producción incompleto, un host no numérico, un puerto inválido o un perfil desconocido detienen el arranque en lugar de recurrir silenciosamente a valores de desarrollo.

## 4. Secretos y archivos ignorados

El .gitignore del scaffold excluye .env y .env.*, con una excepción explícita para .env.example. No pongas contraseñas, tokens, claves privadas, credenciales reales o cadenas de conexión privadas en el ejemplo ni en el control de versiones. No publiques valores de producción en errores o logs.

Usa variables inyectadas por el entorno de ejecución o el almacén de secretos administrado de la plataforma, con acceso y rotación restringidos. RustFlow no incluye todavía un gestor de secretos ni necesita secretos para la configuración de host/puerto implementada en esta fase. Evita agregar variables para capacidades que aún no existen.

## 5. API disponible y límites

El scaffold usa este extracto desde su función main asíncrona, después de crear y configurar la aplicación:

~~~rust
let config = rustflow_server::AppConfig::load()?;
let address = config.socket_addr();
app.serve(address).await?;
~~~

AppConfig expone el perfil, la IP y el puerto validados. No hay un formato TOML/YAML, resolución DNS, perfiles personalizados, conexión de base de datos, configuración de sesión ni almacenamiento de secretos. El framework sigue siendo pre-1.0 y no debe considerarse listo para producción solo por validar estas tres variables.

Para crear el scaffold desde cero, sigue el tutorial de primera aplicación. Para consultar comandos y generadores, usa la guía del CLI.
