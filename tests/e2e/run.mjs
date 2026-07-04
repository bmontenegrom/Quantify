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
// Uso: `npm run test:e2e`. Variables opcionales:
//   E2E_PORT        puerto del server (default 8123)
//   E2E_SKIP_BUILD  "1" para no correr `cargo build` (CI ya compiló)
//   E2E_HEADED      "1" para ver el navegador
//
// No lo descubre `node --test` a propósito (no matchea *.test.js): necesita
// un server corriendo y se ejecuta como job aparte en CI.

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const PORT = process.env.E2E_PORT ?? "8123";
const BASE = `http://127.0.0.1:${PORT}`;
const ARTIFACTS = join(ROOT, "tests", "e2e", "artifacts");

const STUDENT = { email: "estudiante@quantify.local", password: "estudiante123" };
const TEACHER = { email: "docente@quantify.local", password: "docente123" };
const REVIEW_COMMENT = "Muy buen trabajo (E2E)";
const STUDENT_COMMENT = "No pude tomar réplicas extra por falta de tiempo (E2E)";

let currentStep = "(inicio)";
function step(name) {
  currentStep = name;
  console.log(`→ ${name}`);
}

function assert(condition, message) {
  if (!condition) throw new Error(`Falló la verificación: ${message}`);
}

function buildServer() {
  if (process.env.E2E_SKIP_BUILD === "1") return;
  step("cargo build --locked");
  const result = spawnSync("cargo", ["build", "--locked"], { cwd: ROOT, stdio: "inherit", shell: false });
  if (result.status !== 0) throw new Error("cargo build falló");
}

function startServer(dataDir) {
  const binary = join(ROOT, "target", "debug", process.platform === "win32" ? "quantify.exe" : "quantify");
  const dbPath = join(dataDir, "quantify-e2e.db").replaceAll("\\", "/");
  const child = spawn(binary, [], {
    cwd: ROOT,
    env: {
      ...process.env,
      DATABASE_URL: `sqlite:${dbPath}`,
      APP_BIND_ADDR: `127.0.0.1:${PORT}`,
      UPLOAD_DIR: join(dataDir, "uploads"),
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  child.on("error", (error) => {
    console.error(`No se pudo lanzar el server (${binary}):`, error.message);
  });
  return child;
}

async function waitForServer(timeoutMs = 30_000) {
  step(`esperando al server en ${BASE}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(BASE);
      if (response.ok) return;
    } catch {
      // todavía no levantó
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 250));
  }
  throw new Error(`El server no respondió en ${BASE} tras ${timeoutMs} ms`);
}

/** Crea un contexto nuevo (cookies limpias), registra errores JS y hace login. */
async function loginAs(browser, pageErrors, { email, password }) {
  const context = await browser.newContext();
  const page = await context.newPage();
  page.on("pageerror", (error) => pageErrors.push(`${email}: ${error.message}`));
  await page.goto(BASE);
  await page.fill('#login-form input[name="email"]', email);
  await page.fill('#login-form input[name="password"]', password);
  await page.click('#login-form button[type="submit"]');
  await page.waitForSelector("#app-shell:not(.hidden)");
  return { context, page };
}

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
  await page.click('.student-results-form button[type="submit"]');
  // El guardado re-renderiza el detalle con los valores persistidos.
  await page.waitForFunction(
    () => document.querySelector('.student-value[data-symbol="g1"]')?.value === "9.78",
  );
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

async function main() {
  buildServer();
  mkdirSync(ARTIFACTS, { recursive: true });
  const dataDir = mkdtempSync(join(tmpdir(), "quantify-e2e-"));
  const server = startServer(dataDir);
  const pageErrors = [];
  let browser;
  let lastPage = null;
  try {
    await waitForServer();
    browser = await chromium.launch({ headless: process.env.E2E_HEADED !== "1" });

    // Sesión 1: el estudiante entrega y carga sus cálculos.
    {
      const { context, page } = await loginAs(browser, pageErrors, STUDENT);
      lastPage = page;
      await studentSubmitsP1(page);
      await studentSavesOwnResults(page);
      step("estudiante: cierra sesión");
      await page.click("#logout-button");
      await page.waitForSelector("#login-screen:not(.hidden)");
      await context.close();
    }

    // Sesión 2: el docente revisa y habilita los resultados.
    {
      const { context, page } = await loginAs(browser, pageErrors, TEACHER);
      lastPage = page;
      await teacherReviews(page);
      await context.close();
    }

    // Sesión 3: el estudiante verifica lo habilitado.
    {
      const { context, page } = await loginAs(browser, pageErrors, STUDENT);
      lastPage = page;
      await studentSeesResults(page);
      await context.close();
    }

    lastPage = null;
    assert(pageErrors.length === 0, `hubo errores de JS en la página:\n${pageErrors.join("\n")}`);
    console.log("✓ E2E verde: entrega, revisión con visibilidad y comparación funcionan de punta a punta.");
  } catch (error) {
    console.error(`✗ E2E falló en el paso: ${currentStep}`);
    console.error(error);
    if (lastPage) {
      const shot = join(ARTIFACTS, "failure.png");
      await lastPage.screenshot({ path: shot, fullPage: true }).catch(() => {});
      console.error(`Captura del fallo: ${shot}`);
    }
    process.exitCode = 1;
  } finally {
    await browser?.close();
    server.kill();
    // En Windows el proceso puede tardar en soltar el archivo de la DB.
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 500));
    rmSync(dataDir, { recursive: true, force: true, maxRetries: 5 });
  }
}

await main();
