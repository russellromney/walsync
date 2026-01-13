// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://walsync.dev',
  integrations: [
    starlight({
      title: 'Walsync',
      logo: {
        src: './src/assets/logo.svg',
      },
      components: {
        ThemeSelect: './src/components/ThemeSelect.astro',
      },
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/russellromney/walsync' }],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Quick Start', link: '/' },
            { label: 'Why Not Litestream?', link: '/start/why-not-litestream/' },
          ],
        },
        { label: 'Guides', autogenerate: { directory: 'guide' } },
        {
          label: 'Configuration',
          items: [
            { label: 'Environment Variables', link: '/config/environment/' },
            { label: 'S3 Providers', link: '/config/s3-providers/' },
            { label: 'Logging', link: '/config/logging/' },
            { label: 'Deployment', link: '/config/deployment/' },
          ],
        },
        { label: 'Concepts', autogenerate: { directory: 'concepts' } },
      ],
      customCss: ['./src/styles/custom.css'],
    }),
  ],
});
