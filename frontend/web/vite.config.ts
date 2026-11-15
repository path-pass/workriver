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
})
