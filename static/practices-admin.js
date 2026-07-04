import { state } from "./state.js";
import { practiceCatalog, practiceWorkspace, practiceStatus } from "./dom.js";
import { fetchJson, postJson, deleteJson, errorText } from "./api.js";
import { escapeHtml, symbolHtml, inlineMathHtml, unitHtml, analysisKindLabel } from "./lib.js";
import { selectView } from "./navigation.js";

/// `point_results`, `aggregates` e `intermediates` son, en el admin, la misma forma —
/// símbolo/nombre/unidad/fórmula con alta/edición/borrado — y solo cambian de endpoint, de clave
/// de estado "en edición" y de textos. `curves` (otras columnas, sin nombre/unidad, con
/// reordenamiento) y `quantities`/`results` (más campos: réplicas, flags, tolerancia) no entran
/// acá: forzarlas al mismo molde pediría un form-builder genérico para ganar poco.
/** `true` si la fila `(kind, id)` es la que está actualmente en edición (a lo sumo una por vez
 *  en todo el workspace de la práctica: ver `state.editing` en state.js). */
function isEditing(kind, id) {
  return state.editing.kind === kind && state.editing.id === id;
}

const SYMBOL_FORMULA_KINDS = {
  pointResult: {
    kind: "pointResult",
    urlSegment: "point-results",
    idParam: "pid",
    dataAttr: "point-result",
    listOf: (def) => def?.point_results ?? [],
    emptyText: "Sin magnitudes derivadas por punto.",
    symbolPlaceholder: "Re",
    namePlaceholder: "Número de Reynolds",
    formulaPlaceholder: "2*rho*Q / (pi*mu*R)",
    statusCreate: "Derivada por punto agregada",
    statusUpdate: "Derivada por punto actualizada",
    statusDelete: "Derivada por punto eliminada",
    confirmDelete: "¿Eliminar esta magnitud derivada por punto? Esta accion no se puede deshacer.",
  },
  aggregate: {
    kind: "aggregate",
    urlSegment: "aggregates",
    idParam: "aid",
    dataAttr: "aggregate",
    listOf: (def) => def?.aggregates ?? [],
    emptyText: "Sin mensurandos agregados.",
    symbolPlaceholder: "Re_medio",
    namePlaceholder: "Reynolds medio",
    formulaPlaceholder: "(Re_max + Re_min) / 2",
    statusCreate: "Mensurando agregado agregado",
    statusUpdate: "Mensurando agregado actualizado",
    statusDelete: "Mensurando agregado eliminado",
    confirmDelete: "¿Eliminar este mensurando agregado? Esta accion no se puede deshacer.",
  },
  intermediate: {
    kind: "intermediate",
    urlSegment: "intermediates",
    idParam: "iid",
    dataAttr: "intermediate",
    listOf: (def) => def?.intermediates ?? [],
    emptyText: "Sin magnitudes intermedias.",
    symbolPlaceholder: "Q",
    namePlaceholder: "Caudal medio",
    formulaPlaceholder: "V / t",
    statusCreate: "Intermedia agregada",
    statusUpdate: "Intermedia actualizada",
    statusDelete: "Intermedia eliminada",
    confirmDelete: "¿Eliminar esta magnitud intermedia? Esta accion no se puede deshacer.",
  },
};

