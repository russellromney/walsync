// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://walsync.dev',
  integrations: [
    starlight({
      title: 'Walsync',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/russellromney/walsync' }],
      sidebar: [
        { label: 'Getting Started', autogenerate: { directory: 'start' } },
        { label: 'Guides', autogenerate: { directory: 'guide' } },
        { label: 'Concepts', autogenerate: { directory: 'concepts' } },
      ],
      customCss: ['./src/styles/custom.css'],
    }),
  ],
});
