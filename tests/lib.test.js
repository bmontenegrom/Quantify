import { test } from "node:test";
import assert from "node:assert/strict";

import {
  analysisSections,
  escapeHtml,
  symbolHtml,
  setDisplaySymbols,
  inlineMathHtml,
  unitHtml,
  cssEscape,
  format,
  formatDate,
  groupBy,
  renderGroupType,
  scalePayload,
  canReview,
  studentCourses,
  studentGroups,
  studentGradebooks,
  studentTotals,
  availableCoursesForStudent,
  availableGroupsForStudent,
  allStudents,
  allGroups,
  analysisKindLabel,
  compatibleInstruments,
  measureText,
  regressionPlot,
  scatterPlot,
  compareResults,
  compareMeasuredVsTheoretical,
  comparisonPairs,
  comparableQuantities,
  pointPower,
  flowRate,
  draftMeasurementsByQuantity,
  draftedQuantityIds,
  SI_PREFIXES,
  prefixFactor,
  seriesStats,
  histogram,
  normalCurve,
  validateMeasurements,
} from "../static/lib.js";

function academicFixture() {
  return {
    courses: [
      {
        id: "c1",
        name: "Fisica",
        term: "2026",
        groups: [
          { id: "g1", name: "Grupo 1", members: [{ id: "s1" }] },
          { id: "g2", name: "Grupo 2", members: [{ id: "s2" }] },
        ],
        members: [{ id: "s1" }, { id: "s2" }],
      },
      {
        id: "c2",
        name: "Quimica",
        term: "2026",
        groups: [{ id: "g3", name: "Grupo 3", members: [] }],
        members: [{ id: "s2" }],
      },
    ],
    users: [
      { id: "s1", role: "estudiante" },
      { id: "d1", role: "docente" },
    ],
  };
}

function gradebooksFixture() {
  return [
    {
      course: { id: "c1" },
      students: [
        { student: { id: "s1" }, total_points: 8, total_possible: 10 },
        { student: { id: "s2" }, total_points: 5, total_possible: 10 },
      ],
    },
    {
      course: { id: "c2" },
      students: [{ student: { id: "s1" }, total_points: 3, total_possible: 5 }],
    },
  ];
}

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

test("symbolHtml escapa HTML y convierte dígitos pegados a letras en subíndices", () => {
  assert.equal(symbolHtml("R1"), "R<sub>1</sub>");
  assert.equal(symbolHtml("C12"), "C<sub>12</sub>");
  assert.equal(symbolHtml("T_oc"), "T<sub>OC</sub>");
  assert.equal(symbolHtml("R_cap"), "R<sub>cap</sub>");
  assert.equal(symbolHtml("E_medio"), "E<sub>medio</sub>");
  assert.equal(symbolHtml("<R1>"), "&lt;R<sub>1</sub>&gt;");
});

test("symbolHtml muestra los nombres de letras griegas como la letra", () => {
  assert.equal(symbolHtml("rho_medio"), "ρ<sub>medio</sub>");
  assert.equal(symbolHtml("rho_f"), "ρ<sub>F</sub>");
  assert.equal(symbolHtml("gamma1"), "γ<sub>1</sub>");
  assert.equal(symbolHtml("mu_agua"), "μ<sub>agua</sub>");
  assert.equal(symbolHtml("tau_exp"), "τ<sub>exp</sub>");
  assert.equal(symbolHtml("mult"), "mult");
  assert.equal(symbolHtml("taux"), "taux");
  assert.equal(symbolHtml("rhoA_b"), "rhoA<sub>B</sub>");
});

test("symbolHtml no anida subíndices y inlineMathHtml usa la misma regla", () => {
  assert.equal(symbolHtml("R_load2"), "R<sub>load2</sub>");
  assert.equal(symbolHtml("crud_test_qty"), "crud<sub>test,qty</sub>");
  assert.equal(symbolHtml("f_res_exp"), "f<sub>res,exp</sub>");
  assert.equal(symbolHtml("m1_2"), "m<sub>1,2</sub>");
  assert.equal(symbolHtml("R1_load"), "R<sub>1,load</sub>");
  assert.equal(symbolHtml("v_máx"), "v<sub>máx</sub>");
  assert.equal(inlineMathHtml("T_ñu"), "T<sub>ÑU</sub>");
  assert.equal(inlineMathHtml("ver tabla_índice"), "ver tabla_índice");
  assert.equal(inlineMathHtml("ver tabla_resumen"), "ver tabla_resumen");
  assert.equal(inlineMathHtml("Tiempo T_oc"), "Tiempo T<sub>OC</sub>");
});