function renderSymbolFormulaForm(kind, item, practiceId) {
  const formId = item ? `edit-${kind.dataAttr}-form` : `new-${kind.dataAttr}-form`;
  const formAttr = item ? `data-edit-${kind.dataAttr}-form data-${kind.idParam}="${escapeHtml(item.id)}"` : "";
  const v = (f) => (item ? escapeHtml(String(item[f] ?? "")) : "");
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${item ? `<input name="${kind.idParam}" type="hidden" value="${escapeHtml(item.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="${kind.symbolPlaceholder}" /></label>
      <label>Nombre <input name="name" value="${v("name")}" placeholder="${kind.namePlaceholder}" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="(vacío = adimensional)" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="${kind.formulaPlaceholder}" /></label>
      <div class="detail-actions">
        <button type="submit">${item ? "Guardar" : "Agregar"}</button>
        ${item ? `<button type="button" data-cancel-${kind.dataAttr}>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderSymbolFormulaList(kind, def, practiceId) {
  const items = kind.listOf(def);
  if (items.length === 0) return `<p class="submission-meta">${kind.emptyText}</p>`;
  const rows = items.flatMap((item) => {
    const editing = isEditing(kind.kind, item.id);
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${symbolHtml(item.symbol)}</strong> <span class="submission-meta">${inlineMathHtml(item.name)}${item.unit ? ` (${unitHtml(item.unit)})` : " (adimensional)"}</span></td>
        <td><code>${escapeHtml(item.formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-edit-${kind.dataAttr} data-${kind.idParam}="${escapeHtml(item.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-${kind.dataAttr} data-${kind.idParam}="${escapeHtml(item.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="3" class="scale-edit-cell">${renderSymbolFormulaForm(kind, item, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });
  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead><tr><th>Símbolo</th><th>Fórmula</th><th>Acciones</th></tr></thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function symbolFormulaPayloadFromForm(form) {
  const raw = Object.fromEntries(new FormData(form).entries());
  return {
    symbol: raw.symbol,
    name: raw.name || "",
    unit: raw.unit || "",
    formula: raw.formula,
  };
}

async function saveNewSymbolFormulaRow(kind, event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/${kind.urlSegment}`, symbolFormulaPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = kind.statusCreate;
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditSymbolFormulaRow(kind, event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const id = form.querySelector(`[name="${kind.idParam}"]`).value;
  try {
    await postJson(`/api/practices/${practiceId}/${kind.urlSegment}/${id}`, symbolFormulaPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = kind.statusUpdate;
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deleteSymbolFormulaRow(kind, id, practiceId) {
  if (!window.confirm(kind.confirmDelete)) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/${kind.urlSegment}/${id}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = kind.statusDelete;
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

export function renderPracticesPage() {
  renderPracticeDirectory();
  if (!practiceWorkspace) return;

  const practice = state.practices.find((p) => p.id === state.activePracticeId);
  if (!practice) {
    practiceWorkspace.innerHTML = "";
    practiceWorkspace.classList.add("hidden");
    practiceCatalog?.closest(".panel")?.classList.remove("hidden");
    return;
  }

  const def = state.practiceDefinition;
  // Las curvas pueden tener mensurandos escalares (p. ej. fpasaje, RP_max); el editor los muestra.
  const resultsBlock = `
    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Nuevo mensurando</h3>
        ${renderResultForm(null, practice.id)}
      </section>
      <section class="panel workspace-panel">
        <h3>Mensurandos derivados</h3>
        ${renderResultsList(def, practice.id)}
      </section>
    </div>`;
  practiceWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="practice-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(practice.name)}</h3>
        <p class="submission-meta">${escapeHtml(practice.description)}</p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Tipo</div>
          <div class="metric-value metric-text">${escapeHtml(analysisKindLabel(def?.analysis_kind))}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Magnitudes</div>
          <div class="metric-value">${def?.quantities?.length ?? 0}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Mensurandos</div>
          <div class="metric-value">${def?.results?.length ?? 0}</div>
        </div>
      </div>
    </div>

    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Tipo de análisis</h3>
        ${renderAnalysisKindForm(practice, def)}
        ${def?.analysis_kind === "regresion_lineal" ? renderRegressionFormulasForm(practice, def) : ""}
        ${def?.analysis_kind === "curva" ? renderCurvesSection(practice, def) : ""}
        ${def?.analysis_kind === "regresion_lineal" || def?.analysis_kind === "curva" ? renderIntermediatesSection(practice, def) : ""}
        ${def?.analysis_kind === "regresion_lineal" ? renderPointResultsSection(practice, def) : ""}
        ${def?.analysis_kind === "regresion_lineal" ? renderAggregatesSection(practice, def) : ""}
        ${def?.analysis_kind == null || def?.analysis_kind === "estadistico" ? renderOperatorCountForm(practice, def) : ""}
      </section>
      <section class="panel workspace-panel">
        <h3>Nueva magnitud</h3>
        ${renderQuantityForm(null, practice.id)}
      </section>
    </div>

    <section class="panel workspace-panel">
      <div class="list-head">
        <h3>Magnitudes de entrada</h3>
        <span class="submission-meta">${escapeHtml(state.practiceActionStatus)}</span>
      </div>
      ${renderQuantitiesList(def, practice.id)}
    </section>

    ${resultsBlock}
  `;

  practiceWorkspace.classList.remove("hidden");
  practiceCatalog?.closest(".panel")?.classList.add("hidden");

  practiceWorkspace.querySelector("#practice-workspace-back")?.addEventListener("click", closePracticeWorkspace);
  practiceWorkspace.querySelector("#practice-kind-form")?.addEventListener("submit", savePracticeKind);
  practiceWorkspace.querySelector("#practice-regression-form")?.addEventListener("submit", savePracticeRegressionFormulas);
  practiceWorkspace.querySelector("#practice-operators-form")?.addEventListener("submit", savePracticeOperatorCount);
  practiceWorkspace.querySelector("#new-quantity-form")?.addEventListener("submit", saveNewQuantity);
  practiceWorkspace.querySelector("#new-result-form")?.addEventListener("submit", saveNewResult);
  practiceWorkspace.querySelector("#new-curve-form")?.addEventListener("submit", saveNewCurve);

  for (const kind of Object.values(SYMBOL_FORMULA_KINDS)) {
    practiceWorkspace
      .querySelector(`#new-${kind.dataAttr}-form`)
      ?.addEventListener("submit", (event) => saveNewSymbolFormulaRow(kind, event));
    practiceWorkspace.querySelectorAll(`[data-edit-${kind.dataAttr}]`).forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.dataset[kind.idParam];
        state.editing = isEditing(kind.kind, id) ? { kind: null, id: null } : { kind: kind.kind, id };
        renderPracticesPage();
      });
    });
    practiceWorkspace.querySelectorAll(`[data-delete-${kind.dataAttr}]`).forEach((btn) => {
      btn.addEventListener("click", () => deleteSymbolFormulaRow(kind, btn.dataset[kind.idParam], practice.id));
    });
    practiceWorkspace.querySelectorAll(`[data-cancel-${kind.dataAttr}]`).forEach((btn) => {
      btn.addEventListener("click", () => { state.editing = { kind: null, id: null }; renderPracticesPage(); });
    });
    practiceWorkspace.querySelectorAll(`[data-edit-${kind.dataAttr}-form]`).forEach((form) => {
      form.addEventListener("submit", (event) => saveEditSymbolFormulaRow(kind, event));
    });
  }

  practiceWorkspace.querySelectorAll("[data-edit-curve]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.cid;
      state.editing = isEditing("curve", id) ? { kind: null, id: null } : { kind: "curve", id };
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-curve]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeCurve(btn.dataset.cid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-move-curve]").forEach((btn) => {
    btn.addEventListener("click", () => movePracticeCurve(btn.dataset.cid, practice.id, btn.dataset.dir));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-curve]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editing = { kind: null, id: null }; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-curve-form]").forEach((form) => {
    form.addEventListener("submit", saveEditCurve);
  });

  practiceWorkspace.querySelectorAll("[data-edit-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.qid;
      state.editing = isEditing("quantity", id) ? { kind: null, id: null } : { kind: "quantity", id };
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeQuantity(btn.dataset.qid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editing = { kind: null, id: null }; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-quantity-form]").forEach((form) => {
    form.addEventListener("submit", saveEditQuantity);
  });
  practiceWorkspace.querySelectorAll("[data-edit-result]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.rid;
      state.editing = isEditing("result", id) ? { kind: null, id: null } : { kind: "result", id };
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-result]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeResult(btn.dataset.rid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-result]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editing = { kind: null, id: null }; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-result-form]").forEach((form) => {
    form.addEventListener("submit", saveEditResult);
  });
}

