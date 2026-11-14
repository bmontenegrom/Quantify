
import { TEACHER, assert, openAddForm, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8157";
const PRACTICE_ID = "fluidos-2";

async function fillAndSubmit(page, formSelector, fields) {
  const form = page.locator(formSelector);
  for (const [name, value] of Object.entries(fields)) {
    await form.locator(`[name="${name}"]`).fill(value);
  }
  await form.locator('button[type="submit"]').click();
}

await withE2E({ port: PORT, dbName: "quantify-admin-crud.db" }, async ({ login }) => {
  step("docente: login");
  const { page } = await login(TEACHER, "docente");

  step(`docente: abre ${PRACTICE_ID} en el admin de prácticas`);
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${PRACTICE_ID}"]`);
  await openAddForm(page, "#new-quantity-form");

  step("docente: crea una magnitud nueva (escalar compartido: un mensurando de regresión solo ve esos)");
  const role = page.locator('#new-quantity-form [name="role"]');
  assert((await role.inputValue()) === "serie", "en una práctica de regresión una magnitud nueva nace como serie");
  await role.selectOption("serie_replicas");
  assert(
    await page.locator('#new-quantity-form [name="replicas_per_point"]').isVisible(),
    "una serie con réplicas pide cuántas por punto",
  );
  assert(
    !(await page.locator('#new-quantity-form [name="replicas_per_point"]').evaluate((input) => input.checkValidity())),
    "sin la cantidad de réplicas el formulario no se envía",
  );
  await role.selectOption("replicas");
  await fillAndSubmit(page, "#new-quantity-form", {
    symbol: "crud_test_qty",
    name: "Magnitud de prueba CRUD",
    unit: "m",
  });
  await page.waitForSelector('#practice-workspace tr:has(code:text-is("crud_test_qty"))', { timeout: 10_000 });

  step("docente: crea un mensurando nuevo (usa la magnitud recién creada en la fórmula)");
  await openAddForm(page, "#new-result-form");
  await fillAndSubmit(page, "#new-result-form", {
    symbol: "crud_test_res",
    name: "Mensurando de prueba CRUD",
    unit: "m",
    formula: "crud_test_qty",
  });
  await page.waitForSelector('#practice-workspace tr:has(code:text-is("crud_test_res"))', { timeout: 10_000 });

  step("docente: crea un agregado nuevo (referencia el mensurando recién creado)");
  await openAddForm(page, "#new-aggregate-form");
  await fillAndSubmit(page, "#new-aggregate-form", {
    symbol: "crud_test_agg",
    name: "Agregado de prueba CRUD",
    unit: "m",
    formula: "crud_test_res",
  });
  await page.waitForSelector('#practice-workspace tr:has(code:text-is("crud_test_agg"))', { timeout: 10_000 });

  step("verifica que las tres entidades quedaron listadas");
  for (const symbol of ["crud_test_qty", "crud_test_res", "crud_test_agg"]) {
    const rows = await page.locator(`#practice-workspace tr:has(td:first-child code:text-is("${symbol}"))`).count();
    assert(rows === 1, `"${symbol}" debía aparecer en una fila de la ficha (aparece en ${rows})`);
  }

  step("docente: un mensurando con un símbolo inexistente se rechaza al guardar");
  await openAddForm(page, "#new-result-form");
  await fillAndSubmit(page, "#new-result-form", {
    symbol: "crud_bad_res",
    name: "Mensurando con error",
    unit: "m",
    formula: "crud_test_qty * no_existe",
  });
  await page.waitForFunction(
    () => /"no_existe"/.test(document.querySelector("#practice-workspace .practice-status")?.textContent ?? ""),
    null,
    { timeout: 10_000 },
  );
  const resultSymbols = await page.$$eval("[data-edit-result]", (buttons) => buttons.length);

  step("docente: baja el primer mensurando con ▼ y el orden cambia");
  const firstResult = () => page.locator('[data-move="results"][data-dir="down"]').first().getAttribute("data-id");
  const before = await firstResult();
  await page.locator('[data-move="results"][data-dir="down"]').first().click();
  await page.waitForFunction(
    (id) => document.querySelector('[data-move="results"][data-dir="down"]')?.dataset.id !== id,
    before,
    { timeout: 10_000 },
  );
  assert(
    (await page.$$eval("[data-edit-result]", (buttons) => buttons.length)) === resultSymbols,
    "reordenar no debía agregar ni quitar mensurandos",
  );

  step("docente: subir Re_medio por encima de Re_min, que usa, se rechaza; bajar Re_max sí cambia el orden");
  const aggregateRow = (symbol) => page.locator(`#practice-workspace tr:has(td:first-child code:text-is("${symbol}"))`);
  const aggregateOrder = () =>
    page.$$eval('[data-move="aggregates"][data-dir="up"]', (buttons) => buttons.map((b) => b.dataset.id));
  const beforeAggregates = await aggregateOrder();
  await aggregateRow("Re_medio").locator('[data-move="aggregates"][data-dir="up"]').click();
  await page.waitForFunction(
    () => /No se puede cambiar el orden/.test(document.querySelector("#practice-workspace .practice-status")?.textContent ?? ""),
    null,
    { timeout: 10_000 },
  );
  assert(
    JSON.stringify(await aggregateOrder()) === JSON.stringify(beforeAggregates),
    "un orden rechazado no debía cambiar los agregados",
  );
  await aggregateRow("Re_max").locator('[data-move="aggregates"][data-dir="down"]').click();
  await page.waitForFunction(
    (before) =>
      [...document.querySelectorAll('[data-move="aggregates"][data-dir="up"]')].map((b) => b.dataset.id).join() !== before,
    beforeAggregates.join(),
    { timeout: 10_000 },
  );

  console.log("alta de magnitud, mensurando y agregado desde el admin, sin errores JS.");
});