test("symbolHtml usa el símbolo a mostrar de la definición cargada", () => {
  setDisplaySymbols({
    tmedio: "t_1/2",
    Vg_s: "V_G",
    VR1_s_t: "VR1",
    I_p: "I",
    VRpp: "V_R_PP",
    VR_teo: "V_R_teo",
    phiR_teo: "phi_R_teo",
    trampa: "<b>",
  });
  try {
    assert.equal(symbolHtml("tmedio"), "t<sub>1/2</sub>");
    assert.equal(symbolHtml("Vg_s"), "V<sub>G</sub>");
    assert.equal(symbolHtml("VR1_s_t"), "VR<sub>1</sub>");
    assert.equal(symbolHtml("I_p"), "I");
    assert.equal(symbolHtml("VRpp"), "V<sub>R,PP</sub>");
    assert.equal(symbolHtml("VR_teo"), "V<sub>R,teo</sub>");
    assert.equal(symbolHtml("phiR_teo"), "φ<sub>R,teo</sub>");
    assert.equal(symbolHtml("trampa"), "&lt;b&gt;");
    assert.equal(symbolHtml("f_res_exp"), "f<sub>res,exp</sub>");
  } finally {
    setDisplaySymbols(null);
  }
  assert.equal(symbolHtml("tmedio"), "tmedio");
  assert.equal(symbolHtml("Vg_s"), "Vg<sub>S</sub>");
});

test("inlineMathHtml y unitHtml formatean subíndices y superíndices visibles", () => {
  assert.equal(inlineMathHtml("Resistencia R3"), "Resistencia R<sub>3</sub>");
  assert.equal(inlineMathHtml("Tiempo t1/2 y T_oc"), "Tiempo t<sub>1/2</sub> y T<sub>OC</sub>");
  assert.equal(unitHtml("m3"), "m<sup>3</sup>");
  assert.equal(unitHtml("kg/m3"), "kg/m<sup>3</sup>");
  assert.equal(unitHtml("m/s2"), "m/s<sup>2</sup>");
  assert.equal(unitHtml("R^2"), "R<sup>2</sup>");
});

test("cssEscape cae al escape de comillas sin window (Node)", () => {
  assert.equal(cssEscape('a"b'), 'a\\"b');
  assert.equal(cssEscape("simple"), "simple");
});

test("format usa locale es-UY con 5 cifras significativas", () => {
  assert.equal(format(1234.5678), "1.234,6");
  assert.equal(format(0), "0");
  assert.equal(format(0.000123456), "0,00012346");
});

test("formatDate formatea fecha y hora cortas en es-UY", () => {
  const result = formatDate("2026-05-29T15:30:00Z");
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
    u_cal_pct: "3",
    u_cal_fixed: "",
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
  assert.equal(payload.u_cal_pct, 3);
  assert.equal(payload.u_cal_fixed, null);
});

test("canReview es true solo para docente/admin", () => {
  assert.equal(canReview({ role: "docente" }), true);
  assert.equal(canReview({ role: "admin" }), true);
  assert.equal(canReview({ role: "estudiante" }), false);
  assert.equal(canReview(null), false);
  assert.equal(canReview(undefined), false);
});

test("studentCourses devuelve los cursos donde el estudiante es miembro", () => {
  const academic = academicFixture();
  assert.deepEqual(
    studentCourses(academic, "s1").map((c) => c.id),
    ["c1"],
  );
  assert.deepEqual(
    studentCourses(academic, "s2").map((c) => c.id).sort(),
    ["c1", "c2"],
  );
  assert.deepEqual(studentCourses(academic, "nadie"), []);
});

test("studentGroups devuelve los grupos del estudiante con datos del curso", () => {
  const academic = academicFixture();
  const groups = studentGroups(academic, "s1");
  assert.equal(groups.length, 1);
  assert.equal(groups[0].id, "g1");
  assert.equal(groups[0].courseName, "Fisica");
  assert.equal(groups[0].courseTerm, "2026");
});

test("availableCoursesForStudent excluye los cursos actuales", () => {
  const academic = academicFixture();
  assert.deepEqual(
    availableCoursesForStudent(academic, "s1").map((c) => c.id),
    ["c2"],
  );
  assert.deepEqual(availableCoursesForStudent(academic, "s2"), []);
});

