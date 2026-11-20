/// <reference types="vitest/config" />
import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': path.resolve(import.meta.dirname, './src') } },
  // The service's API, from `workriver serve`. It answers only its own Host and Origin.
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:47820', changeOrigin: true, headers: { origin: 'http://127.0.0.1:47820' } },
    },
  },
  // The script of the frame mermaid draws in (src/content/drawDiagram.ts) is built on its own,
  // as a worker's is; as modules, so each kind of diagram stays a chunk loaded when needed.
  worker: { format: 'es' },
  // `npm test`: the page's logic and its safety-relevant views, in a DOM of happy-dom.
  test: { environment: 'happy-dom', include: ['src/**/*.test.{ts,tsx}'] },
})