function renderPracticeDirectory() {
  if (!practiceCatalog) return;

  const rows = state.practices.map((p) => `
    <tr>
      <td class="directory-primary"><strong>${escapeHtml(p.name)}</strong></td>
      <td><span class="status-chip">${escapeHtml(analysisKindLabel(p.analysis_kind))}</span></td>
      <td>${escapeHtml(p.description)}</td>
      <td class="directory-actions">
        <button type="button" data-practice-open data-practice-id="${escapeHtml(p.id)}">Editar</button>
      </td>
    </tr>
  `);

  practiceCatalog.innerHTML = rows.length
    ? `
      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Práctica</th>
              <th>Tipo de análisis</th>
              <th>Descripción</th>
              <th>Acciones</th>
            </tr>
          </thead>
          <tbody>${rows.join("")}</tbody>
        </table>
      </div>
    `
    : `<p class="submission-meta">No hay prácticas definidas.</p>`;

  practiceCatalog.querySelectorAll("[data-practice-open]").forEach((btn) => {
    btn.addEventListener("click", () => openPracticeWorkspace(btn.dataset.practiceId));
  });
}

function renderAnalysisKindForm(practice, def) {
  const current = def?.analysis_kind ?? "";
  const placeholder = current ? "" : `<option value="" disabled selected>Sin definir</option>`;
  return `
    <form id="practice-kind-form" class="detail-form detail-form-grid">
      <input name="practice_id" type="hidden" value="${escapeHtml(practice.id)}" />
      <label>Tipo de análisis
        <select name="analysis_kind" required>
          ${placeholder}
          ${["estadistico", "regresion_lineal", "curva"].map((k) =>
            `<option value="${k}" ${k === current ? "selected" : ""}>${escapeHtml(analysisKindLabel(k))}</option>`
          ).join("")}
        </select>
      </label>
      <div class="detail-actions">
        <button type="submit">Guardar</button>
      </div>
    </form>
  `;
}

