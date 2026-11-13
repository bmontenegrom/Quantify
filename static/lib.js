
export function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

const GREEK_NAMES = { rho: "ρ", gamma: "γ", mu: "μ", phi: "φ", tau: "τ", omega: "ω" };
const GREEK_PREFIX = new RegExp(`^(${Object.keys(GREEK_NAMES).join("|")})(?=_|\\d|$)`);

const SYMBOL_SUBSCRIPT =
  /([A-Za-zα-ω])(?:_([\p{L}\p{N}/]+(?:_[\p{L}\p{N}/]+)*)|(\d+(?:\/\d+)?(?:_[\p{L}\p{N}/]+)*))/gu;
const NAME_SUBSCRIPT = /([A-Za-zα-ω])(?:_([\p{L}\p{N}/]{1,2})(?![\p{L}\p{N}])|(\d+(?:\/\d+)?))/gu;

function subscriptHtml(escaped, pattern) {
  return escaped.replace(pattern, (_, base, sub, digits) => {
    const parts = (sub ?? digits).split("_");
    const text = parts.map((s) => (s.length <= 2 ? s.toUpperCase() : s)).join(",");
    return `${base}<sub>${text}</sub>`;
  });
}

let displaySymbols = new Map();

export function setDisplaySymbols(symbols) {
  displaySymbols = new Map(Object.entries(symbols ?? {}));
}

export function symbolHtml(value) {
  const raw = String(value);
  return subscriptHtml(
    escapeHtml(displaySymbols.get(raw) ?? raw).replace(GREEK_PREFIX, (name) => GREEK_NAMES[name]),
    SYMBOL_SUBSCRIPT,
  );
}

export function inlineMathHtml(value) {
  return subscriptHtml(escapeHtml(value), NAME_SUBSCRIPT).replace(/\^(-?\d+)/g, "<sup>$1</sup>");
}

export function unitHtml(value) {
  return escapeHtml(value)
    .replace(/\^(-?\d+)/g, "<sup>$1</sup>")
    .replace(/([A-Za-zµΩ]+)(-?\d+)/g, "$1<sup>$2</sup>");
}

export function cssEscape(value) {
  if (typeof window !== "undefined" && window.CSS?.escape) {
    return window.CSS.escape(String(value));
  }
  return String(value).replaceAll('"', '\\"');
}

export function format(value) {
  return Number(value).toLocaleString("es-UY", { maximumSignificantDigits: 5 });
}

export function num(value) {
  return value == null ? "—" : escapeHtml(format(value));
}

export function pct(value) {
  return value == null ? "—" : `${escapeHtml(format(value))} %`;
}

export function formatDate(value) {
  return new Date(value).toLocaleString("es-UY", {
    dateStyle: "short",
    timeStyle: "short",
  });
}

export function groupBy(items, keyFn) {
  return items.reduce((groups, item) => {
    const key = keyFn(item) || "-";
    groups[key] ??= [];
    groups[key].push(item);
    return groups;
  }, {});
}

export function renderGroupType(value) {
  return value === "recuperacion" ? "Recuperacion" : "Regular";
}

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
    u_cal_pct: num("u_cal_pct"),
    u_cal_fixed: num("u_cal_fixed"),
  };
}

export function canReview(user) {
  return !!(user && ["docente", "admin"].includes(user.role));
}

export function studentCourses(academic, studentId) {
  return academic.courses.filter((course) =>
    course.members.some((member) => member.id === studentId),
  );
}

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

export function studentGroups(academic, studentId) {
  return academic.courses.flatMap((course) =>
    course.groups
      .filter((group) => group.members.some((member) => member.id === studentId))
      .map((group) => ({ ...group, courseName: course.name, courseTerm: course.term })),
  );
}

export function availableCoursesForStudent(academic, studentId) {
  const currentCourses = new Set(studentCourses(academic, studentId).map((course) => course.id));
  return academic.courses.filter((course) => !currentCourses.has(course.id));
}

export function availableGroupsForStudent(academic, studentId) {
  const currentGroups = new Set(studentGroups(academic, studentId).map((group) => group.id));
  return studentCourses(academic, studentId).flatMap((course) =>
    course.groups
      .filter((group) => !currentGroups.has(group.id))
      .map((group) => ({ ...group, courseName: course.name, courseTerm: course.term })),
  );
}

export function studentGradebooks(gradebooks, studentId) {
  return gradebooks
    .map((book) => ({
      course: book.course,
      summary: book.students.find((summary) => summary.student.id === studentId),
    }))
    .filter((item) => item.summary);
}

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

export function allStudents(academic) {
  const direct = academic?.students ?? [];
  if (direct.length > 0) return direct;
  return (academic?.users ?? []).filter((user) => user.role === "estudiante");
}

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

export function compatibleInstruments(instruments, magnitude) {
  const list = instruments ?? [];
  if (!magnitude) return list;
  return list.filter((instrument) => instrument.quantity === magnitude);
}

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

export function prefixFactor(prefix) {
  return SI_PREFIXES.find((p) => p.label === prefix)?.factor ?? 1;
}

export function measureText(value, u) {
  const base = format(value);
  if (u == null || !Number.isFinite(u) || u <= 0) return base;
  return `${base} ± ${format(u)}`;
}

