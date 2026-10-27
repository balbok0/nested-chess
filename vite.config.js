import { defineConfig, loadEnv } from 'vite';
import { fileURLToPath } from 'node:url';
import { apiOrigin } from './web/mobile-protocol.js';
const headers = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' };
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), 'NESTED_');
  const mobile = mode === 'mobile';
  return {
    define: { 'import.meta.env.VITE_API_ORIGIN': JSON.stringify(mobile && env.NESTED_CHESS_API_URL ? apiOrigin(env.NESTED_CHESS_API_URL) : '') },
    plugins: mobile ? [{ name: 'mobile-shell', transformIndexHtml: html => html.replace(/<script src="[^"]*coi-serviceworker\.js"><\/script>/, '') }] : [],
    root: 'web',
    publicDir: 'vendor',
    server: { headers, proxy: { '/api': { target: 'http://localhost:4180', ws: true }, '/auth': { target: 'http://localhost:4180' } } },
    preview: { headers: mobile ? {} : headers },
    build: {
      outDir: mobile ? '../dist-mobile' : '../dist', emptyOutDir: true,
      rollupOptions: {
        input: {
          game: fileURLToPath(new URL('./web/index.html', import.meta.url)),
          analysis: fileURLToPath(new URL('./web/analysis.html', import.meta.url)),
          rectangularAnalysis: fileURLToPath(new URL('./web/rectangular-analysis.html', import.meta.url)),
        },
      },
    },
  };
});
