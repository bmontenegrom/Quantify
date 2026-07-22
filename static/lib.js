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

// p2-cc: sufijo de parte (_s serie, _p paralelo, _c curva de potencia) y, en los teóricos, un
// _t adicional (VR1_s, VR1_s_t...). La pestaña ya distingue la parte, así que el símbolo se
// muestra igual que su base — sólo se aplica si la base es una de las conocidas, para no afectar
// símbolos de otras prácticas que puedan terminar en _s/_p/_c por coincidencia.
const CC_PART_SUFFIX = /_[spc](?:_t)?$/;
const CC_BASE_SYMBOLS = new Set(["Vg", "RA", "VR1", "VR2", "VR3", "I"]);

/**
 * Escapa un símbolo y muestra como subíndice los dígitos pegados a letras:
 * `R1` → `R<sub>1</sub>`, `C12` → `C<sub>12</sub>`. No altera guiones bajos ni nombres largos.
 */
export function symbolHtml(value) {
  const specials = {
    tmedio: "t<sub>1/2</sub>",
    T_oc: "T<sub>OC</sub>",
    T_OC: "T<sub>OC</sub>",
    gamma: "γ",
    mu: "μ",
    // Vg/RA no llevan guion bajo en el símbolo guardado, pero se muestran con subíndice como
    // el resto de las magnitudes (R1, rho_e, VR1...) para que la tipografía sea coherente.
    Vg: "V<sub>G</sub>",
    RA: "R<sub>A</sub>",
  };
  const raw = String(value);
  if (specials[raw]) return specials[raw];
  const base = raw.replace(CC_PART_SUFFIX, "");
  if (base !== raw && CC_BASE_SYMBOLS.has(base)) return symbolHtml(base);
  return escapeHtml(raw)
    .replace(/([A-Za-z])_([A-Za-z0-9/]{1,2})\b/g, (_, base, sub) => `${base}<sub>${sub.toUpperCase()}</sub>`)
    .replace(/([A-Za-z])(\d+(?:\/\d+)?)/g, "$1<sub>$2</sub>");
}

/**
 * Escapa texto visible y aplica convenciones tipográficas chicas para símbolos en nombres:
 * subíndices (`R1`, `t1/2`, `T_oc`) y potencias escritas con `^`.
 */
export function inlineMathHtml(value) {
  return escapeHtml(value)
    .replace(/([A-Za-z])_([A-Za-z0-9/]{1,2})\b/g, (_, base, sub) => `${base}<sub>${sub.toUpperCase()}</sub>`)
    .replace(/([A-Za-z])(\d+(?:\/\d+)?)/g, "$1<sub>$2</sub>")
    .replace(/\^(-?\d+)/g, "<sup>$1</sup>");
}

