import { state } from "./state.js";
import { escapeHtml, unitHtml, unitText, compatibleInstruments, POINT_KINDS, seriesPart, hasUncertainty, exactNumber } from "./lib.js";

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
  const pointBased = POINT_KINDS.includes(definition.analysis_kind);
  const seriesSymbols = new Set(
    pointBased ? definition.quantities.filter((q) => q.per_point && !q.is_given).map((q) => q.symbol) : [],
  );
  const seriesId = seriesPartId(definition, seriesSymbols);
  if (seriesId) sections.push({ id: seriesId, series: true });
  return sections;
}

function seriesPartId(definition, seriesSymbols) {
  const chosen = seriesPart(definition);
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
  const series = seriesPart(definition);
  if (series && (definition?.quantities ?? []).some((q) => q.per_point && !q.is_given)) used.add(series.id);
  return (definition?.parts ?? []).filter((p) => used.has(p.id)).map((p) => ({ id: p.id, label: p.title }));
}

export function columnSections(definition, sections) {
  const columns = new Set(definition.quantities.filter((q) => q.per_point && !q.is_given).map((q) => q.symbol));
  const shared = new Set(definition.quantities.filter((q) => !q.per_point || q.is_given).map((q) => q.symbol));
  return sections.filter(
    (sec) =>
      !sec.series &&
      (sec.symbols ?? []).some((s) => columns.has(s)) &&
      !sec.symbols.some((s) => shared.has(s)),
  );
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

export function inputUnits(symbol, unit) {
  const units = state.practiceForm?.definition?.resolved?.input_units?.[symbol];
  return units?.length ? units : [{ label: unit ?? "", factor: 1, offset: 0 }];
}

export function quantityUnits(q) {
  return inputUnits(q.symbol, q.unit);
}

export function unitSelectHtml(units, { text = true } = {}) {
  if (units.length <= 1) return text ? `<span class="replica-unit">${unitHtml(units[0]?.label ?? "")}</span>` : "";
  const options = units
    .map(
      (u, i) =>
        `<option value="${i}" data-factor="${u.factor}" data-offset="${u.offset}"${i === 0 ? " selected" : ""}>${escapeHtml(unitText(u.label))}</option>`,
    )
    .join("");
  return `<select class="unit-select" title="Unidad">${options}</select>`;
}

export function unitConversion(wrap) {
  const option = wrap?.querySelector(".unit-select")?.selectedOptions[0];
  return option
    ? { factor: Number(option.dataset.factor), offset: Number(option.dataset.offset) }
    : { factor: 1, offset: 0 };
}

export function toDeclaredUnit(raw, wrap, { uncertainty = false } = {}) {
  const { factor, offset } = unitConversion(wrap);
  return Number((Number(raw) * factor + (uncertainty ? 0 : offset)).toPrecision(15));
}

export function fromDeclaredUnit(value, { factor, offset }, { uncertainty = false } = {}) {
  return Number(((Number(value) - (uncertainty ? 0 : offset)) / factor).toPrecision(15));
}

const UNIT_WRAPS = ".replica-input-wrap, .replica, .series-cell";

function keyedUnitSelects(container) {
  const seen = new Map();
  return [...container.querySelectorAll(".unit-select")].map((select) => {
    const wrap = select.closest(UNIT_WRAPS);
    const holder = select.closest("[data-quantity-id], [data-symbol]") ?? wrap?.querySelector("[data-quantity-id]");
    const id = `${holder?.dataset.quantityId ?? holder?.dataset.symbol ?? ""}|${wrap?.querySelector('[class$="-u"]') ? "u" : "v"}`;
    const n = seen.get(id) ?? 0;
    seen.set(id, n + 1);
    return { key: `${id}|${n}`, select, wrap };
  });
}

export function unitChoices(container) {
  return Object.fromEntries(
    keyedUnitSelects(container)
      .filter(({ select }) => select.selectedIndex > 0)
      .map(({ key, select }) => [key, select.selectedIndex]),
  );
}

export function restoreUnitChoices(container, choices) {
  if (!choices || typeof choices !== "object") return;
  for (const { key, select, wrap } of keyedUnitSelects(container)) {
    const index = choices[key];
    if (!(Number.isInteger(index) && index > 0 && index < select.options.length) || !wrap) continue;
    select.selectedIndex = index;
    const conversion = unitConversion(wrap);
    wrap.querySelectorAll('input[type="number"]').forEach((input) => {
      if (input.value.trim() === "") return;
      input.value = fromDeclaredUnit(input.value, conversion, { uncertainty: [...input.classList].some((c) => c.endsWith("-u")) });
    });
    select.dispatchEvent(new Event("input", { bubbles: true }));
  }
}

export function isFixedGiven(q) {
  return q.is_given && q.fixed && q.default_value != null;
}

export function fixedGivenHtml(q, legend) {
  const u = hasUncertainty(q) ? q.default_u : null;
  const text = `${exactNumber(q.default_value)}${u != null ? ` ± ${exactNumber(u)}` : ""}`;
  return `
    <fieldset class="measurement-row measurement-row--given" data-quantity-id="${escapeHtml(q.id)}" data-is-given="1"
      data-fixed-value="${escapeHtml(String(q.default_value))}" data-fixed-u="${u ?? ""}">
      <legend>${legend}</legend>
      <p class="fixed-given">${escapeHtml(text)}${q.unit ? ` ${unitHtml(q.unit)}` : ""}</p>
    </fieldset>`;
}

export function givenValueInputHtml(q, inputClass = "measure-given-value") {
  const n = Number(q.slot_count);
  const attrs = q.slot_count ? `min="1" max="${n}" step="1"` : `step="any"`;
  const placeholder = q.slot_count ? `ranura 1 a ${n}` : "valor";
  const value = q.default_value != null ? ` value="${escapeHtml(String(q.default_value))}"` : "";
  return `<input class="${inputClass}" type="number" ${attrs} placeholder="${placeholder}"${value} />`;
}

export function renderReplicaInput(units, defaultValue = null) {
  const value = defaultValue != null ? ` value="${escapeHtml(String(defaultValue))}"` : "";
  return `
    <div class="replica">
      <input class="measure-value" type="number" step="any" placeholder="valor"${value} />
      ${unitSelectHtml(units)}
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

export function instrumentSelectorsHtml(instruments, q, preselectChrono = false) {
  const options = compatibleInstruments(instruments, q.quantity);
  const preferred = preselectChrono ? options.find((i) => /cron[oó]metro/i.test(i.name)) ?? options[0] : null;
  const instrumentOptions = [`<option value="">— sin instrumento —</option>`]
    .concat(options.map((i) => `<option value="${escapeHtml(i.id)}"${preferred && i.id === preferred.id ? " selected" : ""}>${escapeHtml(i.name)}</option>`))
    .join("");
  return `
    <div class="measure-selectors">
      <select class="measure-instrument" title="Instrumento" aria-label="Instrumento">${instrumentOptions}</select>
      <select class="measure-scale" title="Escala" aria-label="Escala"><option value="">sin escala</option></select>
    </div>`;
}

export function wireInstrumentSelectors(root) {
  root.querySelectorAll(".measure-selectors").forEach((selectors) => {
    selectors.querySelector(".measure-instrument").addEventListener("change", () => populateScaleOptions(selectors));
    populateScaleOptions(selectors);
  });
}

export function retiredInstrumentNote(instruments, e) {
  if (!e.instrument_id) return null;
  const instrument = instruments.find((i) => i.id === e.instrument_id);
  if (!instrument) {
    return "El instrumento con el que cargaste esta medición ya no está disponible en el curso para esta magnitud. Elegí otro: si la guardás sin instrumento, no se suma su incertidumbre.";
  }
  const scales = instrument.scales ?? [];
  if (e.scale_id && !scales.some((s) => s.id === e.scale_id)) {
    if (scales.length === 0) {
      return "El instrumento ya no tiene escalas: la medición se guarda sin escala y no se suma su incertidumbre.";
    }
    if (scales.length === 1) {
      return "La escala con la que cargaste esta medición ya no está en el instrumento; se eligió la única que queda. Revisá que sea la que usaste.";
    }
    return "La escala con la que cargaste esta medición ya no está en el instrumento. Elegí otra: si la guardás sin escala, no se suma su incertidumbre.";
  }
  return null;
}

export function replicaInputHtml(quantityId, k) {
  return `<input class="series-replica" type="number" step="any" data-quantity-id="${escapeHtml(quantityId)}" placeholder="valor ${k + 1}" />`;
}

export function cellReplicaValues(cell) {
  return [...cell.querySelectorAll(".series-replica")]
    .map((input) => input.value.trim())
    .filter((raw) => raw !== "")
    .map((raw) => toDeclaredUnit(raw, cell));
}
