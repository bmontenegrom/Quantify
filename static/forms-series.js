import { state } from "./state.js";
import { measurementFields, practiceSelect } from "./dom.js";
import { postJson } from "./api.js";
import { escapeHtml, symbolHtml, unitHtml, format, prefixFactor, pointPower, flowRate, compatibleInstruments, canReview, hasUncertainty, analysisSections } from "./lib.js";
import { SERIES_LIVE_COLUMNS } from "./constants.js";
import { definitionParts, definitionSections, formPracticeVersion, givenValueInputHtml, groupBySections, prefixSelectHtml, renderReplicaInput, populateScaleOptions, replicaInputHtml, cellReplicaValues } from "./forms-shared.js";
import { chronoHelperSectionHtml, wireChronoHelpers } from "./forms-chrono.js";
import { quantityNameHtml, wireRemoveReplica, collectMeasurements, finalResultRowHtml } from "./forms.js";

export const SERIES_INITIAL_ROWS = 3;

export function renderSeriesTable(definition) {
  const cols = definition.quantities.filter((q) => q.per_point && !q.is_given);
  const shared = definition.quantities.filter((q) => !q.per_point || q.is_given);
  const liveCols = SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [];
  const header = cols
    .map((q) => `<th data-quantity-id="${escapeHtml(q.id)}">${symbolHtml(q.symbol)}${q.unit ? ` <span class="submission-meta">(${unitHtml(q.unit)})</span>` : ""}</th>`)
    .join("") + liveCols
    .map((c) => `<th>${symbolHtml(c.symbol)}${c.unit ? ` <span class="submission-meta">(${unitHtml(c.unit)})</span>` : ""}</th>`)
    .join("") + seriesPointResultCols()
    .map((p) => `<th>${symbolHtml(p.symbol)}${p.unit ? ` <span class="submission-meta">(${unitHtml(p.unit)})</span>` : ""}</th>`)
    .join("");
  const body = Array.from({ length: SERIES_INITIAL_ROWS }, () => seriesRowHtml(cols)).join("");
  const sections = definitionSections(definition);
  let sharedSection = "";
  const seriesPart = sections?.find((sec) => sec.series)?.id;
  const seriesSectionAttr = seriesPart ? ` data-section="${escapeHtml(seriesPart)}"` : "";
  if (sections && shared.length) {
    const { grouped, rest } = groupBySections(
      shared,
      sections.filter((sec) => !sec.series),
    );
    const finals = (definition.results ?? []).filter((r) => r.is_final);
    const blocks = grouped
      .filter(({ rows }) => rows.length)
      .map(({ sec, rows }) => {
        const secAttr = sec.id ? ` data-section="${escapeHtml(sec.id)}"` : "";
        const secFinals = finals.filter((r) => (sec.results ?? []).includes(r.symbol));
        const finalsHtml = secFinals.length
          ? `<h5 class="measurement-section-subtitle">Resultado final <span class="submission-meta">— opcional</span></h5>
             ${secFinals.map((r) => finalResultRowHtml(r, sec.id ?? null)).join("")}`
          : "";
        return `<div class="shared-quantities measurement-section"${secAttr}><h4>${escapeHtml(sec.title)}</h4>${rows.map((q) => sharedRowHtml(q)).join("")}${finalsHtml}</div>`;
      });
    if (rest.length) {
      blocks.push(`<div class="shared-quantities"><h4>Medidas</h4>${rest.map((q) => sharedRowHtml(q)).join("")}</div>`);
    }
    sharedSection = blocks.join("");
  } else if (shared.length) {
    sharedSection = `<div class="shared-quantities"><h4>Medidas</h4>${shared.map((q) => sharedRowHtml(q)).join("")}</div>`;
  }
  const partsNote = definitionParts(definition).length
    ? `<p class="submission-meta">La entrega es única e incluye todas las partes: completá cada pestaña antes de entregar.</p>`
    : "";
  const hasReplicatedTime = [...cols, ...shared].some((q) => q.repeated && q.quantity === "tiempo");
  const chronoHelper = hasReplicatedTime ? chronoHelperSectionHtml() : "";
  measurementFields.innerHTML = `
    ${partsNote}
    ${sharedSection}
    <div${seriesSectionAttr}>
      ${chronoHelper}
      <p class="submission-meta">Cargá un punto por fila. Las filas incompletas se ignoran. ${
        definition.analysis_kind === "curva"
          ? "Hacen falta al menos 2 puntos para graficar la curva."
          : "Hacen falta al menos 2 puntos para el ajuste."
      }</p>
      <div class="data-table-wrap">
        <table class="series-table data-table">
          <thead><tr>${header}<th></th></tr></thead>
          <tbody>${body}</tbody>
        </table>
      </div>
      <button type="button" class="add-series-row">＋ agregar punto</button>
      <section class="series-preview panel" aria-live="polite"></section>
    </div>
  `;
  measurementFields.querySelectorAll(".shared-quantities .measurement-row:not([data-final-result])").forEach((row) => {
    if (row.dataset.isGiven === "1") return;
    const inst = row.querySelector(".measure-instrument");
    if (inst) {
      inst.addEventListener("change", () => populateScaleOptions(row));
      populateScaleOptions(row);
    }
    wireRemoveReplica(row);
  });
  measurementFields.querySelector(".add-series-row").addEventListener("click", () => {
    measurementFields.querySelector(".series-table tbody").insertAdjacentHTML("beforeend", seriesRowHtml(cols));
    wireSeriesRemove();
    updateSeriesLive();
    schedulePreview();
  });
  wireSeriesRemove();

  let previewTimer = null;
  const schedulePreview = () => {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(updateRegressionPreview, 350);
  };
  measurementFields.querySelector(".series-table").addEventListener("input", (e) => {
    if (
      e.target.classList.contains("series-value") ||
      e.target.classList.contains("series-replica") ||
      e.target.classList.contains("prefix-select")
    ) {
      updateSeriesMeans();
      updateSeriesLive();
      schedulePreview();
    }
  });
  measurementFields.querySelector(".series-table").addEventListener("change", () => {
    updateSeriesMeans();
    updateSeriesLive();
    schedulePreview();
  });
  measurementFields.querySelectorAll(".shared-quantities").forEach((sharedEl) => {
    const refresh = () => {
      schedulePreview();
      updateSeriesLive();
    };
    sharedEl.addEventListener("input", refresh);
    sharedEl.addEventListener("change", refresh);
  });
  updateSeriesMeans();
  updateSeriesLive();
  wireChronoHelpers();
}

