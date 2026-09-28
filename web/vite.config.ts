import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

const repoRoot = fileURLToPath(new URL('..', import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@traces': repoRoot + 'traces', '@examples': repoRoot + 'examples' },
  },
  server: {
    fs: { allow: [repoRoot] },
    // En desarrollo (npm run dev) las ejecuciones van al servidor local de ./pss.
    proxy: { '/api': 'http://127.0.0.1:8000' },
  },
  // La aplicación se sirve desde localhost: el tamaño del paquete no pesa como en la web.
  build: { chunkSizeWarningLimit: 1200 },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