function renderRegressionFormulasForm(practice, def) {
  const x = escapeHtml(def?.x_formula ?? "");
  const y = escapeHtml(def?.y_formula ?? "");
  return `
    <form id="practice-regression-form" class="detail-form detail-form-grid">
      <input name="practice_id" type="hidden" value="${escapeHtml(practice.id)}" />
      <label>Fórmula eje X <input name="x_formula" value="${x}" placeholder="2*pi*f" /></label>
      <label>Fórmula eje Y <input name="y_formula" value="${y}" placeholder="b / math::sqrt(a*a - b*b)" /></label>
      <p class="submission-meta">Usá los símbolos de las magnitudes. Disponibles: <code>pi</code>, <code>e</code> y funciones <code>math::*</code> (p. ej. <code>math::sqrt</code>). La pendiente del ajuste se referencia como <code>slope</code> y el intercepto como <code>intercept</code> en los mensurandos.</p>
      <div class="detail-actions">
        <button type="submit">Guardar fórmulas</button>
      </div>
    </form>
  `;
}

/// Cantidad de operadores de una práctica estadística (Motor D). 0/1 = sin operadores.
function renderOperatorCountForm(practice, def) {
  const count = def?.operator_count ?? "";
  return `
    <form id="practice-operators-form" class="detail-form detail-form-grid">
      <input name="practice_id" type="hidden" value="${escapeHtml(practice.id)}" />
      <label>Operadores (estadística)
        <input name="operator_count" type="number" min="0" step="1" value="${escapeHtml(String(count))}" placeholder="sin operadores" />
      </label>
      <p class="submission-meta">Con 2 o más operadores, cada uno carga su propia serie de las magnitudes repetidas (las dadas o de medida única se comparten) y se calculan los mensurandos por operador, sin promediar. 0 o 1 = comportamiento normal.</p>
      <div class="detail-actions">
        <button type="submit">Guardar operadores</button>
      </div>
    </form>
  `;
}

