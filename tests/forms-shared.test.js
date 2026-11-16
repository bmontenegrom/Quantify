import { test } from "node:test";
import assert from "node:assert/strict";
import { columnSections } from "../static/forms-shared.js";

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
