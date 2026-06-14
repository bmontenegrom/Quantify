// Smoke visual (Playwright) de la práctica sembrada Fluidos II (regresión + Motor F).
//
// Cubre lo que el E2E automatizado NO toca, siguiendo el flujo real de la app: el
// formulario de entrega vive en el nav del ESTUDIANTE (los docentes no entregan,
// solo revisan). Recorre:
//   1. Estudiante: abre Fluidos II, ve el form de regresión (datos compartidos +
//      grilla por punto h/t), la vista previa del ajuste aparece, y entrega.
//   2. Docente: abre la entrega en revisión y ve el análisis — ajuste lineal,
//      mensurando M_medio y tabla de mensurandos agregados (Re_max/Re_min/
//      Re_medio/M_teorico).
// Falla si hay errores JS en consola.
//
// Uso: `node tests/e2e/smoke-fluidos2.mjs`. Variables: E2E_PORT, E2E_SKIP_BUILD, E2E_HEADED.

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const PORT = process.env.E2E_PORT ?? "8137";
const BASE = `http://127.0.0.1:${PORT}`;
const ARTIFACTS = join(ROOT, "tests", "e2e", "artifacts");
const TEACHER = { email: "docente@quantify.local", password: "docente123" };
const STUDENT = { email: "estudiante@quantify.local", password: "estudiante123" };

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
  const r = spawnSync("cargo", ["build", "--locked"], { cwd: ROOT, stdio: "inherit", shell: false });
  if (r.status !== 0) throw new Error("cargo build falló");
}

