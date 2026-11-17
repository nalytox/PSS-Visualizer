import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const counter = (page: Page) => page.locator('.step-counter strong');
const theory = (page: Page) => page.getByRole('complementary', { name: 'Teoría' });

async function open(page: Page, url: string) {
  await page.goto(url);
  await expect(page.locator('g.proc').first()).toBeVisible({ timeout: 20_000 });
}

test('la teoría acompaña cada paso de un ejemplo: introducción, línea, fichas y cierre', async ({ page }) => {
  await open(page, '/#traza=03_fork_simple&t=0');
  await expect(theory(page).locator('.theory-intro h3')).toHaveText('fork: un proceso se vuelve dos');
  await expect(theory(page).locator('.theory-goal')).toContainText('Objetivo:');

  // Paso 2: el padre ejecuta la línea 7 y nace el hijo.
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(counter(page)).toHaveText('2');
  await expect(theory(page).locator('.theory-line h3')).toContainText('Línea 7');
  await expect(theory(page).locator('.concept summary')).toHaveText(['fork(): un proceso se vuelve dos']);
  await expect(theory(page).locator('.theory-intro')).toHaveCount(0);

  await page.keyboard.press('End');
  await expect(theory(page).locator('.theory-end')).toContainText('Al terminar');
  await expect(theory(page).locator('.theory-end')).toContainText('Para practicar');
});

test('la lección completa se lee de corrido y lleva al paso que ejecuta una línea', async ({ page }) => {
  await open(page, '/#traza=03_fork_simple&t=0');
  await theory(page).getByRole('button', { name: 'Lección completa' }).click();
  const dialog = page.getByRole('dialog', { name: 'fork: un proceso se vuelve dos' });
  await expect(dialog).toBeVisible();
  // Las notas siguen el orden de la ejecución.
  await expect(dialog.locator('.lesson-line h4').first()).toContainText('Línea 6');

  // Mientras está abierta, las flechas no mueven el reproductor.
  await page.keyboard.press('ArrowRight');
  await expect(counter(page)).toHaveText('0');

  await dialog.locator('.lesson-line', { hasText: 'Línea 16' }).getByRole('button', { name: 'Ir a esta línea' }).click();
  await expect(dialog).toBeHidden();
  // El paso 10 es el primero que ejecuta wait (y bloquea al padre).
  await expect(counter(page)).toHaveText('10');
  await expect(theory(page).locator('.theory-line h3')).toContainText('Línea 16');

  await theory(page).getByRole('button', { name: 'Lección completa' }).click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
});

test('la lección completa no tiene problemas de accesibilidad', async ({ page }) => {
  await open(page, '/#traza=09_sigusr1&t=0');
  await theory(page).getByRole('button', { name: 'Lección completa' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).include('.lesson-dialog').analyze();
  expect(results.violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(' ')}`)).toEqual([]);
});

test('un programa sin lección muestra las fichas de sus eventos', async ({ page }) => {
  await open(page, '/#traza=fork_pipe&t=0');
  await expect(theory(page).getByRole('button', { name: 'Lección completa' })).toHaveCount(0);
  await expect(theory(page).locator('.concept summary')).toHaveText(['Qué es un proceso']);
  await page.keyboard.press('Shift+ArrowRight'); // siguiente evento
  await expect(theory(page).locator('.concept').first()).toBeVisible();
  await expect(theory(page).locator('.theory-line')).toHaveCount(0);
});

test('la teoría se oculta y lo recuerda', async ({ page }) => {
  await open(page, '/#traza=03_fork_simple&t=0');
  const toggle = page.getByRole('button', { name: 'Teoría', exact: true });
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await theory(page).getByRole('button', { name: 'Ocultar la teoría' }).click();
  await expect(theory(page)).toHaveCount(0);
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await page.reload();
  await expect(page.locator('g.proc').first()).toBeVisible();
  await expect(theory(page)).toHaveCount(0);
  await toggle.click();
  await expect(theory(page)).toBeVisible();
});

test('el código se agranda y se achica arrastrando sus esquinas', async ({ page }) => {
  await open(page, '/#traza=01_structs&t=0');
  const panel = page.locator('.code-panel');
  const start = (await panel.boundingBox())!;

  const drag = async (corner: string, dx: number, dy: number) => {
    const box = (await page.locator(`.resize-corner.${corner}`).boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + dx, box.y + box.height / 2 + dy, { steps: 6 });
    await page.mouse.up();
  };

  // Abajo a la derecha: hacia afuera agranda en las dos direcciones.
  await drag('bottom-right', 120, 100);
  const bigger = (await panel.boundingBox())!;
  expect(bigger.width).toBeCloseTo(start.width + 120, -1);
  expect(bigger.height).toBeCloseTo(start.height + 100, -1);

  // Arriba a la izquierda: hacia adentro achica.
  await drag('top-left', 60, 40);
  const smaller = (await panel.boundingBox())!;
  expect(smaller.width).toBeCloseTo(bigger.width - 60, -1);
  expect(smaller.height).toBeCloseTo(bigger.height - 40, -1);

  // El tamaño se recuerda.
  await page.reload();
  await expect(page.locator('g.proc').first()).toBeVisible();
  expect((await panel.boundingBox())!.width).toBeCloseTo(smaller.width, 0);

  // Con teclado: las flechas cambian el tamaño, no el paso; Inicio vuelve al original.
  const handle = page.getByRole('button', { name: /Cambiar el tamaño del código/ });
  await handle.focus();
  await page.keyboard.press('ArrowRight');
  await expect(counter(page)).toHaveText('0');
  expect((await panel.boundingBox())!.width).toBeCloseTo(smaller.width + 20, 0);
  await page.keyboard.press('Home');
  expect((await panel.boundingBox())!.width).toBeCloseTo(start.width, 0);

  // Doble clic en cualquier esquina también restablece.
  await drag('bottom-left', -80, 0);
  expect((await panel.boundingBox())!.width).toBeCloseTo(start.width + 80, -1);
  await page.locator('.resize-corner.bottom-left').dblclick();
  expect((await panel.boundingBox())!.width).toBeCloseTo(start.width, 0);
});
