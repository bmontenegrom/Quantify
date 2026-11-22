import { STUDENT, TEACHER, apiPost, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8173";
const NAME = "Dos analisis";

await withE2E({ port: PORT, dbName: "quantify-dos-analisis.db" }, async ({ login }) => {
  step("docente: arma una práctica de dos partes (directa y ajuste)");
  const { page } = await login(TEACHER, "docente");
  const post = (url, body) => apiPost(page, url, body);
  const practice = await post("/api/practices", { name: NAME, analysis_kind: "estadistico" });
  const base = `/api/practices/${practice.id}`;
  const qty = (symbol, extra) =>
    post(`${base}/quantities`, { symbol, name: symbol, unit: "", repeated: false, quantity: null, ...extra });
  const L = await qty("L", { is_given: true, per_point: false });
  const T = await qty("T", { per_point: false });
  const x = await qty("x", {});
  const y = await qty("y", {});
  const directa = await post(`${base}/parts`, { title: "Directa" });
  const ajuste = await post(`${base}/parts`, { title: "Ajuste", help_text: "Ayuda del ajuste" });
  const sDirecta = await post(`${base}/sections`, { title: "Pendulo", part_id: directa.id, help_text: "Ayuda del pendulo" });
  const sAjuste = await post(`${base}/sections`, { title: "Barrido", part_id: ajuste.id, help_text: "Ayuda del barrido" });
  for (const [q, s] of [[L, sDirecta], [T, sDirecta], [x, sAjuste], [y, sAjuste]]) {
    await post(`${base}/quantities/${q.id}/section`, { section_id: s.id });
  }
  await post(`${base}/results`, {
    symbol: "g", name: "g", unit: "", formula: "4*pi^2*L/T^2", is_final: true, section_id: sDirecta.id,
  });

  step("docente: en el admin, la parte Ajuste pasa a regresión lineal");
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${practice.id}"]`);
  await page.selectOption(`[data-part-kind][data-id="${ajuste.id}"]`, "regresion_lineal");
  await page.waitForFunction(
    () => (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes("Análisis de la parte guardado"),
    null,
    { timeout: 10_000 },
  );
  const def = await page.evaluate(async (url) => (await fetch(url)).json(), `${base}/definition`);
  assert(def.analysis_kind === "regresion_lineal", `la práctica debía quedar de regresión (quedó ${def.analysis_kind})`);
  const kinds = Object.fromEntries(def.parts.map((p) => [p.title, p.effective_kind]));
  assert(kinds.Directa === "estadistico" && kinds.Ajuste === "regresion_lineal", `tipos por parte: ${JSON.stringify(kinds)}`);

  step("docente: el rol de cada magnitud depende de su parte");
  const roleOptions = async (symbol) => {
    await page.locator(`#practice-workspace tr:has(code:text-is("${symbol}")) [data-edit-quantity]`).click();
    const values = await page.locator('#edit-quantity-form [name="role"] option').evaluateAll((os) => os.map((o) => o.value));
    await page.locator("[data-cancel-quantity]").click();
    return values;
  };
  assert(!(await roleOptions("T")).includes("serie"), "T está en la parte estadística: sin roles de serie");
  assert((await roleOptions("x")).includes("serie"), "x está en la parte de regresión: con roles de serie");

  step("docente: ejes, mensurando del ajuste, publica y habilita");
  await post(`${base}/regression-formulas`, { x_formula: "x", y_formula: "y" });
  await post(`${base}/results`, {
    symbol: "k", name: "k", unit: "", formula: "slope", is_final: true, section_id: sAjuste.id,
  });
  await post(`${base}/status`, { status: "publicada" });
  const { courses } = await page.evaluate(async () => (await fetch("/api/academic/context")).json());
  await post(`/api/academic/courses/${courses[0].id}/practices`, { practice_id: practice.id });

  step("estudiante: carga las dos partes y entrega");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await sPage.waitForSelector(".series-table", { state: "attached" });
  await sPage.selectOption("#table-select", "1");
  const tabs = sPage.locator("#practice-part-tabs .part-tab");
  await tabs.nth(1).waitFor();

  const help = (text) => sPage.locator(`#measurement-fields .form-help:text-is("${text}")`);
  await tabs.nth(0).click();
  assert(await help("Ayuda del pendulo").isVisible(), "la sección de datos compartidos debía mostrar su ayuda");
  assert(!(await help("Ayuda del ajuste").isVisible()), "la ayuda del ajuste no debía verse en la parte Directa");
  const given = sPage.locator(`.measurement-row--given[data-quantity-id="${L.id}"]`);
  await given.locator(".measure-given-value").fill("1");
  await given.locator(".measure-given-u").fill("0.001");
  await sPage.locator(`.measurement-row[data-quantity-id="${T.id}"] .measure-value`).first().fill("2.006");

  await tabs.nth(1).click();
  assert(
    (await help("Ayuda del ajuste").isVisible()) && (await help("Ayuda del barrido").isVisible()),
    "la parte de la serie debía mostrar su ayuda y la de su sección",
  );
  const rows = sPage.locator(".series-table tbody .series-row");
  for (const [i, [xv, yv]] of [[1, 2.1], [2, 3.9], [3, 6.2]].entries()) {
    await rows.nth(i).locator(`.series-value[data-quantity-id="${x.id}"]`).fill(String(xv));
    await rows.nth(i).locator(`.series-value[data-quantity-id="${y.id}"]`).fill(String(yv));
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