export function regressionPlot(points, slope, intercept, width = 320, height = 220, pad = 32) {
  if (!Array.isArray(points) || points.length < 2) return null;
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
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
    if (idx >= bins) idx = bins - 1;
    if (idx < 0) idx = 0;
    counts[idx] += 1;
  }
  const edges = Array.from({ length: bins + 1 }, (_, i) => min + i * width);
  return { min, max, bins, width, counts, edges };
}

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

function relPct(b, a) {
  if (b == null || a == null || !Number.isFinite(b) || !Number.isFinite(a) || a === 0) {
    return null;
  }
  return ((b - a) / a) * 100;
}

export const QUANTITY_ROLES = [
  { id: "catedra", label: "Dato de cátedra (valor ± U)", short: "Dato de cátedra", series: false },
  { id: "constante", label: "Dato de cátedra sin incertidumbre / constante", short: "Constante", series: false },
  { id: "unica", label: "Medida única con instrumento", short: "Medida única", series: false },
  { id: "replicas", label: "Medida con réplicas (tipo A)", short: "Con réplicas", series: false },
  { id: "serie", label: "Serie de datos (un valor por punto)", short: "Serie", series: true },
  { id: "serie_replicas", label: "Serie con réplicas por punto", short: "Serie con réplicas", series: true },
];

export function quantityRole(quantity, pointBased) {
  if (quantity.is_given) return quantity.has_uncertainty === false ? "constante" : "catedra";
  if (pointBased && quantity.per_point) {
    return quantity.repeated && quantity.replicas_per_point > 0 ? "serie_replicas" : "serie";
  }
  return quantity.repeated ? "replicas" : "unica";
}

export function roleFlags(role, pointBased, current = {}) {
  const given = role === "catedra" || role === "constante";
  const series = role === "serie" || role === "serie_replicas";
  const wasSerie = !current.is_given && current.per_point && !(current.replicas_per_point > 0);
  return {
    is_given: given,
    has_uncertainty: role !== "constante",
    repeated: given || (role === "serie" && wasSerie) ? Boolean(current.repeated) : role === "replicas" || role === "serie_replicas",
    per_point: series || (!pointBased && (current.per_point ?? true)),
  };
}

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

export function draftedQuantityIds(measurements) {
  const hasReadings = (m) => m.values?.length || m.operator_replicas?.some((reps) => reps.length);
  return new Set((measurements ?? []).filter(hasReadings).map((m) => m.quantity_id));
}

export function analysisSections(analysis) {
  const parts = analysis?.parts ?? [];
  if (!parts.length) return [{ title: null, analysis }];
  return [
    { title: "Datos compartidos", analysis },
    ...parts.map((p) => ({ title: p.title, analysis: p.analysis })),
  ];
}

export function compareOptionGroups(definition, { result = null, section = null } = {}, current = "") {
  const options = definition?.compare_options;
  const shared = options?.shared ?? (definition?.quantities ?? []).map((q) => q.symbol);
  const ofPart = (id) => options?.parts?.[id] ?? shared;
  let groups;
  if (result) groups = [[null, options?.results?.[result] ?? shared]];
  else if (section) groups = [[null, section.part_id ? ofPart(section.part_id) : shared]];
  else {
    groups = [[null, shared]].concat(
      (definition?.parts ?? []).map((p) => [p.title, ofPart(p.id).filter((s) => !shared.includes(s))]),
    );
  }
  groups = groups.filter(([label, symbols]) => label == null || symbols.length);
  const offered = groups.some(([, symbols]) => symbols.includes(current));
  return result && current && !offered ? [["Guardada (fuera de alcance)", [current]], ...groups] : groups;
}

export function comparisonPairs(definition) {
  return new Map((definition?.results ?? []).filter((r) => r.compare_with).map((r) => [r.symbol, r.compare_with]));
}

export function compareMeasuredVsTheoretical(quantities, derived, pairs = new Map()) {
  const byQuantity = new Map((quantities ?? []).map((q) => [q.symbol, q]));
  return (derived ?? []).reduce((rows, d) => {
    const q = byQuantity.get(pairs.get(d.symbol));
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

export function validateMeasurements(measurements, analysisKind, metaMap = {}) {
  if (analysisKind === "regresion_lineal" || analysisKind === "curva") {
    const pointCount = (m) => m.point_replicas?.length ?? m.values.length;
    const isSeries = (m) => metaMap[m.quantity_id]?.perPoint !== false && !metaMap[m.quantity_id]?.isGiven;
    const points = Math.max(0, ...measurements.filter(isSeries).map(pointCount));
    if (points < 2) {
      return analysisKind === "curva"
        ? "Cargá al menos 2 puntos completos para graficar la curva."
        : "Cargá al menos 2 puntos completos para el ajuste lineal.";
    }
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
      continue;
    }
    if (meta.isGiven) {
      if (m.values.length === 0 || (meta.hasUncertainty !== false && m.given_u == null)) {
        return meta.hasUncertainty === false
          ? `El dato "${name}" requiere un valor.`
          : `El dato "${name}" requiere valor e incertidumbre U.`;
      }
    } else if (m.operator_replicas) {
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
