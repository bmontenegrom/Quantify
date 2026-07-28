import { state } from "./state.js";
import { measurementFields, practiceSelect } from "./dom.js";
import { postJson } from "./api.js";
import { escapeHtml, symbolHtml, unitHtml, format, prefixFactor, pointPower, flowRate, compatibleInstruments, canReview, hasUncertainty } from "./lib.js";
import { PRACTICE_SECTIONS, PRACTICE_PARTS, SERIES_LIVE_COLUMNS } from "./constants.js";
import { groupBySections, prefixSelectHtml, renderReplicaInput, populateScaleOptions, replicaInputHtml, cellReplicaValues } from "./forms-shared.js";
import { chronoHelperSectionHtml, wireChronoHelpers } from "./forms-chrono.js";
import { quantityNameHtml, wireRemoveReplica, collectMeasurements } from "./forms.js";

export function renderSeriesTable(definition) {
  // Motor E: separa las magnitudes que se miden por punto (van en la serie) de los escalares
  // compartidos (datos de cátedra / medida única), que se cargan una sola vez.
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
  const INITIAL_ROWS = 3;
  const body = Array.from({ length: INITIAL_ROWS }, () => seriesRowHtml(cols)).join("");
  // Secciones temáticas (PRACTICE_SECTIONS): agrupa los escalares por sección, con `data-section`
  // para que las tabs de partes las muestren/oculten. Sin secciones, un solo bloque como siempre.
  const sections = PRACTICE_SECTIONS[practiceSelect.value];
  let sharedSection = "";
  let seriesSectionAttr = "";
  if (sections && shared.length) {
    // La sección `series: true` no agrupa magnitudes: solo marca dónde va la tabla por punto.
    const seriesSec = sections.find((sec) => sec.series);
    if (seriesSec?.id) seriesSectionAttr = ` data-section="${escapeHtml(seriesSec.id)}"`;
    const { grouped, rest } = groupBySections(
      shared,
      sections.filter((sec) => !sec.series),
    );
    const blocks = grouped
      .filter(({ rows }) => rows.length)
      .map(({ sec, rows }) => {
        const secAttr = sec.id ? ` data-section="${escapeHtml(sec.id)}"` : "";
        return `<div class="shared-quantities measurement-section"${secAttr}><h4>${escapeHtml(sec.title)}</h4>${rows.map((q) => sharedRowHtml(q)).join("")}</div>`;
      });
    if (rest.length) {
      blocks.push(`<div class="shared-quantities"><h4>Medidas</h4>${rest.map((q) => sharedRowHtml(q)).join("")}</div>`);
    }
    sharedSection = blocks.join("");
  } else if (shared.length) {
    sharedSection = `<div class="shared-quantities"><h4>Medidas</h4>${shared.map((q) => sharedRowHtml(q)).join("")}</div>`;
  }
  const partsNote = PRACTICE_PARTS[practiceSelect.value]
    ? `<p class="submission-meta">La entrega es única e incluye todas las partes: completá cada pestaña antes de entregar.</p>`
    : "";
  // Si alguna columna es una serie de tiempos con réplicas (p. ej. tiempo de caída en
  // viscosidad), ofrecemos un cronómetro de apoyo suelto arriba de la tabla.
  const hasReplicatedTime = [...cols, ...shared].some((q) => q.repeated && q.quantity === "tiempo");
  const chronoHelper = hasReplicatedTime ? chronoHelperSectionHtml() : "";
  measurementFields.innerHTML = `
    ${chronoHelper}
    ${partsNote}
    ${sharedSection}
    <div${seriesSectionAttr}>
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
  // Wiring de las filas compartidas de medida única: instrumento → escalas compatibles.
  measurementFields.querySelectorAll(".shared-quantities .measurement-row").forEach((row) => {
    if (row.dataset.isGiven === "1") return;
    const inst = row.querySelector(".measure-instrument");
    if (inst) {
      inst.addEventListener("change", () => populateScaleOptions(row));
      populateScaleOptions(row);
    }
    // Oculta el botón ✕ de la única réplica (medida única: no se quitan ni agregan réplicas).
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
  // Los escalares compartidos también entran en las fórmulas de eje: refrescá la vista previa al
  // editarlos (sus filas viven fuera de la tabla de la serie; puede haber varios bloques).
  // Además, algunas columnas en vivo dependen de una compartida (viscosidad: v_medio = dx/t̄).
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
    const analysis = await postJson(
      `/api/practices/${encodeURIComponent(practiceSelect.value)}/analyze-preview`,
      { measurements }
    );
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
      // Solo docentes ven los mensurandos derivados en la vista previa; los alumnos los
      // descubren tras la entrega, cuando el docente habilita results_visible_to_student.
      const derivedHtml = canReview(state.user) ? derivedBlockMarkup(analysis.derived ?? []) : "";
      container.innerHTML = `<h4>${title}</h4>${blocks}${derivedHtml}`;
    } else {
      container.innerHTML = "";
    }
  } catch {
    container.innerHTML = `<p class="submission-meta">No se pudo calcular la vista previa con los datos actuales.</p>`;
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
  // Columnas calculadas en vivo: solo lectura y sin clase `series-cell`, para que
  // collectMeasurements no las cuente como parte del punto.
  const liveCells = (SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [])
    .map((c) => `<td class="series-live" data-live-symbol="${escapeHtml(c.symbol)}"><span class="series-live-value submission-meta">—</span></td>`)
    .join("");
  // Resultado por corrida cargado a mano por el alumno (Motor E): editable, sin clase `series-cell`
  // para que collectMeasurements no lo cuente como medición. Se recolecta como `Re#k` al entregar.
  const pointResultCells = seriesPointResultCols()
    .map((p) => `<td class="series-point-result-cell"><input class="series-point-result" data-symbol="${escapeHtml(p.symbol)}" type="number" step="any" placeholder="${escapeHtml(p.symbol)}" /></td>`)
    .join("");
  return `<tr class="series-row">${cells}${liveCells}${pointResultCells}<td><button type="button" class="remove-series-row" title="Quitar">✕</button></td></tr>`;
}

