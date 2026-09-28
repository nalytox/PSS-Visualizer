import { expect, test, type Page } from '@playwright/test';

const TRACES = ['fork_pipe', 'threads_mutex', 'signal_handler', 'structs_heap', 'fork_tree'];

function collectErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  return errors;
}

const counter = (page: Page) => page.locator('.step-counter strong');

for (const name of TRACES) {
  test(`${name}: se recorre completa sin errores`, async ({ page }) => {
    const errors = collectErrors(page);
    await page.goto(`/#traza=${name}&t=0`);
    await expect(page.locator('g.proc').first()).toBeVisible();
    await expect(counter(page)).toHaveText('0');
    await page.keyboard.press('End');
    const last = Number(await counter(page).textContent());
    expect(last).toBeGreaterThan(5);
    for (let t = last; t > 0; t--) await page.keyboard.press('ArrowLeft');
    await expect(counter(page)).toHaveText('0');
    expect(errors).toEqual([]);
  });
}

test('atajos de teclado de la sección 10', async ({ page }) => {
  await page.goto('/#traza=fork_pipe&t=0');
  await expect(page.locator('g.proc').first()).toBeVisible();
  await page.keyboard.press('ArrowRight');
  await expect(counter(page)).toHaveText('1');
  await page.keyboard.press('Shift+ArrowRight');
  await expect(counter(page)).toHaveText('2');
  await page.keyboard.press('End');
  await expect(counter(page)).toHaveText('20');
  await page.keyboard.press('Home');
  await expect(counter(page)).toHaveText('0');
});

test('el paso actual se comparte por URL', async ({ page }) => {
  await page.goto('/#traza=threads_mutex&t=16');
  await expect(counter(page)).toHaveText('16');
  await expect(page.locator('g.proc').first()).toBeVisible();
  await page.keyboard.press('ArrowRight');
  await expect(page).toHaveURL(/traza=threads_mutex&t=17/);
});

test('el tema cambia entre sistema, claro y oscuro', async ({ page }) => {
  await page.goto('/#traza=fork_pipe&t=0');
  const button = page.getByRole('button', { name: /Cambiar tema/ });
  await button.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await button.click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('los elementos del lienzo son inspeccionables con teclado', async ({ page }) => {
  await page.goto('/#traza=fork_pipe&t=10');
  const pipe = page.locator('g.pipe-tube').first();
  await pipe.focus();
  await expect(page.locator('.tooltip')).toContainText('Pipe p0');
});

test('un paso responde en menos de 100 ms con 16 procesos en pantalla', async ({ page }) => {
  await page.goto('/#traza=fork_tree&t=60');
  await expect(page.locator('g.proc')).toHaveCount(16);
  const median = await page.evaluate(async () => {
    const times: number[] = [];
    const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    for (let i = 0; i < 20; i++) {
      await new Promise((r) => setTimeout(r, 150));
      const start = performance.now();
      window.dispatchEvent(new KeyboardEvent('keydown', { key: i % 2 ? 'ArrowLeft' : 'ArrowRight' }));
      await frame();
      times.push(performance.now() - start);
    }
    times.sort((a, b) => a - b);
    return times[10];
  });
  expect(median).toBeLessThan(100);
});
