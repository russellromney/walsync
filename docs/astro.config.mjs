import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'Walsync',
      description: 'Lightweight SQLite WAL sync to S3/Tigris with explicit data integrity verification',
      logo: {
        src: './public/logo.svg',
        alt: 'Walsync',
      },
      favicon: './public/favicon.svg',
      social: [
        { label: 'GitHub', href: 'https://github.com/russellromney/walsync', icon: 'github' },
      ],
      // Disable dark mode - clean simple design
      customCss: ['./src/styles/custom.css'],
      head: [
        {
          tag: 'meta',
          attrs: {
            name: 'color-scheme',
            content: 'light',
          },
        },
      ],
    }),
  ],
  site: 'https://walsync.dev',
  output: 'static',
});
