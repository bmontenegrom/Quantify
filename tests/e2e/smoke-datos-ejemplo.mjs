import { join } from "node:path";
import { ARTIFACTS, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8181";

async function tryWithExampleData(page, practiceId) {
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${practiceId}"]`);
  await page.click("[data-practice-try-draft]");
  await page.waitForSelector("#draft-preview-banner:not(.hidden)");
  await page.waitForSelector("#measurement-fields .measurement-row, #measurement-fields .series-table");
  await page.click("#draft-preview-fill");
  await page.click("#submit-button");
  await page.waitForSelector('#submit-status:has-text("no se guardó nada")', { timeout: 15_000 });
  const result = (await page.locator("#latest-result").textContent()) ?? "";
  assert(result.trim().length > 0, `${practiceId}: la prueba debía mostrar un resultado`);
  assert(!result.includes("NaN"), `${practiceId}: el resultado no debía tener NaN: ${result.slice(0, 400)}`);
  await page.screenshot({ path: join(ARTIFACTS, `datos-ejemplo-${practiceId}.png`), fullPage: true });
  await page.click("#draft-preview-back");
}

await withE2E({ port: PORT, dbName: "quantify-datos-ejemplo.db" }, async ({ login }) => {
  step("docente: login");
  const { page } = await login(TEACHER, "docente");

  step("docente: prueba Fluidos I (regresión con réplicas por punto) con datos de ejemplo");
  await tryWithExampleData(page, "fluidos-1");

  step("docente: prueba P1 (operadores y cronómetro) con datos de ejemplo");
  await tryWithExampleData(page, "p1-estadistica");

  console.log("probar el borrador con datos de ejemplo calcula sin errores JS.");
});
