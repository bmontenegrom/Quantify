
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
  return formatSymbolHtml(displaySymbols.get(raw) ?? raw);
}

export function formatSymbolHtml(value) {
  return subscriptHtml(
    escapeHtml(String(value)).replace(GREEK_PREFIX, (name) => GREEK_NAMES[name]),
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

const SUPERSCRIPTS = { "-": "⁻", 0: "⁰", 1: "¹", 2: "²", 3: "³", 4: "⁴", 5: "⁵", 6: "⁶", 7: "⁷", 8: "⁸", 9: "⁹" };

const sup = (digits) => [...digits].map((c) => SUPERSCRIPTS[c]).join("");

export function unitText(value) {
  return String(value ?? "")
    .replace(/\^(-?\d+)/g, (_, d) => sup(d))
    .replace(/([A-Za-zµΩ]+)(-?\d+)/g, (_, base, d) => base + sup(d));
}

export function cssEscape(value) {
  if (typeof window !== "undefined" && window.CSS?.escape) {
    return window.CSS.escape(String(value));
  }
  return String(value).replaceAll('"', '\\"');
}

const decimalsOf = (x) => {
  const text = String(x);
  return /e/.test(text) ? 0 : (text.split(".")[1] ?? "").length;
};

export function exactNumber(value, uncertainty = null) {
  const v = Number(value);
  if (v !== 0 && (Math.abs(v) >= 1e6 || Math.abs(v) < 1e-4)) {
    const [mantissa, exponent] = v.toExponential().split("e");
    return `${mantissa.replace(".", ",")} × 10${sup(String(Number(exponent)))}`;
  }
  const places = uncertainty == null ? 0 : decimalsOf(uncertainty);
  return (decimalsOf(v) < places ? v.toFixed(places) : String(v)).replace(".", ",");
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

export function measureText(value, u) {
  const base = format(value);
  if (u == null || !Number.isFinite(u) || u <= 0) return base;
  return `${base} ± ${format(u)}`;
}

export function regressionPlot(points, slope, intercept, { uPoints = [], width = 320, height = 220, pad = 32 } = {}) {
  if (!Array.isArray(points) || points.length < 2) return null;
  const half = (i, axis) => {
    const u = uPoints?.[i]?.[axis];
    return Number.isFinite(u) && u > 0 ? u : 0;
  };
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const dataMinX = Math.min(...xs);
  const dataMaxX = Math.max(...xs);
  const lineYmin = slope * dataMinX + intercept;
  const lineYmax = slope * dataMaxX + intercept;
  const dataMinY = Math.min(...ys, lineYmin, lineYmax);
  const dataMaxY = Math.max(...ys, lineYmin, lineYmax);
  const widen = (lo, hi, axis) => {
    const room = (hi - lo) / 2;
    return [
      Math.max(lo - room, Math.min(lo, ...points.map((p, i) => p[axis] - half(i, axis)))),
      Math.min(hi + room, Math.max(hi, ...points.map((p, i) => p[axis] + half(i, axis)))),
    ];
  };
  const [minX, maxX] = widen(dataMinX, dataMaxX, 0);
  const [minY, maxY] = widen(dataMinY, dataMaxY, 1);
  const spanX = maxX - minX;
  const spanY = maxY - minY;
  if (spanX === 0 || spanY === 0) return null;
  const innerW = width - 2 * pad;
  const innerH = height - 2 * pad;
  const sx = (x) => pad + ((x - minX) / spanX) * innerW;
  const sy = (y) => height - pad - ((y - minY) / spanY) * innerH;
  const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);
  const bx = (px) => clamp(px, pad, width - pad);
  const by = (py) => clamp(py, pad, height - pad);
  return {
    width,
    height,
    pad,
    scatter: points.map((p, i) => {
      const point = { cx: sx(p[0]), cy: sy(p[1]) };
      if (Array.isArray(uPoints?.[i]) && uPoints[i].some((u) => !Number.isFinite(u))) point.noU = true;
      const bar = (lo, hi, fit) => ({ ends: [fit(lo), fit(hi)], clipped: [fit(lo) !== lo, fit(hi) !== hi] });
      const [ux, uy] = [half(i, 0), half(i, 1)];
      if (ux) point.ex = bar(sx(p[0] - ux), sx(p[0] + ux), bx);
      if (uy) point.ey = bar(sy(p[1] - uy), sy(p[1] + uy), by);
      return point;
    }),
    line: { x1: sx(dataMinX), y1: sy(lineYmin), x2: sx(dataMaxX), y2: sy(lineYmax) },
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

const GUIDE_STEPS = [
  { id: "datos", title: "Datos", help: "Nombre, descripción y tipo de análisis. Si la práctica va a tener partes, el análisis se elige en cada una en el paso siguiente." },
  { id: "partes", title: "Partes", help: "Cada parte es una pestaña del formulario del alumno, con su propio análisis. Sin partes, el formulario va en una sola pestaña." },
  { id: "secciones", title: "Secciones", help: "Agrupan magnitudes y resultados bajo un título. Una sección sin parte se ve en todas las pestañas (datos compartidos). Cada magnitud y mensurando elige su sección en su tabla, en los pasos siguientes." },
  { id: "magnitudes", title: "Magnitudes", help: "Lo que carga el alumno o da la cátedra. El rol dice cómo se ingresa cada una." },
  { id: "serie", title: "Serie y ejes", help: "Intermedias que se calculan en cada punto, curvas y los ejes del ajuste." },
  { id: "mensurandos", title: "Mensurandos", help: "Lo que se calcula a partir de las magnitudes, con sus fórmulas." },
  { id: "revisar", title: "Revisar y publicar", help: "Resolvé los problemas que queden, probá el borrador con datos de ejemplo y publicá." },
];

export const POINT_KINDS = ["regresion_lineal", "curva"];

export function practiceGuideSteps(definition) {
  const series = POINT_KINDS.includes(definition?.analysis_kind);
  return GUIDE_STEPS.filter((s) => s.id !== "serie" || series);
}

export function guideStepOrNext(definition, step) {
  const steps = practiceGuideSteps(definition);
  const order = GUIDE_STEPS.map((s) => s.id);
  const from = order.indexOf(step);
  return (steps.find((s) => s.id === step) ?? steps.find((s) => order.indexOf(s.id) > from) ?? steps[0]).id;
}

export function seriesPart(definition) {
  return (definition?.parts ?? []).find((p) => p.effective_kind && p.effective_kind !== "estadistico");
}

export function symbolPart(definition, symbol) {
  return definition?.resolved?.symbol_parts?.[symbol] ?? null;
}

export function determinationsFor(definition, symbol) {
  return definition?.resolved?.determinations?.[symbol] ?? null;
}

export function expandDeterminationResults(definition, results) {
  return results.flatMap((r) => {
    const d = determinationsFor(definition, r.symbol);
    if (!d) return [r];
    const label = d.label.trim();
    const each = Array.from({ length: d.count }, (_, i) => ({
      ...r,
      symbol: `${r.symbol}#${i + 1}`,
      name: `${r.name} — ${label} ${i + 1}`,
      base: r.symbol,
      part: d.scope || null,
      determination: { scope: d.scope, index: i, tabs: d.display === "pestanas" },
    }));
    const average = { ...r, symbol: `${r.symbol}_medio`, name: `${r.name} (promedio)`, base: r.symbol, part: d.scope || null };
    return d.average ? [...each, average] : each;
  });
}

export function exampleMeasurements(definition, { points = 5, replicas = 3, instruments = [] } = {}) {
  const pointBased = POINT_KINDS.includes(definition?.analysis_kind);
  const round = (x) => Number(x.toPrecision(6));
  const series = (n, v) => Array.from({ length: n }, (_, k) => round(v * (1 + 0.01 * (k - (n - 1) / 2))));
  const quantities = definition?.quantities ?? [];
  return quantities.map((q, i) => {
    const base = q.default_value != null && (q.is_given || q.default_value !== 0) ? q.default_value : quantities.length - i;
    const instrument = q.quantity && !q.is_given ? compatibleInstruments(instruments, q.quantity).find((x) => x.scales?.length) : null;
    const m = { quantity_id: q.id, instrument_id: instrument?.id ?? null, scale_id: instrument?.scales[0].id ?? null, values: [], given_u: null };
    const groups = q.per_determination ? determinationsFor(definition, q.symbol)?.count ?? 0 : 0;
    if (q.is_given) {
      const value = q.slot_count ? q.default_value ?? 1 : base;
      const given_u = hasUncertainty(q) ? round(Math.abs(value) * 0.01) : null;
      if (groups) {
        const each = (o) => (q.slot_count ? value : round(value * (1 + 0.02 * o)));
        return { ...m, given_u, operator_replicas: Array.from({ length: groups }, (_, o) => [each(o)]) };
      }
      return { ...m, values: [value], given_u };
    }
    if (pointBased && q.per_point) {
      const at = (p) => base * (1 + p / (i + 2));
      if (q.repeated && q.replicas_per_point > 0) {
        return { ...m, point_replicas: Array.from({ length: points }, (_, p) => series(q.replicas_per_point, at(p))) };
      }
      return { ...m, values: Array.from({ length: points }, (_, p) => round(at(p))) };
    }
    if (groups) {
      const n = q.repeated ? replicas : 1;
      return { ...m, operator_replicas: Array.from({ length: groups }, (_, o) => series(n, base * (1 + 0.02 * o))) };
    }
    return { ...m, values: q.repeated ? series(replicas, base) : [base] };
  });
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
      operatorInstruments: m.operator_instruments ?? [],
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

export function formulaScope(definition, kind, { id = "", symbol = "", section = null } = {}) {
  const scopes = definition?.formula_scopes;
  if (!scopes) return (definition?.quantities ?? []).map((q) => q.symbol);
  if (kind === "result") {
    if (symbol) return scopes.existing_results?.[symbol] ?? [];
    if (section) return scopes.part_results?.[section.part_id] ?? scopes.results;
    return [...new Set([scopes.results, ...Object.values(scopes.part_results ?? {})].flat())];
  }
  if (kind === "point_result") return scopes.point_results;
  if (kind === "axis") return scopes.axes;
  const byId = { intermediate: scopes.intermediates, aggregate: scopes.aggregates }[kind] ?? {};
  return byId[id ?? ""] ?? [];
}

export function formulaHelpHtml(symbols, functions = [], known = null) {
  const chip = (insert, label, title) =>
    `<button type="button" class="formula-chip" data-insert="${escapeHtml(insert)}" title="${escapeHtml(title)}">${label}</button>`;
  const all = [...new Set([...symbols, "pi", "e"])];
  const isKnown = (s) => !known || known.has(s) || s === "pi" || s === "e";
  const row = (chips) => (chips ? `<div class="formula-chips">${chips}</div>` : "");
  const symbolChips = all.filter(isKnown).map((s) => chip(s, symbolHtml(s), s)).join("");
  const derivedChips = all.filter((s) => !isKnown(s)).map((s) => chip(s, `<code>${escapeHtml(s)}</code>`, s)).join("");
  const functionChips = functions.map(([name, description]) => chip(`${name}()`, escapeHtml(name), description)).join("");
  return `<details class="formula-help"><summary>Símbolos y funciones</summary>${row(symbolChips)}${row(derivedChips)}${row(functionChips)}</details>`;
}

export function comparisonPairs(definition) {
  return new Map(Object.entries(definition?.resolved?.comparisons ?? {}));
}

export function compareMeasuredVsTheoretical(quantities, derived, pairs = new Map()) {
  const byQuantity = new Map((quantities ?? []).map((q) => [q.symbol, q]));
  return (derived ?? []).reduce((rows, d) => {
    const pair = pairs.get(d.symbol);
    const q = byQuantity.get(pair?.with);
    if (!q) return rows;
    const { factor = 1 } = pair;
    const mean = q.result?.mean;
    const u = q.result?.u_expanded;
    const ev = mean == null ? null : mean * factor;
    const eu = u == null ? null : u * factor;
    rows.push({
      symbol: q.symbol,
      theoreticalSymbol: d.symbol,
      name: q.name,
      unit: d.unit ?? q.unit,
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

export function groupError(m, meta) {
  const name = meta.name ?? m.quantity_id;
  const empty = m.operator_replicas.length === 0 ? 0 : m.operator_replicas.findIndex((reps) => reps.length === 0);
  if (empty !== -1) {
    return `"${name}": falta cargar ${meta.groupLabel} ${empty + 1}.`;
  }
  if (meta.isGiven && meta.hasUncertainty !== false && m.given_u == null) {
    return `El dato "${name}" requiere la incertidumbre U.`;
  }
  return null;
}

export function validateMeasurements(measurements, analysisKind, metaMap = {}) {
  if (POINT_KINDS.includes(analysisKind)) {
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
      if (m.operator_replicas) {
        const error = meta.optional ? null : groupError(m, meta);
        if (error) return error;
      } else if (meta.isGiven) {
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
    if (m.operator_replicas) {
      const error = groupError(m, meta);
      if (error) return error;
    } else if (meta.isGiven) {
      if (m.values.length === 0 || (meta.hasUncertainty !== false && m.given_u == null)) {
        return meta.hasUncertainty === false
          ? `El dato "${name}" requiere un valor.`
          : `El dato "${name}" requiere valor e incertidumbre U.`;
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

export function downloadJson(data, filename) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export async function readJsonFile(file) {
  try {
    return JSON.parse(await file.text());
  } catch {
    throw new Error("El archivo no es un JSON válido.");
  }
}
