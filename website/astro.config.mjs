import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://rustflow.pages.dev',
  integrations: [
    starlight({
      title: 'RustFlow',
      description: 'Documentación para desarrolladores de RustFlow.',
      defaultLocale: 'root',
      locales: { root: { label: 'Español', lang: 'es' } },
      sidebar: [
        { label: 'Introducción', items: [
          { label: 'Bienvenido a RustFlow', slug: 'index' },
          { label: 'Estado del proyecto', slug: 'project-status' },
        ]},
        { label: 'Primeros pasos', items: [
          { label: 'Preparar el entorno', slug: 'getting-started/setup' },
        ]},
        { label: 'Conceptos', items: [
          { label: 'Reactividad', slug: 'concepts/reactivity' },
        ]},
        { label: 'Calidad y verificación', items: [
          { label: 'Verificación del runtime', slug: 'verification/reactivity' },
        ]},
        { label: 'Contribuir', items: [
          { label: 'Escribir documentación', slug: 'contributing/documentation' },
        ]},
      ],
    }),
  ],
});
