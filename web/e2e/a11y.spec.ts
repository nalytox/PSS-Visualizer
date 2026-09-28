// Revisión automática de accesibilidad (WCAG 2.1 AA) en las vistas principales y en ambos temas.
import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

const views = [
  ['visualizador con procesos y pipes', '/#traza=08_pipeline&t=28'],
  ['visualizador con hilos', '/#traza=13_hilos_mutex&t=10'],
  ['visualizador con señales', '/#traza=09_sigusr1&t=17'],
  ['introducción', '/'],
] as const;

for (const theme of ['light', 'dark'] as const) {
  for (const [name, url] of views) {
    test(`${name}, tema ${theme === 'light' ? 'claro' : 'oscuro'}: sin problemas de accesibilidad`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
      await page.goto(url);
      await expect(page.locator(url === '/' ? '.intro-caption' : 'g.proc').first()).toBeVisible({ timeout: 20_000 });
      await page.waitForTimeout(600);
      // La introducción es un diálogo modal: lo que queda detrás no es alcanzable mientras está abierta.
      const axe = new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']);
      const results = await (url === '/' ? axe.include('.intro') : axe).analyze();
      const found = results.violations.map((v) => `${v.id}: ${v.nodes.length} × ${v.nodes[0]?.target.join(' ')} — ${v.nodes[0]?.failureSummary?.split('\n')[1] ?? ''}`);
      expect(found).toEqual([]);
    });
  }
}