/** Escapa y formatea unidades simples: `m3`, `m/s2`, `kg/m3`, `R^2`. */
export function unitHtml(value) {
  return escapeHtml(value)
    .replace(/\^(-?\d+)/g, "<sup>$1</sup>")
    .replace(/([A-Za-zµΩ]+)(-?\d+)/g, "$1<sup>$2</sup>");
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
 * Valores reconocidos: `estadistico`, `regresion_lineal`, `curva`.
 * Cualquier otro valor (incluido `null`/`undefined`) devuelve `"Sin definir"`.
 */
export function analysisKindLabel(kind) {
  switch (kind) {
    case "estadistico":
      return "Estadístico";
    case "regresion_lineal":
      return "Regresión lineal";
    case "curva":
      return "Curva (sin ajuste)";
    default:
      return "Sin definir";
  }
}

/**
 * Instrumentos compatibles con una magnitud física (p. ej. "longitud"): filtra por el campo
 * `quantity` del instrumento. Solo muestra los de la magnitud indicada; si no se da magnitud
 * devuelve todos. Nunca hace fallback al catálogo completo.
 */
export function compatibleInstruments(instruments, magnitude) {
  const list = instruments ?? [];
  if (!magnitude) return list;
  return list.filter((instrument) => instrument.quantity === magnitude);
}

/**
 * Prefijos SI disponibles para seleccionar la escala de una lectura.
 * `factor` es el multiplicador para convertir a la unidad base (SI).
 */
export const SI_PREFIXES = [
  { label: "T",  factor: 1e12  },
  { label: "G",  factor: 1e9   },
  { label: "M",  factor: 1e6   },
  { label: "k",  factor: 1e3   },
  { label: "",   factor: 1     },
  { label: "m",  factor: 1e-3  },
  { label: "µ",  factor: 1e-6  },
  { label: "n",  factor: 1e-9  },
  { label: "p",  factor: 1e-12 },
];

/**
 * Retorna el factor multiplicador para un prefijo SI dado.
 * Si el prefijo no se reconoce retorna 1 (sin prefijo).
 *
 * @param {string} prefix - Etiqueta del prefijo (p. ej. "m", "k", "µ").
 * @returns {number}
 *
 * @example
 * console.assert(prefixFactor("k") === 1e3);
 * console.assert(prefixFactor("µ") === 1e-6);
 * console.assert(prefixFactor("") === 1);
 * console.assert(prefixFactor("?") === 1);
 */
export function prefixFactor(prefix) {
  return SI_PREFIXES.find((p) => p.label === prefix)?.factor ?? 1;
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

/**
 * Proyecta una serie de puntos a coordenadas de SVG para un gráfico de dispersión **sin recta**
 * (`analysis_kind = "curva"`). Si `xLog` es `true`, el eje x se escala logarítmicamente (base 10),
 * útil para barridos en frecuencia. Devuelve `null` si hay menos de 2 puntos, si el rango en algún
 * eje es nulo, o si `xLog` y algún `x <= 0`.
 */
export function scatterPlot(points, { width = 320, height = 220, pad = 32, xLog = false } = {}) {
  if (!Array.isArray(points) || points.length < 2) return null;
  if (xLog && points.some((p) => p[0] <= 0)) return null;
  const tx = (x) => (xLog ? Math.log10(x) : x);
  const xs = points.map((p) => tx(p[0]));
  const ys = points.map((p) => p[1]);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const spanX = maxX - minX;
  const spanY = maxY - minY;
  if (spanX === 0 || spanY === 0) return null;
  const innerW = width - 2 * pad;
  const innerH = height - 2 * pad;
  const sx = (x) => pad + ((tx(x) - minX) / spanX) * innerW;
  const sy = (y) => height - pad - ((y - minY) / spanY) * innerH;
  return {
    width,
    height,
    pad,
    xLog,
    scatter: points.map((p) => ({ cx: sx(p[0]), cy: sy(p[1]) })),
    bounds: { minX, maxX, minY, maxY },
  };
}

/**
 * Estadísticos de una serie de medidas: `n`, media (`mean`), desviación estándar muestral
 * (`std`, con divisor n-1) y error estándar de la media (`stdMean = s/√n`). Ignora valores no
 * finitos. Devuelve `NaN` en mean/std si no hay datos; `std=stdMean=0` si hay un solo dato.
 * Función pura (sin DOM): la lista ordenada y el histograma del formulario se arman con esto.
 */
export function seriesStats(values) {
  const xs = (values ?? []).filter((v) => Number.isFinite(v));
  const n = xs.length;
  if (n === 0) return { n: 0, mean: NaN, std: NaN, stdMean: NaN };
  const mean = xs.reduce((a, b) => a + b, 0) / n;
  if (n < 2) return { n, mean, std: 0, stdMean: 0 };
  const variance = xs.reduce((a, b) => a + (b - mean) ** 2, 0) / (n - 1);
  const std = Math.sqrt(variance);
  return { n, mean, std, stdMean: std / Math.sqrt(n) };
}

/**
 * Histograma de `values` en `bins` intervalos iguales sobre `[min, max]` de los datos. Devuelve
 * `{ min, max, bins, width, counts, edges }` (counts.length === bins, edges.length === bins+1).
 * El valor máximo cae en el último bin. Si todos los valores son iguales devuelve un único bin.
 * Devuelve `null` si no hay datos finitos o `bins < 1`. Función pura.
 */
export function histogram(values, bins) {
  const xs = (values ?? []).filter((v) => Number.isFinite(v));
  if (xs.length === 0 || bins < 1) return null;
  const min = Math.min(...xs);
  const max = Math.max(...xs);
  if (min === max) {
    return { min, max, bins: 1, width: 0, counts: [xs.length], edges: [min, max] };
  }
  const width = (max - min) / bins;
  const counts = new Array(bins).fill(0);
  for (const x of xs) {
    let idx = Math.floor((x - min) / width);
    if (idx >= bins) idx = bins - 1; // el máximo entra en el último bin
    if (idx < 0) idx = 0;
    counts[idx] += 1;
  }
  const edges = Array.from({ length: bins + 1 }, (_, i) => min + i * width);
  return { min, max, bins, width, counts, edges };
}

/**
 * Muestrea la densidad normal (pdf) de media `mean` y desviación `std` en `steps+1` puntos
 * equiespaciados de `[min, max]`. Devuelve pares `[x, y]` (y = densidad). Lista vacía si
 * `std <= 0` o `max <= min`. Función pura: se usa para superponer la curva al histograma.
 */
export function normalCurve(mean, std, min, max, steps = 60) {
  if (!(std > 0) || !(max > min)) return [];
  const coef = 1 / (std * Math.sqrt(2 * Math.PI));
  const out = [];
  for (let i = 0; i <= steps; i++) {
    const x = min + ((max - min) * i) / steps;
    const y = coef * Math.exp(-((x - mean) ** 2) / (2 * std * std));
    out.push([x, y]);
  }
  return out;
}

/** Diferencia relativa porcentual `(b - a) / a * 100`, o `null` si no es calculable. */
function relPct(b, a) {
  if (b == null || a == null || !Number.isFinite(b) || !Number.isFinite(a) || a === 0) {
    return null;
  }
  return ((b - a) / a) * 100;
}

/**
 * Compara los mensurandos automáticos (`autoDerived`: lista con `symbol`, `name`, `unit`, `value`,
 * `u_expanded`) contra los que cargó el estudiante (`studentResults`: lista con `symbol`, `value`,
 * `u_expanded`). Itera sobre los automáticos (la fuente de los mensurandos) y empareja por símbolo.
 * Para cada uno devuelve la medida automática, la del estudiante (o `null` si no la cargó) y las
 * diferencias absoluta y relativa (%) de valor y de U. Las relativas son `null` si el denominador
 * automático es nulo o no finito.
 *
 * `tolerances` es un mapa `{ symbol → porcentaje_máximo }`. Si el símbolo tiene tolerancia y el
 * alumno cargó un valor, `verdict` es `"pass"` o `"fail"`; en otro caso es `null`.
 * Función pura: el render arma la tabla con esto.
 */
/** `true` salvo que `has_uncertainty` sea explícitamente `false` (magnitud/resultado sin ±U por
 *  diseño, p. ej. un dato de tabla o un mensurando que se muestra sin incertidumbre). */
export function hasUncertainty(entity) {
  return entity?.has_uncertainty !== false;
}

export function compareResults(autoDerived, studentResults, tolerances = {}) {
  const auto = autoDerived ?? [];
  const byStudent = new Map((studentResults ?? []).map((s) => [s.symbol, s]));
  return auto.map((d) => {
    const s = byStudent.get(d.symbol) ?? null;
    const sv = s ? s.value : null;
    const su = s && s.u_expanded != null ? s.u_expanded : null;
    const dValuePct = relPct(sv, d.value);
    const tol = tolerances[d.symbol] ?? null;
    const verdict =
      tol != null && dValuePct != null
        ? Math.abs(dValuePct) <= tol ? "pass" : "fail"
        : null;
    return {
      symbol: d.symbol,
      name: d.name,
      unit: d.unit,
      hasUncertainty: hasUncertainty(d),
      auto: { value: d.value, u: d.u_expanded },
      student: s ? { value: sv, u: su } : null,
      dValue: sv == null ? null : sv - d.value,
      dValuePct,
      dU: su == null || d.u_expanded == null ? null : su - d.u_expanded,
      dUPct: relPct(su, d.u_expanded),
      verdict,
    };
  });
}

/** Potencia disipada en una resistencia: P = I²·R. Función pura para la columna en vivo. */
export function pointPower(r, i) {
  return i * i * r;
}

/** Caudal instantáneo: Q = V/t. Función pura para las columnas en vivo de Fluidos I. */
export function flowRate(v, t) {
  return v / t;
}

/**
 * Convierte la forma *agrupada por magnitud* que devuelve `collectMeasurements()` (una fila por
 * magnitud, con `values`/`point_replicas`/`operator_replicas`) al mismo `Map` que ya espera el
 * prefill de edición (`pointGroups`/`operatorGroups`/`values`/`value_u`/instrumento/escala),
 * para poder restaurar un borrador local con los mismos helpers de pintado que restauran una
 * entrega guardada. Sin magnitudes con réplicas por punto, cada punto se trata como un array de
 * un solo valor (igual que el prefill de edición, que agrupa una lectura por punto).
 */
export function draftMeasurementsByQuantity(measurements) {
  const map = new Map();
  for (const m of measurements ?? []) {
    map.set(m.quantity_id, {
      pointGroups: m.point_replicas ?? (m.values ?? []).map((v) => [v]),
      operatorGroups: m.operator_replicas ?? [],
      values: m.values ?? [],
      value_u: m.given_u ?? null,
      instrument_id: m.instrument_id ?? null,
      scale_id: m.scale_id ?? null,
    });
  }
  return map;
}

/**
 * Empareja cada magnitud MEDIDA con su mensurando teórico automático por convención de símbolos:
 * la magnitud `X` se compara con el derivado `X_t` (p. ej. `VR1_s` medida con multímetro contra
 * `VR1_s_t` calculada por el programa). Devuelve una fila por par encontrado, con la medida
 * experimental (valor ± U del instrumento), la teórica (valor ± U propagada) y las diferencias
 * absolutas y relativas (%) de valor y de U (`null` si el denominador teórico es nulo/no finito).
 *
 * `quantities` es `analysis.quantities` (con `symbol`, `name`, `unit`, `result: {mean, u_expanded}`)
 * y `derived` es `analysis.derived`. Función pura: el render arma la tabla con esto.
 */
export function compareMeasuredVsTheoretical(quantities, derived) {
  const byQuantity = new Map((quantities ?? []).map((q) => [q.symbol, q]));
  return (derived ?? []).reduce((rows, d) => {
    if (!d.symbol.endsWith("_t")) return rows;
    const q = byQuantity.get(d.symbol.slice(0, -2));
    if (!q) return rows;
    const ev = q.result?.mean ?? null;
    const eu = q.result?.u_expanded ?? null;
    rows.push({
      symbol: q.symbol,
      theoreticalSymbol: d.symbol,
      name: q.name,
      unit: q.unit,
      exp: { value: ev, u: eu },
      teo: { value: d.value, u: d.u_expanded },
      dValue: ev == null || !Number.isFinite(ev) ? null : ev - d.value,
      dValuePct: relPct(ev, d.value),
      dU: eu == null || d.u_expanded == null || !Number.isFinite(eu) ? null : eu - d.u_expanded,
      dUPct: relPct(eu, d.u_expanded),
    });
    return rows;
  }, []);
}

/**
 * Valida que las medidas del formulario sean suficientes para el tipo de análisis.
 * Devuelve un mensaje de error en español, o `null` si todo está bien.
 *
 * `metaMap` es un mapa `{ [quantity_id]: { name, isGiven, isChrono } }` con la metadata de cada
 * magnitud (extraída del DOM por el llamador). Si falta la metadata de una magnitud se usa su id
 * como nombre. Función pura: no accede al DOM.
 *
 * @param {Array<{quantity_id: string, values: number[], given_u: number|null}>} measurements
 * @param {string} analysisKind
 * @param {Record<string, {name: string, isGiven: boolean, isChrono: boolean}>} [metaMap]
 * @returns {string|null}
 */
export function validateMeasurements(measurements, analysisKind, metaMap = {}) {
  if (analysisKind === "regresion_lineal" || analysisKind === "curva") {
    // Cantidad de puntos por magnitud: réplicas por punto si las hay, si no los valores planos.
    const pointCount = (m) => m.point_replicas?.length ?? m.values.length;
    const points = Math.max(0, ...measurements.map(pointCount));
    if (points < 2) {
      return analysisKind === "curva"
        ? "Cargá al menos 2 puntos completos para graficar la curva."
        : "Cargá al menos 2 puntos completos para el ajuste lineal.";
    }
    // Motor E: los escalares compartidos (datos de cátedra / medida única) también deben estar
    // completos, aunque no formen parte de la serie de puntos.
    for (const m of measurements) {
      const meta = metaMap[m.quantity_id] ?? {};
      const name = meta.name ?? m.quantity_id;
      if (meta.isGiven) {
        if (m.values.length === 0 || (meta.hasUncertainty !== false && m.given_u == null)) {
          return meta.hasUncertainty === false
            ? `El dato "${name}" requiere un valor.`
            : `El dato "${name}" requiere valor e incertidumbre U.`;
        }
      } else if (meta.perPoint === false && !meta.optional && m.values.length === 0) {
        return `La magnitud compartida "${name}" no tiene valor cargado.`;
      }
    }
    return null;
  }
  for (const m of measurements) {
    const meta = metaMap[m.quantity_id] ?? {};
    const name = meta.name ?? m.quantity_id;
    if (meta.optional) {
      continue; // puede quedar sin lecturas sin bloquear el envío (p. ej. operador 2/3).
    }
    if (meta.isGiven) {
      if (m.values.length === 0 || (meta.hasUncertainty !== false && m.given_u == null)) {
        return meta.hasUncertainty === false
          ? `El dato "${name}" requiere un valor.`
          : `El dato "${name}" requiere valor e incertidumbre U.`;
      }
    } else if (m.operator_replicas) {
      // Motor D: cada operador debe tener al menos una lectura de la magnitud repetida.
      const empty = m.operator_replicas.findIndex((reps) => reps.length === 0);
      if (m.operator_replicas.length === 0 || empty !== -1) {
        return `"${name}": cada operador debe cargar al menos una lectura (falta el operador ${empty + 1}).`;
      }
    } else if (meta.isChrono) {
      if (m.values.length === 0) {
        return `"${name}": registrá al menos una lectura con el cronómetro antes de entregar.`;
      }
    } else if (m.values.length === 0) {
      return `La magnitud "${name}" no tiene lecturas cargadas.`;
    }
  }
  return null;
}
