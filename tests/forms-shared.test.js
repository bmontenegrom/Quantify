import { test } from "node:test";
import assert from "node:assert/strict";
import {
  columnSections,
  instrumentSelectorsHtml,
  retiredInstrumentNote,
  inputUnits,
  unitSelectHtml,
  unitConversion,
  toDeclaredUnit,
  fromDeclaredUnit,
  restoreUnitChoices,
} from "../static/forms-shared.js";
import { state } from "../static/state.js";

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
  assert.match(retiredInstrumentNote(instruments, { instrument_id: "regla", scale_id: "cm" }), /única que queda/);
  const twoScales = [{ id: "regla", scales: [{ id: "mm" }, { id: "dm" }] }];
  assert.match(retiredInstrumentNote(twoScales, { instrument_id: "regla", scale_id: "cm" }), /Elegí otra/);
  const noScales = [{ id: "regla", scales: [] }];
  assert.match(retiredInstrumentNote(noScales, { instrument_id: "regla", scale_id: "cm" }), /ya no tiene escalas/);
});

test("unidades de entrada: las del servidor, o la declarada sola", () => {
  state.practiceForm = {
    definition: { resolved: { input_units: { L: [{ label: "cm", factor: 1, offset: 0 }, { label: "mm", factor: 0.1, offset: 0 }] } } },
  };
  assert.deepEqual(inputUnits("L", "cm").map((u) => u.label), ["cm", "mm"]);
  assert.deepEqual(inputUnits("x", "s"), [{ label: "s", factor: 1, offset: 0 }]);
  state.practiceForm = null;
});

test("unitSelectHtml: desplegable con 2 o más unidades, texto con una", () => {
  const select = unitSelectHtml([
    { label: "cm", factor: 1, offset: 0 },
    { label: "mm", factor: 0.1, offset: 0 },
  ]);
  assert.match(select, /<select class="unit-select"/);
  assert.match(select, /<option value="0" data-factor="1" data-offset="0" selected>cm<\/option>/);
  assert.match(select, /data-factor="0.1"/);
  assert.match(unitSelectHtml([{ label: "s", factor: 1, offset: 0 }]), /<span class="replica-unit">/);
  assert.equal(unitSelectHtml([{ label: "s", factor: 1, offset: 0 }], { text: false }), "");
});

test("unitConversion: factor y desplazamiento de la unidad elegida, o ninguno", () => {
  const wrap = (dataset) => ({ querySelector: () => (dataset ? { selectedOptions: [{ dataset }] } : null) });
  assert.deepEqual(unitConversion(wrap({ factor: "0.1", offset: "0" })), { factor: 0.1, offset: 0 });
  assert.deepEqual(unitConversion(wrap({ factor: "1", offset: "273.15" })), { factor: 1, offset: 273.15 });
  assert.deepEqual(unitConversion(wrap(null)), { factor: 1, offset: 0 });
  assert.deepEqual(unitConversion(null), { factor: 1, offset: 0 });
});

test("toDeclaredUnit: convierte sin ruido de coma flotante y la U ignora el desplazamiento", () => {
  const wrap = (factor, offset) => ({ querySelector: () => ({ selectedOptions: [{ dataset: { factor, offset } }] }) });
  assert.equal(toDeclaredUnit("3", wrap("0.1", "0")), 0.3);
  assert.equal(toDeclaredUnit("20", wrap("1", "273.15")), 293.15);
  assert.equal(toDeclaredUnit("0.5", wrap("1", "273.15"), { uncertainty: true }), 0.5);
  assert.ok(Number.isNaN(toDeclaredUnit("abc", wrap("1", "0"))));
});

test("fromDeclaredUnit deshace toDeclaredUnit, con la U sin desplazamiento", () => {
  assert.equal(fromDeclaredUnit(1, { factor: 0.01, offset: 0 }), 100);
  assert.equal(fromDeclaredUnit(293.15, { factor: 1, offset: 273.15 }), 20);
  assert.equal(fromDeclaredUnit(0.5, { factor: 1, offset: 273.15 }, { uncertainty: true }), 0.5);
});

test("restoreUnitChoices: una U de temperatura en °C vuelve sin el desplazamiento", () => {
  const holder = { dataset: { quantityId: "T" } };
  const field = (cls, value) => {
    const input = { value, classList: [cls] };
    const select = {
      selectedIndex: 0,
      options: [{ dataset: { factor: "1", offset: "0" } }, { dataset: { factor: "1", offset: "273.15" } }],
      get selectedOptions() { return [this.options[this.selectedIndex]]; },
      closest: (sel) => (sel.includes(".replica-input-wrap") ? wrap : holder),
      dispatchEvent() {},
    };
    const wrap = {
      querySelector: (sel) => (sel === ".unit-select" ? select : sel === '[class$="-u"]' && cls.endsWith("-u") ? input : null),
      querySelectorAll: () => [input],
    };
    return { input, select };
  };
  const value = field("measure-given-value", "293.15");
  const u = field("measure-given-u", "0.5");
  const container = { querySelectorAll: () => [value.select, u.select] };
  restoreUnitChoices(container, { "T|v|0": 1, "T|u|0": 1 });
  assert.equal(value.input.value, 20);
  assert.equal(u.input.value, 0.5);
  assert.equal(u.select.selectedIndex, 1);
});
