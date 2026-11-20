import { TEACHER, assert, assignSection, fillAndSubmit, openAddForm, step, waitForStatus, withE2E } from "./lib.mjs";

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
  page.on("dialog", (dialog) => dialog.accept());

  step("Fluidos I: Nueva práctica abre el modo guiado en Datos");
  await createGuided(page, "Fluidos guiada", "regresion_lineal");
  await next(page, "Partes");
  await next(page, "Secciones");
  await next(page, "Magnitudes");

  step("Fluidos I: magnitudes (serie, serie con réplicas, datos de cátedra y medida única)");
  await addQuantity(page, { symbol: "h", name: "Altura del Mariotte", unit: "m", role: "serie" });
  for (const symbol of ["V", "t"]) {
    await addQuantity(page, { symbol, name: symbol, unit: "", role: "serie_replicas", replicas_per_point: "2" });
  }
  for (const symbol of ["R", "L", "g"]) await addQuantity(page, { symbol, name: symbol, unit: "", role: "catedra" });
  await addQuantity(page, { symbol: "rho", name: "Densidad del agua", unit: "kg/m3", role: "unica" });

  step("Fluidos I: intermedia Q y ejes del ajuste");
  await next(page, "Serie y ejes");
  await openAddForm(page, "#new-intermediate-form");
  await fillAndSubmit(page, "#new-intermediate-form", { symbol: "Q", name: "Caudal medio", unit: "m3/s", formula: "V/t" });
  await waitForStatus(page, "Intermedia agregada");
  await fillAndSubmit(page, "#practice-regression-form", { x_formula: "1/Q", y_formula: "h/(Q*Q)" });
  await waitForStatus(page, "Fórmulas de ajuste guardadas");

  step("Fluidos I: mensurando mu y derivada por punto Re");
  await next(page, "Mensurandos");
  await addResult(page, { symbol: "mu", name: "Viscosidad del agua", unit: "Pa.s", formula: "slope*(pi*rho*g*R^4)/(8*L)" });
  await openAddForm(page, "#new-point-result-form");
  await fillAndSubmit(page, "#new-point-result-form", { symbol: "Re", name: "Numero de Reynolds", unit: "", formula: "2*rho*Q/(pi*mu*R)" });
  await waitForStatus(page, "Derivada por punto agregada");

  step("Fluidos I: revisar, probar con datos de ejemplo y publicar");
  await next(page, "Revisar");
  await reviewTryAndPublish(page, "Fluidos I");

  step("Hidrostática: un problema del panel lleva a su paso");
  await createGuided(page, "Hidrostatica guiada", "estadistico");
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
  for (const [symbol, , section] of quantities) await assignSection(page, "quantities", symbol, section);

  step("Hidrostática: un mensurando en cada parte");
  await next(page, "Mensurandos");
  await addResult(page, { symbol: "E", name: "Empuje", unit: "N", formula: "m*g_ac/1000" }, "Arquimedes — Empuje");
  await addResult(page, { symbol: "gamma", name: "Tension superficial", unit: "N/m", formula: "F/(2*l)" }, "Wilhelmy — Tension");

  step("Hidrostática: revisar, probar con datos de ejemplo y publicar");
  await next(page, "Revisar");
  await reviewTryAndPublish(page, "Hidrostática");
  assert((await currentStep(page)).includes("Revisar"), "publicar debía dejar el modo guiado en Revisar");

  console.log("modo guiado: Fluidos I y una Hidrostática de dos partes creadas desde la UI, probadas y publicadas.");
});
