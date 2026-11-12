
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { symbolHtml } from "../../static/lib.js";

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

export async function openAddForm(page, formSelector) {
  await page.waitForSelector(formSelector, { state: "attached" });
  const details = page.locator(`details:has(${formSelector})`);
  if (!(await details.evaluate((d) => d.open))) await details.locator("summary").first().click();
  await page.waitForSelector(formSelector);
}

export async function apiPost(page, url, body) {
  return page.evaluate(
    async ({ url, body }) => {
      const me = await (await fetch("/api/auth/me")).json();
      const r = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json", "x-csrf-token": me.csrf_token },
        body: JSON.stringify(body),
      });
      if (!r.ok) throw new Error(`${url}: ${await r.text()}`);
      return r.json();
    },
    { url, body },
  );
}

export async function waitForDraft(page, expected) {
  const found = await page.waitForFunction(
    ({ quantityId, count, result, value, chronoOf, marks, discarded }) => {
      for (const key of Object.keys(localStorage).filter((k) => k.startsWith("quantify-draft:"))) {
        const draft = JSON.parse(localStorage.getItem(key));
        if (chronoOf) {
          const entry = Object.entries(draft.chrono ?? {}).find(([k]) => k.split("#")[0] === chronoOf)?.[1];
          const ok = entry && (marks == null || entry.marks?.length === marks) && (discarded == null || entry.discarded?.length === discarded);
          if (ok) return entry;
        } else if (result) {
          if (draft.finalResults?.some((r) => r.symbol === result && r.value === value)) return true;
        } else if (draft.measurements?.some((m) => m.quantity_id === quantityId && m.values?.length === count)) {
          return true;
        }
      }
      return null;
    },
    expected,
    { timeout: 10_000 },
  );
  return found.jsonValue();
}

export function symbolText(symbol) {
  return symbolHtml(symbol).replace(/<[^>]+>/g, "");
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
      SEED_DEMO_DATA: "true",
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  child.on("error", (error) => {
    console.error(`No se pudo lanzar el server (${binary}):`, error.message);
  });
  return child;
}

async function waitForServer(base, timeoutMs = 60_000) {
  step(`esperando al server en ${base}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(base);
      if (response.ok) return;
    } catch {
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 250));
  }
  throw new Error(`El server no respondió en ${base} tras ${timeoutMs} ms`);
}

async function login(browser, base, session, { email, password }, who = email) {
  const previousStep = currentStep;
  currentStep = `${previousStep} (login de ${who})`;
  const context = await browser.newContext();
  context.setDefaultTimeout(60_000);
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
  currentStep = previousStep;
  return { context, page };
}

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
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 500));
    rmSync(dataDir, { recursive: true, force: true, maxRetries: 5 });
  }
}