function startServer(dataDir) {
  const binary = join(ROOT, "target", "debug", process.platform === "win32" ? "quantify.exe" : "quantify");
  const dbPath = join(dataDir, "quantify-smoke.db").replaceAll("\\", "/");
  return spawn(binary, [], {
    cwd: ROOT,
    env: {
      ...process.env,
      DATABASE_URL: `sqlite:${dbPath}`,
      APP_BIND_ADDR: `127.0.0.1:${PORT}`,
      UPLOAD_DIR: join(dataDir, "uploads"),
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
}

async function waitForServer(timeoutMs = 30_000) {
  step(`esperando al server en ${BASE}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(BASE);
      if (r.ok) return;
    } catch {
      // todavía no levantó
    }
    await new Promise((s) => setTimeout(s, 250));
  }
  throw new Error(`El server no respondió en ${BASE} tras ${timeoutMs} ms`);
}

async function run() {
  mkdirSync(ARTIFACTS, { recursive: true });
  const dataDir = mkdtempSync(join(tmpdir(), "quantify-smoke-"));
  buildServer();
  const server = startServer(dataDir);
  const pageErrors = [];
  let browser;
  try {
    await waitForServer();
    browser = await chromium.launch({ headless: process.env.E2E_HEADED !== "1" });

    // Engancha errores JS reales (ignora el ruido de red, p. ej. el 401 del chequeo de sesión
    // antes del login, que es una respuesta HTTP esperada, no una excepción de JS).
    const wireErrors = (page, who) => {
      page.on("pageerror", (e) => pageErrors.push(`${who}/${currentStep}: ${e.message}`));
      page.on("console", (m) => {
        if (m.type() === "error" && !/Failed to load resource/i.test(m.text())) {
          pageErrors.push(`${who}/${currentStep} [console.error]: ${m.text()}`);
        }
      });
    };
    const login = async (creds, who) => {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();
      wireErrors(page, who);
      await page.goto(BASE);
      await page.fill('#login-form input[name="email"]', creds.email);
      await page.fill('#login-form input[name="password"]', creds.password);
      await page.click('#login-form button[type="submit"]');
      await page.waitForSelector("#app-shell:not(.hidden)");
      return page;
    };

    // ── 1) Estudiante entrega Fluidos II ─────────────────────────────────────
    step("estudiante: login");
    const sPage = await login(STUDENT, "estudiante");

    step("estudiante: abre la práctica Fluidos II desde el nav");
    await sPage.click('#practice-nav-children .nav-child:has-text("Fluidos II")');
    await sPage.waitForSelector(".series-table");

    step("verifica estructura: datos compartidos + grilla por punto h/t");
    const sharedText = await sPage.locator(".shared-quantities").textContent();
    for (const sym of ["h_max", "R_cap", "L_cap", "R_recip", "g", "rho", "mu_agua", "kp", "Temp"]) {
      assert((sharedText ?? "").includes(sym), `falta el escalar compartido ${sym} en "Datos compartidos"`);
    }
    const headers = (await sPage.locator(".series-table thead th").allTextContents()).join(" ");
    assert(/\bh\b/.test(headers), `la tabla de la serie debía tener columna h (vi: ${headers})`);
    assert(/\bt\b/.test(headers), `la tabla de la serie debía tener columna t (vi: ${headers})`);
    await sPage.screenshot({ path: join(ARTIFACTS, "fluidos2-form.png"), fullPage: true });

    step("estudiante: selecciona mesa");
    await sPage.selectOption("#table-select", "1");

    // Mapa símbolo→id desde la definición (robusto frente a cambios de orden).
    const idBySym = await sPage.evaluate(async () => {
      const r = await fetch("/api/practices/fluidos-2/definition");
      const def = await r.json();
      return Object.fromEntries(def.quantities.map((q) => [q.symbol, q.id]));
    });

    step("estudiante: completa los escalares compartidos");
    const shared = { h_max: 0.36, R_cap: 0.001, L_cap: 0.1, R_recip: 0.05, rho: 1000, mu_agua: 0.001, kp: 0.78, Temp: 20 };
    for (const [sym, val] of Object.entries(shared)) {
      const row = sPage.locator(`.shared-quantities .measurement-row[data-quantity-id="${idBySym[sym]}"]`);
      await row.locator(".measure-value").first().fill(String(val));
    }
    // g es dato de cátedra (valor ± U).
    const gRow = sPage.locator(`.measurement-row--given[data-quantity-id="${idBySym["g"]}"]`);
    await gRow.locator(".measure-given-value").fill("9.8");
    await gRow.locator(".measure-given-u").fill("0.1");

    step("estudiante: carga 3 puntos (h, t)");
    const points = [
      { h: 0.36, t: 0 },
      { h: 0.25, t: 10 },
      { h: 0.16, t: 20 },
    ];
    const rows = sPage.locator(".series-table tbody .series-row");
    for (let i = 0; i < points.length; i++) {
      const row = rows.nth(i);
      await row.locator(`.series-value[data-quantity-id="${idBySym["h"]}"]`).fill(String(points[i].h));
      await row.locator(`.series-value[data-quantity-id="${idBySym["t"]}"]`).fill(String(points[i].t));
    }

    step("verifica que la vista previa del ajuste aparece");
    // El input dispara un preview con debounce (~350ms); esperamos a que pinte algo.
    await sPage.locator(".series-table").dispatchEvent("change");
    await sPage.waitForFunction(
      () => (document.querySelector(".series-preview")?.textContent ?? "").trim().length > 0,
      { timeout: 10_000 },
    );
    await sPage.screenshot({ path: join(ARTIFACTS, "fluidos2-preview.png"), fullPage: true });

    step("estudiante: entrega el formulario");
    await sPage.click("#submit-button");
    await sPage.waitForSelector('#submit-status:has-text("Entrega guardada")', { timeout: 15_000 });

    step("estudiante: el cálculo automático sigue oculto (gating del server)");
    const latest = (await sPage.locator("#latest-result").textContent()) ?? "";
    assert(
      latest.includes("El docente todavia no habilito"),
      "la entrega recién creada no debía mostrar el análisis al estudiante (gating)",
    );

    // ── 2) Docente revisa y ve el análisis ───────────────────────────────────
    step("docente: login");
    const tPage = await login(TEACHER, "docente");

    step("docente: abre la entrega de Fluidos II en revisión");
    await tPage.click('.tab.teacher-only[data-view="submissions"]');
    await tPage.click('.submission-item:has-text("Fluidos II")');
    await tPage.waitForSelector(".review-form");

    step("docente: ve el ajuste, M_medio y la tabla de mensurandos agregados");
    const detail = (await tPage.locator("#submission-detail-body").textContent()) ?? "";
    assert(/Mensurandos agregados/.test(detail), "el docente debía ver la tabla de mensurandos agregados");
    for (const sym of ["M_medio", "Re_max", "Re_min", "Re_medio", "M_teorico"]) {
      assert(detail.includes(sym), `el análisis debía mostrar ${sym}`);
    }
    await tPage.screenshot({ path: join(ARTIFACTS, "fluidos2-analisis.png"), fullPage: true });

    // ── 3) Docente agrega una magnitud ADIMENSIONAL desde el admin ───────────
    step("docente: abre Fluidos II en el admin de prácticas");
    await tPage.click('.tab.teacher-only[data-view="practices"]');
    await tPage.click('[data-practice-open][data-practice-id="fluidos-2"]');
    await tPage.waitForSelector("#new-quantity-form");

    step("docente: agrega una magnitud sin unidad (adimensional)");
    const qForm = tPage.locator("#new-quantity-form");
    await qForm.locator('input[name="symbol"]').fill("factor_test");
    await qForm.locator('input[name="name"]').fill("Factor de prueba adimensional");
    // Unidad: la dejamos vacía a propósito.
    await qForm.locator('input[name="unit"]').fill("");
    await qForm.locator('button[type="submit"]').click();

    step("verifica que la magnitud adimensional se guardó y se muestra como tal");
    await tPage.waitForFunction(
      () => document.querySelector("#practice-workspace")?.textContent?.includes("Magnitud agregada"),
      { timeout: 10_000 },
    );
    const adminText = (await tPage.locator("#practice-workspace").textContent()) ?? "";
    assert(adminText.includes("factor_test"), "la magnitud adimensional debía aparecer en la lista");
    assert(adminText.includes("adimensional"), 'la unidad vacía debía mostrarse como "adimensional"');
    assert(
      !adminText.includes("datos de magnitud invalidos"),
      "no debía rechazar la magnitud por unidad vacía",
    );
    await tPage.screenshot({ path: join(ARTIFACTS, "fluidos2-admin-adimensional.png"), fullPage: true });

    assert(pageErrors.length === 0, `hubo errores JS en consola:\n${pageErrors.join("\n")}`);
    console.log("\n✅ Smoke Fluidos II OK — entrega+análisis (M_medio/agregados) y alta de magnitud adimensional desde el admin, sin errores JS.");
  } catch (error) {
    console.error(`\n❌ Smoke falló en paso "${currentStep}": ${error.message}`);
    if (pageErrors.length) console.error(`Errores JS:\n${pageErrors.join("\n")}`);
    process.exitCode = 1;
  } finally {
    if (browser) await browser.close();
    server.kill();
    try {
      rmSync(dataDir, { recursive: true, force: true });
    } catch {
      // best-effort
    }
  }
}

run();
