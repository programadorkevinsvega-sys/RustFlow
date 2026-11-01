---
title: Accesibilidad en interfaces
description: Construye interfaces RustFlow con HTML semántico, formularios comprensibles, foco visible y navegación por teclado.
---

Una interfaz accesible comienza con HTML que expresa la función de cada control. RustFlow representa elementos estructurales y eventos; no corrige automáticamente una semántica incorrecta ni añade atributos ARIA por ti.

## 1. Encabezados y estructura semántica

Usa landmarks y encabezados en orden lógico. No selecciones un elemento solo por su apariencia visual.

~~~rust
rustflow::view! {
    <main>
        <header>
            <h1>"Cuenta"</h1>
        </header>
        <section aria-labelledby="profile-heading">
            <h2 id="profile-heading">"Perfil"</h2>
            <p>"Administra tus datos públicos."</p>
        </section>
    </main>
}
~~~

Usa <code>main</code>, <code>nav</code>, <code>header</code>, <code>footer</code>, <code>section</code>, <code>article</code> y listas cuando describan la estructura. No saltes niveles de encabezado para conseguir un tamaño.

## 2. Labels, errores y campos

Cada control necesita un nombre accesible. Prefiere <code>label</code> asociado mediante <code>for</code> y <code>id</code>.

~~~rust
rustflow::view! {
    <form action="/signup" method="post">
        <div>
            <label for="signup-email">"Correo electrónico"</label>
            <input
                id="signup-email"
                name="email"
                type="email"
                autocomplete="email"
                required
                aria-describedby="email-hint email-error"
            />
            <p id="email-hint">"Usaremos este correo para tu cuenta."</p>
            <p id="email-error" role="alert">"Introduce un correo válido."</p>
        </div>
        <button type="submit">"Crear cuenta"</button>
    </form>
}
~~~

Asocia mensajes de ayuda y error con <code>aria-describedby</code>. Usa <code>aria-invalid="true"</code> cuando el control tenga un error real y muestra el mensaje en texto. No dependas solo del color, icono o placeholder para comunicar un problema.

## 3. Botones y acciones

Un botón debe describir su acción y declarar su tipo dentro de un formulario. El valor por defecto HTML es <code>submit</code>; para una acción local que no debe enviar el formulario, indica <code>type="button"</code>.

~~~rust
rustflow::view! {
    <button type="button" aria-expanded="false" aria-controls="help-panel">
        "Mostrar ayuda"
    </button>
    <div id="help-panel" hidden>
        "Explicación adicional."
    </div>
}
~~~

No construyas botones o enlaces con <code>div</code> o <code>span</code> clicables. Los elementos nativos ya ofrecen foco, activación por teclado y roles conocidos. Los enlaces navegan; los botones realizan acciones.

## 4. Teclado y foco

- Comprueba que toda acción importante se pueda realizar con Tab, Shift+Tab, Enter, Espacio y las teclas propias del control nativo.
- Mantén un indicador de foco visible; no elimines el outline sin proporcionar una alternativa clara.
- No uses un elemento no interactivo con un handler de clic como único mecanismo de interacción.
- No crees un orden artificial con valores positivos de <code>tabindex</code>.
- Después de una actualización dinámica, no muevas el foco a un nodo nuevo si la acción del usuario no lo exige.
- Si un diálogo o menú modal se implementa a nivel de aplicación, define explícitamente gestión de foco, Escape, contención del foco y restauración al cerrar; RustFlow no añade un componente Dialog accesible automáticamente.

~~~css
:focus-visible {
  outline: 3px solid currentColor;
  outline-offset: 3px;
}
~~~

## 5. Imágenes, listas y contenido dinámico

Describe imágenes informativas mediante <code>alt</code>. Las imágenes decorativas deben llevar <code>alt=""</code>. Usa listas HTML para colecciones y conserva claves de entidad estables para que una actualización no confunda identidad visual con posición.

Para una región que cambia tras una acción, evalúa si un estado breve necesita <code>role="status"</code> o <code>aria-live="polite"</code>. Usa <code>role="alert"</code> solo para mensajes importantes que requieren aviso inmediato; no marques toda la aplicación como live region porque provoca anuncios excesivos.

## 6. ARIA como complemento, no sustituto

Primero elige el elemento HTML correcto. Añade ARIA solo cuando necesites describir un estado o relación que el elemento nativo no expresa:

- <code>aria-expanded</code> para el estado abierto/cerrado de un disclosure.
- <code>aria-controls</code> para referenciar la región asociada cuando ayude a comprender la interfaz.
- <code>aria-describedby</code> para texto de ayuda o error.
- <code>aria-current</code> para el elemento actual de una navegación.
- <code>aria-live</code> o roles de estado para anuncios dinámicos justificados.

Mantén los valores sincronizados con el estado real. Una etiqueta ARIA incorrecta puede confundir más que no añadir una etiqueta decorativa.

## 7. Revisión práctica

Antes de publicar una pantalla, prueba solo teclado, zoom de texto, lector de pantalla, contraste de estados/foco y mensajes de error. Inspecciona el árbol de accesibilidad del navegador y valida que labels, nombres, roles y estados coincidan con la acción real. Los ejemplos de esta guía muestran patrones; no constituyen una certificación WCAG ni sustituyen pruebas con usuarios y tecnologías de asistencia.
