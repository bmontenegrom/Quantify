// Lógica pura y testeable del frontend (sin DOM, sin red, sin efectos al cargar).
// Se importa desde app.js y desde los tests en tests/. Cada función exportada
// lleva su doc (JSDoc) y tiene un test en tests/lib.test.js (espeja la convención Rust).

/** Escapa los caracteres con significado especial en HTML para interpolar texto seguro. */
export function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

/**
 * Escapa un valor para usarlo dentro de un selector CSS; usa `CSS.escape` si está
 * disponible (browser) y cae a un escape mínimo de comillas en su ausencia (Node/tests).
 */
export function cssEscape(value) {
  if (typeof window !== "undefined" && window.CSS?.escape) {
    return window.CSS.escape(String(value));
  }
  return String(value).replaceAll('"', '\\"');
}

/** Formatea un número con locale es-UY y hasta 5 cifras significativas. */
export function format(value) {
  return Number(value).toLocaleString("es-UY", { maximumSignificantDigits: 5 });
}

/** Formatea un timestamp (ISO o Date) como fecha y hora cortas en locale es-UY. */
export function formatDate(value) {
  return new Date(value).toLocaleString("es-UY", {
    dateStyle: "short",
    timeStyle: "short",
  });
}

/**
 * Agrupa los elementos de `items` por la clave que devuelve `keyFn`; las claves
 * nulas o vacías caen en el grupo "-".
 */
export function groupBy(items, keyFn) {
  return items.reduce((groups, item) => {
    const key = keyFn(item) || "-";
    groups[key] ??= [];
    groups[key].push(item);
    return groups;
  }, {});
}

/**
 * Traduce el tipo de grupo a su etiqueta legible (cualquier valor distinto de
 * "recuperacion" se muestra como "Regular").
 */
export function renderGroupType(value) {
  return value === "recuperacion" ? "Recuperacion" : "Regular";
}

/**
 * Convierte los campos crudos de una escala (strings de un formulario) al payload
 * del API: `step` siempre numérico; el resto de los campos opcionales pasan a número
 * o `null` si vienen vacíos.
 */
export function scalePayload(raw) {
  const num = (key) => (raw[key] !== "" && raw[key] != null ? Number(raw[key]) : null);
  return {
    label: raw.label,
    unit: raw.unit,
    b_model: raw.b_model,
    step: Number(raw.step),
    full_scale: num("full_scale"),
    appreciation: num("appreciation"),
    spec_pct_reading: num("spec_pct_reading"),
    spec_step_coeff: num("spec_step_coeff"),
    spec_fixed: num("spec_fixed"),
    internal_res: num("internal_res"),
    internal_res_u: num("internal_res_u"),
  };
}

// ── Selectores de datos ───────────────────────────────────────────────────────
// Derivan información del contexto académico (`academic`) o las libretas
// (`gradebooks`); reciben el estado por parámetro para ser puros y testeables.

/** `true` si el usuario tiene rol docente o admin (puede revisar/administrar). */
export function canReview(user) {
  return !!(user && ["docente", "admin"].includes(user.role));
}

/** Cursos en los que el estudiante figura como miembro. */
export function studentCourses(academic, studentId) {
  return academic.courses.filter((course) =>
    course.members.some((member) => member.id === studentId),
  );
}

/** Todos los grupos de todos los cursos, anotados con datos del curso. */
export function allGroups(academic) {
  return academic.courses.flatMap((course) =>
    course.groups.map((group) => ({
      ...group,
      courseId: course.id,
      courseName: course.name,
      courseTerm: course.term,
    })),
  );
}

/** Grupos en los que el estudiante es miembro, anotados con nombre/periodo del curso. */
export function studentGroups(academic, studentId) {
  return academic.courses.flatMap((course) =>
    course.groups
      .filter((group) => group.members.some((member) => member.id === studentId))
      .map((group) => ({ ...group, courseName: course.name, courseTerm: course.term })),
  );
}

/** Cursos donde el estudiante todavía NO está inscrito (para ofrecer inscripción). */
export function availableCoursesForStudent(academic, studentId) {
  const currentCourses = new Set(studentCourses(academic, studentId).map((course) => course.id));
  return academic.courses.filter((course) => !currentCourses.has(course.id));
}

/** Grupos disponibles (dentro de los cursos del estudiante) a los que aún no pertenece. */
export function availableGroupsForStudent(academic, studentId) {
  const currentGroups = new Set(studentGroups(academic, studentId).map((group) => group.id));
  return studentCourses(academic, studentId).flatMap((course) =>
    course.groups
      .filter((group) => !currentGroups.has(group.id))
      .map((group) => ({ ...group, courseName: course.name, courseTerm: course.term })),
  );
}

