import { TEACHER, apiPost, assert, openAddForm, step, withE2E } from "./lib.mjs";

const PORT = process.env.E2E_PORT ?? "8172";
const PRACTICE_ID = "p1-estadistica";
const SAVE_URL = `**/api/practices/${PRACTICE_ID}/quantities`;

await withE2E({ port: PORT, dbName: "quantify-simbolo-mostrar.db" }, async ({ login }) => {
  const { page } = await login(TEACHER, "docente");
  const definition = () =>
    page.evaluate(async (id) => (await fetch(`/api/practices/${id}/definition`)).json(), PRACTICE_ID);
  const statusIncludes = (text) =>
    page.waitForFunction(
      (expected) =>
        (document.querySelector("#practice-workspace .practice-status")?.textContent ?? "").includes(expected),
      text,
    );

  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${PRACTICE_ID}"]`);

  step("docente: si falla la conexión no se guarda nada y el error está en español");
  const fillNew = async () => {
    await openAddForm(page, "#new-quantity-form");
    await page.fill('#new-quantity-form [name="symbol"]', "Zz");
    await page.fill('#new-quantity-form [name="display_symbol"]', "Z_z");
    await page.fill('#new-quantity-form [name="name"]', "Zeta");
  };
  await fillNew();
  await page.route(SAVE_URL, (route) => route.abort());
  await page.click('#new-quantity-form button[type="submit"]');
  await statusIncludes("No se pudo conectar con el servidor");
  await page.unroute(SAVE_URL);
  assert(!(await definition()).quantities.some((q) => q.symbol === "Zz"), "no debía guardarse la magnitud");

  step("docente: el alta guarda el elemento y su símbolo a mostrar juntos");
  await fillNew();
  await page.click('#new-quantity-form button[type="submit"]');
  await statusIncludes("Magnitud agregada");
  let saved = await definition();
  const zz = saved.quantities.find((q) => q.symbol === "Zz");
  assert(zz, "la magnitud debía quedar guardada");
  assert(saved.display_symbols.Zz === "Z_z", "el alta debía guardar el símbolo a mostrar");

  step("docente: la edición renombra y cambia el símbolo a mostrar a la vez");
  await page.click(`[data-edit-quantity][data-qid="${zz.id}"]`);
  await page.fill('#edit-quantity-form [name="symbol"]', "Zy");
  await page.fill('#edit-quantity-form [name="display_symbol"]', "Z_y");
  await page.click('#edit-quantity-form button[type="submit"]');
  await statusIncludes("Magnitud actualizada");
  saved = await definition();
  assert(saved.display_symbols.Zy === "Z_y", "la edición debía guardar el símbolo a mostrar");

  step("docente: el cronómetro solo se puede elegir si la magnitud es de tiempo");
  const zy = saved.quantities.find((q) => q.symbol === "Zy");
  await page.click(`[data-edit-quantity][data-qid="${zy.id}"]`);
  const stopwatch = page.locator('#edit-quantity-form [name="input_widget"] option[value="cronometro"]');
  assert(await stopwatch.evaluate((o) => o.disabled), "el cronómetro debía estar deshabilitado sin magnitud tiempo");

  step("docente: el rótulo y la entrada se guardan y se precargan al editar");
  await page.fill('#edit-quantity-form [name="quantity"]', "tiempo");
  await page.selectOption('#edit-quantity-form [name="label_mode"]', "nombre_simbolo");
  await page.selectOption('#edit-quantity-form [name="input_widget"]', "cronometro");
  await page.click('#edit-quantity-form button[type="submit"]');
  await page.waitForSelector("#edit-quantity-form", { state: "detached" });
  const edited = (await definition()).quantities.find((q) => q.symbol === "Zy");
  assert(edited.label_mode === "nombre_simbolo", `rótulo guardado: ${edited.label_mode}`);
  assert(edited.input_widget === "cronometro", `entrada guardada: ${edited.input_widget}`);
  await page.click(`[data-edit-quantity][data-qid="${zy.id}"]`);
  const shownLabel = await page.locator('#edit-quantity-form [name="label_mode"]').inputValue();
  const shownWidget = await page.locator('#edit-quantity-form [name="input_widget"]').inputValue();
  assert(shownLabel === "nombre_simbolo" && shownWidget === "cronometro", "el formulario debía precargar lo guardado");
  await page.click("#edit-quantity-form [data-cancel-quantity]");

  step("docente: si la magnitud deja de ser tiempo, el cronómetro que tenía pasa a teclado");
  await page.click(`[data-edit-quantity][data-qid="${zy.id}"]`);
  await page.fill('#edit-quantity-form [name="quantity"]', "longitud");
  await page.click('#edit-quantity-form button[type="submit"]');
  await page.waitForSelector("#edit-quantity-form", { state: "detached" });
  const untimed = (await definition()).quantities.find((q) => q.symbol === "Zy");
  assert(untimed.input_widget === "teclado", `entrada tras dejar de ser tiempo: ${untimed.input_widget}`);

  step("docente: un símbolo llamado como un miembro de Object no precarga basura");
  const named = await apiPost(page, `/api/practices/${PRACTICE_ID}/quantities`, {
    symbol: "constructor",
    name: "Constructor",
    unit: "",
    repeated: true,
    per_point: false,
    has_uncertainty: true,
  });
  await page.reload();
  await page.waitForSelector("#app-shell:not(.hidden)");
  await page.click('.tab.teacher-only[data-view="practices"]');
  await page.click(`[data-practice-open][data-practice-id="${PRACTICE_ID}"]`);
  await page.click(`[data-edit-quantity][data-qid="${named.id}"]`);
  const prefilled = await page.locator('#edit-quantity-form [name="display_symbol"]').inputValue();
  assert(prefilled === "", `el campo debía estar vacío, tenía: ${prefilled}`);

  step("docente: un símbolo a mostrar demasiado largo rechaza el elemento entero");
  const url = `/api/practices/${PRACTICE_ID}/quantities`;
  const body = { symbol: "Zl", name: "Larga", unit: "", repeated: true, per_point: false, has_uncertainty: true };
  const error = await apiPost(page, url, { ...body, display_symbol: "x".repeat(41) }).then(
    () => null,
    (e) => e.message,
  );
  assert(error?.includes("hasta 40 caracteres"), `debía rechazarse con un mensaje claro: ${error}`);
  assert(!(await definition()).quantities.some((q) => q.symbol === "Zl"), "no debía guardarse la magnitud");
});
