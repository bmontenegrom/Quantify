// Harness compartido por los smokes E2E (Playwright): build/arranque del server sobre una DB
// temporal, login, step/assert, y el try/finally de limpieza + screenshot-on-fail. Cada script
// en tests/e2e/*.mjs llama a `withE2E({port, dbName}, flow)` y se queda solo con sus pasos.
//
// Variables de entorno (las mismas para todos los scripts):
//   E2E_SKIP_BUILD  "1" para no correr `cargo build` (CI ya compiló)
//   E2E_HEADED      "1" para ver el navegador

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const ARTIFACTS = join(ROOT, "tests", "e2e", "artifacts");

export const STUDENT = { email: "estudiante@quantify.local", password: "estudiante123" };
export const TEACHER = { email: "docente@quantify.local", password: "docente123" };

let currentStep = "(inicio)";

export function step(name) {
  currentStep = name;
  console.log(`→ ${name}`);
}

export function assert(condition, message) {
  if (!condition) throw new Error(`Falló la verificación: ${message}`);
}

function buildServer() {
  if (process.env.E2E_SKIP_BUILD === "1") return;
  step("cargo build --locked");
  const result = spawnSync("cargo", ["build", "--locked"], { cwd: ROOT, stdio: "inherit", shell: false });
  if (result.status !== 0) throw new Error("cargo build falló");
}

function startServer({ port, dbName, dataDir }) {
  const binary = join(ROOT, "target", "debug", process.platform === "win32" ? "quantify.exe" : "quantify");
  const dbPath = join(dataDir, dbName).replaceAll("\\", "/");
  const child = spawn(binary, [], {
    cwd: ROOT,
    env: {
      ...process.env,
      DATABASE_URL: `sqlite:${dbPath}`,
      APP_BIND_ADDR: `127.0.0.1:${port}`,
      UPLOAD_DIR: join(dataDir, "uploads"),
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  child.on("error", (error) => {
    console.error(`No se pudo lanzar el server (${binary}):`, error.message);
  });
  return child;
}

async function waitForServer(base, timeoutMs = 30_000) {
  step(`esperando al server en ${base}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(base);
      if (response.ok) return;
    } catch {
      // todavía no levantó
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 250));
  }
  throw new Error(`El server no respondió en ${base} tras ${timeoutMs} ms`);
}

/** Crea un contexto nuevo (cookies limpias), registra errores JS/consola y hace login. */
async function login(browser, base, session, { email, password }, who = email) {
  const context = await browser.newContext();
  const page = await context.newPage();
  session.pages.push(page);
  page.on("pageerror", (error) => session.pageErrors.push(`${who}/${currentStep}: ${error.message}`));
  page.on("console", (message) => {
    if (message.type() === "error" && !/Failed to load resource/i.test(message.text())) {
      session.pageErrors.push(`${who}/${currentStep} [console.error]: ${message.text()}`);
    }
  });
  await page.goto(base);
  await page.fill('#login-form input[name="email"]', email);
  await page.fill('#login-form input[name="password"]', password);
  await page.click('#login-form button[type="submit"]');
  await page.waitForSelector("#app-shell:not(.hidden)");
  return { context, page };
}

/**
 * Arma el server con una DB temporal en `dataDir`, corre `flow({ browser, base, login, session })`
 * y limpia todo (server + DB + screenshot en fallo) al final. `login(creds, who?)` abre una
 * sesión nueva y registra su página para el screenshot-on-fail; `session.pageErrors` acumula
 * errores JS/consola de todas las sesiones abiertas.
 */
export async function withE2E({ port, dbName }, flow) {
  buildServer();
  mkdirSync(ARTIFACTS, { recursive: true });
  const dataDir = mkdtempSync(join(tmpdir(), "quantify-e2e-"));
  const base = `http://127.0.0.1:${port}`;
  const server = startServer({ port, dbName, dataDir });
  const session = { pageErrors: [], pages: [] };
  let browser;
  try {
    await waitForServer(base);
    browser = await chromium.launch({ headless: process.env.E2E_HEADED !== "1" });
    await flow({
      browser,
      base,
      session,
      login: (creds, who) => login(browser, base, session, creds, who),
    });
    assert(session.pageErrors.length === 0, `hubo errores de JS/consola:\n${session.pageErrors.join("\n")}`);
    console.log(`✓ ${dbName}: OK`);
  } catch (error) {
    console.error(`✗ ${dbName} falló en el paso: ${currentStep}`);
    console.error(error);
    if (session.pageErrors.length) {
      console.error(`Errores JS/consola acumulados hasta el fallo:\n${session.pageErrors.join("\n")}`);
    }
    const lastPage = session.pages.at(-1);
    if (lastPage) {
      const shot = join(ARTIFACTS, `failure-${dbName.replace(/\.db$/, "")}.png`);
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