/** Resultados derivados por punto (Motor E) de la práctica actual; el alumno carga uno por corrida. */
function seriesPointResultCols() {
  return state.practiceForm?.definition?.point_results ?? [];
}

/** ¿La fila de la serie tiene todas sus celdas de medición completas? Mismo criterio que
 *  collectMeasurements (fila incompleta = punto ignorado), para alinear el índice de corrida. */
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

/** Re por corrida cargado en la tabla: `Re#k`, con k = índice entre las filas completas (mismo
 *  orden que las mediciones, para que la comparación empareje con el valor automático). */
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

/// HTML de una fila de escalar compartido (Motor E): dato de cátedra (valor ± U) o medida única
/// (instrumento/escala + un valor). Se cargan una sola vez, fuera de la tabla de la serie.
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
            <div class="replica-input-wrap">${prefixSelectHtml()}<input class="measure-given-value" type="number" step="any" placeholder="valor" /><span class="replica-unit">${unitHtml(q.unit)}</span></div>
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
          <div class="measure-values" data-repeated="0">${renderReplicaInput(q.unit)}</div>
        </div>
      </div>
    </fieldset>`;
}

/** Valor numérico (con prefijo SI aplicado) del input de una magnitud dentro de una fila. */
function seriesCellValue(row, quantityId) {
  const input = row.querySelector(`.series-value[data-quantity-id="${CSS.escape(quantityId)}"]`);
  if (!input) return NaN;
  const raw = input.value.trim();
  if (raw === "") return NaN;
  const factor = prefixFactor(input.closest(".series-cell").querySelector(".prefix-select").value);
  return Number(raw) * factor;
}

/** Valor numérico (con prefijo SI aplicado) de la réplica `k` (0-based) de una magnitud repetida. */
function replicaValueAt(row, quantityId, k) {
  const cell = row.querySelector(`.series-replica[data-quantity-id="${CSS.escape(quantityId)}"]`)?.closest(".series-cell--replicas");
  const input = cell?.querySelectorAll(".series-replica")[k];
  if (!input) return NaN;
  const raw = input.value.trim();
  if (raw === "") return NaN;
  return Number(raw) * prefixFactor(cell.querySelector(".prefix-select").value);
}

/** fluidos-1: Q_1=V1/t1, Q_2=V2/t2, Q_medio=media(Q_1,Q_2) (V y t tienen 2 réplicas cada una). */
function fluidosCaudalLiveValue(symbol, row, idBySymbol) {
  const q1 = flowRate(replicaValueAt(row, idBySymbol.get("V") ?? "", 0), replicaValueAt(row, idBySymbol.get("t") ?? "", 0));
  const q2 = flowRate(replicaValueAt(row, idBySymbol.get("V") ?? "", 1), replicaValueAt(row, idBySymbol.get("t") ?? "", 1));
  if (symbol === "Q_1") return q1;
  if (symbol === "Q_2") return q2;
  if (symbol === "Q_medio") return Number.isFinite(q1) && Number.isFinite(q2) ? (q1 + q2) / 2 : NaN;
  return NaN;
}

/** Valor (con prefijo) de una magnitud compartida de medida única (fuera de la tabla de series). */
function sharedSingleValue(quantityId) {
  const replica = measurementFields
    .querySelector(`.measurement-row[data-quantity-id="${CSS.escape(quantityId)}"]`)
    ?.querySelector(".replica");
  const raw = replica?.querySelector(".measure-value")?.value.trim();
  if (!raw) return NaN;
  return Number(raw) * prefixFactor(replica.querySelector(".prefix-select").value);
}

/** Media de las réplicas de una magnitud repetida dentro de una fila de la serie. */
function rowReplicaMean(row, quantityId) {
  const cell = row
    .querySelector(`.series-replica[data-quantity-id="${CSS.escape(quantityId)}"]`)
    ?.closest(".series-cell--replicas");
  const reps = cell ? cellReplicaValues(cell) : [];
  return reps.length ? reps.reduce((a, b) => a + b, 0) / reps.length : NaN;
}

/** viscosidad: v_medio = dx (compartida) / t̄ (media de réplicas de t del punto). */
function viscosidadVelocityLiveValue(row, idBySymbol) {
  const dx = sharedSingleValue(idBySymbol.get("dx") ?? "");
  const tMean = rowReplicaMean(row, idBySymbol.get("t") ?? "");
  return Number.isFinite(dx) && Number.isFinite(tMean) && tMean !== 0 ? dx / tMean : NaN;
}

/** Recalcula las columnas en vivo (p. ej. P = I²·R, Q = V/t) de cada fila de la tabla de series. */
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

/** Actualiza el promedio (x̄) mostrado en cada celda de réplicas de la tabla de series. */
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