export function refreshSeries() {
  updateSeriesMeans();
  updateSeriesLive();
  updateRegressionPreview();
}

async function updateRegressionPreview() {
  const container = measurementFields.querySelector(".series-preview");
  if (!container) return;
  const measurements = collectMeasurements();
  const points = measurements.reduce(
    (n, m) => Math.max(n, m.point_replicas?.length ?? m.values.length),
    0,
  );
  if (points < 2) {
    container.innerHTML = `<p class="submission-meta">Cargá al menos 2 puntos completos para ver la vista previa.</p>`;
    return;
  }
  try {
    const full = await postJson(
      `/api/practices/${encodeURIComponent(practiceSelect.value)}/analyze-preview`,
      {
        measurements,
        submission_id:
          state.editingPracticeId === practiceSelect.value ? state.editingSubmissionId : null,
        practice_version: formPracticeVersion(practiceSelect.value),
      }
    );
    const analysis = analysisSections(full)
      .map((s) => s.analysis)
      .find((a) => a.regression || a.scatters?.length) ?? full;
    if (analysis.regression) {
      const { regressionMarkup } = await import("./analysis.js");
      container.innerHTML = `<h4>Vista previa del ajuste</h4>${regressionMarkup(analysis.regression)}`;
      return;
    }
    const scatters = analysis.scatters ?? [];
    if (scatters.length) {
      const { scatterMarkup, derivedBlockMarkup } = await import("./analysis.js");
      const blocks = scatters
        .map((s) => {
          const heading = scatters.length > 1
            ? `<h5>${escapeHtml(s.y_label)} vs ${escapeHtml(s.x_label)}${s.x_log ? " (x log)" : ""}</h5>`
            : "";
          return `${heading}${scatterMarkup(s)}`;
        })
        .join("");
      const title = scatters.length > 1 ? "Vista previa de las curvas" : "Vista previa de la curva";
      const derivedHtml = canReview(state.user) ? derivedBlockMarkup(analysis.derived ?? []) : "";
      container.innerHTML = `<h4>${title}</h4>${blocks}${derivedHtml}`;
    } else {
      container.innerHTML = "";
    }
  } catch (error) {
    const stale = error.message?.includes("recargá");
    container.innerHTML = `<p class="submission-meta">${stale ? escapeHtml(error.message) : "No se pudo calcular la vista previa con los datos actuales."}</p>`;
  }
}

