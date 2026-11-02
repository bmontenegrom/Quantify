// Recorre las vistas del docente a ancho de escritorio y de tablet y verifica que el layout no se
// rompa: sin scroll horizontal de la página, ningún panel más ancho que su contenedor y checkboxes
// de tamaño normal (el bug que los estiraba a 100% de ancho). Deja una captura por vista en
// tests/e2e/artifacts/teacher-*.png para revisar a ojo.

import { join } from "node:path";
import { ARTIFACTS, STUDENT, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8161";
const WIDTHS = [1366, 900];

/** Problemas de layout de la vista visible: desbordes horizontales y checkboxes gigantes. */
async function layoutProblems(page) {
  return page.evaluate(() => {
    const problems = [];
    const root = document.documentElement;
    if (root.scrollWidth > root.clientWidth + 1) {
      problems.push(`la página tiene scroll horizontal (${root.scrollWidth} > ${root.clientWidth})`);
    }
    const visible = (el) => el.offsetParent !== null;
    for (const panel of document.querySelectorAll("main .panel")) {
      if (!visible(panel)) continue;
      const parent = panel.parentElement.getBoundingClientRect();
      const box = panel.getBoundingClientRect();
      if (box.right > parent.right + 1) {
        const title = panel.querySelector("h2, h3")?.textContent?.trim() ?? panel.id;
        problems.push(`el panel "${title}" se sale de su contenedor (${Math.round(box.right - parent.right)}px)`);
      }
    }
    for (const box of document.querySelectorAll('main input[type="checkbox"], main input[type="radio"]')) {
      if (!visible(box)) continue;
      const { width, height } = box.getBoundingClientRect();
      if (width > 24 || height > 24) {
        problems.push(`un checkbox mide ${Math.round(width)}x${Math.round(height)}px`);
        break;
      }
    }
    return problems;
  });
}

/** Captura la vista a cada ancho (y en tema oscuro al ancho de escritorio) y acumula sus problemas
 *  en `found`, para reportarlos todos juntos. */
async function check(page, name, found) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.waitForTimeout(150);
    await page.screenshot({ path: join(ARTIFACTS, `teacher-${name}-${width}.png`), fullPage: true });
    for (const problem of await layoutProblems(page)) found.push(`${name} a ${width}px: ${problem}`);
    if (width === WIDTHS[0]) {
      await page.evaluate(() => (document.documentElement.dataset.theme = "dark"));
      await page.screenshot({ path: join(ARTIFACTS, `teacher-${name}-dark.png`), fullPage: true });
      await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    }
  }
}

await withE2E({ port: PORT, dbName: "quantify-visual-teacher.db" }, async ({ login }) => {
  step("estudiante: crea una entrega de Hidrostática por API, para tener un detalle que revisar");
  const { page: sPage } = await login(STUDENT, "estudiante");
  const error = await sPage.evaluate(async () => {
    const me = await (await fetch("/api/auth/me")).json();
    const { courses } = await (await fetch("/api/academic/context")).json();
    const course = courses[0];
    const def = await (await fetch("/api/practices/hidrostatica/definition")).json();
    // Valores plausibles: ranuras y brazos con su default (o 1), el resto 1.
    const measurements = def.quantities.map((q) => ({
      quantity_id: q.id,
      instrument_id: null,
      scale_id: null,
      values: [q.default_value ?? 1],
      given_u: q.is_given && q.has_uncertainty ? 0.1 : null,
    }));
    const response = await fetch("/api/submissions/form", {
      method: "POST",
      headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
      body: JSON.stringify({
        course_id: course.id,
        group_id: course.groups[0].id,
        practice_id: "hidrostatica",
        table_number: 1,
        measurements,
      }),
    });
    return response.ok ? null : await response.text();
  });
  assert(error === null, `no se pudo crear la entrega de prueba: ${error}`);
  await sPage.context().close();

  step("docente: login");
  const { page } = await login(TEACHER, "docente");
  const found = [];

  const views = ["submissions", "gradebook", "students", "groups", "courses", "users", "practices", "instruments", "account"];
  for (const view of views) {
    step(`docente: vista ${view}`);
    await page.click(`.tab[data-view="${view}"]:visible`);
    await page.waitForTimeout(300);
    await check(page, view, found);
  }

  step("docente: detalle de una entrega (corrección)");
  await page.click('.tab[data-view="submissions"]:visible');
  await page.click('.submission-item:has-text("Hidrost")');
  await page.waitForSelector(".review-form");
  await check(page, "submission-detail", found);

  for (const practice of ["fluidos-2", "hidrostatica"]) {
    step(`docente: detalle de la práctica ${practice}`);
    await page.click('.tab[data-view="practices"]:visible');
    await page.click(`[data-practice-open][data-practice-id="${practice}"]`);
    await page.waitForSelector("#practice-workspace:not(.hidden) #new-quantity-form", { state: "attached" });
    // Con las altas plegadas y también abiertas: los forms son lo que antes se superponía.
    await check(page, `practice-${practice}`, found);
    await page.$$eval("details[data-add-panel]", (all) => all.forEach((d) => (d.open = true)));
    await check(page, `practice-${practice}-forms`, found);
  }

  assert(found.length === 0, `problemas de layout:\n  ${found.join("\n  ")}`);
});
