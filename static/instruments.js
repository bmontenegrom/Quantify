import { state } from "./state.js";
import { instrumentCatalog, instrumentWorkspace, instrumentCourseFilter, instrumentStatus } from "./dom.js";
import { fetchJson, postJson, deleteJson, errorText } from "./api.js";
import { escapeHtml, format, scalePayload } from "./lib.js";
import { selectView } from "./navigation.js";

export function renderInstrumentCourseOptions() {
  const options = state.academic.courses
    .map((c) => `<option value="${escapeHtml(c.id)}">${escapeHtml(c.name)} (${escapeHtml(c.term)})</option>`)
    .join("");
  instrumentCourseFilter.innerHTML = options;
  if (!state.instrumentCourseId && state.academic.courses.length > 0) {
    state.instrumentCourseId = state.academic.courses[0].id;
  }
  instrumentCourseFilter.value = state.instrumentCourseId ?? "";
}

async function loadInstruments() {
  const courseId = state.instrumentCourseId || state.academic?.courses[0]?.id;
  if (!courseId) { state.instruments = []; return; }
  state.instrumentCourseId = courseId;
  state.instruments = await fetchJson(`/api/instruments?course_id=${encodeURIComponent(courseId)}`);
}

export async function refreshInstruments() {
  try {
    await loadInstruments();
    renderInstrumentsPage();
  } catch (error) {
    state.instruments = [];
    renderInstrumentsPage();
    withInstrumentStatus(error.message);
  }
}

