import { TEACHER, assert, openAddForm, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8172";
const PRACTICE_ID = "p1-estadistica";

await withE2E({ port: PORT, dbName: "quantify-simbolo-mostrar.db" }, async ({ login }) => {
  const { page } = await login(TEACHER, "docente");
  const status = async () => (await page.locator("#practice-workspace .practice-status").first().textContent()) ?? "";
  const definition = () =>
    page.evaluate(async (id) => (await fetch(`/api/practices/${id}/definition`)).json(), PRACTICE_ID);
  const failDisplaySymbol = () => page.route("**/display-symbol", (route) => route.abort());
  const restoreDisplaySymbol = () => page.unroute("**/display-symbol");

  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${PRACTICE_ID}"]`);

  step("docente: el alta se guarda aunque falle el símbolo a mostrar, y se ve");
  await openAddForm(page, "#new-quantity-form");
  await page.fill('#new-quantity-form [name="symbol"]', "Zz");
  await page.fill('#new-quantity-form [name="display_symbol"]', "Z_z");
  await page.fill('#new-quantity-form [name="name"]', "Zeta");
  await failDisplaySymbol();
  await page.click('#new-quantity-form button[type="submit"]');
  await page.waitForFunction(() =>
    (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes("símbolo a mostrar"),
  );
  await restoreDisplaySymbol();
  const message = await status();
  assert(message.includes("Se guardó Zz"), `el mensaje debía decir qué se guardó: ${message}`);
  assert(message.includes("No se pudo conectar"), `el error debía estar en español: ${message}`);
  const zz = (await definition()).quantities.find((q) => q.symbol === "Zz");
  assert(zz, "la magnitud debía quedar guardada");
  await page.locator(`[data-edit-quantity][data-qid="${zz.id}"]`).waitFor({ state: "attached" });

  step("docente: al editarla se reintenta el símbolo a mostrar");
  await page.click(`[data-edit-quantity][data-qid="${zz.id}"]`);
  await page.fill('#edit-quantity-form [name="display_symbol"]', "Z_z");
  await page.click('#edit-quantity-form button[type="submit"]');
  await page.waitForFunction(() =>
    (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes("Magnitud actualizada"),
  );
  assert((await definition()).display_symbols.Zz === "Z_z", "el reintento debía guardar el símbolo a mostrar");

  step("docente: una edición a medias muestra lo que quedó guardado");
  await page.click(`[data-edit-quantity][data-qid="${zz.id}"]`);
  await page.fill('#edit-quantity-form [name="name"]', "Nombre nuevo");
  await page.fill('#edit-quantity-form [name="display_symbol"]', "Z_otro");
  await failDisplaySymbol();
  await page.click('#edit-quantity-form button[type="submit"]');
  await page.waitForFunction(() =>
    (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes("símbolo a mostrar"),
  );
  await restoreDisplaySymbol();
  const shownName = await page.locator('#edit-quantity-form [name="name"]').inputValue();
  assert(shownName === "Nombre nuevo", `el formulario debía mostrar lo guardado, mostró: ${shownName}`);
  assert((await definition()).display_symbols.Zz === "Z_z", "el símbolo fallido no debía cambiar");
});