export function seriesRowHtml(cols) {
  const cells = cols
    .map((q) => {
      const n = q.repeated ? Number(q.replicas_per_point) || 0 : 0;
      if (n > 0) {
        const inputs = Array.from({ length: n }, (_, k) => replicaInputHtml(q.id, k)).join("");
        return `<td class="series-cell series-cell--replicas" style="--replicas: ${n}"><div class="series-input-wrap">${prefixSelectHtml()}<div class="series-replica-group">${inputs}</div></div><span class="series-mean submission-meta">x̄ —</span></td>`;
      }
      return `<td class="series-cell"><div class="series-input-wrap">${prefixSelectHtml()}<input class="series-value" type="number" step="any" data-quantity-id="${escapeHtml(q.id)}" placeholder="valor" /></div></td>`;
    })
    .join("");
  const liveCells = (SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [])
    .map((c) => `<td class="series-live" data-live-symbol="${escapeHtml(c.symbol)}"><span class="series-live-value submission-meta">—</span></td>`)
    .join("");
  const pointResultCells = seriesPointResultCols()
    .map((p) => `<td class="series-point-result-cell"><input class="series-point-result" data-symbol="${escapeHtml(p.symbol)}" type="number" step="any" placeholder="${escapeHtml(p.symbol)}" /></td>`)
    .join("");
  return `<tr class="series-row">${cells}${liveCells}${pointResultCells}<td><button type="button" class="remove-series-row" title="Quitar">✕</button></td></tr>`;
}

function seriesPointResultCols() {
  return state.practiceForm?.definition?.point_results ?? [];
}

export function seriesRowComplete(row) {
  return [...row.querySelectorAll(".series-cell")].every((cell) => {
    if (cell.querySelector(".series-replica")) {
      const reps = cellReplicaValues(cell);
      return reps.length > 0 && reps.every(Number.isFinite);
    }
    const raw = cell.querySelector(".series-value").value.trim();
    return raw !== "" && Number.isFinite(Number(raw));
  });
}

export function collectSeriesPointResults() {
  const out = [];
  let k = -1;
  measurementFields.querySelectorAll(".series-row").forEach((row) => {
    if (!seriesRowComplete(row)) return;
    k += 1;
    row.querySelectorAll(".series-point-result").forEach((input) => {
      const raw = input.value.trim();
      if (raw === "") return;
      const value = Number(raw);
      if (Number.isFinite(value)) out.push({ symbol: `${input.dataset.symbol}#${k}`, value, u_expanded: null });
    });
  });
  return out;
}

function sharedRowHtml(q) {
  if (q.is_given) {
    const uField = !hasUncertainty(q)
      ? ""
      : `<label>Incertidumbre U (expandida)
            <div class="replica-input-wrap">${prefixSelectHtml()}<input class="measure-given-u" type="number" step="any" min="0" placeholder="U" /><span class="replica-unit">${unitHtml(q.unit)}</span></div>
          </label>`;
    return `
      <fieldset class="measurement-row measurement-row--given" data-quantity-id="${escapeHtml(q.id)}" data-is-given="1">
        <legend>${quantityNameHtml(q)}</legend>
        <div class="form-grid">
          <label>Valor
            <div class="replica-input-wrap">${givenValueInputHtml(q)}<span class="replica-unit">${unitHtml(q.unit)}</span></div>
          </label>
          ${uField}
        </div>
      </fieldset>`;
  }
  const instruments = state.practiceForm?.instruments ?? [];
  const options = compatibleInstruments(instruments, q.quantity);
  const instrumentOptions = [`<option value="">— sin instrumento —</option>`]
    .concat(options.map((i) => `<option value="${escapeHtml(i.id)}">${escapeHtml(i.name)}</option>`))
    .join("");
  return `
    <fieldset class="measurement-row" data-quantity-id="${escapeHtml(q.id)}">
      <legend>${quantityNameHtml(q)}</legend>
      <div class="measure-body">
        <div class="measure-selectors">
          <select class="measure-instrument" title="Instrumento" aria-label="Instrumento">${instrumentOptions}</select>
          <select class="measure-scale" title="Escala" aria-label="Escala"><option value="">sin escala</option></select>
        </div>
        <div class="measure-sep"></div>
        <div class="measure-right">
          <div class="measure-values" data-repeated="0">${renderReplicaInput(q.unit, q.default_value)}</div>
        </div>
      </div>
    </fieldset>`;
}

function seriesCellValue(row, quantityId) {
  const input = row.querySelector(`.series-value[data-quantity-id="${CSS.escape(quantityId)}"]`);
  if (!input) return NaN;
  const raw = input.value.trim();
  if (raw === "") return NaN;
  const factor = prefixFactor(input.closest(".series-cell").querySelector(".prefix-select").value);
  return Number(raw) * factor;
}