test("availableGroupsForStudent ofrece grupos no asignados dentro de sus cursos", () => {
  const academic = academicFixture();
  assert.deepEqual(
    availableGroupsForStudent(academic, "s1").map((g) => g.id),
    ["g2"],
  );
});

test("studentGradebooks proyecta solo las libretas con resumen del estudiante", () => {
  const gradebooks = gradebooksFixture();
  const books = studentGradebooks(gradebooks, "s1");
  assert.deepEqual(books.map((b) => b.course.id), ["c1", "c2"]);
  assert.deepEqual(studentGradebooks(gradebooks, "s2").map((b) => b.course.id), ["c1"]);
});

test("studentTotals suma sobre todas las libretas, o null sin notas", () => {
  const gradebooks = gradebooksFixture();
  assert.deepEqual(studentTotals(gradebooks, "s1"), { points: 11, possible: 15 });
  assert.deepEqual(studentTotals(gradebooks, "s2"), { points: 5, possible: 10 });
  assert.equal(studentTotals(gradebooks, "sin-notas"), null);
});

test("allStudents usa academic.students si viene, si no filtra users por rol", () => {
  assert.deepEqual(allStudents(academicFixture()).map((u) => u.id), ["s1"]);
  const withStudents = { students: [{ id: "x" }, { id: "y" }], users: [] };
  assert.deepEqual(allStudents(withStudents).map((u) => u.id), ["x", "y"]);
});

test("allGroups aplana todos los grupos anotando el curso", () => {
  const groups = allGroups(academicFixture());
  assert.deepEqual(groups.map((g) => g.id).sort(), ["g1", "g2", "g3"]);
  const g3 = groups.find((g) => g.id === "g3");
  assert.equal(g3.courseId, "c2");
  assert.equal(g3.courseName, "Quimica");
});

test("analysisKindLabel devuelve etiqueta legible o 'Sin definir'", () => {
  assert.equal(analysisKindLabel("estadistico"), "Estadístico");
  assert.equal(analysisKindLabel("regresion_lineal"), "Regresión lineal");
  assert.equal(analysisKindLabel("curva"), "Curva (sin ajuste)");
  assert.equal(analysisKindLabel("relajacion_exponencial"), "Sin definir");
  assert.equal(analysisKindLabel(null), "Sin definir");
  assert.equal(analysisKindLabel(undefined), "Sin definir");
  assert.equal(analysisKindLabel("desconocido"), "Sin definir");
});

test("compatibleInstruments filtra estrictamente por magnitud (sin fallback)", () => {
  const instruments = [
    { id: "i1", quantity: "longitud" },
    { id: "i2", quantity: "longitud" },
    { id: "i3", quantity: "masa" },
  ];
  assert.deepEqual(
    compatibleInstruments(instruments, "longitud").map((i) => i.id),
    ["i1", "i2"],
  );
  assert.equal(compatibleInstruments(instruments, "tiempo").length, 0);
  assert.equal(compatibleInstruments(instruments, null).length, 3);
  assert.deepEqual(compatibleInstruments(undefined, "longitud"), []);
});

test("measureText formatea 'valor ± U' y omite U inválida", () => {
  assert.equal(measureText(50, 5), "50 ± 5");
  assert.equal(measureText(1234.5, 0), "1.234,5");
  assert.equal(measureText(10, null), "10");
  assert.equal(measureText(10, NaN), "10");
  assert.equal(measureText(10, -1), "10");
});

test("regressionPlot escala puntos y recta al lienzo (recta conocida)", () => {
  const plot = regressionPlot([[0, 1], [1, 3], [2, 5]], 2, 1, 320, 220, 32);
  assert.deepEqual(plot.bounds, { minX: 0, maxX: 2, minY: 1, maxY: 5 });
  assert.deepEqual(plot.scatter, [
    { cx: 32, cy: 188 },
    { cx: 160, cy: 110 },
    { cx: 288, cy: 32 },
  ]);
  assert.deepEqual(plot.line, { x1: 32, y1: 188, x2: 288, y2: 32 });
});

test("regressionPlot extiende el rango Y para incluir la recta", () => {
  const plot = regressionPlot([[0, 0], [1, 0]], 5, 0, 320, 220, 32);
  assert.equal(plot.bounds.minY, 0);
  assert.equal(plot.bounds.maxY, 5);
  assert.deepEqual(plot.line, { x1: 32, y1: 188, x2: 288, y2: 32 });
});

