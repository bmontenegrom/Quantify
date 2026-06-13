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
  SI_PREFIXES,
  prefixFactor,
  seriesStats,
  histogram,
  normalCurve,
  validateMeasurements,
} from "../static/lib.js";

// Fixture chico de contexto académico: 2 cursos, el estudiante s1 está en c1 (grupo g1)
// pero no en c2; g2 de c1 queda libre para s1.
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
  // s1 está en c1 → disponible solo c2.
  assert.deepEqual(
    availableCoursesForStudent(academic, "s1").map((c) => c.id),
    ["c2"],
  );
  // s2 está en ambos → no queda ninguno disponible.
  assert.deepEqual(availableCoursesForStudent(academic, "s2"), []);
});

test("availableGroupsForStudent ofrece grupos no asignados dentro de sus cursos", () => {
  const academic = academicFixture();
  // s1 está en c1/g1; dentro de c1 le queda g2 disponible (g3 es de c2, donde no está).
  assert.deepEqual(
    availableGroupsForStudent(academic, "s1").map((g) => g.id),
    ["g2"],
  );
});

test("studentGradebooks proyecta solo las libretas con resumen del estudiante", () => {
  const gradebooks = gradebooksFixture();
  const books = studentGradebooks(gradebooks, "s1");
  assert.deepEqual(books.map((b) => b.course.id), ["c1", "c2"]);
  // s2 solo tiene resumen en c1.
  assert.deepEqual(studentGradebooks(gradebooks, "s2").map((b) => b.course.id), ["c1"]);
});

test("studentTotals suma sobre todas las libretas, o null sin notas", () => {
  const gradebooks = gradebooksFixture();
  // s1: (8/10) + (3/5) = 11/15.
  assert.deepEqual(studentTotals(gradebooks, "s1"), { points: 11, possible: 15 });
  assert.deepEqual(studentTotals(gradebooks, "s2"), { points: 5, possible: 10 });
  assert.equal(studentTotals(gradebooks, "sin-notas"), null);
});

test("allStudents usa academic.students si viene, si no filtra users por rol", () => {
  // Sin students explícitos → filtra users por rol estudiante.
  assert.deepEqual(allStudents(academicFixture()).map((u) => u.id), ["s1"]);
  // Con students explícitos → los usa tal cual.
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
  // Kind eliminado (decisión docente 2026-06: τ se obtiene por medida directa y desfasaje).
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
  // Sin coincidencias -> lista vacía (filtrado estricto, sin fallback).
  assert.equal(compatibleInstruments(instruments, "tiempo").length, 0);
  // Sin magnitud -> todos.
  assert.equal(compatibleInstruments(instruments, null).length, 3);
  // Lista vacía/indefinida -> []
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
  // y = 2x + 1 sobre x = 0,1,2; lienzo 320x220, pad 32.
  const plot = regressionPlot([[0, 1], [1, 3], [2, 5]], 2, 1, 320, 220, 32);
  assert.deepEqual(plot.bounds, { minX: 0, maxX: 2, minY: 1, maxY: 5 });
  // Eje X: minX -> pad, maxX -> width-pad, medio -> centro.
  assert.deepEqual(plot.scatter, [
    { cx: 32, cy: 188 },
    { cx: 160, cy: 110 },
    { cx: 288, cy: 32 },
  ]);
  // La recta cruza desde (minX, minY) abajo-izquierda hasta (maxX, maxY) arriba-derecha.
  assert.deepEqual(plot.line, { x1: 32, y1: 188, x2: 288, y2: 32 });
});

test("regressionPlot extiende el rango Y para incluir la recta", () => {
  // Puntos planos (y=0) pero recta con pendiente: el rango Y debe llegar hasta y=5.
  const plot = regressionPlot([[0, 0], [1, 0]], 5, 0, 320, 220, 32);
  assert.equal(plot.bounds.minY, 0);
  assert.equal(plot.bounds.maxY, 5);
  assert.deepEqual(plot.line, { x1: 32, y1: 188, x2: 288, y2: 32 });
});

test("regressionPlot devuelve null con <2 puntos o rango nulo", () => {
  assert.equal(regressionPlot([], 1, 0), null);
  assert.equal(regressionPlot([[1, 2]], 1, 0), null);
  // Todos los x iguales -> spanX 0.
  assert.equal(regressionPlot([[1, 2], [1, 4]], 0, 1), null);
  // y constante y recta plana -> spanY 0.
  assert.equal(regressionPlot([[0, 3], [2, 3]], 0, 3), null);
});

test("scatterPlot proyecta puntos sin recta (no expone línea)", () => {
  const plot = scatterPlot([[0, 1], [1, 3], [2, 5]]);
  assert.equal(plot.scatter.length, 3);
  assert.equal(plot.line, undefined);
  assert.equal(plot.xLog, false);
  // Primer punto a la izquierda-abajo, último a la derecha-arriba (eje Y invertido).
  assert.ok(plot.scatter[0].cx < plot.scatter[2].cx);
  assert.ok(plot.scatter[0].cy > plot.scatter[2].cy);
});

test("scatterPlot con xLog escala el eje x logarítmicamente", () => {
  // x = 1, 10, 100 -> log10 = 0, 1, 2: equiespaciados en pantalla.
  const plot = scatterPlot([[1, 5], [10, 6], [100, 7]], { xLog: true });
  assert.equal(plot.xLog, true);
  const d1 = plot.scatter[1].cx - plot.scatter[0].cx;
  const d2 = plot.scatter[2].cx - plot.scatter[1].cx;
  assert.ok(Math.abs(d1 - d2) < 1e-9);
});

