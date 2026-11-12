import { join } from "node:path";
import { ARTIFACTS, STUDENT, TEACHER, apiPost, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8177";
const NAME = "Versionado";
const ID = "versionado";

const getJson = (page, url) => page.evaluate(async (u) => (await fetch(u)).json(), url);

async function submissionsOfPractice(page) {
  const all = await getJson(page, "/api/submissions");
  return all.filter((s) => s.practice_id === ID);
}

async function derivedY(page, submissionId) {
  const detail = await getJson(page, `/api/submissions/${submissionId}`);
  return detail.analysis.derived.find((d) => d.symbol === "y").value;
}

await withE2E({ port: PORT, dbName: "quantify-versionado.db" }, async ({ login }) => {
  step("docente: crea y publica una práctica con y = 2*x, y la habilita en el curso");
  const { page } = await login(TEACHER, "docente");
  page.on("dialog", (dialog) => dialog.accept());
  const base = `/api/practices/${ID}`;
  await apiPost(page, "/api/practices", { name: NAME, analysis_kind: "estadistico" });
  const x = await apiPost(page, `${base}/quantities`, {
    symbol: "x",
    name: "Dato",
    unit: "m",
    repeated: false,
    quantity: null,
    is_given: true,
  });
  const y = await apiPost(page, `${base}/results`, {
    symbol: "y",
    name: "Salida",
    unit: "m",
    formula: "2*x",
    is_final: true,
    compare_with: "x",
  });
  await apiPost(page, `${base}/status`, { status: "publicada" });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="courses"]');
  await page.click("[data-course-open]");
  await page.selectOption('#course-practice-form select[name="practice_id"]', ID);
  await page.click('#course-practice-form button[type="submit"]');
  await page.waitForSelector(`[data-disable-practice][data-practice-id="${ID}"]`);

  step("estudiante: entrega sobre la versión 1");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await sPage.waitForSelector("#measurement-fields .measure-given-value");
  assert(!(await sPage.locator("#draft-preview-banner").isVisible()), "el estudiante no debía ver el aviso del borrador");
  await sPage.selectOption("#table-select", "1");
  await sPage.waitForLoadState("networkidle");
  const given = sPage.locator(".measurement-row--given").first();
  await given.locator(".measure-given-value").fill("5");
  await given.locator(".measure-given-u").fill("0.1");
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });
  const courseId = await sPage.locator("#course-select").inputValue();
  const groupId = await sPage.locator("#group-select").inputValue();

  const [old] = await submissionsOfPractice(page);
  assert(old, "la entrega del estudiante debía estar en la lista del docente");
  const oldY = await derivedY(page, old.id);
  assert(Math.abs(oldY - 10) < 1e-9, `y debía dar 10 con 2*x, dio ${oldY}`);

  step("docente: la entrega compara y con x en medido vs teórico");
  await page.click('.tab.teacher-only[data-view="submissions"]');
  await page.click(`.submission-item:has-text("${NAME}")`);
  const compareRows = page.locator("#submission-detail-body .measured-vs-theoretical tbody tr");
  await compareRows.first().waitFor();
  assert((await compareRows.count()) === 1, "medido vs teórico debía tener una fila");
  const cells = await compareRows.first().locator("td").allTextContents();
  assert(/^x\b/.test(cells[0].trim()), `la fila debía ser la de x, fue "${cells[0]}"`);

  step("docente: recién publicada no hay cambios sin publicar");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${ID}"]`);
  await page.waitForSelector('#practice-workspace .practice-lifecycle:has-text("Versión 1")');
  await page.waitForLoadState("networkidle");
  assert((await page.locator(".draft-changes").count()) === 0, "no debía avisar cambios recién publicada");

  step("docente: un cambio cosmético actualiza la versión 1 sin crear otra");
  const cosmetic = { symbol: "y", unit: "m", formula: "2*x", is_final: true, has_uncertainty: true };
  await apiPost(page, `${base}/results/${y.id}`, { ...cosmetic, name: "Salida editada" });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${ID}"]`);
  await page.waitForSelector('.draft-changes [data-practice-publish-version]:has-text("Actualizar versión 1")');
  await page.click("[data-practice-publish-version]");
  await page.waitForFunction(() =>
    (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes("Versión 1 actualizada"),
  );
  await page.waitForLoadState("networkidle");
  assert((await page.locator(".draft-changes").count()) === 0, "el aviso debía irse al actualizar");
  await page.waitForSelector('.version-update:has-text("Versión 1 actualizada por Docente")');

  step("docente: cambia la fórmula a 3*x; el aviso resume el cambio y cuenta las entregas");
  await apiPost(page, `${base}/results/${y.id}`, { ...cosmetic, name: "Salida editada", formula: "3*x" });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${ID}"]`);
  const banner = page.locator(".draft-changes");
  await banner.waitFor();
  const text = (await banner.textContent()) ?? "";
  assert(text.includes("Cambia la fórmula de y"), `el resumen debía nombrar la fórmula: ${text}`);
  assert(text.includes("Entregas existentes: 1"), `el aviso debía contar la entrega: ${text}`);
  await page.screenshot({ path: join(ARTIFACTS, "versionado-cambios.png"), fullPage: true });

  step("docente: prueba el borrador sin publicar ni guardar");
  await page.click("[data-practice-try-draft]");
  await page.waitForSelector("#draft-preview-banner:not(.hidden)");
  assert(await page.locator("#course-select").isVisible(), "el curso define los instrumentos: debía poder elegirse");
  assert(!(await page.locator("#group-select").isVisible()), "el grupo no aplica al probar el borrador");
  const trial = page.locator(".measurement-row--given").first();
  await trial.locator(".measure-given-value").fill("5");
  await trial.locator(".measure-given-u").fill("0.1");
  await page.locator("#course-select").dispatchEvent("change");
  await page.waitForLoadState("networkidle");
  assert(
    (await page.locator(".measurement-row--given .measure-given-value").first().inputValue()) === "5",
    "cambiar de curso no debía borrar lo cargado",
  );
  await page.click("#submit-button");
  await page.waitForSelector('#submit-status:has-text("no se guardó nada")', { timeout: 15_000 });
  const previewed = (await page.locator("#latest-result").textContent()) ?? "";
  assert(/\b15\b/.test(previewed), `la prueba debía calcular 15 con la fórmula del borrador: ${previewed}`);
  await page.screenshot({ path: join(ARTIFACTS, "versionado-probar.png"), fullPage: true });
  assert((await submissionsOfPractice(page)).length === 1, "probar el borrador no debía crear una entrega");
  const draftDef = await getJson(page, `${base}/definition?published=true`);
  assert(draftDef.current_version === 1, "probar el borrador no debía crear una versión");

  step("estudiante: la vista previa con el borrador no le corresponde");
  const denied = await sPage.evaluate(
    async ({ url, xId }) => {
      const me = await (await fetch("/api/auth/me")).json();
      const r = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
        body: JSON.stringify({ measurements: [{ quantity_id: xId, values: [], given_u: 0.1 }], draft: true }),
      });
      return r.status;
    },
    { url: `${base}/analyze-preview`, xId: x.id },
  );
  assert(denied === 403, `el estudiante debía recibir 403, recibió ${denied}`);

  step("docente: vuelve al editor y publica la versión 2");
  await page.click("#draft-preview-back");
  await page.waitForSelector("#practice-workspace .draft-changes");
  await page.click('[data-practice-publish-version]:has-text("Publicar versión 2")');
  await page.waitForSelector('#practice-workspace .practice-lifecycle:has-text("Versión 2")');

  step("la entrega vieja muestra los mismos números y sigue con su fórmula");
  assert((await derivedY(page, old.id)) === oldY, "la entrega vieja no debía cambiar");
  const oldDef = await getJson(page, `/api/submissions/${old.id}/definition`);
  assert(
    oldDef.results.find((r) => r.symbol === "y").formula === "2*x",
    "la entrega vieja debía seguir atada a la versión 1",
  );

  step("una entrega nueva usa la fórmula nueva");
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  const created = await apiPost(sPage, "/api/submissions/form", {
    course_id: courseId,
    group_id: groupId,
    practice_id: ID,
    table_number: 2,
    measurements: [{ quantity_id: x.id, values: [5], given_u: 0.1 }],
    student_results: [],
    practice_version: 2,
  });
  const newY = await derivedY(page, created.id);
  assert(Math.abs(newY - 15) < 1e-9, `y debía dar 15 con 3*x, dio ${newY}`);

  step("salir mientras se prueba el borrador no le deja el modo al siguiente usuario");
  await page.click("[data-practice-try-draft]");
  await page.waitForSelector("#draft-preview-banner:not(.hidden)");
  await page.click("#logout-button");
  await page.waitForSelector("#login-form");
  await page.fill('#login-form [name="email"]', STUDENT.email);
  await page.fill('#login-form [name="password"]', STUDENT.password);
  await page.click('#login-form button[type="submit"]');
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await page.waitForSelector("#practica-view.active #measurement-fields .edit-banner, #practica-view.active #measurement-fields .measure-given-value");
  assert(!(await page.locator("#draft-preview-banner").isVisible()), "el estudiante no debía ver el aviso del borrador");
  assert(await page.locator("#group-select").isVisible(), "el estudiante debía poder elegir su grupo");
  assert((await page.locator("#submit-button").textContent()) === "Entregar", "el botón debía decir Entregar");
});