test("regressionPlot devuelve null con <2 puntos o rango nulo", () => {
  assert.equal(regressionPlot([], 1, 0), null);
  assert.equal(regressionPlot([[1, 2]], 1, 0), null);
  assert.equal(regressionPlot([[1, 2], [1, 4]], 0, 1), null);
  assert.equal(regressionPlot([[0, 3], [2, 3]], 0, 3), null);
});

test("scatterPlot proyecta puntos sin recta (no expone línea)", () => {
  const plot = scatterPlot([[0, 1], [1, 3], [2, 5]]);
  assert.equal(plot.scatter.length, 3);
  assert.equal(plot.line, undefined);
  assert.equal(plot.xLog, false);
  assert.ok(plot.scatter[0].cx < plot.scatter[2].cx);
  assert.ok(plot.scatter[0].cy > plot.scatter[2].cy);
});

test("scatterPlot con xLog escala el eje x logarítmicamente", () => {
  const plot = scatterPlot([[1, 5], [10, 6], [100, 7]], { xLog: true });
  assert.equal(plot.xLog, true);
  const d1 = plot.scatter[1].cx - plot.scatter[0].cx;
  const d2 = plot.scatter[2].cx - plot.scatter[1].cx;
  assert.ok(Math.abs(d1 - d2) < 1e-9);
});

test("scatterPlot devuelve null con <2 puntos, rango nulo o x≤0 en log", () => {
  assert.equal(scatterPlot([]), null);
  assert.equal(scatterPlot([[1, 2]]), null);
  assert.equal(scatterPlot([[1, 2], [1, 4]]), null);
  assert.equal(scatterPlot([[0, 2], [10, 4]], { xLog: true }), null);
});

test("compareResults empareja por símbolo y calcula Δ abs y %", () => {
  const auto = [{ symbol: "Q", name: "Area", unit: "mm2", value: 70, u_expanded: 2 }];
  const student = [{ symbol: "Q", value: 73.5, u_expanded: 2.2 }];
  const [row] = compareResults(auto, student);
  assert.deepEqual(row.auto, { value: 70, u: 2 });
  assert.deepEqual(row.student, { value: 73.5, u: 2.2 });
  assert.ok(Math.abs(row.dValue - 3.5) < 1e-9);
  assert.ok(Math.abs(row.dValuePct - 5) < 1e-9);
  assert.ok(Math.abs(row.dU - 0.2) < 1e-9);
  assert.ok(Math.abs(row.dUPct - 10) < 1e-9);
});

test("compareResults marca null cuando el alumno no cargó el mensurando", () => {
  const auto = [{ symbol: "Q", name: "Area", unit: "mm2", value: 70, u_expanded: 2 }];
  const [row] = compareResults(auto, []);
  assert.equal(row.student, null);
  assert.equal(row.dValue, null);
  assert.equal(row.dValuePct, null);
  assert.equal(row.dU, null);
  assert.equal(row.dUPct, null);
});

test("compareResults: % null si el automático es 0; Δ U null si falta una U", () => {
  const auto = [
    { symbol: "Z", name: "Cero", unit: "u", value: 0, u_expanded: 0 },
    { symbol: "W", name: "SinU", unit: "u", value: 10, u_expanded: 1 },
  ];
  const student = [
    { symbol: "Z", value: 5, u_expanded: 1 },
    { symbol: "W", value: 12, u_expanded: null },
  ];
  const rows = compareResults(auto, student);
  assert.ok(Math.abs(rows[0].dValue - 5) < 1e-9);
  assert.equal(rows[0].dValuePct, null);
  assert.equal(rows[0].dUPct, null);
  assert.ok(Math.abs(rows[1].dValue - 2) < 1e-9);
  assert.equal(rows[1].dU, null);
  assert.equal(rows[1].dUPct, null);
});

test("compareResults: verdict pass/fail según tolerancia", () => {
  const auto = [{ symbol: "Q", name: "Area", unit: "mm2", value: 100, u_expanded: 2 }];
  const student = [{ symbol: "Q", value: 104, u_expanded: 2 }];
  const [noTol] = compareResults(auto, student);
  assert.equal(noTol.verdict, null);
  const [pass] = compareResults(auto, student, { Q: 5 });
  assert.equal(pass.verdict, "pass");
  const [fail] = compareResults(auto, student, { Q: 3 });
  assert.equal(fail.verdict, "fail");
});

test("pointPower calcula P = I²·R", () => {
  assert.equal(pointPower(100, 0.5), 25);
  assert.equal(pointPower(0, 3), 0);
  assert.equal(pointPower(200, 0), 0);
  assert.equal(pointPower(100, -0.5), 25);
});

