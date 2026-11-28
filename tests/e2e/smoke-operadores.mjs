import { join } from "node:path";
import { ARTIFACTS, STUDENT, TEACHER, apiPost, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8178";
const NAME = "Operadores";
const ID = "operadores";

const getJson = (page, url) => page.evaluate(async (u) => (await fetch(u)).json(), url);

await withE2E({ port: PORT, dbName: "quantify-operadores.db" }, async ({ login }) => {
  step("docente: crea una práctica estadística con dos operadores (determinaciones) y la habilita en el curso");
  const { page } = await login(TEACHER, "docente");
  const base = `/api/practices/${ID}`;
  await apiPost(page, "/api/practices", { name: NAME, analysis_kind: "estadistico" });
  await apiPost(page, `${base}/determinations`, {
    determinations: { count: 2, label: "Operador", average: false, display: "bloques" },
  });
  const t = await apiPost(page, `${base}/quantities`, {
    symbol: "t",
    name: "Tiempo",
    unit: "s",
    repeated: true,
    quantity: "tiempo",
    input_widget: "teclado",
    per_determination: true,
  });
  const timed = await apiPost(page, `${base}/quantities`, {
    symbol: "T",
    name: "Periodo",
    unit: "s",
    repeated: true,
    quantity: "tiempo",
    input_widget: "cronometro",
    optional: true,
    per_determination: true,
  });
  await apiPost(page, `${base}/results`, { symbol: "y", name: "Salida", unit: "s", formula: "t", is_final: true });
  await apiPost(page, `${base}/status`, { status: "publicada" });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="courses"]');
  await page.click("[data-course-open]");
  await page.selectOption('#course-practice-form select[name="practice_id"]', ID);
  await page.click('#course-practice-form button[type="submit"]');
  await page.waitForSelector(`[data-disable-practice][data-practice-id="${ID}"]`);

  step("estudiante: cada operador elige su instrumento");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  const group = sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${t.id}"]`);
  await group.waitFor();
  const labels = await group.locator(".operator-label").allTextContents();
  assert(labels.join("|") === "Operador 1|Operador 2", `rótulos inesperados: ${labels}`);
  const rows = group.locator(".measurement-row");
  const instrumentOf = (i) => rows.nth(i).locator(".measure-instrument");
  const scaleOf = (i) => rows.nth(i).locator(".measure-scale");
  assert((await group.locator(".measure-instrument").count()) === 2, "cada operador debía tener su selector de instrumento");
  const instrumentId = await instrumentOf(0)
    .locator("option")
    .evaluateAll((opts) => opts.map((o) => o.value).find((v) => v !== ""));
  assert(instrumentId, "debía haber un instrumento de tiempo en el curso");
  await instrumentOf(0).selectOption(instrumentId);
  const scaleId = await scaleOf(0)
    .locator("option")
    .evaluateAll((opts) => opts.map((o) => o.value).find((v) => v !== ""));
  assert(scaleId, "el instrumento debía tener una escala");
  await scaleOf(0).selectOption(scaleId);
  await instrumentOf(1).selectOption("");
  await group.screenshot({ path: join(ARTIFACTS, "operadores-instrumento.png") });

  step("estudiante: cada cronómetro preselecciona el cronómetro, con sus escalas");
  const timedRows = sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${timed.id}"] .measurement-row--chrono`);
  assert((await timedRows.count()) === 2, "cada operador debía tener su cronómetro");
  for (let i = 0; i < 2; i++) {
    assert((await timedRows.nth(i).locator(".measure-instrument").inputValue()) !== "", `el operador ${i + 1} debía preseleccionar el cronómetro`);
    assert((await timedRows.nth(i).locator(".measure-scale option").count()) > 1, `el cronómetro del operador ${i + 1} debía traer sus escalas`);
  }

  step("estudiante: carga las lecturas y el borrador conserva el instrumento de cada operador");
  for (const [i, values] of [["1.20", "1.30"], ["1.25", "1.35"]].entries()) {
    const row = rows.nth(i);
    await row.locator(".add-replica").click();
    const inputs = row.locator(".measure-value");
    await inputs.nth(0).fill(values[0]);
    await inputs.nth(1).fill(values[1]);
  }
  await sPage.waitForFunction(
    (id) =>
      Object.keys(localStorage).some((k) =>
        k.startsWith("quantify-draft:") &&
        JSON.parse(localStorage.getItem(k)).measurements?.some(
          (m) => m.quantity_id === id && m.operator_instruments?.[0]?.scale_id && m.operator_replicas?.[1]?.length === 2,
        ),
      ),
    t.id,
  );
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await group.waitFor();
  assert((await instrumentOf(0).inputValue()) === instrumentId, "el borrador debía restaurar el instrumento del operador 1");
  assert((await scaleOf(0).inputValue()) === scaleId, "el borrador debía restaurar la escala del operador 1");
  assert((await instrumentOf(1).inputValue()) === "", "el operador 2 debía seguir sin instrumento");

  step("estudiante: entrega y solo el operador con instrumento suma su incertidumbre");
  await sPage.selectOption("#table-select", "1");
  await sPage.waitForLoadState("networkidle");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  const [submission] = (await getJson(page, "/api/submissions")).filter((s) => s.practice_id === ID);
  assert(submission, "la entrega debía estar en la lista del docente");
  const detail = await getJson(page, `/api/submissions/${submission.id}`);
  const dets = detail.analysis.determinations;
  assert(dets?.length === 2, `debía haber dos operadores, hay ${dets?.length}`);
  const [first, second] = dets.map((d) => d.quantities.find((x) => x.symbol === "t"));
  assert(first.scale_id === scaleId && first.result.u_b > 0, `Operador 1: la escala debía llegar al cálculo (vi ${first.scale_id}, u_b ${first.result.u_b})`);
  assert(second.scale_id === null && second.result.u_b === 0, `Operador 2: sin instrumento no debía sumar tipo B (vi ${second.scale_id}, u_b ${second.result.u_b})`);
});
