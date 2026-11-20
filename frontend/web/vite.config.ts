/// <reference types="vitest/config" />
import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

// Mermaid registers its ELK layout (1.5 MB) as a chunk it loads by name; the frame never asks
// for it (layout 'dagre', which a block cannot change: src/content/diagram/config.ts), so the
// build puts an empty module in its place. Were it asked for anyway, that diagram fails to
// draw and shows its code.
const noElk = (): Plugin => ({
  name: 'mermaid-without-elk',
  enforce: 'pre',
  resolveId: (source, importer) =>
    importer?.includes('/mermaid/dist/') && /^\.\/elk-\w+\.mjs$/.test(source) ? '\0mermaid-without-elk' : undefined,
  load: id => (id === '\0mermaid-without-elk' ? 'export {}' : undefined),
})

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': path.resolve(import.meta.dirname, './src') } },
  // The service's API, from `workriver serve`. It answers only its own Host and Origin.
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:47820', changeOrigin: true, headers: { origin: 'http://127.0.0.1:47820' } },
    },
  },
  // The script of the frame mermaid draws in (src/content/diagram/draw.ts) is built on its own,
  // as a worker's is; as modules, so each kind of diagram stays a chunk loaded when needed.
  worker: { format: 'es', plugins: () => [noElk()] },
  // `npm test`: the page's logic and its safety-relevant views, in a DOM of happy-dom.
  test: { environment: 'happy-dom', include: ['src/**/*.test.{ts,tsx}'] },
})