test("flowRate calcula Q = V/t", () => {
  assert.equal(flowRate(1e-3, 10), 1e-4);
  assert.equal(flowRate(0, 10), 0);
  assert.equal(flowRate(1e-3, 0), Infinity);
});

test("draftMeasurementsByQuantity: columna de un valor por punto", () => {
  const map = draftMeasurementsByQuantity([
    { quantity_id: "q1", instrument_id: null, scale_id: null, values: [1, 2, 3], given_u: null },
  ]);
  assert.deepEqual(map.get("q1"), {
    pointGroups: [[1], [2], [3]],
    operatorGroups: [],
    values: [1, 2, 3],
    value_u: null,
    instrument_id: null,
    scale_id: null,
  });
});

test("draftMeasurementsByQuantity: columna con réplicas por punto", () => {
  const map = draftMeasurementsByQuantity([
    {
      quantity_id: "q1",
      instrument_id: null,
      scale_id: null,
      values: [],
      given_u: null,
      point_replicas: [[1, 1.1], [2, 2.1, 2.2]],
    },
  ]);
  assert.deepEqual(map.get("q1").pointGroups, [[1, 1.1], [2, 2.1, 2.2]]);
});

test("draftMeasurementsByQuantity: magnitud por operador (Motor D)", () => {
  const map = draftMeasurementsByQuantity([
    {
      quantity_id: "q1",
      instrument_id: null,
      scale_id: null,
      values: [],
      given_u: null,
      operator_replicas: [[1, 1.1], [2, 2.1, 2.2]],
    },
  ]);
  assert.deepEqual(map.get("q1").operatorGroups, [[1, 1.1], [2, 2.1, 2.2]]);
});

test("draftMeasurementsByQuantity: fila suelta dada (valor ± U)", () => {
  const map = draftMeasurementsByQuantity([
    { quantity_id: "q1", instrument_id: null, scale_id: null, values: [0.5], given_u: 0.01 },
  ]);
  const e = map.get("q1");
  assert.equal(e.values[0], 0.5);
  assert.equal(e.value_u, 0.01);
});

test("draftMeasurementsByQuantity: fila suelta medida con réplicas e instrumento/escala", () => {
  const map = draftMeasurementsByQuantity([
    { quantity_id: "q1", instrument_id: "inst-1", scale_id: "sc-1", values: [10.1, 10.2, 9.9], given_u: null },
  ]);
  const e = map.get("q1");
  assert.deepEqual(e.values, [10.1, 10.2, 9.9]);
  assert.equal(e.instrument_id, "inst-1");
  assert.equal(e.scale_id, "sc-1");
  assert.equal(e.value_u, null);
});

test("draftedQuantityIds: solo las magnitudes con lecturas", () => {
  const ids = draftedQuantityIds([
    { quantity_id: "con", values: [1] },
    { quantity_id: "vacia", values: [] },
    { quantity_id: "sin" },
    { quantity_id: "operadores", values: [], operator_replicas: [[1.2], []] },
    { quantity_id: "operadores_vacios", values: [], operator_replicas: [[], []] },
  ]);
  assert.deepEqual([...ids], ["con", "operadores"]);
  assert.equal(draftedQuantityIds(undefined).size, 0);
});

test("draftMeasurementsByQuantity: lista vacía o ausente da un Map vacío", () => {
  assert.equal(draftMeasurementsByQuantity([]).size, 0);
  assert.equal(draftMeasurementsByQuantity(undefined).size, 0);
});

test("compareMeasuredVsTheoretical empareja cada resultado con su compare_with y calcula deltas", () => {
  const quantities = [
    { symbol: "VR1_s", name: "Voltaje en R1", unit: "V", result: { mean: 1.5, u_expanded: 0.1 } },
    { symbol: "Vg_s", name: "Fuente", unit: "V", result: { mean: 8, u_expanded: 0.2 } },
  ];
  const derived = [
    { symbol: "VR1_s_t", name: "Teorica R1", unit: "V", value: 1.6, u_expanded: 0.08 },
    { symbol: "I_s", name: "Corriente", unit: "A", value: 0.015, u_expanded: 0.001 },
  ];
  const rows = compareMeasuredVsTheoretical(quantities, derived, new Map([["VR1_s_t", "VR1_s"]]));
  assert.equal(rows.length, 1);
  const [row] = rows;
  assert.equal(row.symbol, "VR1_s");
  assert.equal(row.theoreticalSymbol, "VR1_s_t");
  assert.deepEqual(row.exp, { value: 1.5, u: 0.1 });
  assert.deepEqual(row.teo, { value: 1.6, u: 0.08 });
  assert.ok(Math.abs(row.dValue - -0.1) < 1e-9);
  assert.ok(Math.abs(row.dValuePct - -6.25) < 1e-9);
  assert.ok(Math.abs(row.dU - 0.02) < 1e-9);
  assert.ok(Math.abs(row.dUPct - 25) < 1e-9);
});

