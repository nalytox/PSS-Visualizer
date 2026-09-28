import { expect, test, type Page } from '@playwright/test';

const counter = (page: Page) => page.locator('.step-counter strong');
const terminal = (page: Page) => page.locator('.terminal-body');

async function waitForTrace(page: Page) {
  await expect(page.locator('g.proc').first()).toBeVisible({ timeout: 20_000 });
}

test('un ejemplo se compila, se traza y se recorre', async ({ page }) => {
  await page.goto('/#ejemplo=02_lista_enlazada&t=0');
  await waitForTrace(page);
  await page.keyboard.press('End');
  await expect(terminal(page)).toContainText('suma = 100');
  await expect(terminal(page)).toContainText('ahora la lista empieza en 30');
  // Al final quedan tres nodos sin liberar.
  await expect(page.locator('.mem-heap.leak')).toHaveCount(3);
});

test('un error de compilación se muestra en su línea', async ({ page }) => {
  await page.goto('/#ejemplo=01_structs&t=0');
  await waitForTrace(page);
  await page.getByRole('button', { name: 'Editar' }).click();
  const editor = page.locator('.cm-content');
  await editor.click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.type('int main(void) {\nreturn x;\n}\n');
  await page.getByRole('button', { name: 'Ejecutar' }).click();
  await expect(page.locator('.diagnostics')).toContainText('línea 2');
  await expect(page.locator('.cm-diag-error')).toHaveCount(1);
});

test('si el stdin se acaba, se pide más y la ejecución sigue desde ahí', async ({ page }) => {
  await page.goto('/#ejemplo=entrada_estandar&t=0');
  await waitForTrace(page);
  await page.keyboard.press('End');
  const more = page.getByLabel('Más entrada');
  await expect(more).toBeVisible();
  const before = Number(await counter(page).textContent());
  await more.fill('10');
  await more.press('Enter');
  // La nueva ejecución retoma en el mismo paso, con el 10 ya leído: ya no es el último paso.
  await expect(more).toBeHidden({ timeout: 20_000 });
  await expect(counter(page)).toHaveText(String(before));
  await page.keyboard.press('End');
  await page.getByRole('button', { name: /Enviar EOF/ }).click();
  await expect(page.getByRole('button', { name: /Enviar EOF/ })).toBeHidden({ timeout: 20_000 });
  await page.keyboard.press('End');
  await expect(terminal(page)).toContainText('Leí 4 números; la suma es 22');
});

test('el programa editado viaja en la URL', async ({ page }) => {
  await page.goto('/#ejemplo=01_structs&t=0');
  await waitForTrace(page);
  await page.getByRole('button', { name: 'Editar' }).click();
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.type('#include <stdio.h>\nint main(void) {\nprintf("desde la URL\\n");\nreturn 0;\n}\n');
  await page.getByRole('button', { name: 'Ejecutar' }).click();
  // Hasta que termina la nueva ejecución se sigue viendo la anterior.
  await expect(page.locator('.badge', { hasText: 'visualizando' })).toBeVisible({ timeout: 20_000 });
  await expect(page).toHaveURL(/codigo=/);
  const url = page.url();
  const other = await page.context().newPage();
  await other.goto(url);
  await waitForTrace(other);
  await other.keyboard.press('End');
  await expect(terminal(other)).toContainText('desde la URL');
});

test('fork + exec: el hijo se vuelve una caja negra que imprime la salida de ls', async ({ page }) => {
  await page.goto('/#ejemplo=05_exec&t=0');
  await waitForTrace(page);
  await page.keyboard.press('End');
  await expect(terminal(page)).toContainText('prog  prog.c');
  await expect(terminal(page)).toContainText('ls terminó');
  await expect(page.locator('g.proc')).toHaveCount(2);
});

test('un fork bomb termina en una traza truncada por el límite de procesos', async ({ page }) => {
  await page.goto('/#ejemplo=16_fork_bomb&t=0');
  await waitForTrace(page);
  await page.keyboard.press('End');
  await expect(page.locator('.narration')).toContainText('límite de 32 procesos');
});
