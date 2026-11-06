import { STUDENT, TEACHER, assert, openAddForm, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8171";

await withE2E({ port: PORT, dbName: "quantify-p3.db" }, async ({ login }) => {
  step("estudiante: P3 es una sola práctica en el menú");
  const { page: sPage } = await login(STUDENT, "estudiante");
  const p3Items = await sPage.locator('#practice-nav-children .nav-child:has-text("Relajacion Exponencial")').count();
  assert(p3Items === 1, `P3 debía aparecer una sola vez en el menú (aparece ${p3Items})`);
  await sPage.click('#practice-nav-children .nav-child:has-text("Relajacion Exponencial")');
  await sPage.waitForSelector(".series-table", { state: "attached" });
  await sPage.selectOption("#table-select", "1");

  const tabs = sPage.locator("#practice-part-tabs .part-tab");
  await tabs.nth(1).waitFor();
  assert((await tabs.count()) === 2, "P3 debía mostrar dos pestañas");
  assert(((await tabs.nth(0).textContent()) ?? "").includes("Relajación directa"), "la primera pestaña es la relajación directa");
  assert(((await tabs.nth(1).textContent()) ?? "").includes("Desfasaje"), "la segunda pestaña es el desfasaje");

  const idBySym = await sPage.evaluate(async () => {
    const def = await (await fetch("/api/practices/p3-relajacion/definition")).json();
    return Object.fromEntries(def.quantities.map((q) => [q.symbol, q.id]));
  });

  step("estudiante: parte 1, medida directa");
  await tabs.nth(0).click();
  const table = sPage.locator(".series-table");
  assert(!(await table.isVisible()), "la tabla de la serie no debía verse en la parte 1");
  for (const [sym, val] of Object.entries({ R: 9980, C: 4.71e-5, T_oc: 0.5, tmedio: 0.33 })) {
    const row = sPage.locator(`.measurement-row[data-quantity-id="${idBySym[sym]}"]`);
    await row.locator(".measure-value").first().fill(String(val));
  }
  const rint = sPage.locator(`.measurement-row--given[data-quantity-id="${idBySym["Rint"]}"]`);
  await rint.locator(".measure-given-value").fill("50");
  await rint.locator(".measure-given-u").fill("1");

  step("estudiante: parte 2, desfasaje");
  await tabs.nth(1).click();
  assert(await table.isVisible(), "la tabla de la serie debía verse en la parte 2");
  const points = [
    { f: 50, a: 4, b: 1.21 },
    { f: 100, a: 4, b: 2.113 },
    { f: 200, a: 4, b: 3.136 },
  ];
  const rows = sPage.locator(".series-table tbody .series-row");
  for (let i = 0; i < points.length; i++) {
    for (const [sym, val] of Object.entries(points[i])) {
      await rows.nth(i).locator(`.series-value[data-quantity-id="${idBySym[sym]}"]`).fill(String(val));
    }
  }

  step("estudiante: entrega");
  const [response] = await Promise.all([
    sPage.waitForResponse((r) => r.url().endsWith("/api/submissions/form") && r.request().method() === "POST"),
    sPage.click("#submit-button"),
  ]);
  const { id: submissionId } = await response.json();
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("docente: la entrega muestra las dos partes");
  const { page: tPage } = await login(TEACHER, "docente");
  await tPage.click('.tab.teacher-only[data-view="submissions"]');
  await tPage.click(`.submission-item[data-id="${submissionId}"]`);
  await tPage.waitForSelector(".review-form");
  const detail = (await tPage.locator("#submission-detail-body").textContent()) ?? "";
  assert(detail.includes("Relajación directa"), "el análisis debía tener la parte de relajación directa");
  assert(detail.includes("Desfasaje"), "el análisis debía tener la parte de desfasaje");
  assert(detail.includes("Ajuste lineal"), "la parte de desfasaje debía mostrar el ajuste lineal");
  assert(!detail.includes("No se pudo calcular"), "ninguna parte debía fallar");

  const detailHtml = (page) => page.locator("#submission-detail-body").innerHTML();
  const reviewSaved = (page) =>
    Promise.all([
      page.waitForResponse((r) => /\/review$/.test(r.url()) && r.request().method() === "POST"),
      page.click('.review-form button[type="submit"]'),
    ]);

  step("docente: guardar la revisión conserva el resultado entregado");
  assert(detail.includes("Resultado final entregado"), "la entrega debía mostrar el resultado final");
  await reviewSaved(tPage);
  await tPage.waitForFunction(() => document.querySelector("#submission-detail-body .review-form"));
  await tPage.locator('#submission-detail-body:has-text("Resultado final entregado")').waitFor();

  step("docente: guardar usa los símbolos de la entrega aunque otra pantalla los haya cambiado");
  const { page: racePage } = await login(TEACHER, "docente");
  await racePage.click('.tab.teacher-only[data-view="practices"]');
  await racePage.click('[data-practice-open][data-practice-id="p1-estadistica"]');
  await openAddForm(racePage, "#new-quantity-form");
  await racePage.fill('#new-quantity-form [name="symbol"]', "Lenta");
  await racePage.fill('#new-quantity-form [name="name"]', "Lenta");
  const practiceSaved = racePage.waitForResponse((r) => r.url().endsWith("/p1-estadistica/definition"));
  await racePage.route("**/api/practices/p1-estadistica/quantities", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1500));
    await route.continue();
  });
  await racePage.click('#new-quantity-form button[type="submit"]');
  await racePage.click('.tab.teacher-only[data-view="submissions"]');
  await racePage.click(`.submission-item[data-id="${submissionId}"]`);
  await racePage.waitForSelector(".review-form");
  await practiceSaved;
  await racePage.waitForTimeout(300);
  await reviewSaved(racePage);
  await racePage.locator('#submission-detail-body:has-text("Resultado final entregado")').waitFor();
  assert((await detailHtml(racePage)).includes("t<sub>1/2</sub>"), "tmedio debía seguir mostrándose como t_1/2");

  step("docente: si falla la definición de la entrega, lo avisa y se puede reintentar");
  const { page: failPage } = await login(TEACHER, "docente");
  await failPage.route("**/api/submissions/*/definition", (route) =>
    route.fulfill({ status: 500, contentType: "application/json", body: '{"error":"x"}' }),
  );
  await failPage.click('.tab.teacher-only[data-view="submissions"]');
  await failPage.click(`.submission-item[data-id="${submissionId}"]`);
  await failPage.waitForSelector("#submission-definition-error");
  await failPage.unroute("**/api/submissions/*/definition");
  await failPage.click("#submission-definition-retry");
  await failPage.locator('#submission-detail-body:has-text("Resultado final entregado")').waitFor();
  assert((await failPage.locator("#submission-definition-error").count()) === 0, "el aviso debía irse al reintentar");
  assert((await detailHtml(failPage)).includes("t<sub>1/2</sub>"), "tras reintentar debía mostrarse t_1/2");

  console.log("P3 unificada: dos pestañas, entrega y análisis por parte, sin errores JS.");
});