test("compareMeasuredVsTheoretical: deltas null con teórico 0 o U faltante", () => {
  const quantities = [
    { symbol: "X", name: "X", unit: "u", result: { mean: 2, u_expanded: null } },
  ];
  const derived = [{ symbol: "X_t", name: "X teorica", unit: "u", value: 0, u_expanded: 0.1 }];
  const [row] = compareMeasuredVsTheoretical(quantities, derived, new Map([["X_t", "X"]]));
  assert.ok(Math.abs(row.dValue - 2) < 1e-9);
  assert.equal(row.dValuePct, null);
  assert.equal(row.dU, null);
  assert.equal(row.dUPct, null);
});

test("compareMeasuredVsTheoretical ya no empareja por el sufijo _t sino por los pares", () => {
  const quantities = [
    { symbol: "X", name: "X", unit: "u", result: { mean: 2, u_expanded: 0.1 } },
    { symbol: "Y", name: "Y", unit: "u", result: { mean: 3, u_expanded: 0.1 } },
  ];
  const derived = [
    { symbol: "X_t", name: "X teórica", unit: "u", value: 2.1, u_expanded: 0.1 },
    { symbol: "Y_calc", name: "Y calculada", unit: "u", value: 3.2, u_expanded: 0.1 },
  ];
  assert.deepEqual(compareMeasuredVsTheoretical(quantities, derived).map((r) => r.symbol), []);
  const rows = compareMeasuredVsTheoretical(quantities, derived, new Map([["Y_calc", "Y"]]));
  assert.deepEqual(rows.map((r) => [r.symbol, r.theoreticalSymbol]), [["Y", "Y_calc"]]);
});

test("comparisonPairs arma los pares desde los resultados con compare_with", () => {
  const definition = {
    results: [
      { symbol: "VR1_s_t", compare_with: "VR1_s" },
      { symbol: "I_s", compare_with: "" },
      { symbol: "P_max_t" },
    ],
  };
  assert.deepEqual([...comparisonPairs(definition)], [["VR1_s_t", "VR1_s"]]);
  assert.equal(comparisonPairs(null).size, 0);
});

test("SI_PREFIXES es un array con entradas para 'm', 'k' y ''", () => {
  assert.ok(Array.isArray(SI_PREFIXES));
  assert.ok(SI_PREFIXES.some((p) => p.label === "m" && Math.abs(p.factor - 1e-3) < 1e-15));
  assert.ok(SI_PREFIXES.some((p) => p.label === "k" && Math.abs(p.factor - 1e3) < 1e-9));
  assert.ok(SI_PREFIXES.some((p) => p.label === "" && p.factor === 1));
});

test("prefixFactor devuelve el factor correcto o 1 para desconocidos", () => {
  assert.equal(prefixFactor("m"), 1e-3);
  assert.equal(prefixFactor("k"), 1e3);
  assert.equal(prefixFactor(""), 1);
  assert.equal(prefixFactor("?"), 1);
  assert.equal(prefixFactor(undefined), 1);
});

test("seriesStats: n=0 devuelve NaN; n=1 std=0; n>1 calcula bien", () => {
  const empty = seriesStats([]);
  assert.equal(empty.n, 0);
  assert.ok(Number.isNaN(empty.mean));
  assert.ok(Number.isNaN(empty.std));

  const one = seriesStats([7]);
  assert.equal(one.n, 1);
  assert.equal(one.mean, 7);
  assert.equal(one.std, 0);
  assert.equal(one.stdMean, 0);

  const two = seriesStats([2, 4]);
  assert.equal(two.n, 2);
  assert.equal(two.mean, 3);
  assert.ok(Math.abs(two.std - Math.SQRT2) < 1e-12);
  assert.ok(Math.abs(two.stdMean - Math.SQRT2 / Math.sqrt(2)) < 1e-12);
});

test("seriesStats ignora valores no finitos", () => {
  const s = seriesStats([1, NaN, Infinity, 3]);
  assert.equal(s.n, 2);
  assert.equal(s.mean, 2);
});

