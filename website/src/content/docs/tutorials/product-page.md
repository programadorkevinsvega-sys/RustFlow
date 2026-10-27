---
title: Tutorial: página de producto SSR
description: Construye una página de producto con una ruta HTTP, props públicas, un componente registrado y un layout.
---

Este tutorial reúne el modelo de páginas SSR actual de RustFlow. El ejemplo ejecutable completo vive en [crates/rustflow-server/examples/product_page.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs). La guía explica las partes principales de ese archivo para que puedas entender y modificar el patrón en vez de copiarlo a ciegas.

**Alcance del ejemplo:** el catálogo está en memoria para enseñar la arquitectura. No incluye base de datos, carrito persistente, checkout, pagos, cuentas, autorización ni server actions. No es una tienda lista para producción.

## 1. Ejecutar el ejemplo completo

Desde la raíz del repositorio:

~~~sh
cargo run -p rustflow-server --example product_page
~~~

Abre <code>http://127.0.0.1:3000/</code>. También puedes abrir directamente <code>/products/keyboard</code> y <code>/products/mouse</code>. Una ruta de producto desconocida devuelve 404.

Este programa es un ejemplo ejecutable independiente. El workspace contiene sus dependencias y Cargo compila los ejemplos de servidor; la documentación debe acompañarse de la comprobación de compilación del CI.

## 2. Qué contiene la aplicación

El ejemplo está dividido en cuatro responsabilidades para que cada parte pueda cambiar de forma independiente:

| Parte | Para qué sirve |
|---|---|
| <code>Product</code> y <code>demo_product</code> | Fuente de datos fija de demostración. En una aplicación real se sustituiría por un servicio/repository, que RustFlow no integra automáticamente. |
| <code>ProductPage</code> | Convierte props públicas del producto a un árbol de vistas. No recibe el objeto Request ni accede a almacenamiento. |
| <code>StoreLayout</code> | Envuelve el contenido con cabecera, área principal y pie de página comunes. |
| <code>SharedComponentCatalog</code> | Hace que las definiciones inmutables estén disponibles para peticiones concurrentes, sin compartir el estado local del render. |
| Ruta <code>/products/{id}</code> | Resuelve el identificador, verifica que exista y decide qué props públicas pasan al componente. |
| <code>HtmlPage</code> | Genera el documento HTML completo y aplica el layout SSR antes de devolver la respuesta. |

## 3. El componente de página

En el código, <code>ProductPage</code> se registra como un <code>SharedComponentDefinition</code>. El callback de render localiza las props por nombre, obtiene el texto del nombre, precio y descripción, y construye los nodos con <code>View::element</code> y <code>View::text</code>.

La diferencia frente a concatenar HTML es importante: el componente describe estructura y texto; el renderer SSR aplica la política de escape al serializar. El árbol de vista es una representación tipada, no un string que se considere seguro por defecto.

Las props están limitadas al subconjunto que el ejemplo necesita. Las páginas deberían pasar solo valores que sean públicos y estén destinados a mostrarse. No se deben pasar cookies, tokens, credenciales, decisiones de autorización ni estado privado del servidor.

## 4. El layout y los children

<code>StoreLayout</code> recibe los children que el componente de página produjo. Los coloca entre una cabecera y un pie de página dentro de una estructura común. El layout no conoce ni debe conocer la identidad del producto.

Este patrón sirve para tiendas, blogs, documentación y paneles con una estructura repetida. Consulta [Componer páginas con layouts](/guides/layouts/) para más detalles sobre composición anidada y orden de wrapping.

## 5. Por qué se construye el catálogo una sola vez

El código construye el catálogo con <code>SharedComponentCatalogBuilder</code> durante el startup y lo comparte con <code>Arc</code>. El handler clona ese handle compartible por petición, pero el catálogo solo guarda definiciones/fábricas. El runtime, scopes, propiedades y vistas del render son locales.

Esto permite atender requests concurrentes sin almacenar estado de usuario en un singleton compartido. Para callbacks locales que no cumplan <code>Send + Sync</code>, hay un registry local, pero la vía canónica del ejemplo HTTP concurrente es el catálogo compartido.

## 6. Cómo la ruta prepara la página

La ruta sigue este flujo:

1. Lee el valor de <code>id</code> desde el patrón del path.
2. Consulta la tabla de demostración y devuelve 404 cuando no encuentra el producto.
3. Convierte nombre, precio y descripción en props explícitas.
4. Llama a <code>HtmlPage::from_component</code> para elegir el componente de cuerpo.
5. Establece idioma y catálogo, y añade <code>StoreLayout</code> con <code>with_layout</code>.
6. Llama a <code>render()</code> para devolver el documento como respuesta HTTP.

El handler es responsable de resolver la URL y seleccionar los datos. <code>HtmlPage</code> compone y renderiza la página; no implementa un router ni una base de datos por su cuenta.

## 7. Casos de uso y extensiones seguras

Este patrón puede adaptarse a:

- **Blog:** componente ArticlePage con slug, título y contenido público; layout de sitio y layout de sección.
- **Catálogo:** componentes de producto reutilizables y props derivadas de una capa de datos.
- **Documentación:** páginas estáticas con navegación y estructura compartida.
- **Panel de control:** contenido específico envuelto por un layout común; autenticación/autorización deben construirse explícitamente, pues no están integradas aquí.

Para reemplazar la tabla fija por datos externos, añade una capa de aplicación que valide los datos y el acceso y que convierta solo campos públicos a props. El ejemplo no muestra permisos, persistencia ni operaciones de mutación.

## 8. ¿Y la interactividad del navegador?

Este ejemplo demuestra SSR. No convierte automáticamente el HTML en una interfaz interactiva en el navegador. RustFlow también dispone de hidratación opt-in y navegación cliente opt-in, pero necesitan configuración explícita y una vista inicial compatible. La mejora progresiva de formularios se limita a ciertos GET URL-encoded; no es un carrito ni una server action.

Lee [SSR e hidratación](/guides/ssr/) y [Navegación cliente y formularios](/guides/navigation/) antes de añadir interacción.

## Referencias

- [Fuente ejecutable: product_page.rs](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/product_page.rs)
- [Ejemplo base HTTP, con JSON y grupos](https://github.com/programadorkevinsvega-sys/RustFlow/blob/main/crates/rustflow-server/examples/basic.rs)
- [Servidor HTTP](/guides/server/)
- [Disponibilidad de API](/reference/api-status/)
