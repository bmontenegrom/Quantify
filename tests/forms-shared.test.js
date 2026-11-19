import { test } from "node:test";
import assert from "node:assert/strict";
import { columnSections, instrumentSelectorsHtml, retiredInstrumentNote } from "../static/forms-shared.js";

test("columnSections: solo las secciones con columnas de la serie y sin magnitudes compartidas", () => {
  const definition = {
    quantities: [
      { symbol: "x", per_point: true, is_given: false },
      { symbol: "L", per_point: false, is_given: false },
      { symbol: "g0", per_point: true, is_given: true },
    ],
  };
  const sections = [
    { title: "Barrido", symbols: ["x"] },
    { title: "Mixta", symbols: ["x", "L"] },
    { title: "Dato", symbols: ["g0"] },
    { title: "Final", symbols: [] },
    { id: "p", series: true },
  ];
  assert.deepEqual(
    columnSections(definition, sections).map((s) => s.title),
    ["Barrido"],
  );
});

test("instrumentSelectorsHtml: filtra por magnitud y solo preselecciona el cronómetro si se pide", () => {
  const instruments = [
    { id: "regla", name: "Regla", quantity: "longitud" },
    { id: "crono", name: "Cronómetro", quantity: "tiempo" },
    { id: "reloj", name: "Reloj", quantity: "tiempo" },
  ];
  const q = { quantity: "tiempo" };
  const plain = instrumentSelectorsHtml(instruments, q);
  assert.ok(!plain.includes("regla"));
  assert.ok(plain.includes('value="reloj"'));
  assert.ok(!plain.includes("selected"));
  assert.ok(instrumentSelectorsHtml(instruments, q, true).includes('value="crono" selected'));
});

test("retiredInstrumentNote: avisa si el instrumento o la escala guardados ya no están", () => {
  const instruments = [{ id: "regla", scales: [{ id: "mm" }] }];
  assert.equal(retiredInstrumentNote(instruments, { instrument_id: null }), null);
  assert.equal(retiredInstrumentNote(instruments, { instrument_id: "regla", scale_id: "mm" }), null);
  assert.match(retiredInstrumentNote(instruments, { instrument_id: "calibre" }), /instrumento/);
  assert.match(retiredInstrumentNote(instruments, { instrument_id: "regla", scale_id: "cm" }), /escala/);
});
