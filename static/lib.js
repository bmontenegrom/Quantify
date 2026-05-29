// Lógica pura y testeable del frontend (sin DOM, sin red, sin efectos al cargar).
// Se importa desde app.js y desde los tests en tests/. Cada función exportada
// lleva su doc y tiene un test en tests/lib.test.js (espeja la convención Rust).

/// Escapa los caracteres con significado especial en HTML para interpolar texto seguro.
export function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

/// Escapa un valor para usarlo dentro de un selector CSS; usa `CSS.escape` si está
/// disponible (browser) y cae a un escape mínimo de comillas en su ausencia (Node/tests).
export function cssEscape(value) {
  if (typeof window !== "undefined" && window.CSS?.escape) {
    return window.CSS.escape(String(value));
  }
  return String(value).replaceAll('"', '\\"');
}

/// Formatea un número con locale es-UY y hasta 5 cifras significativas.
export function format(value) {
  return Number(value).toLocaleString("es-UY", { maximumSignificantDigits: 5 });
}

/// Formatea un timestamp (ISO o Date) como fecha y hora cortas en locale es-UY.
export function formatDate(value) {
  return new Date(value).toLocaleString("es-UY", {
    dateStyle: "short",
    timeStyle: "short",
  });
}

/// Agrupa los elementos de `items` por la clave que devuelve `keyFn`; las claves
/// nulas o vacías caen en el grupo "-".
export function groupBy(items, keyFn) {
  return items.reduce((groups, item) => {
    const key = keyFn(item) || "-";
    groups[key] ??= [];
    groups[key].push(item);
    return groups;
  }, {});
}

/// Traduce el tipo de grupo a su etiqueta legible (cualquier valor distinto de
/// "recuperacion" se muestra como "Regular").
export function renderGroupType(value) {
  return value === "recuperacion" ? "Recuperacion" : "Regular";
}

/// Convierte los campos crudos de una escala (strings de un formulario) al payload
/// del API: `step` siempre numérico; el resto de los campos opcionales pasan a número
/// o `null` si vienen vacíos.
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
