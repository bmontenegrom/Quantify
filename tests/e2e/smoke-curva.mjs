// Smoke E2E de un análisis `analysis_kind = "curva"` (scatter sin ajuste, Motor B), sobre la
// práctica sembrada Filtros (barrido en frecuencia, dos curvas: razon/omega y phi/omega).
//
// No hay flujo E2E que ejercite este tipo de análisis todavía (run.mjs cubre estadístico,
// smoke-fluidos2.mjs cubre regresión). Recorre:
//   1. Estudiante: abre Filtros, completa los escalares medidos + el dato de cátedra L,
//      carga un barrido de puntos (f, VRpp, Vgpp, a, b) y entrega.
//   2. Docente: abre la entrega en revisión y ve las dos curvas (scatter + tabla, sin
//      mensurandos con incertidumbre — comportamiento esperado de "curva").
//
// Uso: `npm run test:e2e` (corre todos los tests/e2e/*.mjs). Variables: E2E_PORT,
// E2E_SKIP_BUILD, E2E_HEADED.

import { STUDENT, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8149";

await withE2E({ port: PORT, dbName: "quantify-curva.db" }, async ({ login }) => {
  // ── 1) Estudiante entrega Filtros ────────────────────────────────────────
  step("estudiante: login");
  const { page: sPage } = await login(STUDENT, "estudiante");

  step("estudiante: abre la práctica Filtros desde el nav");
  await sPage.click('#practice-nav-children .nav-child:has-text("Filtros")');
  await sPage.waitForSelector(".series-table");
  await sPage.selectOption("#table-select", "1");

  // Mapa símbolo→id desde la definición (robusto frente a cambios de orden).
  const idBySym = await sPage.evaluate(async () => {
    const r = await fetch("/api/practices/filtros/definition");
    const def = await r.json();
    return Object.fromEntries(def.quantities.map((q) => [q.symbol, q.id]));
  });

  step("estudiante: completa los escalares medidos (R, C1, C2, fpasaje_exp, fbloqueo_exp)");
  const shared = { R: 220, C1: 1e-8, C2: 4.7e-8, fpasaje_exp: 2200, fbloqueo_exp: 950 };
  for (const [sym, val] of Object.entries(shared)) {
    const row = sPage.locator(`.shared-quantities .measurement-row[data-quantity-id="${idBySym[sym]}"]`);
    await row.locator(".measure-value").first().fill(String(val));
  }
  // L es dato de cátedra (valor ± U).
  const lRow = sPage.locator(`.measurement-row--given[data-quantity-id="${idBySym["L"]}"]`);
  await lRow.locator(".measure-given-value").fill("0.047");
  await lRow.locator(".measure-given-u").fill("0.001");

  step("estudiante: carga un barrido de 3 puntos (f, VRpp, Vgpp, a, b)");
  const points = [
    { f: 500, VRpp: 1.2, Vgpp: 5.0, a: 5.0, b: 1.2 },
    { f: 1500, VRpp: 3.1, Vgpp: 5.0, a: 5.0, b: 2.9 },
    { f: 4000, VRpp: 0.8, Vgpp: 5.0, a: 5.0, b: 0.7 },
  ];
  const rows = sPage.locator(".series-table tbody .series-row");
  for (let i = 0; i < points.length; i++) {
    const row = rows.nth(i);
    for (const [sym, val] of Object.entries(points[i])) {
      await row.locator(`.series-value[data-quantity-id="${idBySym[sym]}"]`).fill(String(val));
    }
  }

  step("estudiante: entrega el formulario");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("estudiante: el análisis sigue oculto (gating del server)");
  const latest = (await sPage.locator("#latest-result").textContent()) ?? "";
  assert(
    latest.includes("El docente todavia no habilito"),
    "la entrega recién creada no debía mostrar el análisis al estudiante (gating)",
  );

  // ── 2) Docente revisa y ve las curvas (scatter sin ajuste) ───────────────
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

  console.log("entrega + análisis de curva (Filtros) sin errores JS.");
});
