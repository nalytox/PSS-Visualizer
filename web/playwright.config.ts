import { defineConfig } from '@playwright/test';

// Pruebas de extremo a extremo contra el servidor real (pss-server + pss-tracer) sirviendo la
// compilación de producción. Antes: npm run build y cargo build --release (make e2e lo hace).
export default defineConfig({
  testDir: 'e2e',
  timeout: 45_000,
  use: { baseURL: 'http://localhost:4173', viewport: { width: 1440, height: 900 } },
  webServer: {
    command: '../target/release/pss-server',
    env: { PSS_PORT: '4173', PSS_WEB_DIST: 'dist', PSS_TRACER: '../target/release/pss-tracer', PSS_LIMITS: '../config/limits.toml' },
    port: 4173,
    reuseExistingServer: !process.env.CI,
  },
});
