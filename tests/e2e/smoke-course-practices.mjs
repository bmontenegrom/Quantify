
import { TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8162";

await withE2E({ port: PORT, dbName: "quantify-course-practices.db" }, async ({ login }) => {
  step("docente: abre el curso");
  const { page } = await login(TEACHER, "docente");
  page.on("dialog", (dialog) => dialog.accept());
  await page.click('.tab[data-view="courses"]:visible');
  await page.click("[data-course-open]");
  const disable = '[data-disable-practice][data-practice-id="p1-estadistica"]';
  await page.waitForSelector(disable);

  step("docente: deshabilita p1-estadistica");
  await page.click(disable);
  await page.waitForSelector(disable, { state: "detached" });
  assert(
    (await page.$$eval('#course-practice-form option[value="p1-estadistica"]', (o) => o.length)) === 1,
    "la práctica deshabilitada no volvió al selector de habilitar",
  );

  step("docente: la vuelve a habilitar");
  await page.selectOption('#course-practice-form select[name="practice_id"]', "p1-estadistica");
  await page.click('#course-practice-form button[type="submit"]');
  await page.waitForSelector(disable);
});
