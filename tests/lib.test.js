import { test } from "node:test";
import assert from "node:assert/strict";

import {
  escapeHtml,
  cssEscape,
  format,
  formatDate,
  groupBy,
  renderGroupType,
  scalePayload,
} from "../static/lib.js";

test("escapeHtml escapa todos los caracteres especiales", () => {
  assert.equal(escapeHtml("&"), "&amp;");
  assert.equal(escapeHtml("<"), "&lt;");
  assert.equal(escapeHtml(">"), "&gt;");
  assert.equal(escapeHtml('"'), "&quot;");
  assert.equal(escapeHtml("'"), "&#039;");
  assert.equal(
    escapeHtml(`<img src=x onerror="alert('x')">`),
    "&lt;img src=x onerror=&quot;alert(&#039;x&#039;)&quot;&gt;",
  );
  assert.equal(escapeHtml("sin especiales"), "sin especiales");
});

test("cssEscape cae al escape de comillas sin window (Node)", () => {
  // En Node no hay window.CSS, así que usa el fallback.
  assert.equal(cssEscape('a"b'), 'a\\"b');
  assert.equal(cssEscape("simple"), "simple");
});

test("format usa locale es-UY con 5 cifras significativas", () => {
  assert.equal(format(1234.5678), "1.234,6");
  assert.equal(format(0), "0");
  assert.equal(format(0.000123456), "0,00012346");
});

test("formatDate formatea fecha y hora cortas en es-UY", () => {
  // Fecha fija con offset explícito para que no dependa de la zona del runner.
  const result = formatDate("2026-05-29T15:30:00Z");
  // es-UY usa dd/mm/aa; afirmamos el patrón en vez del valor exacto (depende de TZ).
  assert.match(result, /\d{1,2}\/\d{1,2}\/\d{2}/);
  assert.match(result, /\d{1,2}:\d{2}/);
});

test("groupBy agrupa por clave y manda las nulas a '-'", () => {
  const items = [
    { course: "Fis", name: "a" },
    { course: "Fis", name: "b" },
    { course: "Qui", name: "c" },
    { course: null, name: "d" },
  ];
  const grouped = groupBy(items, (item) => item.course);
  assert.deepEqual(Object.keys(grouped).sort(), ["-", "Fis", "Qui"]);
  assert.equal(grouped.Fis.length, 2);
  assert.equal(grouped["-"][0].name, "d");
  assert.deepEqual(groupBy([], (x) => x), {});
});

test("renderGroupType traduce el tipo de grupo", () => {
  assert.equal(renderGroupType("recuperacion"), "Recuperacion");
  assert.equal(renderGroupType("regular"), "Regular");
  assert.equal(renderGroupType(undefined), "Regular");
});

test("scalePayload convierte vacíos a null y step a número", () => {
  const raw = {
    label: "200 uA",
    unit: "A",
    b_model: "fabricante",
    step: "0.0000001",
    full_scale: "0.0002",
    appreciation: "",
    spec_pct_reading: "1",
    spec_step_coeff: "5",
    spec_fixed: "",
    internal_res: "1002",
    internal_res_u: "",
  };
  const payload = scalePayload(raw);
  assert.equal(payload.label, "200 uA");
  assert.equal(payload.b_model, "fabricante");
  assert.equal(payload.step, 1e-7);
  assert.equal(payload.full_scale, 0.0002);
  assert.equal(payload.appreciation, null);
  assert.equal(payload.spec_pct_reading, 1);
  assert.equal(payload.spec_step_coeff, 5);
  assert.equal(payload.spec_fixed, null);
  assert.equal(payload.internal_res, 1002);
  assert.equal(payload.internal_res_u, null);
});