test("histogram: null con vacío o bins<1; un valor -> bins=1; distribución uniforme", () => {
  assert.equal(histogram([], 5), null);
  assert.equal(histogram([1, 2], 0), null);

  const one = histogram([5], 3);
  assert.equal(one.bins, 1);
  assert.equal(one.counts[0], 1);
  assert.equal(one.width, 0);

  const h = histogram([0, 1, 2, 3], 2);
  assert.equal(h.bins, 2);
  assert.deepEqual(h.counts, [2, 2]);
  assert.equal(h.edges.length, 3);
});

test("normalCurve: vacía con std<=0 o rango nulo; steps+1 puntos si válida", () => {
  assert.deepEqual(normalCurve(0, 0, -1, 1), []);
  assert.deepEqual(normalCurve(0, 1, 5, 5), []);
  assert.deepEqual(normalCurve(0, -1, -1, 1), []);

  const pts = normalCurve(0, 1, -3, 3, 10);
  assert.equal(pts.length, 11);
  const ys = pts.map((p) => p[1]);
  const maxY = Math.max(...ys);
  const midPt = pts[5];
  assert.ok(Math.abs(midPt[1] - maxY) < 1e-12);
});

test("validateMeasurements: regresion_lineal necesita ≥2 puntos completos", () => {
  const empty = validateMeasurements([], "regresion_lineal");
  assert.ok(typeof empty === "string");
  const onePoint = validateMeasurements([{ quantity_id: "x", values: [1], given_u: null }], "regresion_lineal");
  assert.ok(typeof onePoint === "string");
  const valid = validateMeasurements(
    [{ quantity_id: "x", values: [1, 2, 3], given_u: null }, { quantity_id: "y", values: [4, 5, 6], given_u: null }],
    "regresion_lineal",
  );
  assert.equal(valid, null);
});

test("validateMeasurements: curva necesita ≥2 puntos y avisa con texto de curva", () => {
  const onePoint = validateMeasurements([{ quantity_id: "x", values: [1], given_u: null }], "curva");
  assert.ok(typeof onePoint === "string");
  assert.ok(onePoint.includes("curva"));
  const valid = validateMeasurements(
    [{ quantity_id: "x", values: [1, 2], given_u: null }, { quantity_id: "y", values: [4, 5], given_u: null }],
    "curva",
  );
  assert.equal(valid, null);
});

test("validateMeasurements: cuenta puntos desde point_replicas (réplicas por punto)", () => {
  const ok = validateMeasurements(
    [
      { quantity_id: "x", values: [1, 2], given_u: null },
      { quantity_id: "y", values: [], given_u: null, point_replicas: [[4, 5], [6, 7]] },
    ],
    "regresion_lineal",
  );
  assert.equal(ok, null);
  const few = validateMeasurements(
    [{ quantity_id: "y", values: [], given_u: null, point_replicas: [[4, 5]] }],
    "regresion_lineal",
  );
  assert.ok(typeof few === "string");
});

test("validateMeasurements: regresión exige escalares compartidos completos (Motor E)", () => {
  const meta = {
    x: { name: "x", perPoint: true },
    R: { name: "Radio", perPoint: false, isGiven: false },
    g: { name: "g", perPoint: false, isGiven: true },
  };
  const measurements = [
    { quantity_id: "x", values: [1, 2], given_u: null },
    { quantity_id: "R", values: [], given_u: null },
    { quantity_id: "g", values: [9.8], given_u: 0.1 },
  ];
  const err = validateMeasurements(measurements, "regresion_lineal", meta);
  assert.ok(typeof err === "string" && err.includes("Radio"));
  measurements[1].values = [0.001];
  assert.equal(validateMeasurements(measurements, "regresion_lineal", meta), null);
  measurements[2].given_u = null;
  const errG = validateMeasurements(measurements, "regresion_lineal", meta);
  assert.ok(typeof errG === "string" && errG.includes("g"));
});

test("validateMeasurements: estadistico reporta mensurando sin lecturas", () => {
  const meta = { q1: { name: "Longitud", isGiven: false, isChrono: false } };
  const err = validateMeasurements([{ quantity_id: "q1", values: [], given_u: null }], "estadistico", meta);
  assert.ok(typeof err === "string");
  assert.ok(err.includes("Longitud"));
  const ok = validateMeasurements([{ quantity_id: "q1", values: [5], given_u: null }], "estadistico", meta);
  assert.equal(ok, null);
});

