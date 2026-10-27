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
            { label: 'Servidor HTTP', slug: 'guides/server' },
            { label: 'SSR e hidratación', slug: 'guides/ssr' },
            { label: 'Layouts y páginas', slug: 'guides/layouts' },
            { label: 'Navegación y formularios', slug: 'guides/navigation' },
          ],
        },
        {
          label: 'Tutoriales',
          items: [
            { label: 'Página de producto SSR', slug: 'tutorials/product-page' },
          ],
        },
        {
          label: 'Referencia',
          items: [
            { label: 'Disponibilidad de API', slug: 'reference/api-status' },
          ],
        },
        {
          label: 'Proyecto y contribución',
          items: [
            { label: 'Escribir documentación', slug: 'contributing/documentation' },
            { label: 'Verificación del runtime', slug: 'verification/reactivity' },
            { label: 'CLI planificado', slug: 'guides/cli' },
          ],
        },
      ],
    }),
  ],
});
