// E2E de navegador (Playwright) sobre el flujo completo de Quantify.
//
// Levanta el server compilado sobre una base temporal sembrada y recorre:
//   1. Login del estudiante, formulario de medición de P1 (péndulo con
//      cronómetro), entrega y verificación del gating (cálculo oculto).
//   2. Carga de "Mis cálculos" del estudiante.
//   3. Login del docente, revisión con nota/comentario y habilitación de
//      visibilidad del cálculo automático.
//   4. Login del estudiante de nuevo: ve el análisis, la comparación
//      auto-vs-alumno y el comentario del docente.
//
// Uso: `npm run test:e2e` (corre todos los tests/e2e/*.mjs). Variables opcionales:
//   E2E_PORT        puerto del server (default 8123)
//   E2E_SKIP_BUILD  "1" para no correr `cargo build` (CI ya compiló)
//   E2E_HEADED      "1" para ver el navegador

import { STUDENT, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8123";
const REVIEW_COMMENT = "Muy buen trabajo (E2E)";
const STUDENT_COMMENT = "No pude tomar réplicas extra por falta de tiempo (E2E)";

async function studentSubmitsP1(page) {
  step("estudiante: abre la práctica P1 (péndulo, tratamiento estadístico)");
  await page.click('#practice-nav-children .nav-child:has-text("Tratamiento estad")');
  await page.waitForSelector(".measurement-row--chrono");
  await page.selectOption("#table-select", "1");

  // Operador 1 (obligatorio) es la pestaña activa por default; operador 2/3 son opcionales y
  // quedan sin cargar en este test.
  step("estudiante: registra períodos del Operador 1 con el cronómetro");
  const chronoRow = page.locator('[data-section="op1"] .measurement-row--chrono');
  await chronoRow.locator(".chrono-start").click();
  for (let i = 0; i < 6; i++) {
    await page.waitForTimeout(120);
    await chronoRow.locator(".chrono-mark").click();
  }
  await chronoRow.locator(".chrono-stop").click();
  const chronoCount = await chronoRow.locator(".chrono-count").textContent();
  assert(/3 lecturas/.test(chronoCount ?? ""), `el cronómetro debía producir 3 lecturas (vi: "${chronoCount}")`);

  step("estudiante: completa L (dato de cátedra) y t_med (sin incertidumbre, sin instrumento)");
  const lRow = page.locator('.measurement-row--given:has-text("Longitud")');
  await lRow.locator(".measure-given-value").fill("1");
  await lRow.locator(".measure-given-u").fill("0.002");
  const tMedRow = page.locator('.measurement-row--given:has-text("semiamplitud")');
  await tMedRow.locator(".measure-given-value").fill("12.5");
  assert(
    (await tMedRow.locator(".measure-given-u").count()) === 0,
    "t_med no debía tener campo de incertidumbre U",
  );

  step("estudiante: agrega observaciones opcionales");
  await page.fill("#student-comment", STUDENT_COMMENT);

  step("estudiante: entrega el formulario");
  await page.click("#submit-button");
  await page.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

  step("estudiante: el cálculo automático sigue oculto (gating en el server)");
  const latest = await page.locator("#latest-result").textContent();
  assert(
    (latest ?? "").includes("El docente todavia no habilito"),
    "la entrega recién creada no debía mostrar el cálculo automático al estudiante",
  );
  assert(
    (latest ?? "").includes(STUDENT_COMMENT),
    "las observaciones del alumno debían verse aunque el análisis esté oculto",
  );
}

async function studentSavesOwnResults(page) {
  step("estudiante: abre la entrega y carga sus cálculos");
  await page.click('.tab.student-only[data-view="submissions"]');
  await page.click('.submission-item:has-text("Mesa 1")');
  await page.waitForSelector(".student-results-form");
  await page.fill('.student-value[data-symbol="g1"]', "9.78");
  await page.fill('.student-u[data-symbol="g1"]', "0.08");
  // Marcamos el form actual para poder esperar al que lo reemplaza: el guardado refetchea el
  // detalle y lo re-renderiza. No sirve mirar el valor del input (es el que acabamos de tipear,
  // matchea al toque y deja el refetch en vuelo cuando el test cierra sesión -> 401/pageerror).
  await page.$eval(".student-results-form", (form) => { form.dataset.e2ePrev = "1"; });
  await page.click('.student-results-form button[type="submit"]');
  await page.waitForSelector(".student-results-form:not([data-e2e-prev])");
}

async function teacherReviews(page) {
  step("docente: abre la entrega de Mesa 1");
  await page.click('.tab.teacher-only[data-view="submissions"]');
  await page.click(".submission-table-group .submission-item");
  await page.waitForSelector(".review-form");

  step("docente: ve el análisis automático y la comparación del alumno");
  const detail = await page.locator("#submission-detail-body").textContent();
  assert((detail ?? "").includes("Mensurandos"), "el docente debía ver los mensurandos derivados");
  assert(
    (detail ?? "").includes("Comparación: tus cálculos vs automático"),
    "el docente debía ver la tabla de comparación",
  );
  assert(
    (detail ?? "").includes(STUDENT_COMMENT),
    "el docente debía ver las observaciones del alumno sin habilitar la visibilidad",
  );

  step("docente: guarda la corrección y habilita la visibilidad");
  await page.selectOption('.review-form select[name="status"]', "aprobada");
  await page.fill('.review-form input[name="score"]', "9");
  await page.fill('.review-form textarea[name="teacher_comment"]', REVIEW_COMMENT);
  await page.check('.review-form input[name="results_visible"]');
  await page.click('.review-form button[type="submit"]');
  await page.waitForSelector('.review-form:has-text("Revisada:")');
}

async function studentSeesResults(page) {
  step("estudiante: ahora ve análisis, comparación y comentario");
  await page.click('.tab.student-only[data-view="submissions"]');
  await page.click('.submission-item:has-text("Mesa 1")');
  await page.waitForSelector(".compare-table");
  const detail = await page.locator("#submission-detail-body").textContent();
  assert((detail ?? "").includes("u_A"), "el estudiante debía ver la tabla de incertidumbres");
  assert((detail ?? "").includes(REVIEW_COMMENT), "el estudiante debía ver el comentario del docente");
  assert(
    (detail ?? "").includes(STUDENT_COMMENT),
    "el estudiante debía seguir viendo sus propias observaciones",
  );
  assert(
    (detail ?? "").includes("quedó congelado"),
    "el formulario de cálculos propios debía quedar bloqueado",
  );
  const statusBadge = await page.locator("#submission-detail-body .status").first().textContent();
  assert((statusBadge ?? "").includes("aprobada"), `la entrega debía figurar aprobada (vi: "${statusBadge}")`);
}

await withE2E({ port: PORT, dbName: "quantify-e2e.db" }, async ({ login }) => {
  // Sesión 1: el estudiante entrega y carga sus cálculos.
  {
    const { context, page } = await login(STUDENT, "estudiante");
    await studentSubmitsP1(page);
    await studentSavesOwnResults(page);
    step("estudiante: cierra sesión");
    await page.click("#logout-button");
    await page.waitForSelector("#login-screen:not(.hidden)");
    await context.close();
  }

  // Sesión 2: el docente revisa y habilita los resultados.
  {
    const { context, page } = await login(TEACHER, "docente");
    await teacherReviews(page);
    await context.close();
  }

  // Sesión 3: el estudiante verifica lo habilitado.
  {
    const { context, page } = await login(STUDENT, "estudiante");
    await studentSeesResults(page);
    await context.close();
  }

  console.log("✓ entrega, revisión con visibilidad y comparación funcionan de punta a punta.");
});
