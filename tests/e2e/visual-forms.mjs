
import { join } from "node:path";
import { ARTIFACTS, STUDENT, assert, waitForDraft, withE2E } from "./lib.mjs";

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
  const fieldsHtml = await page.locator("#measurement-fields").innerHTML();
  assert(fieldsHtml.includes("V<sub>G</sub>"), "Vg_s debía mostrarse como V_G (símbolo a mostrar)");
  assert(!fieldsHtml.includes("Vg<sub>S</sub>"), "Vg_s no debía mostrar el sufijo de la parte");

  const partTabs = page.locator("#practice-part-tabs .part-tab");
  assert((await partTabs.count()) === 3, "p2-cc debe tener 3 tabs de partes");
  await partTabs.nth(0).click();
  assert(
    await page.locator('#measurement-fields [data-section="p2-cc-serie"]').first().isVisible(),
    "la sección serie debe verse en su tab",
  );
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-serie.png"), fullPage: true });
  await partTabs.nth(1).click();
  assert(
    await page.locator('#measurement-fields [data-section="p2-cc-paralelo"]').first().isVisible(),
    "la sección paralelo debe verse en su tab",
  );
  assert(
    await page.locator('#measurement-fields [data-section="p2-cc-serie"]').first().isHidden(),
    "la sección serie debe ocultarse fuera de su tab",
  );
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-paralelo.png"), fullPage: true });
  await partTabs.nth(2).click();
  await page.waitForSelector("#measurement-fields .series-table", { state: "visible" });

  const firstRow = page.locator("#measurement-fields .series-row").first();
  await firstRow.locator(".series-value").nth(0).fill("100");
  await firstRow.locator(".series-value").nth(1).fill("0.5");
  const liveText = await firstRow.locator(".series-live-value").innerText();
  assert(liveText.replace(",", ".").includes("25"), `celda P debía mostrar 25, mostró "${liveText}"`);
  await page.screenshot({ path: join(ARTIFACTS, "visual-cc-potencia.png"), fullPage: true });

  for (const [sym, inputsU] of [
    ["P_max_e", 0],
    ["P_max_t", 0],
    ["RP_max_e", 0],
    ["RP_max_t", 0],
    ["VR1_s_t", 1],
  ]) {
    const row = page.locator(`#measurement-fields [data-final-result="1"][data-symbol="${sym}"]`);
    assert((await row.count()) === 1, `falta la fila de resultado final ${sym}`);
    assert((await row.locator(".final-result-u").count()) === inputsU, `${sym} debía tener ${inputsU} inputs U`);
  }

  await openPractice(page, "fluidos-1");
  await assertInputContrast(page);
  await page.screenshot({ path: join(ARTIFACTS, "visual-fluidos1-dark.png"), fullPage: true });

  const row = page.locator("#measurement-fields .series-row").first();
  for (const input of await row.locator(".series-value, .series-replica").all()) await input.fill("2");
  await row.locator(".series-point-result").first().fill("1234");
  await waitForDraft(page, { result: "Re#0", value: 1234 });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await openPractice(page, "fluidos-1");
  const restored = await page
    .waitForFunction(
      () => document.querySelector("#measurement-fields .series-row .series-point-result")?.value === "1234",
      null,
      { timeout: 10_000 },
    )
    .then(() => true, () => false);
  const restoredRe = await page
    .locator("#measurement-fields .series-row .series-point-result")
    .first()
    .inputValue({ timeout: 1_000 })
    .catch(() => "(sin campo Re)");
  assert(restored, `el borrador debía restaurar el Re de la corrida (vi: "${restoredRe}")`);

  await openPractice(page, "hidrostatica");
  await page.locator("#practice-part-tabs .part-tab").nth(1).click();
  const slot = page.locator('#measurement-fields input[placeholder^="ranura"]').first();
  await slot.waitFor();
  const layout = await slot.evaluate((input) => {
    const unit = input.parentElement.querySelector(".replica-unit");
    const box = input.getBoundingClientRect();
    return { width: box.width, overlaps: unit ? unit.getBoundingClientRect().left < box.right : false };
  });
  assert(layout.width >= 120 && !layout.overlaps, `el campo de ranura quedó angosto o tapado: ${JSON.stringify(layout)}`);

  await openPractice(page, "fluidos-2");
  await assertInputContrast(page);
  await page.screenshot({ path: join(ARTIFACTS, "visual-fluidos2-dark.png"), fullPage: true });

  console.log("Visual forms smoke OK");
});
