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
  assert.equal(analysisKindLabel("relajacion_exponencial"), "Relajación exponencial");
  assert.equal(analysisKindLabel(null), "Sin definir");
  assert.equal(analysisKindLabel(undefined), "Sin definir");
  assert.equal(analysisKindLabel("desconocido"), "Sin definir");
});

test("compatibleInstruments filtra por magnitud, o devuelve todos si no hay match", () => {
  const instruments = [
    { id: "i1", quantity: "longitud" },
    { id: "i2", quantity: "longitud" },
    { id: "i3", quantity: "masa" },
  ];
  assert.deepEqual(
    compatibleInstruments(instruments, "longitud").map((i) => i.id),
    ["i1", "i2"],
  );
  // Sin coincidencias -> devuelve todos (no bloquear la elección).
  assert.equal(compatibleInstruments(instruments, "tiempo").length, 3);
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
