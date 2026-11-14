
import { ARTIFACTS, STUDENT, TEACHER, assert, openAddForm, step, symbolText, waitForDraft, withE2E } from "./lib.mjs";
import { join } from "node:path";

const PORT = process.env.E2E_PORT ?? "8137";

await withE2E({ port: PORT, dbName: "quantify-smoke.db" }, async ({ login }) => {
  step("estudiante: login");
  const { page: sPage } = await login(STUDENT, "estudiante");

  step("estudiante: abre la práctica Fluidos II desde el nav");
  await sPage.click('#practice-nav-children .nav-child:has-text("Fluidos II")');
  await sPage.waitForSelector(".series-table");

  const idBySym = await sPage.evaluate(async () => {
    const r = await fetch("/api/practices/fluidos-2/definition");
    const def = await r.json();
    return Object.fromEntries(def.quantities.map((q) => [q.symbol, q.id]));
  });

  step("verifica estructura: datos compartidos + grilla por punto h/t");
  for (const sym of ["h_max", "R_cap", "L_cap", "R_recip", "g", "rho", "mu_agua", "kp", "Temp"]) {
    const count = await sPage
      .locator(`.shared-quantities .measurement-row[data-quantity-id="${idBySym[sym]}"]`)
      .count();
    assert(count === 1, `falta el escalar compartido ${sym} en "Datos compartidos"`);
  }
  const headers = (await sPage.locator(".series-table thead th").allTextContents()).join(" ");
  assert(/\bh\b/.test(headers), `la tabla de la serie debía tener columna h (vi: ${headers})`);
  assert(/\bt\b/.test(headers), `la tabla de la serie debía tener columna t (vi: ${headers})`);
  await sPage.screenshot({ path: join(ARTIFACTS, "fluidos2-form.png"), fullPage: true });

  step("estudiante: selecciona mesa");
  await sPage.selectOption("#table-select", "1");

  step("estudiante: completa los escalares compartidos");
  const shared = { h_max: 0.36, R_cap: 0.001, L_cap: 0.1, R_recip: 0.05, rho: 1000, Temp: 20 };
  for (const [sym, val] of Object.entries(shared)) {
    const row = sPage.locator(`.shared-quantities .measurement-row[data-quantity-id="${idBySym[sym]}"]`);
    await row.locator(".measure-value").first().fill(String(val));
  }
  const given = { mu_agua: 0.001, kp: 0.78 };
  for (const [sym, val] of Object.entries(given)) {
    const row = sPage.locator(`.measurement-row--given[data-quantity-id="${idBySym[sym]}"]`);
    await row.locator(".measure-given-value").fill(String(val));
  }
  const gRow = sPage.locator(`.measurement-row--given[data-quantity-id="${idBySym["g"]}"]`);
  await gRow.locator(".measure-given-value").fill("9.8");
  await gRow.locator(".measure-given-u").fill("0.1");

  step("estudiante: carga 3 puntos (h, t)");
  const points = [
    { h: 0.36, t: 0 },
    { h: 0.25, t: 10 },
    { h: 0.16, t: 20 },
  ];
  const rows = sPage.locator(".series-table tbody .series-row");
  for (let i = 0; i < points.length; i++) {
    const row = rows.nth(i);
    await row.locator(`.series-value[data-quantity-id="${idBySym["h"]}"]`).fill(String(points[i].h));
    await row.locator(`.series-value[data-quantity-id="${idBySym["t"]}"]`).fill(String(points[i].t));
  }

  step("verifica que la vista previa del ajuste aparece");
  await sPage.locator(".series-table").dispatchEvent("change");
  await sPage.waitForFunction(
    () => (document.querySelector(".series-preview")?.textContent ?? "").trim().length > 0,
    { timeout: 10_000 },
  );
  await sPage.screenshot({ path: join(ARTIFACTS, "fluidos2-preview.png"), fullPage: true });

  step("estudiante: recarga y el borrador restaura la serie");
  await waitForDraft(sPage, { quantityId: idBySym["t"], count: points.length });
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  await sPage.click('#practice-nav-children .nav-child:has-text("Fluidos II")');
  await sPage
    .waitForFunction(
      ({ id, value }) =>
        [...document.querySelectorAll(`.series-value[data-quantity-id="${id}"]`)].some((i) => i.value === value),
      { id: idBySym["h"], value: String(points.at(-1).h) },
      { timeout: 10_000 },
    )
    .catch(() => {});
  for (const sym of ["h", "t"]) {
    const restored = await sPage.$$eval(`.series-value[data-quantity-id="${idBySym[sym]}"]`, (inputs) =>
      inputs.map((input) => input.value),
    );
    const expected = points.map((p) => String(p[sym]));
    assert(
      JSON.stringify(restored) === JSON.stringify(expected),
      `el borrador debía restaurar ${sym} = ${expected} (vi: ${restored})`,
    );
  }
  const restoredRho = await sPage
    .locator(`.shared-quantities .measurement-row[data-quantity-id="${idBySym["rho"]}"] .measure-value`)
    .first()
    .inputValue();
  assert(restoredRho === String(shared.rho), `el borrador debía restaurar rho (vi: ${restoredRho})`);
  await sPage.waitForSelector('.series-preview h4:has-text("Vista previa del ajuste")', { timeout: 10_000 });
  await sPage.selectOption("#table-select", "1");

  step("estudiante: entrega el formulario");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("estudiante: el cálculo automático sigue oculto (gating del server)");
  const latest = (await sPage.locator("#latest-result").textContent()) ?? "";
  assert(
    latest.includes("El docente todavia no habilito"),
    "la entrega recién creada no debía mostrar el análisis al estudiante (gating)",
  );

  step("docente: login");
  const { page: tPage } = await login(TEACHER, "docente");

  step("docente: abre la entrega de Fluidos II en revisión");
  await tPage.click('.tab.teacher-only[data-view="submissions"]');
  await tPage.click('.submission-item:has-text("Fluidos II")');
  await tPage.waitForSelector(".review-form");

  step("docente: ve el ajuste, M_medio y la tabla de mensurandos agregados");
  const detail = (await tPage.locator("#submission-detail-body").textContent()) ?? "";
  assert(/Mensurandos agregados/.test(detail), "el docente debía ver la tabla de mensurandos agregados");
  for (const sym of ["M_medio", "Re_max", "Re_min", "Re_medio", "M_teorico"]) {
    assert(detail.includes(symbolText(sym)), `el análisis debía mostrar ${sym}`);
  }
  await tPage.screenshot({ path: join(ARTIFACTS, "fluidos2-analisis.png"), fullPage: true });

  step("docente: abre Fluidos II en el admin de prácticas");
  await tPage.click('.tab.teacher-only[data-view="practices"]');
  await tPage.click('[data-practice-open][data-practice-id="fluidos-2"]');
  await openAddForm(tPage, "#new-quantity-form");

  step("docente: agrega una magnitud sin unidad (adimensional)");
  const qForm = tPage.locator("#new-quantity-form");
  await qForm.locator('input[name="symbol"]').fill("factor_test");
  await qForm.locator('input[name="name"]').fill("Factor de prueba adimensional");
  await qForm.locator('input[name="unit"]').fill("");
  await qForm.locator('button[type="submit"]').click();

  step("verifica que la magnitud adimensional se guardó y se muestra como tal");
  await tPage.waitForFunction(
    () => document.querySelector("#practice-workspace")?.textContent?.includes("Magnitud agregada"),
    { timeout: 10_000 },
  );
  const adminText = (await tPage.locator("#practice-workspace").textContent()) ?? "";
  const factorRow = tPage.locator('#practice-workspace tr:has(td:first-child code:text-is("factor_test"))');
  assert((await factorRow.count()) === 1, "la magnitud adimensional debía aparecer en una fila de la lista");
  assert(((await factorRow.textContent()) ?? "").includes("adimensional"), 'la unidad vacía debía mostrarse como "adimensional"');
  assert(
    !adminText.includes("datos de magnitud invalidos"),
    "no debía rechazar la magnitud por unidad vacía",
  );
  await tPage.screenshot({ path: join(ARTIFACTS, "fluidos2-admin-adimensional.png"), fullPage: true });

  console.log("entrega+análisis (M_medio/agregados) y alta de magnitud adimensional desde el admin, sin errores JS.");
});
