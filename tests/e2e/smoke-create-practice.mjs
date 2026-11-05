
import { STUDENT, TEACHER, assert, openAddForm, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8163";
const NAME = "Péndulo de prueba";
const ID = "pendulo-de-prueba";

async function fillAndSubmit(page, formSelector, fields, checks = {}) {
  const form = page.locator(formSelector);
  for (const [name, value] of Object.entries(fields)) {
    await form.locator(`[name="${name}"]`).fill(value);
  }
  for (const [name, checked] of Object.entries(checks)) {
    await form.locator(`[name="${name}"]`).setChecked(checked);
  }
  await form.locator('button[type="submit"]').click();
}

async function assignSection(page, list, symbol, label) {
  const select = page.locator(`#practice-workspace tr:has(code:text-is("${symbol}")) [data-section-select="${list}"]`);
  for (let attempt = 0; attempt < 3; attempt++) {
    await select.selectOption({ label });
    await page.waitForLoadState("networkidle");
    const ok = await page
      .waitForFunction(
        ([list, symbol, label]) => {
          const row = [...document.querySelectorAll("#practice-workspace tr")].find(
            (tr) => tr.querySelector("code")?.textContent === symbol,
          );
          const sel = row?.querySelector(`[data-section-select="${list}"]`);
          return sel?.selectedOptions[0]?.textContent === label;
        },
        [list, symbol, label],
        { timeout: 3_000 },
      )
      .then(() => true, () => false);
    if (ok) return;
  }
  throw new Error(`no se pudo asignar ${symbol} a ${label}`);
}

async function waitForStatus(page, text) {
  await page.waitForFunction(
    (t) => (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes(t),
    text,
    { timeout: 10_000 },
  );
}

await withE2E({ port: PORT, dbName: "quantify-create-practice.db" }, async ({ login }) => {
  step("docente: crea la práctica en borrador");
  const { page } = await login(TEACHER, "docente");
  page.on("dialog", (dialog) => dialog.accept());
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click('details[data-add-panel="practice"] > summary');
  await fillAndSubmit(page, "#new-practice-form", { name: NAME, description: "Prueba E2E" });
  await page.waitForSelector("#practice-workspace .practice-state--borrador");

  step("docente: agrega L (dato) y T (réplicas)");
  await openAddForm(page, "#new-quantity-form");
  await fillAndSubmit(page, "#new-quantity-form", { symbol: "L", name: "Largo", unit: "m" }, { is_given: true, repeated: false });
  await waitForStatus(page, "Magnitud agregada");
  await openAddForm(page, "#new-quantity-form");
  await fillAndSubmit(page, "#new-quantity-form", { symbol: "T", name: "Periodo", unit: "s" });
  await waitForStatus(page, "Magnitud agregada");

  step("docente: dos partes con sus secciones; L sin sección (compartida) y T en la segunda");
  for (const title of ["Parte A", "Parte B"]) {
    await openAddForm(page, "#new-part-form");
    await fillAndSubmit(page, "#new-part-form", { title });
    await page.waitForSelector(`#practice-workspace input[aria-label="Título de la parte"][value="${title}"]`);
  }
  for (const [title, part] of [["Seccion A", "Parte A"], ["Seccion B", "Parte B"], ["Seccion R", "Parte B"]]) {
    await openAddForm(page, "#new-section-form");
    await page.locator('#new-section-form [name="title"]').fill(title);
    await page.locator('#new-section-form [name="part_id"]').selectOption({ label: part });
    await page.locator('#new-section-form button[type="submit"]').click();
    await page.waitForSelector(`#practice-workspace input[aria-label="Título de la sección"][value="${title}"]`);
  }
  await assignSection(page, "quantities", "T", "Parte B — Seccion B");

  step("docente: publicar sin mensurandos se rechaza");
  await page.click('[data-practice-status="publicada"]');
  await waitForStatus(page, "no tiene mensurandos");

  step("docente: agrega g y Lr, los ubica en sus partes y publica");
  for (const [symbol, name, formula, section] of [
    ["g", "Gravedad", "4*pi^2*L/T^2", "Parte B — Seccion R"],
    ["Lr", "Largo informado", "L", "Parte A — Seccion A"],
  ]) {
    await openAddForm(page, "#new-result-form");
    await page.locator('#new-result-form [name="section_id"]').selectOption({ label: section });
    await fillAndSubmit(
      page,
      "#new-result-form",
      { symbol, name, unit: symbol === "g" ? "m/s2" : "m", formula },
      { is_final: true },
    );
    await waitForStatus(page, "Mensurando agregado");
    await page.waitForSelector(`#practice-workspace tr:has(code:text-is("${symbol}")) [data-section-select="results"]`);
  }
  await page.click('[data-practice-status="publicada"]');
  await page.waitForSelector("#practice-workspace .practice-state--publicada");
  await page.waitForSelector('#practice-workspace .practice-lifecycle:has-text("Versión 1")');

  step("docente: duplica y elimina la copia");
  await page.click("#practice-workspace-back");
  await page.click(`[data-practice-duplicate][data-practice-id="${ID}"]`);
  await page.waitForSelector("#practice-workspace .practice-state--borrador");
  assert(
    ((await page.locator("#practice-workspace h3").first().textContent()) ?? "").includes(`Copia de ${NAME}`),
    "la copia debía llamarse 'Copia de …'",
  );
  await page.click("[data-practice-delete]");
  await page.waitForSelector("#practice-catalog [data-practice-open]");
  const copies = await page.locator(`[data-practice-open][data-practice-id="copia-de-${ID}"]`).count();
  assert(copies === 0, "la copia eliminada no debía seguir en el catálogo");

  step("docente: la habilita en el curso");
  await page.click('.tab.teacher-only[data-view="courses"]');
  await page.click("[data-course-open]");
  await page.selectOption('#course-practice-form select[name="practice_id"]', ID);
  await page.click('#course-practice-form button[type="submit"]');
  await page.waitForSelector(`[data-disable-practice][data-practice-id="${ID}"]`);

  step("estudiante: la ve en el menú y la entrega");
  const { page: sPage } = await login(STUDENT, "estudiante");
  await sPage.click(`#practice-nav-children .nav-child:has-text("${NAME}")`);
  await sPage.waitForSelector("#measurement-fields .measurement-row");
  await sPage.selectOption("#table-select", "1");
  await sPage.waitForLoadState("networkidle");
  const tabs = sPage.locator("#practice-part-tabs .part-tab");
  await tabs.nth(1).waitFor();
  assert((await tabs.count()) === 2, "la práctica debía mostrar las dos pestañas");
  const given = sPage.locator(".measurement-row--given").first();
  const measured = sPage.locator('#measurement-fields .measurement-row:not(.measurement-row--given):not([data-final-result])').first();
  await tabs.nth(0).click();
  assert((await given.isVisible()) && !(await measured.isVisible()), "la parte A debía mostrar L y ocultar T");
  await given.locator(".measure-given-value").fill("1");
  await given.locator(".measure-given-u").fill("0.002");
  await tabs.nth(1).click();
  assert((await given.isVisible()) && (await measured.isVisible()), "la parte B debía mostrar T y también L, que es compartida");
  await measured.locator(".measure-value").first().fill("2.006");
  const finalG = sPage.locator('[data-final-result="1"][data-symbol="g"]');
  assert(await finalG.isVisible(), "el resultado final g debía verse en la parte B");
  const finalSection = sPage.locator('.measurement-section:has([data-final-result="1"][data-symbol="g"]) .measurement-section-title');
  assert((await finalSection.textContent()) === "Seccion R", "g debía estar en su sección, con su título");
  await tabs.nth(0).click();
  assert(!(await finalG.isVisible()), "el resultado final g no debía verse en la parte A");
  await tabs.nth(1).click();
  await sPage.click("#submit-button");
  await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("docente: la archiva y el estudiante deja de verla");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${ID}"]`);
  await page.waitForSelector("#practice-workspace .practice-state--publicada");
  await page.click('[data-practice-status="archivada"]');
  await page.waitForSelector("#practice-workspace .practice-state--archivada");
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  const visible = await sPage.locator(`#practice-nav-children .nav-child:has-text("${NAME}")`).count();
  assert(visible === 0, "una práctica archivada no debía seguir en el menú del estudiante");

  step("docente: la archivada sigue en su curso, marcada, y la puede deshabilitar");
  await page.click('.tab.teacher-only[data-view="courses"]');
  await page.click("[data-course-open]");
  const archivedRow = page.locator(`tr:has([data-disable-practice][data-practice-id="${ID}"])`);
  await archivedRow.locator(".practice-state--archivada").waitFor();
  await archivedRow.locator("[data-disable-practice]").click();
  await page.waitForSelector(`[data-disable-practice][data-practice-id="${ID}"]`, { state: "detached" });

  step("estudiante: no puede leer un borrador por id; de una archivada, solo la definición");
  const draft = await page.evaluate(async (id) => {
    const me = await (await fetch("/api/auth/me")).json();
    const r = await fetch(`/api/practices/${id}/duplicate`, {
      method: "POST",
      headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
      body: "{}",
    });
    return (await r.json()).id;
  }, ID);
  const statuses = await sPage.evaluate(
    async ({ archived, draft }) => {
      const me = await (await fetch("/api/auth/me")).json();
      const preview = (id) =>
        fetch(`/api/practices/${id}/analyze-preview`, {
          method: "POST",
          headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
          body: JSON.stringify({ measurements: [] }),
        }).then((r) => r.status);
      return {
        draftDefinition: (await fetch(`/api/practices/${draft}/definition`)).status,
        draftPreview: await preview(draft),
        archivedDefinition: (await fetch(`/api/practices/${archived}/definition`)).status,
        archivedPreview: await preview(archived),
      };
    },
    { archived: ID, draft },
  );
  assert(
    statuses.draftDefinition === 404 && statuses.draftPreview === 404,
    `un borrador debía responder 404 al estudiante (${JSON.stringify(statuses)})`,
  );
  assert(
    statuses.archivedDefinition === 200 && statuses.archivedPreview === 404,
    `de una archivada el estudiante lee la definición pero no la vista previa (${JSON.stringify(statuses)})`,
  );

  step("serie sin datos compartidos: la tabla va solo en la pestaña de su parte");
  await page.evaluate(async () => {
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
    const practice = await post("/api/practices", { name: "Curva por partes", analysis_kind: "curva" });
    const base = `/api/practices/${practice.id}`;
    const qty = (symbol) => post(`${base}/quantities`, { symbol, name: symbol, unit: "", repeated: false, quantity: null });
    const r = await qty("R");
    const i = await qty("I");
    await post(`${base}/curves`, { x_formula: "R", y_formula: "I", x_log: false });
    const partA = await post(`${base}/parts`, { title: "Datos" });
    const partB = await post(`${base}/parts`, { title: "Serie" });
    const section = await post(`${base}/sections`, { title: "Barrido", part_id: partB.id });
    await post(`${base}/quantities/${r.id}/section`, { section_id: section.id });
    await post(`${base}/quantities/${i.id}/section`, { section_id: section.id });
    const summary = await post(`${base}/sections`, { title: "Resumen", part_id: partB.id });
    await post(`${base}/results`, { symbol: "Rm", name: "R maxima", unit: "", formula: "R_max", is_final: true, section_id: summary.id });
    const v = await post(`${base}/quantities`, { symbol: "V", name: "V", unit: "", repeated: false, quantity: null, per_point: false });
    const source = await post(`${base}/sections`, { title: "Fuente", part_id: partA.id });
    await post(`${base}/results`, { symbol: "Vm", name: "V fuente", unit: "", formula: "V", is_final: true, section_id: source.id });
    await post(`${base}/quantities/${v.id}/section`, { section_id: source.id });
    await post(`${base}/status`, { status: "publicada" });
    const { courses } = await (await fetch("/api/academic/context")).json();
    await post(`/api/academic/courses/${courses[0].id}/practices`, { practice_id: practice.id });
  });
  await sPage.reload();
  await sPage.waitForSelector("#app-shell:not(.hidden)");
  await sPage.click('#practice-nav-children .nav-child:has-text("Curva por partes")');
  await sPage.waitForSelector("#measurement-fields .series-table", { state: "attached" });
  await sPage.waitForLoadState("networkidle");
  const seriesTabs = sPage.locator("#practice-part-tabs .part-tab");
  const table = sPage.locator("#measurement-fields .series-table");
  await seriesTabs.nth(0).click();
  assert(!(await table.isVisible()), "la serie no debía verse en la pestaña Datos");
  const rmTitle = sPage.locator('.measurement-section:has([data-final-result="1"][data-symbol="Rm"]) .measurement-section-title');
  assert((await rmTitle.textContent()) === "Resumen", "Rm debía verse bajo su sección Resumen");
  const vmTitle = sPage.locator('.measurement-section:has([data-final-result="1"][data-symbol="Vm"]) h4');
  assert((await vmTitle.textContent()) === "Fuente", "Vm debía verse bajo su sección Fuente, junto a V");
  await seriesTabs.nth(1).click();
  assert(await table.isVisible(), "la serie debía verse en la pestaña Serie");
  const visibleFinals = await sPage.locator("#measurement-fields .final-results-section:visible").count();
  assert(visibleFinals === 0, "la pestaña Serie no debía mostrar un bloque de resultado final vacío");
});