/// Gestión de magnitudes derivadas por punto (Motor E): lista editable + alta. Se evalúan tras el
/// ajuste, una por corrida, usando magnitudes/intermedias del punto + slope/intercept + mensurandos.
function renderPointResultsSection(practice, def) {
  const kind = SYMBOL_FORMULA_KINDS.pointResult;
  return `
    <h4>Magnitudes derivadas por punto</h4>
    <p class="submission-meta">Se calculan tras el ajuste, una por corrida (p. ej. Reynolds). La fórmula puede usar las magnitudes y las intermedias del punto, <code>slope</code>/<code>intercept</code> y los mensurandos. Sin incertidumbre.</p>
    ${renderSymbolFormulaList(kind, def, practice.id)}
    <h4>Nueva derivada por punto</h4>
    ${renderSymbolFormulaForm(kind, null, practice.id)}
  `;
}

/// Gestión de mensurandos agregados (Motor F): lista editable + alta. Se evalúan una vez tras el
/// ajuste (un valor escalar) y pueden usar escalares compartidos, slope/intercept, los mensurandos,
/// los agregados anteriores y los extremos de cada magnitud por punto.
function renderAggregatesSection(practice, def) {
  const kind = SYMBOL_FORMULA_KINDS.aggregate;
  return `
    <h4>Mensurandos agregados</h4>
    <p class="submission-meta">Se calculan una vez tras el ajuste, un valor escalar (p. ej. Reynolds medio). La fórmula puede usar los escalares compartidos, <code>slope</code>/<code>intercept</code>, los mensurandos, los agregados anteriores y los extremos de cada magnitud por punto: <code>x_first</code>, <code>x_first2</code>, <code>x_last</code>, <code>x_last2</code>. Sin incertidumbre.</p>
    ${renderSymbolFormulaList(kind, def, practice.id)}
    <h4>Nuevo agregado</h4>
    ${renderSymbolFormulaForm(kind, null, practice.id)}
  `;
}

/// Gestión de magnitudes intermedias por punto (Motor C): lista editable + alta. Cada una define
/// un símbolo y una fórmula que se promedia por punto y queda disponible en las fórmulas de eje.
function renderIntermediatesSection(practice, def) {
  const kind = SYMBOL_FORMULA_KINDS.intermediate;
  return `
    <h4>Magnitudes intermedias por punto</h4>
    <p class="submission-meta">Se evalúan por réplica de cada punto y se promedian (p. ej. Q = V/t por réplica → Q medio). El símbolo queda disponible en las fórmulas de eje.</p>
    ${renderSymbolFormulaList(kind, def, practice.id)}
    <h4>Nueva intermedia</h4>
    ${renderSymbolFormulaForm(kind, null, practice.id)}
  `;
}

/// Gestión de curvas de una práctica `curva`: lista editable + alta. Una práctica de curva grafica
/// las curvas de esta lista; sin curvas no hay nada para graficar.
function renderCurvesSection(practice, def) {
  return `
    <h4>Curvas</h4>
    <p class="submission-meta">Una práctica de curva grafica una o varias series sobre el mismo barrido (p. ej. dos curvas en Filtros). Se grafican los puntos sin ajuste ni mensurandos derivados.</p>
    ${renderCurvesList(def, practice.id)}
    <h4>Nueva curva</h4>
    ${renderCurveForm(null, practice.id)}
  `;
}

