import { join } from "node:path";
import { ARTIFACTS, STUDENT, TEACHER, apiPost, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8191";
const NAME = "Determinaciones";
const ID = "determinaciones";

const getJson = (page, url) => page.evaluate(async (u) => (await fetch(u)).json(), url);
const determinations = (display) => ({ determinations: { count: 3, label: "Medida", average: true, display } });

await withE2E({ port: PORT, dbName: "quantify-determinaciones.db" }, async ({ login }) => {
  step("docente: crea una práctica con 3 medidas promediadas y un resultado que usa el promedio");
  const { page } = await login(TEACHER, "docente");
  const base = `/api/practices/${ID}`;
  await apiPost(page, "/api/practices", { name: NAME, analysis_kind: "estadistico" });
  await apiPost(page, `${base}/determinations`, determinations("bloques"));
  const T = await apiPost(page, `${base}/quantities`, {
    symbol: "T", name: "Periodo", unit: "s", repeated: true, quantity: "tiempo", input_widget: "teclado", per_determination: true,
  });
  const m = await apiPost(page, `${base}/quantities`, {
    symbol: "m", name: "Masa", unit: "kg", repeated: false, is_given: true, per_determination: true,
  });
  const x = await apiPost(page, `${base}/quantities`, {
    symbol: "x", name: "Extra", unit: "s", repeated: true, quantity: "tiempo", input_widget: "teclado", per_determination: true, optional: true, default_value: 9,
  });
  await apiPost(page, `${base}/results`, { symbol: "g", name: "Gravedad", unit: "m/s2", formula: "m/T^2", is_final: true });
  await apiPost(page, `${base}/results`, { symbol: "rel", name: "Relativo", unit: "", formula: "g_medio/9.8", is_final: true });
  await apiPost(page, `${base}/status`, { status: "publicada" });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="courses"]');
  await page.click("[data-course-open]");
  await page.selectOption('#course-practice-form select[name="practice_id"]', ID);
  await page.click('#course-practice-form button[type="submit"]');
  await page.waitForSelector(`[data-disable-practice][data-practice-id="${ID}"]`);

  step("estudiante: cada magnitud por medida tiene tres bloques y el dato de cátedra una sola U");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  const group = (q) => sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${q.id}"]`);
  await group(T).waitFor();
  const labels = await group(T).locator(".operator-label").allTextContents();
  assert(labels.join("|") === "Medida 1|Medida 2|Medida 3", `rótulos inesperados: ${labels}`);
  assert((await group(m).locator(".measure-given-u").count()) === 1, "el dato de cátedra debía tener una sola U");
  assert((await group(m).locator(".measure-value").count()) === 3, "el dato de cátedra debía tener un valor por medida");

  step("estudiante: carga las tres medidas (la Extra sin la 2), sus resultados y entrega");
  for (const [i, v] of ["2.0", "2.1", "1.9"].entries()) {
    await group(T).locator(".measurement-row").nth(i).locator(".measure-value").first().fill(v);
    await group(m).locator(".measure-value").nth(i).fill(String(40 + i));
  }
  await group(m).locator(".measure-given-u").fill("0.5");
  await group(x).locator(".measurement-row").nth(0).locator(".measure-value").first().fill("1");
  await group(x).locator(".measurement-row").nth(1).locator(".measure-value").first().fill("");
  await group(x).locator(".measurement-row").nth(2).locator(".measure-value").first().fill("5");
  await sPage.locator('[data-final-result][data-symbol="g#2"] .final-result-value').fill("9.5");
  await sPage.locator('[data-final-result][data-symbol="g_medio"] .final-result-value').fill("10");
  await sPage.waitForFunction(
    (id) =>
      Object.keys(localStorage).some((k) =>
        k.startsWith("quantify-draft:") &&
        JSON.parse(localStorage.getItem(k)).measurements?.some((mm) => mm.quantity_id === id && mm.given_u === 0.5),
      ),
    m.id,
  );
  await sPage.selectOption("#table-select", "1");
  await sPage.waitForLoadState("networkidle");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("docente: el análisis trae cada medida, el promedio y el resultado que lo usa");
  const [submission] = (await getJson(page, "/api/submissions")).filter((s) => s.practice_id === ID);
  assert(submission, "la entrega debía estar en la lista del docente");
  const detail = await getJson(page, `/api/submissions/${submission.id}`);
  const dets = detail.analysis.determinations;
  assert(dets?.length === 3, `debía haber tres medidas, hay ${dets?.length}`);
  const g2 = dets[1].derived.find((d) => d.symbol === "g");
  assert(Math.abs(g2.value - 41 / 2.1 ** 2) < 1e-9, `g de la medida 2 inesperado: ${g2.value}`);
  const medio = detail.analysis.derived.find((d) => d.symbol === "g_medio");
  const rel = detail.analysis.derived.find((d) => d.symbol === "rel");
  assert(medio && rel, "debían estar g_medio y rel");
  assert(Math.abs(rel.value - medio.value / 9.8) < 1e-9, `rel debía ser g_medio/9.8: ${rel.value}`);
  const symbols = detail.student_results.map((s) => s.symbol).sort();
  assert(symbols.join(",") === "g#2,g_medio", `resultados del alumno inesperados: ${symbols}`);
  await page.evaluate((id) => import("/submissions.js").then((mod) => mod.openSubmissionWorkspace(id)), submission.id);
  await page.waitForSelector('h3:has-text("Comparación: tus cálculos vs automático — Medida 2")');
  await page.screenshot({ path: join(ARTIFACTS, "determinaciones-analisis.png"), fullPage: true });

  step("docente: pasa a pestañas; al editar, cada medida vuelve a su lugar aunque la 2 esté vacía");
  await apiPost(page, `${base}/determinations`, determinations("pestanas"));
  await apiPost(page, `${base}/versions`, {});
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await sPage.click(".view-existing-btn");
  await sPage.click(".edit-submission-btn");
  await group(x).waitFor();
  const tabs = sPage.locator("#measurement-fields [data-det-tabs] .part-tab");
  assert((await tabs.count()) === 3, "debía haber una pestaña por medida");
  const visible = (q) =>
    group(q).locator(".operator-block").evaluateAll((blocks) => blocks.map((b) => !b.classList.contains("det-hidden")));
  assert((await visible(x)).join() === "true,false,false", "solo la medida 1 debía verse al abrir");
  await tabs.nth(2).click();
  assert((await visible(x)).join() === "false,false,true", "la pestaña 3 debía mostrar la medida 3");
  const value = (i) => group(x).locator(".measurement-row").nth(i).locator(".measure-value").first().inputValue();
  assert((await value(1)) === "", "la medida 2 de Extra debía quedar vacía");
  assert((await value(2)) === "5", `la medida 3 de Extra debía conservar 5, vi "${await value(2)}"`);
  assert((await group(m).locator(".measure-given-u").inputValue()) === "0.5", "la U del dato de cátedra debía volver");
  assert(
    await sPage.locator('[data-final-result][data-symbol="g#3"]').isVisible(),
    "el resultado final de la medida 3 debía verse en su pestaña",
  );
  await sPage.screenshot({ path: join(ARTIFACTS, "determinaciones-pestanas.png"), fullPage: true });
  await sPage.locator('[data-final-result][data-symbol="g#1"] .final-result-u').evaluate((el) => {
    el.value = "-1";
  });
  await sPage.click("#submit-button");
  await sPage.waitForFunction(
    () => document.querySelector("[data-det-tabs] .part-tab.active")?.dataset.determination === "0",
  );
  assert(
    await sPage.locator('[data-final-result][data-symbol="g#1"]').isVisible(),
    "un valor inválido en una pestaña oculta debía abrir esa pestaña",
  );
  await sPage.locator('[data-final-result][data-symbol="g#1"] .final-result-u').fill("");
  await tabs.nth(2).click();
  await group(T).locator(".measurement-row").nth(2).locator(".measure-value").first().fill("");
  await tabs.nth(0).click();
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("falta cargar Medida 3")');
  assert(
    (await visible(T)).join() === "false,false,true",
    "una medida vacía debía abrir su pestaña al entregar",
  );

  step("práctica mixta: la parte estadística con determinaciones convive con un ajuste y se entrega");
  const post = (url, body) => apiPost(page, url, body);
  const mixed = await post("/api/practices", { name: "Mixta", analysis_kind: "estadistico" });
  const mbase = `/api/practices/${mixed.id}`;
  const qty = (symbol, extra) =>
    post(`${mbase}/quantities`, { symbol, name: symbol, unit: "", repeated: false, quantity: null, ...extra });
  const directa = await post(`${mbase}/parts`, { title: "Directa" });
  const ajuste = await post(`${mbase}/parts`, { title: "Ajuste" });
  await post(`${mbase}/parts/${directa.id}/determinations`, determinations("pestanas"));
  const sDirecta = await post(`${mbase}/sections`, { title: "Pendulo", part_id: directa.id });
  const sAjuste = await post(`${mbase}/sections`, { title: "Barrido", part_id: ajuste.id });
  const P = await qty("P", {});
  const S = await qty("S", { is_given: true, slot_count: 4, has_uncertainty: false });
  const xq = await qty("x", {});
  const yq = await qty("y", {});
  for (const [q, sec] of [[P, sDirecta], [S, sDirecta], [xq, sAjuste], [yq, sAjuste]]) {
    await post(`${mbase}/quantities/${q.id}/section`, { section_id: sec.id });
  }
  await post(`${mbase}/quantities/${P.id}`, {
    symbol: "P", name: "P", unit: "", repeated: false, quantity: null, per_determination: true,
  });
  await post(`${mbase}/quantities/${S.id}`, {
    symbol: "S", name: "S", unit: "", repeated: false, quantity: null, is_given: true, slot_count: 4,
    has_uncertainty: false, per_determination: true,
  });
  await post(`${mbase}/parts/${ajuste.id}/analysis-kind`, { analysis_kind: "regresion_lineal" });
  await post(`${mbase}/regression-formulas`, { x_formula: "x", y_formula: "y" });
  await post(`${mbase}/results`, { symbol: "f", name: "Frecuencia", unit: "", formula: "1/P", is_final: true });
  await post(`${mbase}/results`, { symbol: "k", name: "k", unit: "", formula: "slope", is_final: true, section_id: sAjuste.id });
  await post(`${mbase}/results`, { symbol: "c", name: "Constante", unit: "", formula: "2", is_final: true });
  await post(`${mbase}/status`, { status: "publicada" });
  const { courses } = await page.evaluate(async () => (await fetch("/api/academic/context")).json());
  await post(`/api/academic/courses/${courses[0].id}/practices`, { practice_id: mixed.id });

  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  await sPage.click('#practice-nav-children .nav-child:has-text("Mixta")');
  await sPage.waitForSelector(".series-table", { state: "attached" });
  await sPage.selectOption("#table-select", "1");
  const partTabs = sPage.locator("#practice-part-tabs .part-tab");
  await partTabs.nth(0).click();
  const pGroup = sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${P.id}"]`);
  assert((await pGroup.locator(".measurement-row").count()) === 3, "P debía cargarse en tres medidas");
  const sGroup = sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${S.id}"]`);
  const slots = sGroup.locator(".measure-value");
  assert((await slots.count()) === 3, "la ranura debía pedirse en cada medida");
  assert((await slots.first().getAttribute("max")) === "4", "la ranura por medida debía ir de 1 a 4");
  assert((await sGroup.locator(".prefix-select").count()) === 0, "la ranura no debía tener prefijo");
  const detTabs = sPage.locator("#measurement-fields [data-det-tabs] .part-tab");
  for (const [i, [pv, sv]] of [["2", "1"], ["4", "2"], ["5", "3"]].entries()) {
    await detTabs.nth(i).click();
    await pGroup.locator(".measurement-row").nth(i).locator(".measure-value").first().fill(pv);
    await slots.nth(i).fill(sv);
  }
  assert(
    await sPage.locator('[data-final-result][data-symbol="f#3"]').isVisible(),
    "f (sin sección) debía pedirse por medida en su parte",
  );
  await sPage.locator('[data-final-result][data-symbol="f#3"] .final-result-value').fill("0.2");
  await partTabs.nth(1).click();
  assert(!(await detTabs.first().isVisible()), "las pestañas de las medidas no debían verse en la parte Ajuste");
  assert(
    !(await sPage.locator('.final-results-section:has([data-symbol="f#1"])').isVisible()),
    "el resultado final de f no debía quedar como título vacío en la parte Ajuste",
  );
  assert(
    await sPage.locator('[data-final-result][data-symbol="c"]').isVisible(),
    "el resultado final común debía verse en todas las partes",
  );
  const seriesRows = sPage.locator(".series-table tbody .series-row");
  for (const [i, [xv, yv]] of [[1, 2.1], [2, 3.9], [3, 6.2]].entries()) {
    await seriesRows.nth(i).locator(`.series-value[data-quantity-id="${xq.id}"]`).fill(String(xv));
    await seriesRows.nth(i).locator(`.series-value[data-quantity-id="${yq.id}"]`).fill(String(yv));
  }
  const [response] = await Promise.all([
    sPage.waitForResponse((r) => r.url().endsWith("/api/submissions/form") && r.request().method() === "POST"),
    sPage.click("#submit-button"),
  ]);
  assert(response.ok(), `la entrega mixta debía guardarse: ${await response.text()}`);
  const { id: mixedId } = await response.json();
  const mixedDetail = await getJson(page, `/api/submissions/${mixedId}`);
  const directaAnalysis = mixedDetail.analysis.parts.find((p) => p.title === "Directa").analysis;
  assert(directaAnalysis.determinations?.length === 3, "la parte Directa debía traer tres medidas");
  assert(directaAnalysis.derived.some((d) => d.symbol === "f_medio"), "debía estar f_medio");
  assert(mixedDetail.student_results.some((s) => s.symbol === "f#3"), "debía guardarse f#3");
});
