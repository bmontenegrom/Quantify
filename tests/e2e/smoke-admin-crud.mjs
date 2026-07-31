// Smoke E2E de alta desde el admin de prácticas: un solo flujo que crea una magnitud, un
// mensurando y un agregado, y verifica que los tres aparecen — no un flujo por entidad (sería
// caro de mantener sin agregar cobertura real).
//
// Uso: `npm run test:e2e` (corre todos los tests/e2e/*.mjs). Variables: E2E_PORT,
// E2E_SKIP_BUILD, E2E_HEADED.

import { TEACHER, assert, step, withE2E } from "./lib.mjs";

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
  await page.waitForSelector("#new-quantity-form");

  step("docente: crea una magnitud nueva");
  await fillAndSubmit(page, "#new-quantity-form", {
    symbol: "crud_test_qty",
    name: "Magnitud de prueba CRUD",
    unit: "m",
  });
  await page.waitForFunction(
    () => document.querySelector("#practice-workspace")?.textContent?.includes("crud_test_qty"),
    { timeout: 10_000 },
  );

  step("docente: crea un mensurando nuevo (usa la magnitud recién creada en la fórmula)");
  await page.waitForSelector("#new-result-form");
  await fillAndSubmit(page, "#new-result-form", {
    symbol: "crud_test_res",
    name: "Mensurando de prueba CRUD",
    unit: "m",
    formula: "crud_test_qty",
  });
  await page.waitForFunction(
    () => document.querySelector("#practice-workspace")?.textContent?.includes("crud_test_res"),
    { timeout: 10_000 },
  );

  step("docente: crea un agregado nuevo (referencia el mensurando recién creado)");
  await page.waitForSelector("#new-aggregate-form");
  await fillAndSubmit(page, "#new-aggregate-form", {
    symbol: "crud_test_agg",
    name: "Agregado de prueba CRUD",
    unit: "m",
    formula: "crud_test_res",
  });
  await page.waitForFunction(
    () => document.querySelector("#practice-workspace")?.textContent?.includes("crud_test_agg"),
    { timeout: 10_000 },
  );

  step("verifica que las tres entidades quedaron listadas");
  const workspaceText = (await page.locator("#practice-workspace").textContent()) ?? "";
  for (const symbol of ["crud_test_qty", "crud_test_res", "crud_test_agg"]) {
    assert(workspaceText.includes(symbol), `"${symbol}" debía aparecer en el workspace de la práctica`);
  }

  console.log("alta de magnitud, mensurando y agregado desde el admin, sin errores JS.");
});
