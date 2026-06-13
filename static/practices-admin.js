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
        ${def?.analysis_kind === "regresion_lineal" || def?.analysis_kind === "curva" ? renderRegressionFormulasForm(practice, def) : ""}
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
  practiceWorkspace.querySelector("#new-quantity-form")?.addEventListener("submit", saveNewQuantity);
  practiceWorkspace.querySelector("#new-result-form")?.addEventListener("submit", saveNewResult);

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
  const isCurva = def?.analysis_kind === "curva";
  const slopeHint = isCurva
    ? "Se grafican los puntos sin ajuste ni mensurandos derivados."
    : "La pendiente del ajuste se referencia como <code>slope</code> y el intercepto como <code>intercept</code> en los mensurandos.";
  const xLogField = isCurva
    ? `<label class="detail-form-checkbox">
        <input type="checkbox" name="x_log" ${def?.x_log ? "checked" : ""} />
        Eje X logarítmico (barridos en frecuencia)
      </label>`
    : "";
  return `
    <form id="practice-regression-form" class="detail-form detail-form-grid">
      <input name="practice_id" type="hidden" value="${escapeHtml(practice.id)}" />
      <label>Fórmula eje X <input name="x_formula" value="${x}" placeholder="2*pi*f" /></label>
      <label>Fórmula eje Y <input name="y_formula" value="${y}" placeholder="b / math::sqrt(a*a - b*b)" /></label>
      ${xLogField}
      <p class="submission-meta">Usá los símbolos de las magnitudes. Disponibles: <code>pi</code>, <code>e</code> y funciones <code>math::*</code> (p. ej. <code>math::sqrt</code>). ${slopeHint}</p>
      <div class="detail-actions">
        <button type="submit">Guardar fórmulas</button>
      </div>
    </form>
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
  state.editingQuantityId = null;
  state.editingResultId = null;
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

async function savePracticeRegressionFormulas(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  const xLog = event.currentTarget.querySelector('[name="x_log"]')?.checked ?? false;
  try {
    await postJson(`/api/practices/${payload.practice_id}/regression-formulas`, {
      x_formula: payload.x_formula ?? "",
      y_formula: payload.y_formula ?? "",
      x_log: xLog,
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
  const replicas = Number(raw.replicas_per_point);
  return {
    symbol: raw.symbol,
    name: raw.name,
    unit: raw.unit,
    quantity: raw.quantity || null,
    repeated: "repeated" in raw,
    replicas_per_point: raw.replicas_per_point && replicas > 0 ? replicas : null,
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
