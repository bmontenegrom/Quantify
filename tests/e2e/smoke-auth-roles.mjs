// Smoke E2E de autenticación y ruteo por rol — costuras que ningún unit test cubre:
//   1. Login con contraseña incorrecta: error visible, no entra a la app.
//   2. Estudiante logueado: los tabs de docente (.teacher-only) quedan ocultos.
//   3. Logout: vuelve a la pantalla de login.
//   4. Docente logueado: los mismos tabs quedan visibles.
//
// Uso: `npm run test:e2e` (corre todos los tests/e2e/*.mjs). Variables: E2E_PORT,
// E2E_SKIP_BUILD, E2E_HEADED.

import { STUDENT, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8153";

await withE2E({ port: PORT, dbName: "quantify-auth-roles.db" }, async ({ browser, base, session, login }) => {
  // ── 1) Login con contraseña incorrecta ───────────────────────────────────
  step("login con contraseña incorrecta: muestra error y no entra");
  const badContext = await browser.newContext();
  const badPage = await badContext.newPage();
  session.pages.push(badPage);
  await badPage.goto(base);
  await badPage.fill('#login-form input[name="email"]', STUDENT.email);
  await badPage.fill('#login-form input[name="password"]', "contrasena-incorrecta");
  await badPage.click('#login-form button[type="submit"]');
  await badPage.waitForFunction(
    () => (document.querySelector("#login-status")?.textContent ?? "").trim().length > 0,
  );
  const loginError = (await badPage.locator("#login-status").textContent()) ?? "";
  assert(loginError.trim().length > 0, "el login fallido debía mostrar un mensaje de error");
  assert(
    await badPage.locator("#app-shell").evaluate((el) => el.classList.contains("hidden")),
    "con contraseña incorrecta la app no debía mostrarse",
  );
  await badContext.close();

  // ── 2) Estudiante: no ve los tabs de docente ─────────────────────────────
  step("estudiante: login y verifica que los tabs de docente están ocultos");
  const { context: studentContext, page: studentPage } = await login(STUDENT, "estudiante");
  const teacherTabsHiddenForStudent = await studentPage.evaluate(() =>
    Array.from(document.querySelectorAll(".tab.teacher-only")).every(
      (el) => el.classList.contains("hidden") || getComputedStyle(el).display === "none",
    ),
  );
  assert(teacherTabsHiddenForStudent, "el estudiante no debía ver ningún tab .teacher-only");

  // ── 3) Logout: vuelve a la pantalla de login ─────────────────────────────
  step("estudiante: cierra sesión y vuelve al login");
  await studentPage.click("#logout-button");
  await studentPage.waitForSelector("#login-screen:not(.hidden)");
  assert(
    await studentPage.locator("#app-shell").evaluate((el) => el.classList.contains("hidden")),
    "tras el logout la app debía volver a ocultarse",
  );
  await studentContext.close();

  // ── 4) Docente: sí ve los tabs de docente ────────────────────────────────
  step("docente: login y verifica que los tabs de docente están visibles");
  const { page: teacherPage } = await login(TEACHER, "docente");
  const teacherTabsVisibleForTeacher = await teacherPage.evaluate(() => {
    const tabs = Array.from(document.querySelectorAll(".tab.teacher-only"));
    return tabs.length > 0 && tabs.every((el) => !el.classList.contains("hidden"));
  });
  assert(teacherTabsVisibleForTeacher, "el docente debía ver los tabs .teacher-only");

  console.log("login fallido, ocultamiento por rol y logout se comportan como se espera.");
});