/** Libretas del estudiante: pares curso + resumen, solo donde tiene resumen cargado. */
export function studentGradebooks(gradebooks, studentId) {
  return gradebooks
    .map((book) => ({
      course: book.course,
      summary: book.students.find((summary) => summary.student.id === studentId),
    }))
    .filter((item) => item.summary);
}

/**
 * Totales acumulados (puntos / posibles) del estudiante sobre todas sus libretas;
 * `null` si no tiene ninguna nota cargada.
 */
export function studentTotals(gradebooks, studentId) {
  const books = studentGradebooks(gradebooks, studentId);
  if (books.length === 0) return null;
  return books.reduce(
    (acc, item) => ({
      points: acc.points + item.summary.total_points,
      possible: acc.possible + item.summary.total_possible,
    }),
    { points: 0, possible: 0 },
  );
}

/** Lista de estudiantes: usa `academic.students` si viene, si no filtra `users` por rol. */
export function allStudents(academic) {
  const direct = academic?.students ?? [];
  if (direct.length > 0) return direct;
  return (academic?.users ?? []).filter((user) => user.role === "estudiante");
}

/**
 * Devuelve la etiqueta legible del tipo de análisis de una práctica.
 * Valores reconocidos: `estadistico`, `regresion_lineal`, `relajacion_exponencial`.
 * Cualquier otro valor (incluido `null`/`undefined`) devuelve `"Sin definir"`.
 */
export function analysisKindLabel(kind) {
  switch (kind) {
    case "estadistico":
      return "Estadístico";
    case "regresion_lineal":
      return "Regresión lineal";
    case "relajacion_exponencial":
      return "Relajación exponencial";
    default:
      return "Sin definir";
  }
}

/**
 * Instrumentos compatibles con una magnitud física (p. ej. "longitud"): filtra por el campo
 * `quantity` del instrumento. Si ninguno coincide (o no se da magnitud), devuelve todos, para
 * no bloquear la elección del estudiante.
 */
export function compatibleInstruments(instruments, magnitude) {
  const list = instruments ?? [];
  if (!magnitude) return list;
  const matches = list.filter((instrument) => instrument.quantity === magnitude);
  return matches.length > 0 ? matches : list;
}

/**
 * Formatea una medida como `"valor ± U"` (locale es-UY). Si `u` es nula, no positiva o no
 * finita, muestra solo el valor.
 */
export function measureText(value, u) {
  const base = format(value);
  if (u == null || !Number.isFinite(u) || u <= 0) return base;
  return `${base} ± ${format(u)}`;
}

/**
 * Calcula las coordenadas (en píxeles del lienzo SVG, con `y` hacia abajo) para dibujar un
 * scatter de `points` (pares `[x, y]`) junto a la recta ajustada `y = slope*x + intercept`,
 * en un lienzo `width`×`height` con margen `pad`. El rango se ajusta al bounding box de los
 * datos extendido con los extremos de la recta. Función pura (sin DOM): el SVG se arma en
 * `app.js` a partir de lo que devuelve. Devuelve `null` si hay menos de 2 puntos o el rango
 * en `x` o `y` es nulo (no se puede escalar).
 */
export function regressionPlot(points, slope, intercept, width = 320, height = 220, pad = 32) {
  if (!Array.isArray(points) || points.length < 2) return null;
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  // Extiende el rango en Y para que la recta entre en el lienzo en ambos extremos de X.
  const lineYmin = slope * minX + intercept;
  const lineYmax = slope * maxX + intercept;
  const minY = Math.min(...ys, lineYmin, lineYmax);
  const maxY = Math.max(...ys, lineYmin, lineYmax);
  const spanX = maxX - minX;
  const spanY = maxY - minY;
  if (spanX === 0 || spanY === 0) return null;
  const innerW = width - 2 * pad;
  const innerH = height - 2 * pad;
  const sx = (x) => pad + ((x - minX) / spanX) * innerW;
  // Eje Y invertido: el píxel 0 está arriba, así que los valores grandes van arriba.
  const sy = (y) => height - pad - ((y - minY) / spanY) * innerH;
  return {
    width,
    height,
    pad,
    scatter: points.map((p) => ({ cx: sx(p[0]), cy: sy(p[1]) })),
    line: { x1: sx(minX), y1: sy(lineYmin), x2: sx(maxX), y2: sy(lineYmax) },
    bounds: { minX, maxX, minY, maxY },
  };
}
