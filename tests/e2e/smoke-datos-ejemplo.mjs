import { join } from "node:path";
import { ARTIFACTS, TEACHER, assert, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8181";

const PRACTICES = [
  "p1-estadistica", "p3-relajacion", "p2-cc", "fluidos-1", "viscosidad",
  "fluidos-2", "filtros", "ca-rlc", "hidrostatica",
];

function collect(node, key, out = []) {
  if (Array.isArray(node)) node.forEach((n) => collect(n, key, out));
  else if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node)) {
      if (k === key) out.push(v);
      else collect(v, key, out);
    }
  }
  return out;
}

async function tryWithExampleData(page, practiceId) {
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${practiceId}"]`);
  await page.click("[data-practice-try-draft]");
  await page.waitForSelector("#draft-preview-banner:not(.hidden)");
  await page.click("#draft-preview-fill:not([disabled])");
  const response = page.waitForResponse((r) => r.url().includes("/analyze-preview"));
  await page.click("#submit-button");
  const analysis = await (await response).json();
  await page.waitForSelector('#submit-status:has-text("no se guardó nada")', { timeout: 15_000 });
  const warnings = collect(analysis, "warnings").flat();
  assert(warnings.length === 0, `${practiceId}: los datos de ejemplo no debían dar advertencias: ${warnings.join(" | ")}`);
  for (const points of collect(analysis, "points")) {
    const xs = (points ?? []).map((p) => (Array.isArray(p) ? p[0] : p?.x)).filter(Number.isFinite);
    if (xs.length < 2) continue;
    const range = Math.max(...xs) - Math.min(...xs);
    assert(range > 1e-9 * Math.max(...xs.map(Math.abs)), `${practiceId}: un eje horizontal quedó constante: ${xs}`);
  }
  const result = (await page.locator("#latest-result").textContent()) ?? "";
  assert(result.trim().length > 0, `${practiceId}: la prueba debía mostrar un resultado`);
  assert(!result.includes("NaN"), `${practiceId}: el resultado no debía tener NaN: ${result.slice(0, 400)}`);
  await page.screenshot({ path: join(ARTIFACTS, `datos-ejemplo-${practiceId}.png`), fullPage: true });
  let asked = false;
  page.once("dialog", (d) => { asked = true; return d.dismiss(); });
  await page.click("#draft-preview-fill");
  assert(asked, `${practiceId}: rellenar sobre valores cargados debía pedir confirmación`);
  assert(await page.locator("#latest-result").isVisible(), `${practiceId}: cancelar no debía tocar el resultado`);
  page.once("dialog", (d) => d.accept());
  await page.click("#draft-preview-fill");
  assert(await page.locator("#latest-result").isHidden(), `${practiceId}: rellenar debía ocultar el resultado anterior`);
  await page.click("#draft-preview-back");
}

await withE2E({ port: PORT, dbName: "quantify-datos-ejemplo.db" }, async ({ login }) => {
  step("docente: login");
  const { page } = await login(TEACHER, "docente");
  for (const practiceId of PRACTICES) {
    step(`docente: prueba ${practiceId} con datos de ejemplo`);
    await tryWithExampleData(page, practiceId);
  }
  console.log("todas las prácticas sembradas calculan con datos de ejemplo, sin advertencias.");
});