function renderCurveForm(curve, practiceId) {
  const formId = curve ? "edit-curve-form" : "new-curve-form";
  const formAttr = curve ? `data-edit-curve-form data-cid="${escapeHtml(curve.id)}"` : "";
  const x = escapeHtml(curve?.x_formula ?? "");
  const y = escapeHtml(curve?.y_formula ?? "");
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${curve ? `<input name="cid" type="hidden" value="${escapeHtml(curve.id)}" />` : ""}
      <label>Fórmula eje X <input name="x_formula" value="${x}" required placeholder="math::log10(2*pi*f)" /></label>
      <label>Fórmula eje Y <input name="y_formula" value="${y}" required placeholder="VR / Vg" /></label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="x_log" ${curve?.x_log ? "checked" : ""} />
        Eje X logarítmico (barridos en frecuencia)
      </label>
      <div class="detail-actions">
        <button type="submit">${curve ? "Guardar" : "Agregar"}</button>
        ${curve ? `<button type="button" data-cancel-curve>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderCurvesList(def, practiceId) {
  const curves = def?.curves ?? [];
  if (curves.length === 0) return `<p class="submission-meta">Sin curvas en la lista.</p>`;

  const rows = curves.flatMap((c, i) => {
    const editing = isEditing("curve", c.id);
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${i + 1}</strong></td>
        <td><code>${escapeHtml(c.x_formula)}</code>${c.x_log ? ' <span class="submission-meta">(log)</span>' : ""}</td>
        <td><code>${escapeHtml(c.y_formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-move-curve data-cid="${escapeHtml(c.id)}" data-dir="up" title="Subir" ${i === 0 ? "disabled" : ""}>▲</button>
          <button type="button" data-move-curve data-cid="${escapeHtml(c.id)}" data-dir="down" title="Bajar" ${i === curves.length - 1 ? "disabled" : ""}>▼</button>
          <button type="button" data-edit-curve data-cid="${escapeHtml(c.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-curve data-cid="${escapeHtml(c.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="4" class="scale-edit-cell">${renderCurveForm(c, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr><th>#</th><th>Eje X</th><th>Eje Y</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function renderQuantityForm(qty, practiceId) {
  const v = (f) => qty ? escapeHtml(String(qty[f] ?? "")) : "";
  const formId = qty ? "edit-quantity-form" : "new-quantity-form";
  const formAttr = qty ? `data-edit-quantity-form data-qid="${escapeHtml(qty.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${qty ? `<input name="qid" type="hidden" value="${escapeHtml(qty.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="l" /></label>
      <label>Nombre <input name="name" value="${v("name")}" required placeholder="Longitud del cordón" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="mm (vacío = adimensional)" /></label>
      <label>Magnitud física <input name="quantity" value="${v("quantity")}" placeholder="longitud" /></label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="repeated" ${qty ? (qty.repeated ? "checked" : "") : "checked"} />
        Admite réplicas (tipo A)
      </label>
      <label>Réplicas por punto (regresión/curva)
        <input name="replicas_per_point" type="number" min="1" step="1" value="${v("replicas_per_point")}" placeholder="sin grilla" />
      </label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="per_point" ${qty ? (qty.per_point ? "checked" : "") : "checked"} />
        Se mide por punto (regresión/curva; desmarcá para escalar compartido)
      </label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="is_given" ${qty?.is_given ? "checked" : ""} />
        Dato dado (valor ± U cargado a mano, sin instrumento ni réplicas)
      </label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="has_uncertainty" ${qty ? (qty.has_uncertainty ? "checked" : "") : "checked"} />
        Tiene incertidumbre (solo aplica si es dato dado; desmarcá para pedir solo "Valor")
      </label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="optional" ${qty?.optional ? "checked" : ""} />
        Opcional (puede quedar sin lecturas sin bloquear la entrega)
      </label>
      <div class="detail-actions">
        <button type="submit">${qty ? "Guardar" : "Agregar"}</button>
        ${qty ? `<button type="button" data-cancel-quantity>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderQuantitiesList(def, practiceId) {
  const quantities = def?.quantities ?? [];
  if (quantities.length === 0) return `<p class="submission-meta">Sin magnitudes. Agrega una desde el panel lateral.</p>`;

  const rows = quantities.flatMap((q) => {
    const editing = isEditing("quantity", q.id);
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${symbolHtml(q.symbol)}</strong></td>
        <td>${inlineMathHtml(q.name)}</td>
        <td>${q.unit ? unitHtml(q.unit) : '<span class="submission-meta">adimensional</span>'}</td>
        <td>${q.quantity ? escapeHtml(q.quantity) : "-"}</td>
        <td>${q.repeated ? "Sí" : "No"}</td>
        <td class="directory-actions">
          <button type="button" data-edit-quantity data-qid="${escapeHtml(q.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-quantity data-qid="${escapeHtml(q.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="6" class="scale-edit-cell">${renderQuantityForm(q, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Magnitud</th><th>Réplicas</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function renderResultForm(res, practiceId) {
  const v = (f) => res ? escapeHtml(String(res[f] ?? "")) : "";
  const formId = res ? "edit-result-form" : "new-result-form";
  const formAttr = res ? `data-edit-result-form data-rid="${escapeHtml(res.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${res ? `<input name="rid" type="hidden" value="${escapeHtml(res.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="Q" /></label>
      <label>Nombre <input name="name" value="${v("name")}" required placeholder="Área transversal" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="mm2 (vacío = adimensional)" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="l*a + l*b" /></label>
      <label>Tolerancia (%)
        <input name="tolerance" type="number" min="0" step="any"
          value="${res?.tolerance != null ? escapeHtml(String(res.tolerance)) : ""}"
          placeholder="sin veredicto" />
      </label>
      <label>
        <input name="is_final" type="checkbox" ${res?.is_final ? "checked" : ""} />
        Resultado final (el alumno lo entrega para comparar)
      </label>
      <label>
        <input name="has_uncertainty" type="checkbox" ${res ? (res.has_uncertainty ? "checked" : "") : "checked"} />
        Tiene incertidumbre (desmarcá para mostrarlo siempre sin ±U)
      </label>
      <div class="detail-actions">
        <button type="submit">${res ? "Guardar" : "Agregar"}</button>
        ${res ? `<button type="button" data-cancel-result>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderResultsList(def, practiceId) {
  const results = def?.results ?? [];
  if (results.length === 0) return `<p class="submission-meta">Sin mensurandos. Agrega uno desde el panel lateral.</p>`;

  const rows = results.flatMap((r) => {
    const tolLabel = r.tolerance != null ? `${escapeHtml(String(r.tolerance))} %` : `<span class="submission-meta">—</span>`;
    const finalLabel = r.is_final ? "Sí" : "";
    const editing = isEditing("result", r.id);
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${symbolHtml(r.symbol)}</strong></td>
        <td>${inlineMathHtml(r.name)}</td>
        <td>${r.unit ? unitHtml(r.unit) : '<span class="submission-meta">adimensional</span>'}</td>
        <td><code>${escapeHtml(r.formula)}</code></td>
        <td>${tolLabel}</td>
        <td>${finalLabel}</td>
        <td class="directory-actions">
          <button type="button" data-edit-result data-rid="${escapeHtml(r.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-result data-rid="${escapeHtml(r.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="7" class="scale-edit-cell">${renderResultForm(r, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Fórmula</th><th>Tolerancia (%)</th><th>Final</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

export async function openPracticeWorkspace(practiceId) {
  state.activePracticeId = practiceId;
  state.practiceActionStatus = "";
  state.editing = { kind: null, id: null };
  state.practiceDefinition = null;
  renderPracticesPage();
  selectView("practices");
  try {
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    renderPracticesPage();
  } catch (error) {
    withPracticeStatus(error.message);
  }
}

export function closePracticeWorkspace() {
  state.activePracticeId = null;
  state.practiceDefinition = null;
  state.practiceActionStatus = "";
  state.editing = { kind: null, id: null };
  renderPracticesPage();
}

async function savePracticeKind(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    await postJson(`/api/practices/${payload.practice_id}/analysis-kind`, {
      analysis_kind: payload.analysis_kind,
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
    state.practices = await fetchJson("/api/practices");
    state.practiceActionStatus = "Tipo de análisis guardado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function savePracticeOperatorCount(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  const count = Number(payload.operator_count) || 0;
  try {
    await postJson(`/api/practices/${payload.practice_id}/operator-count`, { count });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
    state.practiceActionStatus = "Operadores actualizados";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function savePracticeRegressionFormulas(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    await postJson(`/api/practices/${payload.practice_id}/regression-formulas`, {
      x_formula: payload.x_formula ?? "",
      y_formula: payload.y_formula ?? "",
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
    state.practiceActionStatus = "Fórmulas de ajuste guardadas";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

function quantityPayloadFromForm(form) {
  const raw = Object.fromEntries(new FormData(form).entries());
  const repeated = "repeated" in raw;
  const replicas = Number(raw.replicas_per_point);
  return {
    symbol: raw.symbol,
    name: raw.name,
    unit: raw.unit,
    quantity: raw.quantity || null,
    repeated,
    // La grilla de réplicas por punto solo aplica a magnitudes `repeated`: si no lo es, no
    // guardamos un ancho de grilla muerto aunque el campo traiga un número.
    replicas_per_point: repeated && raw.replicas_per_point && replicas > 0 ? replicas : null,
    per_point: "per_point" in raw,
    is_given: "is_given" in raw,
    has_uncertainty: "has_uncertainty" in raw,
    optional: "optional" in raw,
  };
}

async function saveNewQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/quantities`, quantityPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Magnitud agregada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const qid = form.querySelector('[name="qid"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/quantities/${qid}`, quantityPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Magnitud actualizada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeQuantity(qid, practiceId) {
  if (!window.confirm("¿Eliminar esta magnitud? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/quantities/${qid}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Magnitud eliminada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

function curvePayloadFromForm(form) {
  const raw = Object.fromEntries(new FormData(form).entries());
  return {
    x_formula: raw.x_formula ?? "",
    y_formula: raw.y_formula ?? "",
    x_log: "x_log" in raw,
  };
}

async function saveNewCurve(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/curves`, curvePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Curva agregada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditCurve(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const cid = form.querySelector('[name="cid"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/curves/${cid}`, curvePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Curva actualizada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function movePracticeCurve(cid, practiceId, dir) {
  try {
    await postJson(`/api/practices/${practiceId}/curves/${cid}/move`, { up: dir === "up" });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeCurve(cid, practiceId) {
  if (!window.confirm("¿Eliminar esta curva? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/curves/${cid}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Curva eliminada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

/** Parsea el campo tolerancia del formulario: número ≥ 0 o null si vacío/negativo. */
function parseTolerance(raw) {
  const v = parseFloat(raw);
  return isNaN(v) || raw.trim() === "" || v < 0 ? null : v;
}

async function saveNewResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const raw = Object.fromEntries(new FormData(form).entries());
  try {
    await postJson(`/api/practices/${practiceId}/results`, {
      symbol: raw.symbol,
      name: raw.name,
      unit: raw.unit,
      formula: raw.formula,
      tolerance: parseTolerance(raw.tolerance ?? ""),
      is_final: raw.is_final === "on",
      has_uncertainty: raw.has_uncertainty === "on",
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Mensurando agregado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const rid = form.querySelector('[name="rid"]').value;
  const raw = Object.fromEntries(new FormData(form).entries());
  try {
    await postJson(`/api/practices/${practiceId}/results/${rid}`, {
      symbol: raw.symbol,
      name: raw.name,
      unit: raw.unit,
      formula: raw.formula,
      tolerance: parseTolerance(raw.tolerance ?? ""),
      is_final: raw.is_final === "on",
      has_uncertainty: raw.has_uncertainty === "on",
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Mensurando actualizado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeResult(rid, practiceId) {
  if (!window.confirm("¿Eliminar este mensurando? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/results/${rid}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editing = { kind: null, id: null };
    state.practiceActionStatus = "Mensurando eliminado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

function withPracticeStatus(message) {
  if (practiceStatus) practiceStatus.textContent = message;
  if (message) window.setTimeout(() => { if (practiceStatus) practiceStatus.textContent = ""; }, 3000);
}
