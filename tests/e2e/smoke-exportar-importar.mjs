import { readFileSync } from "node:fs";
import { TEACHER, assert, fillAndSubmit, step, waitForStatus, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8187";

async function catalogCount(page) {
  await page.waitForSelector("#practice-catalog [data-practice-open]");
  return page.locator("#practice-catalog [data-practice-open]").count();
}

async function backToCatalog(page) {
  await page.click("#practice-workspace-back");
  await page.waitForSelector("#practice-catalog [data-practice-open]");
}

async function catalogStatus(page, text) {
  await page.waitForFunction(
    (t) => (document.querySelector("#practice-status")?.textContent ?? "").includes(t),
    text,
    { timeout: 10_000 },
  );
}

await withE2E({ port: PORT, dbName: "quantify-exportar-importar.db" }, async ({ login }) => {
  step("docente: abre una práctica publicada con partes");
  const { page } = await login(TEACHER, "docente");
  page.on("dialog", (dialog) => dialog.accept());
  await page.click('.tab.teacher-only[data-view="practices"]');
  const before = await catalogCount(page);
  await page.click('[data-practice-open][data-practice-id="p2-cc"]');
  await page.waitForSelector('#practice-workspace [data-practice-export="borrador"]');
  assert(
    await page.locator('#practice-workspace [data-practice-export="publicada"]').isVisible(),
    "una práctica publicada debía ofrecer exportar la versión publicada",
  );
  const parts = await page.locator('#practice-workspace input[aria-label="Título de la parte"]').count();
  assert(parts === 3, `p2-cc debía tener 3 partes (tiene ${parts})`);

  step("docente: exporta la versión publicada");
  const download = page.waitForEvent("download");
  await page.click('#practice-workspace [data-practice-export="publicada"]');
  const file = await (await download).path();
  const template = JSON.parse(readFileSync(file, "utf8"));
  assert(template.schema_version === 1, "el archivo debía llevar schema_version 1");
  assert(template.parts.length === 3, "el archivo debía llevar las 3 partes");
  await waitForStatus(page, "Versión publicada exportada");

  step("docente: importa el archivo y se abre el borrador");
  await backToCatalog(page);
  await page.setInputFiles("#practice-import-file", file);
  await waitForStatus(page, "Práctica importada en borrador");
  await page.waitForSelector("#practice-workspace .practice-state--borrador");
  const imported = await page.locator('#practice-workspace input[aria-label="Título de la parte"]').count();
  assert(imported === 3, `el borrador importado debía tener 3 partes (tiene ${imported})`);
  await page.waitForSelector("#practice-workspace #practice-problems");
  assert(
    !(await page.locator('#practice-workspace [data-practice-export="publicada"]').count()),
    "un borrador sin publicar no debía ofrecer exportar la versión publicada",
  );
  await backToCatalog(page);
  assert((await catalogCount(page)) === before + 1, "importar debía sumar una práctica al catálogo");
  assert(
    !((await page.locator("#practice-status").textContent()) ?? "").includes("Importando"),
    "terminada la importación, el catálogo no debía seguir diciendo «Importando…»",
  );

  step("docente: un archivo inválido muestra su error y no crea nada");
  await page.setInputFiles("#practice-import-file", {
    name: "roto.json",
    mimeType: "application/json",
    buffer: Buffer.from("esto no es json"),
  });
  await catalogStatus(page, "no es un JSON válido");
  await page.setInputFiles("#practice-import-file", {
    name: "futuro.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify({ ...template, schema_version: 2 })),
  });
  await catalogStatus(page, "versión 2 del formato");
  await page.setInputFiles("#practice-import-file", {
    name: "instrumentos.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify({ instruments: [] })),
  });
  await catalogStatus(page, "no es una práctica exportada");
  assert((await catalogCount(page)) === before + 1, "un archivo inválido no debía crear prácticas");

  step("docente: si abre otra práctica mientras se importa, se queda en ella y ve un aviso");
  let release;
  const held = new Promise((resolve) => (release = resolve));
  await page.route("**/api/practice-templates/import", async (route) => {
    await held;
    await route.continue();
  });
  await page.setInputFiles("#practice-import-file", file);
  await page.click('[data-practice-open][data-practice-id="fluidos-1"]');
  await page.waitForSelector('#practice-workspace [data-practice-export="borrador"]');
  const opened = await page.locator("#practice-workspace h3").first().textContent();
  release();
  await page.waitForSelector(".toast", { hasText: "Práctica importada en borrador" });
  await page.unroute("**/api/practice-templates/import");
  assert(
    (await page.locator("#practice-workspace h3").first().textContent()) === opened,
    "terminar la importación no debía reemplazar la ficha abierta",
  );
  await backToCatalog(page);
  assert((await catalogCount(page)) === before + 2, "la importación retenida debía crear la práctica igual");

  step("docente: un borrador nuevo solo exporta el borrador");
  await page.click('details[data-add-panel="practice"] > summary');
  await fillAndSubmit(page, "#new-practice-form", { name: "Sin publicar", description: "E2E" });
  await page.click("#practice-workspace [data-guide-exit]");
  await page.waitForSelector('#practice-workspace [data-practice-export="borrador"]');
  assert(
    !(await page.locator('#practice-workspace [data-practice-export="publicada"]').count()),
    "un borrador nuevo no debía ofrecer exportar la versión publicada",
  );

  console.log("exportar e importar: versión publicada a archivo, archivo a borrador, y errores en el catálogo.");
});
