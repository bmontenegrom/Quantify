// Smoke visual acotado para formularios de carga en tema oscuro.
// Uso: `node tests/e2e/visual-forms.mjs`.

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const PORT = process.env.E2E_PORT ?? "8141";
const BASE = `http://127.0.0.1:${PORT}`;
const ARTIFACTS = join(ROOT, "tests", "e2e", "artifacts");
const STUDENT = { email: "estudiante@quantify.local", password: "estudiante123" };

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function buildServer() {
  if (process.env.E2E_SKIP_BUILD === "1") return;
  const result = spawnSync("cargo", ["build", "--locked"], { cwd: ROOT, stdio: "inherit", shell: false });
  if (result.status !== 0) throw new Error("cargo build fallo");
}

function startServer(dataDir) {
  const binary = join(ROOT, "target", "debug", process.platform === "win32" ? "quantify.exe" : "quantify");
  const dbPath = join(dataDir, "quantify-visual.db").replaceAll("\\", "/");
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
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(BASE);
      if (response.ok) return;
    } catch {
      // todavia no levanto
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 250));
  }
  throw new Error(`El server no respondio en ${BASE}`);
}

async function login(page) {
  await page.goto(BASE);
  await page.fill('#login-form input[name="email"]', STUDENT.email);
  await page.fill('#login-form input[name="password"]', STUDENT.password);
  await page.click('#login-form button[type="submit"]');
  await page.waitForSelector("#app-shell:not(.hidden)");
}

async function openPractice(page, practiceId) {
  const navItem = page.locator(`#practice-nav-children .nav-child[data-practice-id="${practiceId}"]`);
  if (await navItem.count()) {
    await navItem.click();
  } else {
    await page.evaluate((id) => {
      const select = document.querySelector("#practice-select");
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      document.querySelector('#practice-nav-children .nav-child')?.click();
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }, practiceId);
  }
  await page.waitForSelector("#measurement-fields .measurement-row, #measurement-fields .series-table");
}

async function assertInputContrast(page) {
  const sample = await page.locator("#measurement-fields input").first().evaluate((el) => {
    const s = getComputedStyle(el);
    return { background: s.backgroundColor, color: s.color };
  });
  assert(sample.background !== sample.color, `input sin contraste: ${JSON.stringify(sample)}`);
  assert(!/255,\s*255,\s*255/.test(sample.background), `input blanco en tema oscuro: ${sample.background}`);
}

async function main() {
  mkdirSync(ARTIFACTS, { recursive: true });
  const dataDir = mkdtempSync(join(tmpdir(), "quantify-visual-"));
  buildServer();
  const server = startServer(dataDir);
  let browser;
  try {
    await waitForServer();
    browser = await chromium.launch({ headless: process.env.E2E_HEADED !== "1" });
    const page = await browser.newPage({ viewport: { width: 1366, height: 900 } });
    await login(page);
    await page.click("#theme-toggle");

    await openPractice(page, "p2-cc");
    await assertInputContrast(page);
    await page.screenshot({ path: join(ARTIFACTS, "visual-cc-dark.png"), fullPage: true });

    // Tabs de partes: alternan secciones de la MISMA práctica sin recargar la definición.
    const partTabs = page.locator("#practice-part-tabs .part-tab");
    assert((await partTabs.count()) === 3, "p2-cc debe tener 3 tabs de partes");
    await partTabs.nth(0).click(); // Serie
    assert(
      await page.locator('#measurement-fields [data-section="serie"]').first().isVisible(),
      "la sección serie debe verse en su tab",
    );
    await page.screenshot({ path: join(ARTIFACTS, "visual-cc-serie.png"), fullPage: true });
    await partTabs.nth(1).click(); // Paralelo
    assert(
      await page.locator('#measurement-fields [data-section="paralelo"]').first().isVisible(),
      "la sección paralelo debe verse en su tab",
    );
    assert(
      await page.locator('#measurement-fields [data-section="serie"]').first().isHidden(),
      "la sección serie debe ocultarse fuera de su tab",
    );
    await page.screenshot({ path: join(ARTIFACTS, "visual-cc-paralelo.png"), fullPage: true });
    await partTabs.nth(2).click(); // Curva de potencia
    await page.waitForSelector("#measurement-fields .series-table", { state: "visible" });

    // Columna P en vivo: al tipear R e I la celda P muestra I²·R.
    const firstRow = page.locator("#measurement-fields .series-row").first();
    await firstRow.locator(".series-value").nth(0).fill("100"); // R
    await firstRow.locator(".series-value").nth(1).fill("0.5"); // I
    const liveText = await firstRow.locator(".series-live-value").innerText();
    assert(liveText.replace(",", ".").includes("25"), `celda P debía mostrar 25, mostró "${liveText}"`);
    await page.screenshot({ path: join(ARTIFACTS, "visual-cc-potencia.png"), fullPage: true });

    // Finales sin incertidumbre: P_max_*/RP_max_* no tienen input U; los VR teóricos sí.
    for (const sym of ["P_max_e", "P_max_t", "RP_max_e", "RP_max_t"]) {
      const row = page.locator(`#measurement-fields [data-final-result="1"][data-symbol="${sym}"]`);
      assert((await row.count()) === 1, `falta la fila de resultado final ${sym}`);
      assert((await row.locator(".final-result-u").count()) === 0, `${sym} no debe tener input U`);
    }
    assert(
      (await page
        .locator('#measurement-fields [data-final-result="1"][data-symbol="VR1_s_t"] .final-result-u')
        .count()) === 1,
      "VR1_s_t debe tener input U",
    );

    await openPractice(page, "fluidos-1");
    await assertInputContrast(page);
    await page.screenshot({ path: join(ARTIFACTS, "visual-fluidos1-dark.png"), fullPage: true });

    await openPractice(page, "fluidos-2");
    await assertInputContrast(page);
    await page.screenshot({ path: join(ARTIFACTS, "visual-fluidos2-dark.png"), fullPage: true });

    console.log("Visual forms smoke OK");
  } finally {
    await browser?.close();
    server.kill();
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 500));
    rmSync(dataDir, { recursive: true, force: true, maxRetries: 5 });
  }
}

await main();