export function renderInstrumentsPage() {
  renderInstrumentDirectory();
  if (!instrumentWorkspace) return;

  const item = state.instruments.find((i) => i.id === state.activeInstrumentId);
  if (!item) {
    instrumentWorkspace.innerHTML = "";
    instrumentWorkspace.classList.add("hidden");
    instrumentCatalog.closest(".panel")?.classList.remove("hidden");
    return;
  }

  instrumentWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="instrument-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(item.name)}</h3>
        <p class="submission-meta">${escapeHtml(item.quantity)} · ${escapeHtml(item.unit)} · <span class="status-chip">${escapeHtml(item.kind)}</span></p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Escalas</div>
          <div class="metric-value">${item.scales.length}</div>
        </div>
      </div>
    </div>

    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Datos del instrumento</h3>
        ${renderInstrumentProfileForm(item)}
      </section>
      <section class="panel workspace-panel">
        <h3>Nueva escala</h3>
        ${renderScaleForm(null, item.id)}
      </section>
    </div>

    <section class="panel workspace-panel">
      <div class="list-head">
        <h3>Escalas</h3>
        <span class="submission-meta">${escapeHtml(state.instrumentActionStatus)}</span>
      </div>
      ${renderScalesList(item)}
    </section>
  `;

  instrumentWorkspace.classList.remove("hidden");
  instrumentCatalog.closest(".panel")?.classList.add("hidden");

  instrumentWorkspace.querySelector("#instrument-workspace-back")?.addEventListener("click", closeInstrumentWorkspace);
  instrumentWorkspace.querySelector("#instrument-profile-form")?.addEventListener("submit", saveInstrumentEdit);
  const newScaleForm = instrumentWorkspace.querySelector("#new-scale-form");
  if (newScaleForm) {
    wireScaleBModelToggle(newScaleForm);
    newScaleForm.addEventListener("submit", saveNewScale);
  }
  instrumentWorkspace.querySelectorAll("[data-edit-scale]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingScaleId = state.editingScaleId === btn.dataset.scaleId ? null : btn.dataset.scaleId;
      renderInstrumentsPage();
    });
  });
  instrumentWorkspace.querySelectorAll("[data-delete-scale]").forEach((btn) => {
    btn.addEventListener("click", () => deleteScale(btn.dataset.scaleId, item.id));
  });
  instrumentWorkspace.querySelectorAll("[data-cancel-scale]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingScaleId = null; renderInstrumentsPage(); });
  });
  instrumentWorkspace.querySelectorAll("[data-edit-scale-form]").forEach((form) => {
    wireScaleBModelToggle(form);
    form.addEventListener("submit", saveEditScale);
  });
}

function renderInstrumentDirectory() {
  if (!instrumentCatalog || !state.academic) return;

  const rows = state.instruments.map((item) => `
    <tr>
      <td class="directory-primary">
        <strong>${escapeHtml(item.name)}</strong>
      </td>
      <td><span class="status-chip">${escapeHtml(item.kind)}</span></td>
      <td>${escapeHtml(item.quantity)}</td>
      <td>${escapeHtml(item.unit)}</td>
      <td><strong>${item.scales.length}</strong></td>
      <td class="directory-actions">
        <button type="button" data-instrument-open data-instrument-id="${escapeHtml(item.id)}">Editar</button>
        <button type="button" data-instrument-delete data-instrument-id="${escapeHtml(item.id)}">Eliminar</button>
      </td>
    </tr>
  `);

  const courseId = state.instrumentCourseId ?? "";
  const exportImportBar = `
    <div class="detail-actions instrument-toolbar">
      <button type="button" id="instrument-export-btn">Exportar JSON</button>
      <button type="button" id="instrument-import-btn">Importar JSON</button>
      <input type="file" id="instrument-import-file" accept=".json,application/json" class="hidden" />
      <span id="instrument-import-status" class="submission-meta"></span>
    </div>
    <div class="panel instrument-new-panel">
      <h3>Nuevo instrumento</h3>
      <form id="new-instrument-form" class="detail-form detail-form-grid">
        <input name="course_id" type="hidden" value="${escapeHtml(courseId)}" />
        <label>Nombre <input name="name" required placeholder="Tester A830L" /></label>
        <label>Tipo
          <select name="kind" required>
            <option value="digital">digital</option>
            <option value="analogico">analogico</option>
          </select>
        </label>
        <label>Magnitud <input name="quantity" required placeholder="corriente" /></label>
        <label>Unidad <input name="unit" required placeholder="A" /></label>
        <div class="detail-actions">
          <button type="submit">Crear instrumento</button>
          <span id="new-instrument-status" class="submission-meta"></span>
        </div>
      </form>
    </div>
  `;

  instrumentCatalog.innerHTML = exportImportBar + (rows.length
    ? `
      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Nombre</th>
              <th>Tipo</th>
              <th>Magnitud</th>
              <th>Unidad</th>
              <th>Escalas</th>
              <th>Acciones</th>
            </tr>
          </thead>
          <tbody>${rows.join("")}</tbody>
        </table>
      </div>
    `
    : `<p class="submission-meta">No hay instrumentos para este curso.</p>`);

  instrumentCatalog.querySelectorAll("[data-instrument-open]").forEach((btn) => {
    btn.addEventListener("click", () => openInstrumentWorkspace(btn.dataset.instrumentId));
  });
  instrumentCatalog.querySelectorAll("[data-instrument-delete]").forEach((btn) => {
    btn.addEventListener("click", () => deleteInstrument(btn.dataset.instrumentId));
  });
  instrumentCatalog.querySelector("#new-instrument-form")?.addEventListener("submit", saveNewInstrument);
  instrumentCatalog.querySelector("#instrument-export-btn")?.addEventListener("click", exportInstruments);
  instrumentCatalog.querySelector("#instrument-import-btn")?.addEventListener("click", () => {
    instrumentCatalog.querySelector("#instrument-import-file")?.click();
  });
  instrumentCatalog.querySelector("#instrument-import-file")?.addEventListener("change", importInstruments);
}

function renderInstrumentProfileForm(item) {
  return `
    <form id="instrument-profile-form" class="detail-form detail-form-grid">
      <input name="id" type="hidden" value="${escapeHtml(item.id)}" />
      <label>Nombre <input name="name" value="${escapeHtml(item.name)}" required /></label>
      <label>Tipo
        <select name="kind" required>
          ${["digital", "analogico"].map((k) => `<option value="${k}" ${k === item.kind ? "selected" : ""}>${k}</option>`).join("")}
        </select>
      </label>
      <label>Magnitud <input name="quantity" value="${escapeHtml(item.quantity)}" required /></label>
      <label>Unidad <input name="unit" value="${escapeHtml(item.unit)}" required /></label>
      <div class="detail-actions">
        <button type="submit">Guardar cambios</button>
        <span class="submission-meta">${escapeHtml(state.instrumentActionStatus)}</span>
      </div>
    </form>
  `;
}

function renderScaleForm(scale, instrumentId) {
  const v = (field) => scale ? escapeHtml(String(scale[field] ?? "")) : "";
  const bModel = scale?.b_model ?? "resolucion";
  const isApre = bModel === "apreciacion";
  const isFab = bModel === "fabricante";
  const formId = scale ? "edit-scale-form" : "new-scale-form";
  const formAttr = scale ? `data-edit-scale-form data-scale-id="${escapeHtml(scale.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="instrument_id" type="hidden" value="${escapeHtml(instrumentId)}" />
      ${scale ? `<input name="scale_id" type="hidden" value="${escapeHtml(scale.id)}" />` : ""}
      <label>Etiqueta <input name="label" value="${v("label")}" required placeholder="200 uA" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" required placeholder="A" /></label>
      <label>Modelo de incertidumbre tipo B
        <select name="b_model" required>
          ${["resolucion", "apreciacion", "fabricante"].map((m) => `<option value="${m}" ${m === bModel ? "selected" : ""}>${m}</option>`).join("")}
        </select>
      </label>
      <label>Paso / Resolución <input name="step" type="number" step="any" value="${v("step")}" required placeholder="0.1e-6" /></label>
      <label>Fondo de escala <input name="full_scale" type="number" step="any" value="${v("full_scale")}" placeholder="200e-6" /></label>
      <label class="scale-field-apre ${isApre ? "" : "hidden"}">
        Apreciación <input name="appreciation" type="number" step="any" value="${v("appreciation")}" placeholder="0.5" />
      </label>
      <div class="scale-fields-fab ${isFab ? "" : "hidden"}">
        <label>Espec. % lectura <input name="spec_pct_reading" type="number" step="any" value="${v("spec_pct_reading")}" placeholder="1.0" /></label>
        <label>Espec. coef. paso <input name="spec_step_coeff" type="number" step="any" value="${v("spec_step_coeff")}" placeholder="5.0" /></label>
        <label>Espec. fijo <input name="spec_fixed" type="number" step="any" value="${v("spec_fixed")}" placeholder="0.0" /></label>
        <label>Res. interna (Ω) <input name="internal_res" type="number" step="any" value="${v("internal_res")}" /></label>
        <label>Incert. Res. interna <input name="internal_res_u" type="number" step="any" value="${v("internal_res_u")}" /></label>
      </div>
      <div class="detail-actions">
        <button type="submit">${scale ? "Guardar" : "Agregar escala"}</button>
        ${scale ? `<button type="button" data-cancel-scale>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function wireScaleBModelToggle(form) {
  const select = form.querySelector('[name="b_model"]');
  if (!select) return;
  const update = () => {
    const val = select.value;
    form.querySelector(".scale-field-apre")?.classList.toggle("hidden", val !== "apreciacion");
    form.querySelector(".scale-fields-fab")?.classList.toggle("hidden", val !== "fabricante");
  };
  select.addEventListener("change", update);
}

function renderScalesList(item) {
  if (item.scales.length === 0) return `<p class="submission-meta">Sin escalas. Agrega una desde el panel superior.</p>`;

  const rows = item.scales.flatMap((scale) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(scale.label)}</strong></td>
        <td><span class="status-chip">${escapeHtml(scale.b_model)}</span></td>
        <td>${format(scale.step)}</td>
        <td>${scale.full_scale != null ? format(scale.full_scale) : "-"}</td>
        <td>${escapeHtml(scale.unit)}</td>
        <td class="directory-actions">
          <button type="button" data-edit-scale data-scale-id="${escapeHtml(scale.id)}">${state.editingScaleId === scale.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-scale data-scale-id="${escapeHtml(scale.id)}">Eliminar</button>
        </td>
      </tr>
    `;
    const editRow = state.editingScaleId === scale.id
      ? `<tr><td colspan="6" class="scale-edit-cell">${renderScaleForm(scale, item.id)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>Etiqueta</th>
            <th>Modelo</th>
            <th>Paso</th>
            <th>Fondo</th>
            <th>Unidad</th>
            <th>Acciones</th>
          </tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

export function openInstrumentWorkspace(instrumentId) {
  state.activeInstrumentId = instrumentId;
  state.instrumentActionStatus = "";
  state.editingScaleId = null;
  renderInstrumentsPage();
  selectView("instruments");
}

export function closeInstrumentWorkspace() {
  state.activeInstrumentId = null;
  state.instrumentActionStatus = "";
  state.editingScaleId = null;
  renderInstrumentsPage();
}

async function saveNewInstrument(event) {
  event.preventDefault();
  const status = instrumentCatalog.querySelector("#new-instrument-status");
  try {
    if (status) status.textContent = "";
    const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
    await postJson("/api/instruments", payload);
    event.currentTarget.reset();
    event.currentTarget.querySelector('[name="course_id"]').value = state.instrumentCourseId ?? "";
    await loadInstruments();
    renderInstrumentsPage();
    withInstrumentStatus("Instrumento creado");
  } catch (error) {
    if (status) status.textContent = error.message;
    else withInstrumentStatus(error.message);
  }
}

async function saveInstrumentEdit(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.instrumentActionStatus = "";
    await postJson(`/api/instruments/${payload.id}`, {
      name: payload.name,
      kind: payload.kind,
      quantity: payload.quantity,
      unit: payload.unit,
    });
    state.instrumentActionStatus = "Cambios guardados";
    await loadInstruments();
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

function scalePayloadFromForm(form) {
  return scalePayload(Object.fromEntries(new FormData(form).entries()));
}

async function saveNewScale(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const instrumentId = form.querySelector('[name="instrument_id"]').value;
  try {
    await postJson(`/api/instruments/${instrumentId}/scales`, scalePayloadFromForm(form));
    await loadInstruments();
    state.editingScaleId = null;
    state.instrumentActionStatus = "Escala agregada";
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

async function saveEditScale(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const instrumentId = form.querySelector('[name="instrument_id"]').value;
  const scaleId = form.querySelector('[name="scale_id"]').value;
  try {
    await postJson(`/api/instruments/${instrumentId}/scales/${scaleId}`, scalePayloadFromForm(form));
    await loadInstruments();
    state.editingScaleId = null;
    state.instrumentActionStatus = "Escala actualizada";
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

async function deleteScale(scaleId, instrumentId) {
  if (!window.confirm("¿Eliminar esta escala? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/instruments/${instrumentId}/scales/${scaleId}`);
    await loadInstruments();
    state.editingScaleId = null;
    state.instrumentActionStatus = "Escala eliminada";
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

async function deleteInstrument(instrumentId) {
  const item = state.instruments.find((i) => i.id === instrumentId);
  const extra = item?.scales.length ? ` y sus ${item.scales.length} escala(s)` : "";
  if (!window.confirm(`¿Eliminar el instrumento "${item?.name ?? ""}"${extra}? Esta accion no se puede deshacer.`)) return;
  try {
    withInstrumentStatus("");
    await deleteJson(`/api/instruments/${instrumentId}`);
    await loadInstruments();
    renderInstrumentsPage();
    withInstrumentStatus("Instrumento eliminado");
  } catch (error) {
    withInstrumentStatus(error.message);
  }
}

async function exportInstruments() {
  try {
    withInstrumentStatus("");
    const courseId = state.instrumentCourseId;
    const data = await fetchJson(`/api/instruments/export?course_id=${encodeURIComponent(courseId)}`);
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "instrumentos.json";
    a.click();
    URL.revokeObjectURL(url);
    withInstrumentStatus("Catalogo exportado");
  } catch (error) {
    withInstrumentStatus(error.message);
  }
}

async function importInstruments(event) {
  const file = event.target.files?.[0];
  if (!file) return;
  const importStatus = instrumentCatalog.querySelector("#instrument-import-status");
  try {
    if (importStatus) importStatus.textContent = "Importando...";
    const text = await file.text();
    const catalog = JSON.parse(text);
    await postJson("/api/instruments/import", {
      course_id: state.instrumentCourseId,
      instruments: catalog.instruments,
    });
    event.target.value = "";
    await loadInstruments();
    renderInstrumentsPage();
    withInstrumentStatus(`${catalog.instruments?.length ?? 0} instrumentos importados`);
  } catch (error) {
    if (importStatus) importStatus.textContent = error.message;
    else withInstrumentStatus(error.message);
    event.target.value = "";
  }
}

function withInstrumentStatus(message) {
  if (instrumentStatus) instrumentStatus.textContent = message;
  if (message) window.setTimeout(() => { if (instrumentStatus) instrumentStatus.textContent = ""; }, 3000);
}

instrumentCourseFilter.addEventListener("change", () => {
  state.instrumentCourseId = instrumentCourseFilter.value;
  state.activeInstrumentId = null;
  state.editingScaleId = null;
  refreshInstruments();
});
