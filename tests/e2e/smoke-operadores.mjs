import { join } from "node:path";
import { ARTIFACTS, STUDENT, TEACHER, apiPost, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8178";
const NAME = "Operadores";
const ID = "operadores";

const getJson = (page, url) => page.evaluate(async (u) => (await fetch(u)).json(), url);

await withE2E({ port: PORT, dbName: "quantify-operadores.db" }, async ({ login }) => {
  step("docente: crea una práctica estadística con dos operadores y la habilita en el curso");
  const { page } = await login(TEACHER, "docente");
  const base = `/api/practices/${ID}`;
  await apiPost(page, "/api/practices", { name: NAME, analysis_kind: "estadistico" });
  const t = await apiPost(page, `${base}/quantities`, {
    symbol: "t",
    name: "Tiempo",
    unit: "s",
    repeated: true,
    quantity: "tiempo",
    input_widget: "teclado",
  });
  const timed = await apiPost(page, `${base}/quantities`, {
    symbol: "T",
    name: "Periodo",
    unit: "s",
    repeated: true,
    quantity: "tiempo",
    input_widget: "cronometro",
    optional: true,
  });
  await apiPost(page, `${base}/results`, { symbol: "y", name: "Salida", unit: "s", formula: "t", is_final: true });
  await apiPost(page, `${base}/operator-count`, { count: 2 });
  await apiPost(page, `${base}/status`, { status: "publicada" });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="courses"]');
  await page.click("[data-course-open]");
  await page.selectOption('#course-practice-form select[name="practice_id"]', ID);
  await page.click('#course-practice-form button[type="submit"]');
  await page.waitForSelector(`[data-disable-practice][data-practice-id="${ID}"]`);

  step("estudiante: la magnitud por operador tiene un solo instrumento, en el grupo");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  const group = sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${t.id}"]`);
  await group.waitFor();
  assert((await group.locator(".measure-instrument").count()) === 1, "el grupo debía tener un solo selector de instrumento");
  assert((await group.locator(".measurement-row .measure-instrument").count()) === 0, "las filas de cada operador no debían tener instrumento");
  const instrumentId = await group
    .locator(".measure-instrument option")
    .evaluateAll((opts) => opts.map((o) => o.value).find((v) => v !== ""));
  assert(instrumentId, "debía haber un instrumento de tiempo en el curso");
  await group.locator(".measure-instrument").selectOption(instrumentId);
  const scaleId = await group
    .locator(".measure-scale option")
    .evaluateAll((opts) => opts.map((o) => o.value).find((v) => v !== ""));
  assert(scaleId, "el instrumento debía tener una escala");
  await group.locator(".measure-scale").selectOption(scaleId);
  await group.screenshot({ path: join(ARTIFACTS, "operadores-instrumento.png") });

  step("estudiante: el cronómetro por operador preselecciona el cronómetro en el grupo, con sus escalas");
  const timedGroup = sPage.locator(`#measurement-fields .operator-quantity[data-quantity-id="${timed.id}"]`);
  assert((await timedGroup.locator(".measurement-row--chrono").count()) === 2, "cada operador debía tener su cronómetro");
  assert((await timedGroup.locator(".measurement-row .measure-instrument").count()) === 0, "los cronómetros de cada operador no debían tener instrumento");
  assert((await timedGroup.locator(".measure-instrument").inputValue()) !== "", "el grupo debía preseleccionar el cronómetro");
  assert((await timedGroup.locator(".measure-scale option").count()) > 1, "el cronómetro preseleccionado debía traer sus escalas");

  step("estudiante: carga las lecturas de cada operador y el borrador conserva el instrumento");
  const rows = group.locator(".measurement-row");
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
        JSON.parse(localStorage.getItem(k)).measurements?.some((m) => m.quantity_id === id && m.scale_id && m.operator_replicas?.[1]?.length === 2),
      ),
    t.id,
  );
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await group.waitFor();
  assert((await group.locator(".measure-instrument").inputValue()) === instrumentId, "el borrador debía restaurar el instrumento del grupo");
  assert((await group.locator(".measure-scale").inputValue()) === scaleId, "el borrador debía restaurar la escala del grupo");

  step("estudiante: entrega y cada operador se calcula con la incertidumbre del instrumento");
  await sPage.selectOption("#table-select", "1");
  await sPage.waitForLoadState("networkidle");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  const [submission] = (await getJson(page, "/api/submissions")).filter((s) => s.practice_id === ID);
  assert(submission, "la entrega debía estar en la lista del docente");
  const detail = await getJson(page, `/api/submissions/${submission.id}`);
  const operators = detail.analysis.operators;
  assert(operators.length === 2, `debía haber dos operadores, hay ${operators.length}`);
  for (const op of operators) {
    const q = op.quantities.find((x) => x.symbol === "t");
    assert(q.scale_id === scaleId, `${op.label}: la escala debía llegar al cálculo (vi ${q.scale_id})`);
    assert(q.result.u_b > 0, `${op.label}: debía tener incertidumbre tipo B (vi ${q.result.u_b})`);
  }
});