test("validateMeasurements: operadores exigen una serie por operador", () => {
  const meta = { T: { name: "Periodo", isGiven: false, isChrono: false } };
  const err = validateMeasurements(
    [{ quantity_id: "T", values: [], given_u: null, operator_replicas: [[1, 1.1], []] }],
    "estadistico",
    meta,
  );
  assert.ok(typeof err === "string");
  assert.ok(err.includes("operador 2"));
  const ok = validateMeasurements(
    [{ quantity_id: "T", values: [], given_u: null, operator_replicas: [[1, 1.1], [2, 2.1]] }],
    "estadistico",
    meta,
  );
  assert.equal(ok, null);
});

test("validateMeasurements: isGiven exige valor e incertidumbre", () => {
  const meta = { g1: { name: "Dato", isGiven: true, isChrono: false } };
  assert.ok(typeof validateMeasurements([{ quantity_id: "g1", values: [], given_u: 0.1 }], "estadistico", meta) === "string");
  assert.ok(typeof validateMeasurements([{ quantity_id: "g1", values: [3], given_u: null }], "estadistico", meta) === "string");
  assert.equal(validateMeasurements([{ quantity_id: "g1", values: [3], given_u: 0.1 }], "estadistico", meta), null);
});

test("validateMeasurements: isGiven con hasUncertainty:false no exige U (p. ej. t_med)", () => {
  const meta = { t_med: { name: "t_med", isGiven: true, hasUncertainty: false } };
  const err = validateMeasurements([{ quantity_id: "t_med", values: [], given_u: null }], "estadistico", meta);
  assert.ok(typeof err === "string" && err.includes("t_med"));
  assert.equal(
    validateMeasurements([{ quantity_id: "t_med", values: [12.5], given_u: null }], "estadistico", meta),
    null,
  );
});

test("validateMeasurements: magnitud optional puede quedar sin lecturas", () => {
  const meta = {
    T2: { name: "Periodo Operador 2", isGiven: false, isChrono: true, optional: true },
  };
  assert.equal(
    validateMeasurements([{ quantity_id: "T2", values: [] }], "estadistico", meta),
    null,
  );
  const metaRequerida = { T2: { ...meta.T2, optional: false } };
  const err = validateMeasurements([{ quantity_id: "T2", values: [] }], "estadistico", metaRequerida);
  assert.ok(typeof err === "string");
});

test("analysisSections separa lo compartido de cada parte, o devuelve el análisis solo", () => {
  const plain = { quantities: [], derived: [] };
  assert.deepEqual(analysisSections(plain), [{ title: null, analysis: plain }]);
  const serie = { derived: [{ symbol: "I_s" }] };
  const withParts = { quantities: [{ symbol: "R1" }], parts: [{ part_id: "s", title: "Serie", analysis: serie }] };
  const sections = analysisSections(withParts);
  assert.equal(sections.length, 2);
  assert.equal(sections[0].analysis, withParts);
  assert.deepEqual(sections[1], { title: "Serie", analysis: serie });
});

test("symbolHtml dibuja los subíndices genéricos sin símbolo a mostrar", () => {
  setDisplaySymbols({});
  assert.equal(symbolHtml("h_max"), "h<sub>max</sub>");
  assert.equal(symbolHtml("f_res"), "f<sub>res</sub>");
  assert.equal(symbolHtml("I_teo"), "I<sub>teo</sub>");
  assert.equal(symbolHtml("a_R"), "a<sub>R</sub>");
  assert.equal(symbolHtml("b_R"), "b<sub>R</sub>");
});

test("comparableQuantities ofrece las magnitudes compartidas y las de la parte del resultado", () => {
  const definition = {
    parts: [{ id: "serie" }, { id: "paralelo" }],
    sections: [
      { id: "s", part_id: "serie", quantities: ["VR1_s"] },
      { id: "p", part_id: "paralelo", quantities: ["VR1_p"] },
      { id: "c", part_id: null, quantities: ["R1"] },
    ],
    quantities: [{ symbol: "VR1_s" }, { symbol: "VR1_p" }, { symbol: "R1" }, { symbol: "Vg" }],
  };
  assert.deepEqual(comparableQuantities(definition, "serie"), ["VR1_s", "R1", "Vg"]);
  assert.deepEqual(comparableQuantities(definition), ["R1", "Vg"]);
  assert.deepEqual(comparableQuantities({ quantities: [{ symbol: "x" }] }), ["x"]);
});
