import { STUDENT, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8173";
const NAME = "Dos analisis";

await withE2E({ port: PORT, dbName: "quantify-dos-analisis.db" }, async ({ login }) => {
  step("docente: arma una práctica de dos partes (directa y ajuste)");
  const { page } = await login(TEACHER, "docente");
  const ids = await page.evaluate(async (name) => {
    const me = await (await fetch("/api/auth/me")).json();
    const post = async (url, body) => {
      const r = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
        body: JSON.stringify(body),
      });
      if (!r.ok) throw new Error(`${url}: ${await r.text()}`);
      return r.json();
    };
    const practice = await post("/api/practices", { name, analysis_kind: "estadistico" });
    const base = `/api/practices/${practice.id}`;
    const qty = (symbol, extra) =>
      post(`${base}/quantities`, { symbol, name: symbol, unit: "", repeated: false, quantity: null, ...extra });
    const L = await qty("L", { is_given: true, per_point: false });
    const T = await qty("T", { per_point: false });
    const x = await qty("x", {});
    const y = await qty("y", {});
    const directa = await post(`${base}/parts`, { title: "Directa" });
    const ajuste = await post(`${base}/parts`, { title: "Ajuste" });
    const sDirecta = await post(`${base}/sections`, { title: "Pendulo", part_id: directa.id });
    const sAjuste = await post(`${base}/sections`, { title: "Barrido", part_id: ajuste.id });
    for (const [q, s] of [[L, sDirecta], [T, sDirecta], [x, sAjuste], [y, sAjuste]]) {
      await post(`${base}/quantities/${q.id}/section`, { section_id: s.id });
    }
    await post(`${base}/results`, {
      symbol: "g", name: "g", unit: "", formula: "4*pi^2*L/T^2", is_final: true, section_id: sDirecta.id,
    });
    return { id: practice.id, ajuste: ajuste.id, sAjuste: sAjuste.id };
  }, NAME);

  step("docente: en el admin, la parte Ajuste pasa a regresión lineal");
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${ids.id}"]`);
  await page.selectOption(`[data-part-kind][data-id="${ids.ajuste}"]`, "regresion_lineal");
  await page.waitForFunction(
    (id) => document.querySelector(`[data-part-kind][data-id="${id}"]`)?.value === "regresion_lineal",
    ids.ajuste,
  );
  await page.waitForLoadState("networkidle");

  step("docente: ejes, mensurando del ajuste, publica y habilita");
  await page.evaluate(async ({ id, sAjuste }) => {
    const me = await (await fetch("/api/auth/me")).json();
    const post = async (url, body) => {
      const r = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
        body: JSON.stringify(body),
      });
      if (!r.ok) throw new Error(`${url}: ${await r.text()}`);
      return r.json();
    };
    const base = `/api/practices/${id}`;
    const def = await (await fetch(`${base}/definition`)).json();
    if (def.analysis_kind !== "regresion_lineal") throw new Error(`la práctica quedó ${def.analysis_kind}`);
    await post(`${base}/regression-formulas`, { x_formula: "x", y_formula: "y" });
    await post(`${base}/results`, {
      symbol: "k", name: "k", unit: "", formula: "slope", is_final: true, section_id: sAjuste,
    });
    await post(`${base}/status`, { status: "publicada" });
    const { courses } = await (await fetch("/api/academic/context")).json();
    await post(`/api/academic/courses/${courses[0].id}/practices`, { practice_id: id });
  }, ids);

  step("estudiante: carga las dos partes y entrega");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await sPage.waitForSelector(".series-table", { state: "attached" });
  await sPage.selectOption("#table-select", "1");
  const tabs = sPage.locator("#practice-part-tabs .part-tab");
  await tabs.nth(1).waitFor();
  const idBySym = await sPage.evaluate(async (id) => {
    const def = await (await fetch(`/api/practices/${id}/definition`)).json();
    return Object.fromEntries(def.quantities.map((q) => [q.symbol, q.id]));
  }, ids.id);

  await tabs.nth(0).click();
  const given = sPage.locator(`.measurement-row--given[data-quantity-id="${idBySym.L}"]`);
  await given.locator(".measure-given-value").fill("1");
  await given.locator(".measure-given-u").fill("0.001");
  await sPage.locator(`.measurement-row[data-quantity-id="${idBySym.T}"] .measure-value`).first().fill("2.006");

  await tabs.nth(1).click();
  const rows = sPage.locator(".series-table tbody .series-row");
  for (const [i, [x, y]] of [[1, 2.1], [2, 3.9], [3, 6.2]].entries()) {
    await rows.nth(i).locator(`.series-value[data-quantity-id="${idBySym.x}"]`).fill(String(x));
    await rows.nth(i).locator(`.series-value[data-quantity-id="${idBySym.y}"]`).fill(String(y));
  }
  const [response] = await Promise.all([
    sPage.waitForResponse((r) => r.url().endsWith("/api/submissions/form") && r.request().method() === "POST"),
    sPage.click("#submit-button"),
  ]);
  const { id: submissionId } = await response.json();
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("docente: el análisis muestra cada parte con su motor");
  await page.click('.tab.teacher-only[data-view="submissions"]');
  await page.click(`.submission-item[data-id="${submissionId}"]`);
  await page.waitForSelector(".review-form");
  const detail = (await page.locator("#submission-detail-body").textContent()) ?? "";
  assert(detail.includes("Directa") && detail.includes("Ajuste"), "el análisis debía mostrar las dos partes");
  assert(detail.includes("Ajuste lineal"), "la parte Ajuste debía mostrar el ajuste lineal");
  assert(!detail.includes("No se pudo calcular"), "ninguna parte debía fallar");

  console.log("práctica nueva de dos partes con distinto análisis: armada, entregada y analizada.");
});
