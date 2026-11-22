import { TEACHER, apiPost, assert, assignSection, fillAndSubmit, openAddForm, step, waitForStatus, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8183";

const row = (symbol) => `#practice-workspace tr:has(code:text-is("${symbol}"))`;

async function currentStep(page) {
  return ((await page.locator('#practice-workspace .practice-guide [aria-current="step"]').textContent()) ?? "").trim();
}

async function expectStep(page, title) {
  await page.waitForFunction(
    (t) => (document.querySelector('#practice-workspace .practice-guide [aria-current="step"]')?.textContent ?? "").includes(t),
    title,
    { timeout: 10_000 },
  );
}

async function next(page, title) {
  await page.locator("#practice-workspace .practice-guide button", { hasText: `Siguiente: ${title}` }).click();
  await expectStep(page, title);
}

async function createGuided(page, name, analysisKind) {
  if (await page.locator("#practice-workspace-back").isVisible()) await page.click("#practice-workspace-back");
  else await page.click('.tab.teacher-only[data-view="practices"]');
  await openAddForm(page, "#new-practice-form");
  await fillAndSubmit(page, "#new-practice-form", { name, analysis_kind: analysisKind, description: `${name} (E2E)` });
  await expectStep(page, "Datos");
  assert(
    await page.locator('#practice-workspace details[data-add-panel="practice-info"]').isVisible(),
    "el paso Datos debía mostrar los datos de la práctica",
  );
  assert(
    !(await page.locator('#practice-workspace [data-guide-step="magnitudes"]').isVisible()),
    "el paso Datos no debía mostrar las magnitudes",
  );
  assert(
    !(await page.locator('#practice-workspace [data-practice-status="publicada"]').isVisible()),
    "Publicar debía quedar para el paso Revisar",
  );
}

async function addQuantity(page, fields) {
  await openAddForm(page, "#new-quantity-form");
  await fillAndSubmit(page, "#new-quantity-form", fields);
  await page.waitForSelector(row(fields.symbol));
}

async function addResult(page, fields, section) {
  await openAddForm(page, "#new-result-form");
  if (section) await page.locator('#new-result-form [name="section_id"]').selectOption({ label: section });
  await fillAndSubmit(page, "#new-result-form", fields, { is_final: true });
  await page.waitForSelector(row(fields.symbol));
}

function collect(node, key, out = []) {
  if (Array.isArray(node)) node.forEach((n) => collect(n, key, out));
  else if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node)) {
      if (k === key) out.push(v);
      else collect(v, key, out);
    }
  }
  return out;
}

async function reviewTryAndPublish(page, label) {
  await expectStep(page, "Revisar");
  await page.waitForSelector("#practice-workspace .practice-problems-ok");
  await page.click("#practice-workspace [data-practice-try-draft]");
  await page.click("#draft-preview-fill:not([disabled])");
  const response = page.waitForResponse((r) => r.url().includes("/analyze-preview"));
  await page.click("#submit-button");
  const warnings = collect(await (await response).json(), "warnings").flat();
  assert(warnings.length === 0, `${label}: el borrador con datos de ejemplo no debía dar advertencias: ${warnings.join(" | ")}`);
  await page.waitForSelector('#submit-status:has-text("no se guardó nada")');
  await page.click("#draft-preview-back");
  await expectStep(page, "Revisar");
  await page.click('#practice-workspace [data-practice-status="publicada"]');
  await page.waitForSelector("#practice-workspace .practice-state--publicada");
}

