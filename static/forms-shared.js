import { state } from "./state.js";
import { escapeHtml, unitHtml, SI_PREFIXES, prefixFactor } from "./lib.js";

export function formPracticeVersion(practiceId) {
  const definition = state.practiceForm?.definition;
  return definition?.practice_id === practiceId ? definition.current_version ?? null : null;
}

export function definitionSections(definition) {
  if (!definition?.sections?.length) return null;
  const sections = definition.sections.map((s) => ({
    id: s.part_id ?? undefined,
    title: s.title,
    help_text: s.help_text,
    symbols: s.quantities,
    results: s.results,
  }));
  const pointBased = definition.analysis_kind === "regresion_lineal" || definition.analysis_kind === "curva";
  const seriesSymbols = new Set(
    pointBased ? definition.quantities.filter((q) => q.per_point && !q.is_given).map((q) => q.symbol) : [],
  );
  const seriesPart = seriesPartId(definition, seriesSymbols);
  if (seriesPart) sections.push({ id: seriesPart, series: true });
  return sections;
}

function seriesPartId(definition, seriesSymbols) {
  const chosen = (definition.parts ?? []).find((p) => p.effective_kind && p.effective_kind !== "estadistico");
  if (chosen) return chosen.id;
  return definition.sections.find((s) => s.part_id && s.quantities.some((q) => seriesSymbols.has(q)))?.part_id;
}

export function definitionParts(definition) {
  const finals = new Set((definition?.results ?? []).filter((r) => r.is_final).map((r) => r.symbol));
  const used = new Set(
    (definition?.sections ?? [])
      .filter((s) => s.part_id && (s.quantities.length || s.results.some((r) => finals.has(r))))
      .map((s) => s.part_id),
  );
  const series = (definition?.parts ?? []).find((p) => p.effective_kind && p.effective_kind !== "estadistico");
  if (series && (definition?.quantities ?? []).some((q) => q.per_point && !q.is_given)) used.add(series.id);
  return (definition?.parts ?? []).filter((p) => used.has(p.id)).map((p) => ({ id: p.id, label: p.title }));
}

export function formHelpHtml(text, attrs = "") {
  return text ? `<p class="form-help"${attrs}>${escapeHtml(text)}</p>` : "";
}

export function partHelpHtml(definition) {
  const tabs = new Set(definitionParts(definition).map((p) => p.id));
  return (definition.parts ?? [])
    .filter((p) => tabs.has(p.id))
    .map((p) => formHelpHtml(p.help_text, ` data-section="${escapeHtml(p.id)}"`))
    .join("");
}

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

export function givenValueInputHtml(q) {
  const n = Number(q.slot_count);
  const attrs = q.slot_count ? `min="1" max="${n}" step="1"` : `step="any"`;
  const placeholder = q.slot_count ? `ranura 1 a ${n}` : "valor";
  const value = q.default_value != null ? ` value="${escapeHtml(String(q.default_value))}"` : "";
  return `${q.slot_count ? `<span class="prefix-placeholder" aria-hidden="true"></span>` : prefixSelectHtml()}<input class="measure-given-value" type="number" ${attrs} placeholder="${placeholder}"${value} />`;
}

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

export function cellReplicaValues(cell) {
  const factor = prefixFactor(cell.querySelector(".prefix-select").value);
  return [...cell.querySelectorAll(".series-replica")]
    .map((input) => input.value.trim())
    .filter((raw) => raw !== "")
    .map((raw) => Number(raw) * factor);
}
