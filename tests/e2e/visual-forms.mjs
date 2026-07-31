// Smoke visual acotado para formularios de carga en tema oscuro.
// Uso: `npm run test:e2e` (corre todos los tests/e2e/*.mjs).

import { join } from "node:path";
import { ARTIFACTS, STUDENT, assert, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8141";

async function openPractice(page, practiceId) {
  const navItem = page.locator(`#practice-nav-children .nav-child[data-practice-id="${practiceId}"]`);
  if (await navItem.count()) {
    await navItem.click();
  } else {
    await page.evaluate((id) => {
      const select = document.querySelector("#practice-select");
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      document.querySelector('#practice-nav-children .nav-child')?.click();
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }, practiceId);
  }
  await page.waitForSelector("#measurement-fields .measurement-row, #measurement-fields .series-table");
}

async function assertInputContrast(page) {
  // Espera con polling (no un solo evaluate): justo tras cambiar de práctica el primer input
  // puede resolverse en un frame intermedio del re-render y devolver estilos vacíos.
  const handle = await page.waitForFunction(() => {
    const el = document.querySelector("#measurement-fields input");
    if (!el) return null;
    const s = getComputedStyle(el);
    return s.backgroundColor && s.color ? { background: s.backgroundColor, color: s.color } : null;
  }, { timeout: 10_000 });
  const sample = await handle.jsonValue();
  assert(sample.background !== sample.color, `input sin contraste: ${JSON.stringify(sample)}`);
  assert(!/255,\s*255,\s*255/.test(sample.background), `input blanco en tema oscuro: ${sample.background}`);
}

await withE2E({ port: PORT, dbName: "quantify-visual.db" }, async ({ login }) => {
  const { page } = await login(STUDENT, "estudiante");
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.click("#theme-toggle");

  await openPractice(page, "p2-cc");
  await assertInputContrast(page);
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-dark.png"), fullPage: true });

  // Tabs de partes: alternan secciones de la MISMA práctica sin recargar la definición.
  const partTabs = page.locator("#practice-part-tabs .part-tab");
  assert((await partTabs.count()) === 3, "p2-cc debe tener 3 tabs de partes");
  await partTabs.nth(0).click(); // Serie
  assert(
    await page.locator('#measurement-fields [data-section="serie"]').first().isVisible(),
    "la sección serie debe verse en su tab",
  );
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-serie.png"), fullPage: true });
  await partTabs.nth(1).click(); // Paralelo
  assert(
    await page.locator('#measurement-fields [data-section="paralelo"]').first().isVisible(),
    "la sección paralelo debe verse en su tab",
  );
  assert(
    await page.locator('#measurement-fields [data-section="serie"]').first().isHidden(),
    "la sección serie debe ocultarse fuera de su tab",
  );
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-paralelo.png"), fullPage: true });
  await partTabs.nth(2).click(); // Curva de potencia
  await page.waitForSelector("#measurement-fields .series-table", { state: "visible" });

  // Columna P en vivo: al tipear R e I la celda P muestra I²·R.
  const firstRow = page.locator("#measurement-fields .series-row").first();
  await firstRow.locator(".series-value").nth(0).fill("100"); // R
  await firstRow.locator(".series-value").nth(1).fill("0.5"); // I
  const liveText = await firstRow.locator(".series-live-value").innerText();
  assert(liveText.replace(",", ".").includes("25"), `celda P debía mostrar 25, mostró "${liveText}"`);
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-potencia.png"), fullPage: true });

  // Los resultados finales de potencia y los VR teóricos tienen todos input U (has_uncertainty
  // por defecto en el seed); solo se verifica que la fila de cada uno esté presente.
  for (const sym of ["P_max_e", "P_max_t", "RP_max_e", "RP_max_t", "VR1_s_t"]) {
    const row = page.locator(`#measurement-fields [data-final-result="1"][data-symbol="${sym}"]`);
    assert((await row.count()) === 1, `falta la fila de resultado final ${sym}`);
    assert((await row.locator(".final-result-u").count()) === 1, `${sym} debía tener input U`);
  }

  await openPractice(page, "fluidos-1");
  await assertInputContrast(page);
  await page.screenshot({ path: join(ARTIFACTS, "visual-fluidos1-dark.png"), fullPage: true });

  await openPractice(page, "fluidos-2");
  await assertInputContrast(page);
  await page.screenshot({ path: join(ARTIFACTS, "visual-fluidos2-dark.png"), fullPage: true });

  console.log("Visual forms smoke OK");
});
