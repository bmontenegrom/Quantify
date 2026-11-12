
import { STUDENT, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8149";

async function fillSweep(page) {
  const idBySym = await page.evaluate(async () => {
    const r = await fetch("/api/practices/filtros/definition");
    const def = await r.json();
    return Object.fromEntries(def.quantities.map((q) => [q.symbol, q.id]));
  });
  const shared = { R: 220, C1: 1e-8, C2: 4.7e-8, fpasaje_exp: 2200, fbloqueo_exp: 950 };
  for (const [sym, val] of Object.entries(shared)) {
    const row = page.locator(`.shared-quantities .measurement-row[data-quantity-id="${idBySym[sym]}"]`);
    await row.locator(".measure-value").first().fill(String(val));
  }
  const lRow = page.locator(`.measurement-row--given[data-quantity-id="${idBySym["L"]}"]`);
  await lRow.locator(".measure-given-value").fill("0.047");
  await lRow.locator(".measure-given-u").fill("0.001");
  const points = [
    { f: 500, VRpp: 1.2, Vgpp: 5.0, a: 5.0, b: 1.2 },
    { f: 1500, VRpp: 3.1, Vgpp: 5.0, a: 5.0, b: 2.9 },
    { f: 4000, VRpp: 0.8, Vgpp: 5.0, a: 5.0, b: 0.7 },
  ];
  const rows = page.locator(".series-table tbody .series-row");
  for (let i = 0; i < points.length; i++) {
    const row = rows.nth(i);
    for (const [sym, val] of Object.entries(points[i])) {
      await row.locator(`.series-value[data-quantity-id="${idBySym[sym]}"]`).fill(String(val));
    }
  }
  return { rows, idBySym };
}

await withE2E({ port: PORT, dbName: "quantify-curva.db" }, async ({ login }) => {
  step("estudiante: login");
  const { page: sPage } = await login(STUDENT, "estudiante");

  step("estudiante: abre la práctica Filtros desde el nav");
  await sPage.click('#practice-nav-children .nav-child:has-text("Filtros")');
  await sPage.waitForSelector(".series-table");
  await sPage.selectOption("#table-select", "1");

  step("estudiante: completa los escalares y un barrido de 3 puntos");
  await fillSweep(sPage);

  step("estudiante: entrega el formulario");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("estudiante: el análisis sigue oculto (gating del server)");
  const latest = (await sPage.locator("#latest-result").textContent()) ?? "";
  assert(
    latest.includes("El docente todavia no habilito"),
    "la entrega recién creada no debía mostrar el análisis al estudiante (gating)",
  );

  step("docente: login");
  const { page: tPage } = await login(TEACHER, "docente");

  step("docente: abre la entrega de Filtros en revisión");
  await tPage.click('.tab.teacher-only[data-view="submissions"]');
  await tPage.click('.submission-item:has-text("Filtros")');
  await tPage.waitForSelector(".review-form");

  step("docente: ve las dos curvas (razon/omega y phi/omega), sin mensurandos con incertidumbre");
  const detail = (await tPage.locator("#submission-detail-body").textContent()) ?? "";
  assert(
    /Curvas? \(puntos sin ajuste\)/.test(detail),
    "el análisis de Filtros debía mostrarse como curva sin ajuste",
  );
  assert(detail.includes("omega"), "la curva debía graficar contra omega (frecuencia angular)");
  assert(
    !/±.*NaN/.test(detail),
    "una práctica curva no debía mostrar mensurandos con incertidumbre rota",
  );

  step("docente: la vista previa dibuja con los símbolos de la práctica aunque otra pantalla los cambie");
  await tPage.click('.tab.teacher-only[data-view="practices"]');
  await tPage.click('[data-practice-open][data-practice-id="filtros"]');
  await tPage.click("[data-practice-try-draft]");
  await tPage.waitForSelector(".series-table");
  const { rows, idBySym } = await fillSweep(tPage);
  await tPage.locator("#measurement-fields .metric-label").first().waitFor();
  await tPage.evaluate(async () => {
    const def = await (await fetch("/api/practices/filtros/definition")).json();
    const symbols = [...def.results, ...def.point_results, ...def.intermediates].map((item) => [item.symbol, "ZZ"]);
    const { setDisplaySymbols } = await import("/lib.js");
    setDisplaySymbols(Object.fromEntries(symbols));
  });
  const previewed = tPage.waitForResponse((r) => r.url().endsWith("/analyze-preview"));
  await rows.nth(0).locator(`.series-value[data-quantity-id="${idBySym.f}"]`).fill("600");
  await previewed;
  await tPage.waitForTimeout(500);
  const labels = await tPage.locator("#measurement-fields .metric-label").allTextContents();
  assert(labels.length > 0, "la vista previa debía mostrar valores derivados");
  assert(!labels.some((label) => label.includes("ZZ")), `símbolos de otra pantalla: ${labels.join(", ")}`);

  console.log("entrega + análisis de curva (Filtros) sin errores JS.");
});
