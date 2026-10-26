import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://rustflow.pages.dev',
  integrations: [
    starlight({
      title: 'RustFlow',
      description: 'Guías para desarrolladores que quieren evaluar y utilizar RustFlow.',
      defaultLocale: 'root',
      locales: { root: { label: 'Español', lang: 'es' } },
      sidebar: [
        { label: 'Empezar', items: [
          { label: 'Bienvenido a RustFlow', slug: 'index' },
          { label: 'Probar RustFlow hoy', slug: 'getting-started/setup' },
          { label: 'Reactividad disponible', slug: 'concepts/reactivity' },
        ]},
        { label: 'Construir con RustFlow', items: [
          { label: 'Crear componentes', slug: 'guides/components' },
          { label: 'CLI y comandos', slug: 'guides/cli' },
        ]},
        { label: 'Proyecto', items: [
          { label: 'Estado y fases', slug: 'project-status' },
        ]},
        { label: 'Colaborar', items: [
          { label: 'Escribir documentación', slug: 'contributing/documentation' },
          { label: 'Verificación del runtime', slug: 'verification/reactivity' },
        ]},
      ],
    }),
  ],
});
