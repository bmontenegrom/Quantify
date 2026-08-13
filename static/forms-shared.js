import { state } from "./state.js";
import { escapeHtml, unitHtml, SI_PREFIXES, prefixFactor } from "./lib.js";

/** Agrupa `items` (con `.id`/`.symbol`) según `sections[].symbols`, en el mismo orden que las
 *  secciones. Devuelve, por sección, sus `rows` encontrados, y aparte los `items` que no entraron
 *  en ninguna sección (`rest`). Común al render de magnitudes (Motor D) y al de la serie (Motor E),
 *  que solo difieren en cómo pintan cada fila/bloque, no en el matching contra PRACTICE_SECTIONS. */
export function groupBySections(items, sections) {
  const used = new Set();
  const grouped = sections.map((sec) => {
    const rows = (sec.symbols ?? [])
      .map((sym) => items.find((q) => q.symbol === sym))
      .filter(Boolean);
    rows.forEach((q) => used.add(q.id));
    return { sec, rows };
  });
  const rest = items.filter((q) => !used.has(q.id));
  return { grouped, rest };
}

export function prefixSelectHtml() {
  const opts = SI_PREFIXES.map(
    (p) => `<option value="${escapeHtml(p.label)}" ${p.label === "" ? "selected" : ""}>${p.label || "—"}</option>`
  ).join("");
  return `<select class="prefix-select" title="Prefijo SI">${opts}</select>`;
}

/** `defaultValue` (opcional) precarga el campo: lo usan las magnitudes con `default_value` en la
 *  definición (Hidrostática: ranuras vacías de la balanza de Mohr en 0). Las réplicas agregadas a
 *  mano después no lo llevan. */
export function renderReplicaInput(unit, defaultValue = null) {
  const value = defaultValue != null ? ` value="${escapeHtml(String(defaultValue))}"` : "";
  return `
    <div class="replica">
      ${prefixSelectHtml()}
      <input class="measure-value" type="number" step="any" placeholder="valor"${value} data-unit="${escapeHtml(unit)}" />
      <span class="replica-unit">${unitHtml(unit)}</span>
      <button type="button" class="remove-replica" title="Quitar">✕</button>
    </div>
  `;
}

export function populateScaleOptions(row) {
  const instrumentId = row.querySelector(".measure-instrument").value;
  const scaleSelect = row.querySelector(".measure-scale");
  const instrument = state.practiceForm?.instruments.find((i) => i.id === instrumentId);
  const scales = instrument?.scales ?? [];
  scaleSelect.innerHTML = [`<option value="">— sin escala —</option>`]
    .concat(scales.map((s) => `<option value="${escapeHtml(s.id)}">${escapeHtml(s.label)} (${escapeHtml(s.unit)})</option>`))
    .join("");
  if (scales.length === 1) scaleSelect.value = scales[0].id;
}

export function replicaInputHtml(quantityId, k) {
  return `<input class="series-replica" type="number" step="any" data-quantity-id="${escapeHtml(quantityId)}" placeholder="valor ${k + 1}" />`;
}

/** Lee las réplicas no vacías de una celda de réplicas, aplicando el prefijo SI de la celda. */
export function cellReplicaValues(cell) {
  const factor = prefixFactor(cell.querySelector(".prefix-select").value);
  return [...cell.querySelectorAll(".series-replica")]
    .map((input) => input.value.trim())
    .filter((raw) => raw !== "")
    .map((raw) => Number(raw) * factor);
}
