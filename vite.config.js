import { defineConfig } from 'vite';
const headers = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' };
export default defineConfig({
  root: 'web',
  publicDir: 'vendor',
  server: { headers },
  preview: { headers },
  build: { outDir: '../dist', emptyOutDir: true },
});
