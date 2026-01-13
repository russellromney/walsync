# Walsync Documentation

Professional documentation site built with [Astro Starlight](https://starlight.astro.build/).

## Local Development

### Prerequisites
- Node.js 18+
- npm or yarn

### Setup

```bash
# Install dependencies
npm install

# Start development server
npm run dev
```

Visit http://localhost:3000 to see the docs.

### Build

```bash
# Build static site
npm run build

# Preview build output
npm run preview
```

## Structure

```
docs/
├── astro.config.mjs          # Astro configuration
├── package.json              # Dependencies
├── src/
│   ├── content/docs/
│   │   ├── start/            # Getting started guides
│   │   ├── guide/            # How-to guides
│   │   ├── concepts/         # Deep dive topics
│   │   └── reference/        # API reference
│   └── styles/
│       └── custom.css        # Clean light-mode styling
└── public/
    └── logo.svg              # Walsync logo
```

## Writing Documentation

### Add a New Page

1. Create `.mdx` file in appropriate folder under `src/content/docs/`
2. Add frontmatter:
   ```yaml
   ---
   title: Page Title
   description: Short description
   ---
   ```
3. Write content in Markdown
4. Update sidebar in `astro.config.mjs` to include new page

### Markdown Features

All standard Markdown plus:
- Code syntax highlighting
- Callouts/asides:
  ```markdown
  :::note
  This is a note
  :::

  :::caution
  This is a warning
  :::

  :::tip
  This is a tip
  :::
  ```

## Deployment

### GitHub Pages

Push to `main` branch - GitHub Actions automatically:
1. Builds the documentation site
2. Deploys to GitHub Pages
3. Updates at `https://russellromney.github.io/walsync/`

### Custom Domain

To use `walsync.dev`:
1. Set custom domain in GitHub repository settings
2. Configure DNS with CNAME record pointing to `russellromney.github.io`
3. Update `astro.config.mjs` `site` field to `https://walsync.dev`

## Design

- **Light mode only** - No dark mode switcher for simplicity
- **Clean typography** - System fonts, generous spacing
- **Teal accent** - #0f766e for links and highlights
- **Professional** - Inspired by Litestream's documentation
- **Mobile-friendly** - Responsive design

## Styling

All styling is in `src/styles/custom.css`:
- No CSS framework dependencies
- Override Starlight defaults
- Custom light-mode color scheme

## Inspiration

This documentation site draws inspiration from:
- [Litestream](https://litestream.io) - Professional, focused, simple
- [Astro Docs](https://docs.astro.build) - Clean design, excellent navigation
- [Rust Book](https://doc.rust-lang.org/book) - Clear explanations, great examples

---

**Need help?** Check the [Astro Starlight docs](https://starlight.astro.build/).
