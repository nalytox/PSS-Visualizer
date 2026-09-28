import { expect, test } from '@playwright/test';

test('la introducción se abre sola en la primera visita y se puede saltar', async ({ page }) => {
  await page.goto('/');
  const intro = page.getByRole('dialog', { name: 'Introducción' });
  await expect(intro).toBeVisible();
  await expect(page.locator('.intro-caption')).not.toBeEmpty();
  await page.getByRole('button', { name: 'Saltar introducción' }).click();
  await expect(intro).toBeHidden();
  await page.reload();
  await expect(page.locator('g.proc').first()).toBeVisible({ timeout: 20_000 });
  await expect(intro).toBeHidden();
  // Queda disponible desde la barra superior.
  await page.getByRole('button', { name: 'Introducción' }).click();
  await expect(intro).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(intro).toBeHidden();
});

test('al terminar un capítulo se puede probar su ejemplo', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '3. Pipes' }).click();
  await page.getByLabel('Progreso del capítulo').fill('31');
  await page.getByRole('button', { name: 'Probar este ejemplo' }).click();
  await expect(page.getByRole('dialog', { name: 'Introducción' })).toBeHidden();
  await expect(page.locator('select').first()).toHaveValue('ej:06_pipe_padre_hijo');
  await expect(page.locator('g.proc').first()).toBeVisible({ timeout: 20_000 });
});

test.describe('con movimiento reducido', () => {
  test('se avanza por fotogramas clave con clic', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/');
    const caption = page.locator('.intro-caption');
    const first = await caption.textContent();
    await expect(page.getByLabel('Progreso del capítulo')).toHaveCount(0);
    await page.locator('.intro-stage').click();
    await expect(caption).not.toHaveText(first ?? '');
    await expect(page.locator('.intro-frames')).toHaveText('2 / 4');
  });
});