await withE2E({ port: PORT, dbName: "quantify-modo-guiado.db" }, async ({ login }) => {
  step("docente: login");
  const { page } = await login(TEACHER, "docente");
  let dialogs = 0;
  let acceptDialogs = true;
  page.on("dialog", (dialog) => {
    dialogs++;
    return acceptDialogs ? dialog.accept() : dialog.dismiss();
  });

  step("Fluidos I: Nueva práctica abre el modo guiado en Datos");
  await createGuided(page, "Fluidos guiada", "regresion_lineal");
  await next(page, "Partes");
  await next(page, "Secciones");
  await next(page, "Magnitudes");
  assert(dialogs === 0, "cambiar de paso sin datos escritos no debía pedir confirmación");

  step("Fluidos I: magnitudes (serie, serie con réplicas, datos de cátedra y medida única)");
  await addQuantity(page, { symbol: "h", name: "Altura del Mariotte", unit: "m", role: "serie" });
  for (const symbol of ["V", "t"]) {
    await addQuantity(page, { symbol, name: symbol, unit: "", role: "serie_replicas", replicas_per_point: "2" });
  }
  for (const symbol of ["R", "L", "g"]) await addQuantity(page, { symbol, name: symbol, unit: "", role: "catedra" });
  await addQuantity(page, { symbol: "rho", name: "Densidad del agua", unit: "kg/m3", role: "unica" });

  step("Fluidos I: cambiar de paso conserva lo escrito; cerrar una edición con cambios pide confirmación");
  await page.locator(`${row("h")} [data-edit-quantity]`).click();
  await page.locator('#edit-quantity-form [name="unit"]').fill("cm");
  let before = dialogs;
  await next(page, "Serie y ejes");
  assert(dialogs === before + 1, "cambiar de paso con una magnitud en edición debía pedir confirmación");
  await page.locator('#practice-regression-form [name="x_formula"]').fill("h");
  before = dialogs;
  await page.click("#practice-workspace [data-guide-exit]");
  await page.click("#practice-workspace [data-guide-start]");
  await expectStep(page, "Serie y ejes");
  assert(dialogs === before, "Ver todo y volver al paso no debían pedir confirmación");
  assert(
    (await page.locator('#practice-regression-form [name="x_formula"]').inputValue()) === "h",
    "Ver todo y volver al paso debían conservar los ejes escritos",
  );

  step("Fluidos I: ejes del ajuste e intermedia Q; el alta cerrada no se vuelve a abrir sola");
  const intermediatePanel = page.locator('#practice-workspace details[data-add-panel="intermediate"]');
  assert(await intermediatePanel.evaluate((d) => d.open), "al entrar al paso, el alta vacía debía estar abierta");
  await intermediatePanel.locator("> summary").click();
  await fillAndSubmit(page, "#practice-regression-form", { x_formula: "h", y_formula: "h" });
  await waitForStatus(page, "Fórmulas de ajuste guardadas");
  assert(!(await intermediatePanel.evaluate((d) => d.open)), "guardar los ejes no debía reabrir el alta de intermedias");
  await openAddForm(page, "#new-intermediate-form");
  await fillAndSubmit(page, "#new-intermediate-form", { symbol: "Q", name: "Caudal medio", unit: "m3/s", formula: "V/t" });
  await waitForStatus(page, "Intermedia agregada");
  await fillAndSubmit(page, "#practice-regression-form", { x_formula: "1/Q", y_formula: "h/(Q*Q)" });
  await page.waitForSelector('#practice-regression-form [name="x_formula"][value="1/Q"]');

  step("Fluidos I: mensurando mu y derivada por punto Re");
  await next(page, "Mensurandos");
  await addResult(page, { symbol: "mu", name: "Viscosidad del agua", unit: "Pa.s", formula: "slope*(pi*rho*g*R^4)/(8*L)" });
  await openAddForm(page, "#new-point-result-form");
  await fillAndSubmit(page, "#new-point-result-form", { symbol: "Re", name: "Numero de Reynolds", unit: "", formula: "2*rho*Q/(pi*mu*R)" });
  await waitForStatus(page, "Derivada por punto agregada");

  step("Fluidos I: revisar, probar con datos de ejemplo y publicar");
  await next(page, "Revisar");
  await reviewTryAndPublish(page, "Fluidos I");

  step("Fluidos I: con cambios sin publicar, publicar la versión nueva queda en Revisar");
  await page.click('#practice-workspace .practice-guide [data-guide-go="datos"]');
  await expectStep(page, "Datos");
  await fillAndSubmit(page, "#practice-info-form", { description: "Fluidos guiada, descripción nueva" });
  await waitForStatus(page, "Datos de la práctica guardados");
  await page.waitForSelector("#practice-draft-changes [data-practice-publish-version]", { state: "attached" });
  assert(
    !(await page.locator("#practice-draft-changes [data-practice-publish-version]").isVisible()),
    "Publicar la versión nueva no debía verse fuera de Revisar",
  );
  await page.click('#practice-workspace .practice-guide [data-guide-go="revisar"]');
  await page.waitForSelector("#practice-draft-changes [data-practice-publish-version]");

  step("Hidrostática: en la ficha completa, un problema que solo hace scroll no pide confirmación");
  await createGuided(page, "Hidrostatica guiada", "estadistico");
  await page.click("#practice-workspace [data-guide-exit]");
  await openAddForm(page, "#new-quantity-form");
  await page.locator('#new-quantity-form [name="symbol"]').fill("zz");
  before = dialogs;
  await page.locator("#practice-problems [data-problem]", { hasText: "magnitudes de entrada" }).click();
  assert(dialogs === before, "ir a un problema sin volver a dibujar la ficha no debía pedir confirmación");
  await page.locator('#new-quantity-form [name="symbol"]').fill("");
  await page.click("#practice-workspace [data-guide-start]");
  await expectStep(page, "Datos");

  step("Hidrostática: un problema del panel lleva a su paso");
  await page.locator("#practice-problems [data-problem]", { hasText: "magnitudes de entrada" }).click();
  await expectStep(page, "Magnitudes");
  assert(
    await page.locator('#practice-workspace details[data-add-panel="quantity"]').evaluate((d) => d.open),
    "el problema debía abrir el alta de magnitudes",
  );

  step("Hidrostática: dos partes; Ver todo muestra la ficha completa y Modo guiado vuelve al paso");
  await page.click('#practice-workspace .practice-guide [data-guide-go="partes"]');
  await expectStep(page, "Partes");
  for (const title of ["Arquimedes", "Wilhelmy"]) {
    await openAddForm(page, "#new-part-form");
    await fillAndSubmit(page, "#new-part-form", { title });
    await page.waitForSelector(`#practice-workspace input[aria-label="Título de la parte"][value="${title}"]`);
  }
  await page.click("#practice-workspace [data-guide-exit]");
  await page.waitForSelector("#practice-workspace .practice-guide", { state: "detached" });
  assert(
    (await page.locator("#practice-workspace [data-guide-step].hidden").count()) === 0,
    "Ver todo debía mostrar todos los bloques",
  );
  await page.click("#practice-workspace [data-guide-start]");
  await expectStep(page, "Partes");

  step("Hidrostática: secciones compartida y por parte");
  await next(page, "Secciones");
  for (const [title, part] of [["Datos comunes", null], ["Empuje", "Arquimedes"], ["Tension", "Wilhelmy"]]) {
    await openAddForm(page, "#new-section-form");
    await page.locator('#new-section-form [name="title"]').fill(title);
    if (part) await page.locator('#new-section-form [name="part_id"]').selectOption({ label: part });
    await page.locator('#new-section-form button[type="submit"]').click();
    await page.waitForSelector(`#practice-workspace input[aria-label="Título de la sección"][value="${title}"]`);
  }

  step("Hidrostática: magnitudes compartidas y de cada parte");
  await next(page, "Magnitudes");
  const quantities = [
    ["g_ac", "catedra", "Datos comunes (todas las partes)"],
    ["rho_f", "catedra", "Datos comunes (todas las partes)"],
    ["m", "unica", "Arquimedes — Empuje"],
    ["F", "unica", "Wilhelmy — Tension"],
    ["l", "catedra", "Wilhelmy — Tension"],
  ];
  for (const [symbol, role] of quantities) await addQuantity(page, { symbol, name: symbol, unit: "", role });
  await openAddForm(page, "#new-quantity-form");
  await page.locator('#new-quantity-form [name="symbol"]').fill("a_medias");
  for (const [symbol, , section] of quantities) await assignSection(page, "quantities", symbol, section);
  assert(
    (await page.locator('#new-quantity-form [name="symbol"]').inputValue()) === "a_medias",
    "asignar una sección no debía borrar el alta a medio escribir",
  );
  await page.locator('#new-quantity-form [name="symbol"]').fill("");

  step("Hidrostática: un mensurando en cada parte");
  await next(page, "Mensurandos");
  await addResult(page, { symbol: "E", name: "Empuje", unit: "N", formula: "m*g_ac/1000" }, "Arquimedes — Empuje");
  await addResult(page, { symbol: "gamma", name: "Tension superficial", unit: "N/m", formula: "F/(2*l)" }, "Wilhelmy — Tension");

  step("Hidrostática: revisar, probar con datos de ejemplo y publicar");
  await next(page, "Revisar");
  await reviewTryAndPublish(page, "Hidrostática");
  assert((await currentStep(page)).includes("Revisar"), "publicar debía dejar el modo guiado en Revisar");

  step("Hidrostática: una descripción con salto de línea no cuenta como dato sin guardar");
  const practiceId = await page.locator("[data-practice-try-draft]").getAttribute("data-practice-id");
  await apiPost(page, `/api/practices/${practiceId}`, { name: "Hidrostatica guiada", description: "linea 1\nlinea 2" });
  await page.click("#practice-workspace-back");
  await page.click(`[data-practice-open][data-practice-id="${practiceId}"]`);
  await page.click("#practice-workspace [data-guide-start]");
  before = dialogs;
  await next(page, "Partes");
  await next(page, "Secciones");
  assert(dialogs === before, "cambiar de paso sin escribir nada no debía pedir confirmación");

  step("Hidrostática: volver al listado con un alta a medio escribir pide confirmación");
  await openAddForm(page, "#new-section-form");
  await page.locator('#new-section-form [name="title"]').fill("Sin guardar");
  await page.click('.tab.teacher-only[data-view="submissions"]');
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.waitForSelector('#new-section-form [name="title"]');
  assert(dialogs === before, "volver a Prácticas desde otra pestaña debía mostrar la ficha sin preguntar");
  acceptDialogs = false;
  const asked = page.waitForEvent("dialog");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await asked;
  acceptDialogs = true;
  assert(dialogs === before + 1, "la pestaña Prácticas desde la ficha con datos sin guardar debía pedir confirmación");
  assert(
    (await page.locator('#new-section-form [name="title"]').inputValue()) === "Sin guardar",
    "cancelar debía dejar la ficha con lo escrito",
  );
  await page.click("#practice-workspace-back");
  assert(dialogs === before + 2, "volver al listado con datos sin guardar debía pedir confirmación");
  await page.waitForSelector("#practice-catalog [data-practice-open]");

  console.log("modo guiado: Fluidos I y una Hidrostática de dos partes creadas desde la UI, probadas y publicadas.");
});