function replicaValueAt(row, quantityId, k) {
  const cell = row.querySelector(`.series-replica[data-quantity-id="${CSS.escape(quantityId)}"]`)?.closest(".series-cell--replicas");
  const input = cell?.querySelectorAll(".series-replica")[k];
  if (!input) return NaN;
  const raw = input.value.trim();
  if (raw === "") return NaN;
  return Number(raw) * prefixFactor(cell.querySelector(".prefix-select").value);
}

function fluidosCaudalLiveValue(symbol, row, idBySymbol) {
  const q1 = flowRate(replicaValueAt(row, idBySymbol.get("V") ?? "", 0), replicaValueAt(row, idBySymbol.get("t") ?? "", 0));
  const q2 = flowRate(replicaValueAt(row, idBySymbol.get("V") ?? "", 1), replicaValueAt(row, idBySymbol.get("t") ?? "", 1));
  if (symbol === "Q_1") return q1;
  if (symbol === "Q_2") return q2;
  if (symbol === "Q_medio") return Number.isFinite(q1) && Number.isFinite(q2) ? (q1 + q2) / 2 : NaN;
  return NaN;
}

function sharedSingleValue(quantityId) {
  const replica = measurementFields
    .querySelector(`.measurement-row[data-quantity-id="${CSS.escape(quantityId)}"]`)
    ?.querySelector(".replica");
  const raw = replica?.querySelector(".measure-value")?.value.trim();
  if (!raw) return NaN;
  return Number(raw) * prefixFactor(replica.querySelector(".prefix-select").value);
}

function rowReplicaMean(row, quantityId) {
  const cell = row
    .querySelector(`.series-replica[data-quantity-id="${CSS.escape(quantityId)}"]`)
    ?.closest(".series-cell--replicas");
  const reps = cell ? cellReplicaValues(cell) : [];
  return reps.length ? reps.reduce((a, b) => a + b, 0) / reps.length : NaN;
}

function viscosidadVelocityLiveValue(row, idBySymbol) {
  const dx = sharedSingleValue(idBySymbol.get("dx") ?? "");
  const tMean = rowReplicaMean(row, idBySymbol.get("t") ?? "");
  return Number.isFinite(dx) && Number.isFinite(tMean) && tMean !== 0 ? dx / tMean : NaN;
}

export function updateSeriesLive() {
  const liveCols = SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [];
  if (!liveCols.length) return;
  const quantities = state.practiceForm?.definition?.quantities ?? [];
  const idBySymbol = new Map(quantities.map((q) => [q.symbol, q.id]));
  const practice = practiceSelect.value;
  measurementFields.querySelectorAll(".series-row").forEach((row) => {
    for (const col of liveCols) {
      const cell = row.querySelector(`.series-live[data-live-symbol="${CSS.escape(col.symbol)}"]`);
      const out = cell?.querySelector(".series-live-value");
      if (!out) continue;
      let value;
      if (practice === "fluidos-1") {
        value = fluidosCaudalLiveValue(col.symbol, row, idBySymbol);
      } else if (practice === "viscosidad") {
        value = viscosidadVelocityLiveValue(row, idBySymbol);
      } else {
        const args = col.inputs.map((sym) => seriesCellValue(row, idBySymbol.get(sym) ?? ""));
        value = args.every(Number.isFinite) ? pointPower(...args) : NaN;
      }
      out.textContent = Number.isFinite(value) ? format(value) : "—";
    }
  });
}

export function updateSeriesMeans() {
  measurementFields.querySelectorAll(".series-cell--replicas").forEach((cell) => {
    const meanEl = cell.querySelector(".series-mean");
    if (!meanEl) return;
    const reps = cellReplicaValues(cell);
    const valid = reps.filter((n) => Number.isFinite(n));
    if (valid.length === 0) {
      meanEl.textContent = "x̄ —";
      return;
    }
    const mean = valid.reduce((a, b) => a + b, 0) / valid.length;
    meanEl.textContent = `x̄ ${format(mean)} (n=${valid.length})`;
  });
}

export function wireSeriesRemove() {
  const rows = measurementFields.querySelectorAll(".series-row");
  measurementFields.querySelectorAll(".remove-series-row").forEach((btn) => {
    btn.onclick = () => {
      if (measurementFields.querySelectorAll(".series-row").length <= 1) return;
      btn.closest(".series-row").remove();
      wireSeriesRemove();
    };
    btn.style.visibility = rows.length <= 1 ? "hidden" : "visible";
  });
}