test("scatterPlot devuelve null con <2 puntos, rango nulo o x≤0 en log", () => {
  assert.equal(scatterPlot([]), null);
  assert.equal(scatterPlot([[1, 2]]), null);
  // Todos los x iguales -> spanX 0.
  assert.equal(scatterPlot([[1, 2], [1, 4]]), null);
  // Eje log con un x no positivo.
  assert.equal(scatterPlot([[0, 2], [10, 4]], { xLog: true }), null);
});

test("compareResults empareja por símbolo y calcula Δ abs y %", () => {
  const auto = [{ symbol: "Q", name: "Area", unit: "mm2", value: 70, u_expanded: 2 }];
  const student = [{ symbol: "Q", value: 73.5, u_expanded: 2.2 }];
  const [row] = compareResults(auto, student);
  assert.deepEqual(row.auto, { value: 70, u: 2 });
  assert.deepEqual(row.student, { value: 73.5, u: 2.2 });
  assert.ok(Math.abs(row.dValue - 3.5) < 1e-9);
  assert.ok(Math.abs(row.dValuePct - 5) < 1e-9); // 3.5/70*100
  assert.ok(Math.abs(row.dU - 0.2) < 1e-9);
  assert.ok(Math.abs(row.dUPct - 10) < 1e-9); // 0.2/2*100
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
  // Z: valor automático 0 -> % null, pero Δ abs sí (5). U automática 0 -> % null.
  assert.ok(Math.abs(rows[0].dValue - 5) < 1e-9);
  assert.equal(rows[0].dValuePct, null);
  assert.equal(rows[0].dUPct, null);
  // W: el alumno no cargó U -> dU y dUPct null; el valor sí compara.
  assert.ok(Math.abs(rows[1].dValue - 2) < 1e-9);
  assert.equal(rows[1].dU, null);
  assert.equal(rows[1].dUPct, null);
});

test("compareResults: verdict pass/fail según tolerancia", () => {
  const auto = [{ symbol: "Q", name: "Area", unit: "mm2", value: 100, u_expanded: 2 }];
  const student = [{ symbol: "Q", value: 104, u_expanded: 2 }];
  // Sin tolerancia -> verdict null.
  const [noTol] = compareResults(auto, student);
  assert.equal(noTol.verdict, null);
  // |Δ%| = 4 ≤ 5 -> pass.
  const [pass] = compareResults(auto, student, { Q: 5 });
  assert.equal(pass.verdict, "pass");
  // |Δ%| = 4 > 3 -> fail.
  const [fail] = compareResults(auto, student, { Q: 3 });
  assert.equal(fail.verdict, "fail");
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

  // [2, 4]: media=3, varianza=(1+1)/1=2, std=√2.
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

  // Un solo valor -> bins=1 con width=0.
  const one = histogram([5], 3);
  assert.equal(one.bins, 1);
  assert.equal(one.counts[0], 1);
  assert.equal(one.width, 0);

  // [0,1,2,3] en 2 bins: bin0=[0,2) → {0,1}=2, bin1=[2,4] → {2,3}=2.
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
  assert.equal(pts.length, 11); // steps+1
  // La densidad en la media debe ser el máximo.
  const ys = pts.map((p) => p[1]);
  const maxY = Math.max(...ys);
  const midPt = pts[5]; // x=0 con steps=10
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
  // La magnitud de eje 'y' trae réplicas por punto; 'x' un valor por punto. 2 puntos → válido.
  const ok = validateMeasurements(
    [
      { quantity_id: "x", values: [1, 2], given_u: null },
      { quantity_id: "y", values: [], given_u: null, point_replicas: [[4, 5], [6, 7]] },
    ],
    "regresion_lineal",
  );
  assert.equal(ok, null);
  // Un solo punto con réplicas → insuficiente.
  const few = validateMeasurements(
    [{ quantity_id: "y", values: [], given_u: null, point_replicas: [[4, 5]] }],
    "regresion_lineal",
  );
  assert.ok(typeof few === "string");
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
  // Operador 2 sin lecturas -> error que lo identifica.
  const err = validateMeasurements(
    [{ quantity_id: "T", values: [], given_u: null, operator_replicas: [[1, 1.1], []] }],
    "estadistico",
    meta,
  );
  assert.ok(typeof err === "string");
  assert.ok(err.includes("operador 2"));
  // Todos los operadores con lecturas -> ok.
  const ok = validateMeasurements(
    [{ quantity_id: "T", values: [], given_u: null, operator_replicas: [[1, 1.1], [2, 2.1]] }],
    "estadistico",
    meta,
  );
  assert.equal(ok, null);
});

test("validateMeasurements: isGiven exige valor e incertidumbre", () => {
  const meta = { g1: { name: "Dato", isGiven: true, isChrono: false } };
  // Sin valor -> error.
  assert.ok(typeof validateMeasurements([{ quantity_id: "g1", values: [], given_u: 0.1 }], "estadistico", meta) === "string");
  // Sin u -> error.
  assert.ok(typeof validateMeasurements([{ quantity_id: "g1", values: [3], given_u: null }], "estadistico", meta) === "string");
  // Ambos -> ok.
  assert.equal(validateMeasurements([{ quantity_id: "g1", values: [3], given_u: 0.1 }], "estadistico", meta), null);
});
