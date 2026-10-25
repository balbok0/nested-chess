import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';
const headers = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' };
export default defineConfig({
  root: 'web',
  publicDir: 'vendor',
  server: { headers, proxy: { '/api': { target: 'http://localhost:4180', ws: true }, '/auth': { target: 'http://localhost:4180' } } },
  preview: { headers },
  build: {
    outDir: '../dist', emptyOutDir: true,
    rollupOptions: {
      input: {
        game: fileURLToPath(new URL('./web/index.html', import.meta.url)),
        analysis: fileURLToPath(new URL('./web/analysis.html', import.meta.url)),
      },
    },
  },
});
