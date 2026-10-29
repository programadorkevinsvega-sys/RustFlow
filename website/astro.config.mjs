import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://rustflow.pages.dev',
  integrations: [
    starlight({
      title: 'RustFlow',
      description: 'Aprende las APIs implementadas de RustFlow con guías, ejemplos, casos de uso y límites explícitos.',
      defaultLocale: 'root',
      locales: { root: { label: 'Español', lang: 'es' } },
      sidebar: [
        {
          label: 'Empezar',
          items: [
            { label: 'Documentación de RustFlow', slug: 'index' },
            { label: 'Empezar con RustFlow', slug: 'getting-started/setup' },
            { label: 'Estado y capacidades', slug: 'project-status' },
          ],
        },
        {
          label: 'Fundamentos',
          items: [
            { label: 'Estado y reactividad', slug: 'concepts/reactivity' },
            { label: 'Crear componentes', slug: 'guides/components' },
          ],
        },
        {
          label: 'Construir aplicaciones',
          items: [
            { label: 'CLI y generadores', slug: 'guides/cli' },
            { label: 'Configuración por entornos', slug: 'guides/configuration' },
            { label: 'Servidor HTTP', slug: 'guides/server' },
            { label: 'Formularios y validación', slug: 'guides/forms-validation' },
            { label: 'Sesiones y control de acceso', slug: 'guides/sessions-and-access' },
            { label: 'SSR e hidratación', slug: 'guides/ssr' },
            { label: 'Layouts y páginas', slug: 'guides/layouts' },
            { label: 'Navegación y formularios', slug: 'guides/navigation' },
            { label: 'Errores, límites y seguridad', slug: 'guides/errors-and-security' },
            { label: 'Probar y depurar', slug: 'guides/testing' },
          ],
        },
        {
          label: 'Tutoriales',
          items: [
            { label: 'Página de producto SSR', slug: 'tutorials/product-page' },
            { label: 'API JSON', slug: 'tutorials/json-api' },
            { label: 'Interfaz reactiva de tareas', slug: 'tutorials/reactive-todo' },
            { label: 'Primera aplicación con el CLI', slug: 'tutorials/first-cli-app' },
          ],
        },
        {
          label: 'Referencia',
          items: [
            { label: 'Disponibilidad de API', slug: 'reference/api-status' },
            { label: 'Catálogo de ejemplos', slug: 'reference/examples' },
          ],
        },
        {
          label: 'Proyecto y contribución',
          items: [
            { label: 'Escribir documentación', slug: 'contributing/documentation' },
            { label: 'Verificación del runtime', slug: 'verification/reactivity' },
          ],
        },
      ],
    }),
  ],
});
