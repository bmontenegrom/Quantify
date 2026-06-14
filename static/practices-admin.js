import { state } from "./state.js";
import { practiceCatalog, practiceWorkspace, practiceStatus } from "./dom.js";
import { fetchJson, postJson, deleteJson, errorText } from "./api.js";
import { escapeHtml, analysisKindLabel } from "./lib.js";
import { selectView } from "./navigation.js";

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
  // Las curvas (scatter sin ajuste) no derivan mensurandos: se grafican los puntos y nada más.
  const isCurva = def?.analysis_kind === "curva";
  const resultsBlock = isCurva
    ? `
    <section class="panel workspace-panel">
      <h3>Mensurandos derivados</h3>
      <p class="submission-meta">Las prácticas de tipo "Curva (sin ajuste)" solo grafican los puntos; no derivan mensurandos.</p>
    </section>`
    : `
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
  practiceWorkspace.querySelector("#new-intermediate-form")?.addEventListener("submit", saveNewIntermediate);

  practiceWorkspace.querySelectorAll("[data-edit-intermediate]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingIntermediateId = state.editingIntermediateId === btn.dataset.iid ? null : btn.dataset.iid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-intermediate]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeIntermediate(btn.dataset.iid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-intermediate]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingIntermediateId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-intermediate-form]").forEach((form) => {
    form.addEventListener("submit", saveEditIntermediate);
  });

  practiceWorkspace.querySelector("#new-point-result-form")?.addEventListener("submit", saveNewPointResult);
  practiceWorkspace.querySelectorAll("[data-edit-point-result]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingPointResultId = state.editingPointResultId === btn.dataset.pid ? null : btn.dataset.pid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-point-result]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticePointResult(btn.dataset.pid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-point-result]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingPointResultId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-point-result-form]").forEach((form) => {
    form.addEventListener("submit", saveEditPointResult);
  });

  practiceWorkspace.querySelector("#new-aggregate-form")?.addEventListener("submit", saveNewAggregate);
  practiceWorkspace.querySelectorAll("[data-edit-aggregate]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingAggregateId = state.editingAggregateId === btn.dataset.aid ? null : btn.dataset.aid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-aggregate]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeAggregate(btn.dataset.aid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-aggregate]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingAggregateId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-aggregate-form]").forEach((form) => {
    form.addEventListener("submit", saveEditAggregate);
  });

  practiceWorkspace.querySelectorAll("[data-edit-curve]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingCurveId = state.editingCurveId === btn.dataset.cid ? null : btn.dataset.cid;
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
    btn.addEventListener("click", () => { state.editingCurveId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-curve-form]").forEach((form) => {
    form.addEventListener("submit", saveEditCurve);
  });

  practiceWorkspace.querySelectorAll("[data-edit-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingQuantityId = state.editingQuantityId === btn.dataset.qid ? null : btn.dataset.qid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeQuantity(btn.dataset.qid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingQuantityId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-quantity-form]").forEach((form) => {
    form.addEventListener("submit", saveEditQuantity);
  });
  practiceWorkspace.querySelectorAll("[data-edit-result]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingResultId = state.editingResultId === btn.dataset.rid ? null : btn.dataset.rid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-result]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeResult(btn.dataset.rid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-result]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingResultId = null; renderPracticesPage(); });
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
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
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
  return `
    <h4>Magnitudes derivadas por punto</h4>
    <p class="submission-meta">Se calculan tras el ajuste, una por corrida (p. ej. Reynolds). La fórmula puede usar las magnitudes y las intermedias del punto, <code>slope</code>/<code>intercept</code> y los mensurandos. Sin incertidumbre.</p>
    ${renderPointResultsList(def, practice.id)}
    <h4>Nueva derivada por punto</h4>
    ${renderPointResultForm(null, practice.id)}
  `;
}

function renderPointResultForm(pr, practiceId) {
  const formId = pr ? "edit-point-result-form" : "new-point-result-form";
  const formAttr = pr ? `data-edit-point-result-form data-pid="${escapeHtml(pr.id)}"` : "";
  const v = (f) => (pr ? escapeHtml(String(pr[f] ?? "")) : "");
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${pr ? `<input name="pid" type="hidden" value="${escapeHtml(pr.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="Re" /></label>
      <label>Nombre <input name="name" value="${v("name")}" placeholder="Número de Reynolds" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="2*rho*Q / (pi*mu*R)" /></label>
      <div class="detail-actions">
        <button type="submit">${pr ? "Guardar" : "Agregar"}</button>
        ${pr ? `<button type="button" data-cancel-point-result>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderPointResultsList(def, practiceId) {
  const items = def?.point_results ?? [];
  if (items.length === 0) return `<p class="submission-meta">Sin magnitudes derivadas por punto.</p>`;
  const rows = items.flatMap((pr) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(pr.symbol)}</strong> <span class="submission-meta">${escapeHtml(pr.name)} (${escapeHtml(pr.unit)})</span></td>
        <td><code>${escapeHtml(pr.formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-edit-point-result data-pid="${escapeHtml(pr.id)}">${state.editingPointResultId === pr.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-point-result data-pid="${escapeHtml(pr.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingPointResultId === pr.id
      ? `<tr><td colspan="3" class="scale-edit-cell">${renderPointResultForm(pr, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });
  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead><tr><th>Símbolo</th><th>Fórmula</th><th>Acciones</th></tr></thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

/// Gestión de mensurandos agregados (Motor F): lista editable + alta. Se evalúan una vez tras el
/// ajuste (un valor escalar) y pueden usar escalares compartidos, slope/intercept, los mensurandos,
/// los agregados anteriores y los extremos de cada magnitud por punto.
function renderAggregatesSection(practice, def) {
  return `
    <h4>Mensurandos agregados</h4>
    <p class="submission-meta">Se calculan una vez tras el ajuste, un valor escalar (p. ej. Reynolds medio). La fórmula puede usar los escalares compartidos, <code>slope</code>/<code>intercept</code>, los mensurandos, los agregados anteriores y los extremos de cada magnitud por punto: <code>x_first</code>, <code>x_first2</code>, <code>x_last</code>, <code>x_last2</code>. Sin incertidumbre.</p>
    ${renderAggregatesList(def, practice.id)}
    <h4>Nuevo agregado</h4>
    ${renderAggregateForm(null, practice.id)}
  `;
}

function renderAggregateForm(agg, practiceId) {
  const formId = agg ? "edit-aggregate-form" : "new-aggregate-form";
  const formAttr = agg ? `data-edit-aggregate-form data-aid="${escapeHtml(agg.id)}"` : "";
  const v = (f) => (agg ? escapeHtml(String(agg[f] ?? "")) : "");
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${agg ? `<input name="aid" type="hidden" value="${escapeHtml(agg.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="Re_medio" /></label>
      <label>Nombre <input name="name" value="${v("name")}" placeholder="Reynolds medio" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="(Re_max + Re_min) / 2" /></label>
      <div class="detail-actions">
        <button type="submit">${agg ? "Guardar" : "Agregar"}</button>
        ${agg ? `<button type="button" data-cancel-aggregate>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderAggregatesList(def, practiceId) {
  const items = def?.aggregates ?? [];
  if (items.length === 0) return `<p class="submission-meta">Sin mensurandos agregados.</p>`;
  const rows = items.flatMap((agg) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(agg.symbol)}</strong> <span class="submission-meta">${escapeHtml(agg.name)} (${escapeHtml(agg.unit)})</span></td>
        <td><code>${escapeHtml(agg.formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-edit-aggregate data-aid="${escapeHtml(agg.id)}">${state.editingAggregateId === agg.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-aggregate data-aid="${escapeHtml(agg.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingAggregateId === agg.id
      ? `<tr><td colspan="3" class="scale-edit-cell">${renderAggregateForm(agg, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });
  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead><tr><th>Símbolo</th><th>Fórmula</th><th>Acciones</th></tr></thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

/// Gestión de magnitudes intermedias por punto (Motor C): lista editable + alta. Cada una define
/// un símbolo y una fórmula que se promedia por punto y queda disponible en las fórmulas de eje.
function renderIntermediatesSection(practice, def) {
  return `
    <h4>Magnitudes intermedias por punto</h4>
    <p class="submission-meta">Se evalúan por réplica de cada punto y se promedian (p. ej. Q = V/t por réplica → Q medio). El símbolo queda disponible en las fórmulas de eje.</p>
    ${renderIntermediatesList(def, practice.id)}
    <h4>Nueva intermedia</h4>
    ${renderIntermediateForm(null, practice.id)}
  `;
}

function renderIntermediateForm(it, practiceId) {
  const formId = it ? "edit-intermediate-form" : "new-intermediate-form";
  const formAttr = it ? `data-edit-intermediate-form data-iid="${escapeHtml(it.id)}"` : "";
  const v = (f) => (it ? escapeHtml(String(it[f] ?? "")) : "");
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${it ? `<input name="iid" type="hidden" value="${escapeHtml(it.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="Q" /></label>
      <label>Nombre <input name="name" value="${v("name")}" placeholder="Caudal medio" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="m3/s" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="V / t" /></label>
      <div class="detail-actions">
        <button type="submit">${it ? "Guardar" : "Agregar"}</button>
        ${it ? `<button type="button" data-cancel-intermediate>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderIntermediatesList(def, practiceId) {
  const items = def?.intermediates ?? [];
  if (items.length === 0) return `<p class="submission-meta">Sin magnitudes intermedias.</p>`;
  const rows = items.flatMap((it) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(it.symbol)}</strong> <span class="submission-meta">${escapeHtml(it.name)} (${escapeHtml(it.unit)})</span></td>
        <td><code>${escapeHtml(it.formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-edit-intermediate data-iid="${escapeHtml(it.id)}">${state.editingIntermediateId === it.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-intermediate data-iid="${escapeHtml(it.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingIntermediateId === it.id
      ? `<tr><td colspan="3" class="scale-edit-cell">${renderIntermediateForm(it, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });
  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead><tr><th>Símbolo</th><th>Fórmula</th><th>Acciones</th></tr></thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
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
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${i + 1}</strong></td>
        <td><code>${escapeHtml(c.x_formula)}</code>${c.x_log ? ' <span class="submission-meta">(log)</span>' : ""}</td>
        <td><code>${escapeHtml(c.y_formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-move-curve data-cid="${escapeHtml(c.id)}" data-dir="up" title="Subir" ${i === 0 ? "disabled" : ""}>▲</button>
          <button type="button" data-move-curve data-cid="${escapeHtml(c.id)}" data-dir="down" title="Bajar" ${i === curves.length - 1 ? "disabled" : ""}>▼</button>
          <button type="button" data-edit-curve data-cid="${escapeHtml(c.id)}">${state.editingCurveId === c.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-curve data-cid="${escapeHtml(c.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingCurveId === c.id
      ? `<tr><td colspan="4" class="scale-edit-cell">${renderCurveForm(c, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
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
      <label>Unidad <input name="unit" value="${v("unit")}" required placeholder="mm" /></label>
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
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(q.symbol)}</strong></td>
        <td>${escapeHtml(q.name)}</td>
        <td>${escapeHtml(q.unit)}</td>
        <td>${q.quantity ? escapeHtml(q.quantity) : "-"}</td>
        <td>${q.repeated ? "Sí" : "No"}</td>
        <td class="directory-actions">
          <button type="button" data-edit-quantity data-qid="${escapeHtml(q.id)}">${state.editingQuantityId === q.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-quantity data-qid="${escapeHtml(q.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingQuantityId === q.id
      ? `<tr><td colspan="6" class="scale-edit-cell">${renderQuantityForm(q, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
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
      <label>Unidad <input name="unit" value="${v("unit")}" required placeholder="mm2" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="l*a + l*b" /></label>
      <label>Tolerancia (%)
        <input name="tolerance" type="number" min="0" step="any"
          value="${res?.tolerance != null ? escapeHtml(String(res.tolerance)) : ""}"
          placeholder="sin veredicto" />
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
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(r.symbol)}</strong></td>
        <td>${escapeHtml(r.name)}</td>
        <td>${escapeHtml(r.unit)}</td>
        <td><code>${escapeHtml(r.formula)}</code></td>
        <td>${tolLabel}</td>
        <td class="directory-actions">
          <button type="button" data-edit-result data-rid="${escapeHtml(r.id)}">${state.editingResultId === r.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-result data-rid="${escapeHtml(r.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingResultId === r.id
      ? `<tr><td colspan="6" class="scale-edit-cell">${renderResultForm(r, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Fórmula</th><th>Tolerancia (%)</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

export async function openPracticeWorkspace(practiceId) {
  state.activePracticeId = practiceId;
  state.practiceActionStatus = "";
  state.editingQuantityId = null;
  state.editingResultId = null;
  state.editingCurveId = null;
  state.editingIntermediateId = null;
  state.editingPointResultId = null;
  state.editingAggregateId = null;
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
  state.editingAggregateId = null;
  state.practiceActionStatus = "";
  state.editingQuantityId = null;
  state.editingResultId = null;
  state.editingCurveId = null;
  state.editingIntermediateId = null;
  state.editingPointResultId = null;
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
  };
}

async function saveNewQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/quantities`, quantityPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingQuantityId = null;
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
    state.editingQuantityId = null;
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
    state.editingQuantityId = null;
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
    state.editingCurveId = null;
  state.editingIntermediateId = null;
  state.editingPointResultId = null;
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
    state.editingCurveId = null;
  state.editingIntermediateId = null;
  state.editingPointResultId = null;
    state.practiceActionStatus = "Curva actualizada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveNewIntermediate(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/intermediates`, intermediatePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingIntermediateId = null;
  state.editingPointResultId = null;
    state.practiceActionStatus = "Intermedia agregada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditIntermediate(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const iid = form.querySelector('[name="iid"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/intermediates/${iid}`, intermediatePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingIntermediateId = null;
  state.editingPointResultId = null;
    state.practiceActionStatus = "Intermedia actualizada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeIntermediate(iid, practiceId) {
  if (!window.confirm("¿Eliminar esta magnitud intermedia? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/intermediates/${iid}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingIntermediateId = null;
  state.editingPointResultId = null;
    state.practiceActionStatus = "Intermedia eliminada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveNewPointResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/point-results`, intermediatePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingPointResultId = null;
    state.practiceActionStatus = "Derivada por punto agregada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditPointResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const pid = form.querySelector('[name="pid"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/point-results/${pid}`, intermediatePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingPointResultId = null;
    state.practiceActionStatus = "Derivada por punto actualizada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticePointResult(pid, practiceId) {
  if (!window.confirm("¿Eliminar esta magnitud derivada por punto? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/point-results/${pid}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingPointResultId = null;
    state.practiceActionStatus = "Derivada por punto eliminada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveNewAggregate(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/aggregates`, intermediatePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingAggregateId = null;
    state.practiceActionStatus = "Mensurando agregado agregado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditAggregate(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const aid = form.querySelector('[name="aid"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/aggregates/${aid}`, intermediatePayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingAggregateId = null;
    state.practiceActionStatus = "Mensurando agregado actualizado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeAggregate(aid, practiceId) {
  if (!window.confirm("¿Eliminar este mensurando agregado? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}/aggregates/${aid}`);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingAggregateId = null;
    state.practiceActionStatus = "Mensurando agregado eliminado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

function intermediatePayloadFromForm(form) {
  const raw = Object.fromEntries(new FormData(form).entries());
  return {
    symbol: raw.symbol,
    name: raw.name || "",
    unit: raw.unit || "",
    formula: raw.formula,
  };
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
    state.editingCurveId = null;
  state.editingIntermediateId = null;
  state.editingPointResultId = null;
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
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingResultId = null;
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
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingResultId = null;
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
    state.editingResultId = null;
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
