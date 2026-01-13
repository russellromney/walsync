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
      social: {
        github: 'https://github.com/russellromney/walsync',
      },
      sidebar: [
        {
          label: 'Start',
          items: [
            { label: 'Why Walsync?', slug: 'start/why' },
            { label: 'Installation', slug: 'start/install' },
            { label: 'Quick Start', slug: 'start/quickstart' },
          ],
        },
        {
          label: 'Guide',
          items: [
            { label: 'CLI Commands', slug: 'guide/cli' },
            { label: 'Python API', slug: 'guide/python' },
            { label: 'Configuration', slug: 'guide/config' },
            { label: 'S3 Layout', slug: 'guide/s3-layout' },
          ],
        },
        {
          label: 'Deep Dive',
          items: [
            { label: 'Data Integrity', slug: 'concepts/integrity' },
            { label: 'Multi-Database', slug: 'concepts/multi-db' },
            { label: 'Performance', slug: 'concepts/performance' },
            { label: 'Architecture', slug: 'concepts/architecture' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'CLI Reference', slug: 'reference/cli' },
            { label: 'Python API', slug: 'reference/python-api' },
            { label: 'Environment Variables', slug: 'reference/env' },
            { label: 'Troubleshooting', slug: 'reference/troubleshooting' },
          ],
        },
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
