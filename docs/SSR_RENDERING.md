# Renderizado HTML en servidor (prototipo)

El crate `rustflow-ssr` convierte un árbol de vistas estructurales en un fragmento HTML. No sirve HTTP y no vuelve seguro el árbol para insertarlo sin más en otros contextos.

## Fragmento básico

```rust
use rustflow::view::{Element, View};
use rustflow_ssr::render_to_string;

fn example() -> Result<(), Box<dyn std::error::Error>> {
    let view = Element::new("main")?
        .with_attribute("aria-label", "Welcome")?
        .with_children([View::text("Hello & welcome")])
        .build();
    assert_eq!(
        render_to_string(&view)?,
        "<main aria-label=\"Welcome\">Hello &amp; welcome</main>"
    );
    Ok(())
}
```

## Componentes

Usa `HtmlRenderer::with_components(&registry)` o `render_with_components` para resolver descriptores. Cada llamada crea un runtime reactivo aislado. Los callbacks de limpieza del componente se ejecutan antes de retornar, incluso cuando falla la serialización de su vista.

## Política de seguridad y salida soportada

El texto y los valores de atributos se escapan. Solo se serializa un conjunto conservador de elementos comunes; etiquetas como script, iframe, object, embed y style quedan fuera de la lista permitida. Los atributos URL solo aceptan los esquemas `http`, `https`, `mailto`, `tel` o URLs relativas. `srcset` y `ping` se rechazan hasta que se implemente un parser que valide cada URL. Los atributos booleanos falsos se omiten.

Los callbacks de eventos nunca se serializan como atributos HTML `on*`. Un subconjunto de propiedades reflejables (como `value` y `checked` en formularios) se convierte a atributos; las propiedades exclusivas del cliente se omiten. Si un atributo y una propiedad producirían el mismo atributo HTML, se devuelve un error.

El renderer emite fragmentos, no un documento completo ni una respuesta HTTP. Layouts, status/headers, rutas, datos de hidratación e hidratación siguen siendo trabajo posterior.
