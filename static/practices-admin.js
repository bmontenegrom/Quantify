import { state } from "./state.js";
import { practiceCatalog, practiceWorkspace, practiceStatus } from "./dom.js";
import { fetchJson, postJson, deleteJson, errorText } from "./api.js";
import {
  escapeHtml,
  symbolHtml,
  setDisplaySymbols,
  inlineMathHtml,
  unitHtml,
  analysisKindLabel,
  formatDate,
  compareOptionGroups,
  formulaScope,
  formulaHelpHtml,
  formatSymbolHtml,
  QUANTITY_ROLES,
  quantityRole,
  roleFlags,
} from "./lib.js";
import { selectView } from "./navigation.js";

const ANALYSIS_KINDS = ["estadistico", "regresion_lineal", "curva"];

function adminSymbolHtml(symbol) {
  return `<strong>${symbolHtml(symbol)}</strong> <code title="Así se escribe en las fórmulas">${escapeHtml(symbol)}</code>`;
}

function displaySymbolFieldHtml(item) {
  const symbols = state.practiceDefinition?.display_symbols ?? {};
  const shown = item && Object.hasOwn(symbols, item.symbol) ? symbols[item.symbol] : "";
  const preview = formatSymbolHtml(shown || item?.symbol || "");
  return `<div class="display-symbol-field"><label>Símbolo a mostrar <input name="display_symbol" value="${escapeHtml(shown)}" maxlength="40" placeholder="vacío = el símbolo" title="Con _ para subíndice: V_R_PP se ve como V con subíndice R,PP" /></label><span class="symbol-preview" aria-hidden="true">${preview}</span></div>`;
}

function formulaHelpFor(symbols) {
  return formulaHelpHtml(symbols, state.practiceDefinition?.formula_scopes?.functions ?? []);
}

function formulaFieldHtml(label, input, symbols) {
  return `<div class="formula-field wide-field"><label>${label} ${input}</label>${formulaHelpFor(symbols)}</div>`;
}

function insertIntoFormula(button) {
  const input = button.closest(".formula-field")?.querySelector("input");
  if (!input) return;
  const text = button.dataset.insert;
  const focused = document.activeElement === input;
  const start = focused ? input.selectionStart : input.value.length;
  const end = focused ? input.selectionEnd : input.value.length;
  input.setRangeText(text, start, end, "end");
  if (text.endsWith("()")) input.setSelectionRange(input.selectionEnd - 1, input.selectionEnd - 1);
  input.focus();
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function refreshSymbolPreview(input) {
  const preview = input.closest(".display-symbol-field")?.querySelector(".symbol-preview");
  if (!preview) return;
  preview.innerHTML = formatSymbolHtml(input.value.trim() || input.form?.elements.symbol?.value.trim() || "");
}

practiceWorkspace.addEventListener("mousedown", (e) => {
  if (e.target.closest("[data-insert]")) e.preventDefault();
});
practiceWorkspace.addEventListener("click", (e) => {
  const button = e.target.closest("[data-insert]");
  if (button) insertIntoFormula(button);
});
practiceWorkspace.addEventListener("input", (e) => {
  if (e.target.name === "display_symbol") refreshSymbolPreview(e.target);
  if (e.target.name === "symbol") {
    const field = e.target.form?.elements.display_symbol;
    if (field) refreshSymbolPreview(field);
  }
});

function selectFieldHtml(label, name, options, current) {
  const items = options
    .map(([value, text]) => `<option value="${value}"${value === current ? " selected" : ""}>${text}</option>`)
    .join("");
  return `<label>${label} <select name="${name}">${items}</select></label>`;
}

function isEditing(kind, id) {
  return state.editing.kind === kind && state.editing.id === id;
}

function statusStrings(label, fem) {
  const suffix = fem
    ? { create: "agregada", update: "actualizada", delete: "eliminada" }
    : { create: "agregado", update: "actualizado", delete: "eliminado" };
  return {
    statusCreate: `${label} ${suffix.create}`,
    statusUpdate: `${label} ${suffix.update}`,
    statusDelete: `${label} ${suffix.delete}`,
  };
}

const SYMBOL_FORMULA_KINDS = {
  pointResult: {
    kind: "pointResult",
    urlSegment: "point-results",
    idParam: "pid",
    dataAttr: "point-result",
    scopeKind: "point_result",
    listOf: (def) => def?.point_results ?? [],
    emptyText: "Sin magnitudes derivadas por punto.",
    symbolPlaceholder: "Re",
    namePlaceholder: "Número de Reynolds",
    formulaPlaceholder: "2*rho*Q / (pi*mu*R)",
    ...statusStrings("Derivada por punto", true),
    confirmDelete: "¿Eliminar esta magnitud derivada por punto? Esta accion no se puede deshacer.",
  },
  aggregate: {
    kind: "aggregate",
    urlSegment: "aggregates",
    idParam: "aid",
    dataAttr: "aggregate",
    scopeKind: "aggregate",
    listOf: (def) => def?.aggregates ?? [],
    emptyText: "Sin mensurandos agregados.",
    symbolPlaceholder: "Re_medio",
    namePlaceholder: "Reynolds medio",
    formulaPlaceholder: "(Re_max + Re_min) / 2",
    ...statusStrings("Mensurando agregado", false),
    confirmDelete: "¿Eliminar este mensurando agregado? Esta accion no se puede deshacer.",
    flags: [{ field: "is_final", column: "Final", label: "Resultado final (comparable, sin incertidumbre)" }],
  },
  intermediate: {
    kind: "intermediate",
    urlSegment: "intermediates",
    idParam: "iid",
    dataAttr: "intermediate",
    scopeKind: "intermediate",
    listOf: (def) => def?.intermediates ?? [],
    emptyText: "Sin magnitudes intermedias.",
    symbolPlaceholder: "Q",
    namePlaceholder: "Caudal medio",
    formulaPlaceholder: "V / t",
    ...statusStrings("Intermedia", true),
    confirmDelete: "¿Eliminar esta magnitud intermedia? Esta accion no se puede deshacer.",
    flags: [
      { field: "on_means", column: "Sobre promedios", label: "Calcular sobre los promedios del punto (en vez de por réplica y después promediar)" },
      { field: "show_live", column: "En vivo", label: "Mostrar en vivo en la tabla del alumno" },
    ],
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
      ${displaySymbolFieldHtml(item)}
      <label>Nombre <input name="name" value="${v("name")}" placeholder="${kind.namePlaceholder}" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="(vacío = adimensional)" /></label>
      ${formulaFieldHtml("Fórmula", `<input name="formula" value="${v("formula")}" required placeholder="${kind.formulaPlaceholder}" />`, formulaScope(state.practiceDefinition, kind.scopeKind, { id: item?.id ?? "" }))}
      ${
        (kind.flags ?? [])
          .map((f) => `<label class="checkbox-field"><input name="${f.field}" type="checkbox" ${item?.[f.field] ? "checked" : ""} /> ${f.label}</label>`)
          .join("")
      }
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
  const flags = kind.flags ?? [];
  const colspan = 3 + flags.length;
  const rows = items.flatMap((item) => {
    const editing = isEditing(kind.kind, item.id);
    const finalCell = flags.map((f) => `<td>${item[f.field] ? "Sí" : "—"}</td>`).join("");
    const baseRow = `
      <tr>
        <td class="directory-primary">${adminSymbolHtml(item.symbol)} <span class="submission-meta">${inlineMathHtml(item.name)}${item.unit ? ` (${unitHtml(item.unit)})` : " (adimensional)"}</span></td>
        <td><code>${escapeHtml(item.formula)}</code></td>
        ${finalCell}
        <td class="directory-actions">
          <button type="button" data-edit-${kind.dataAttr} data-${kind.idParam}="${escapeHtml(item.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" class="btn-danger" data-delete-${kind.dataAttr} data-${kind.idParam}="${escapeHtml(item.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="${colspan}" class="scale-edit-cell">${renderSymbolFormulaForm(kind, item, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });
  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead><tr><th>Símbolo</th><th>Fórmula</th>${flags.map((f) => `<th>${f.column}</th>`).join("")}<th>Acciones</th></tr></thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function symbolFormulaPayloadFromForm(form, kind) {
  const raw = Object.fromEntries(new FormData(form).entries());
  const payload = {
    symbol: raw.symbol,
    name: raw.name || "",
    unit: raw.unit || "",
    formula: raw.formula,
    display_symbol: raw.display_symbol,
  };
  for (const f of kind.flags ?? []) payload[f.field] = raw[f.field] === "on";
  return payload;
}

async function runPracticeAction(fn, successMsg, { resetEditing = false } = {}) {
  try {
    const message = await fn();
    if (resetEditing) state.editing = { kind: null, id: null };
    state.practiceActionStatus = typeof message === "string" ? message : successMsg;
  } catch (error) {
    state.practiceActionStatus = error.message;
  }
  renderPracticesPage();
}

async function saveNewSymbolFormulaRow(kind, event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/${kind.urlSegment}`, symbolFormulaPayloadFromForm(form, kind));
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    kind.statusCreate,
    { resetEditing: true },
  );
}

async function saveEditSymbolFormulaRow(kind, event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const id = form.querySelector(`[name="${kind.idParam}"]`).value;
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/${kind.urlSegment}/${id}`, symbolFormulaPayloadFromForm(form, kind));
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    kind.statusUpdate,
    { resetEditing: true },
  );
}

async function deleteSymbolFormulaRow(kind, id, practiceId) {
  if (!window.confirm(kind.confirmDelete)) return;
  await runPracticeAction(
    async () => {
      await deleteJson(`/api/practices/${practiceId}/${kind.urlSegment}/${id}`);
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    kind.statusDelete,
    { resetEditing: true },
  );
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
  setDisplaySymbols(def?.display_symbols);
  if (!def) {
    practiceWorkspace.innerHTML = `
      <div class="workspace-head">
        <div>
          <button type="button" class="back-link" id="practice-workspace-back">Volver al listado</button>
          <h3>${escapeHtml(practice.name)}</h3>
          <p class="submission-meta">Cargando definición…</p>
        </div>
      </div>`;
    practiceWorkspace.classList.remove("hidden");
    practiceCatalog?.closest(".panel")?.classList.add("hidden");
    practiceWorkspace.querySelector("#practice-workspace-back")?.addEventListener("click", closePracticeWorkspace);
    return;
  }

  const kind = def?.analysis_kind;
  const pointBased = kind === "regresion_lineal" || kind === "curva";
  const openPanels = new Set(
    [...practiceWorkspace.querySelectorAll("details[open][data-add-panel]")].map((d) => d.dataset.addPanel),
  );
  practiceWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="practice-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(practice.name)}</h3>
        <p class="submission-meta">${escapeHtml(practice.description)}</p>
        ${practiceLifecycleActions(practice, def)}
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

    ${state.practiceActionStatus ? `<p class="inline-status practice-status" role="status">${escapeHtml(state.practiceActionStatus)}</p>` : ""}

    <details class="panel workspace-panel add-panel" data-add-panel="practice-info">
      <summary>Datos de la práctica</summary>
      ${renderPracticeInfoForm(practice)}
    </details>

    <section class="panel workspace-panel">
      <h3 class="admin-section-title">Configuración del análisis</h3>
      ${def?.parts?.length
        ? `<p class="submission-meta">El análisis se elige en cada parte, en «Partes y secciones».</p>`
        : renderAnalysisKindForm(practice, def)}
      ${kind === "regresion_lineal" ? renderRegressionFormulasForm(practice, def) : ""}
      ${kind == null || kind === "estadistico" ? renderOperatorCountForm(practice, def) : ""}
    </section>

    ${renderLayoutSection(practice, def)}
    ${adminSection({
      id: "quantity",
      title: "Magnitudes de entrada",
      count: def?.quantities?.length ?? 0,
      list: renderQuantitiesList(def, practice.id),
      addLabel: "Nueva magnitud",
      form: renderQuantityForm(null, practice.id),
    })}
    ${pointBased ? renderIntermediatesSection(practice, def) : ""}
    ${kind === "curva" ? renderCurvesSection(practice, def) : ""}
    ${adminSection({
      id: "result",
      title: "Mensurandos derivados",
      count: def?.results?.length ?? 0,
      list: renderResultsList(def, practice.id),
      addLabel: "Nuevo mensurando",
      form: renderResultForm(null, practice.id, def),
    })}
    ${kind === "regresion_lineal" ? renderPointResultsSection(practice, def) : ""}
    ${kind === "regresion_lineal" ? renderAggregatesSection(practice, def) : ""}
  `;
  for (const details of practiceWorkspace.querySelectorAll("details[data-add-panel]")) {
    if (openPanels.has(details.dataset.addPanel)) details.open = true;
  }

  practiceWorkspace.classList.remove("hidden");
  practiceCatalog?.closest(".panel")?.classList.add("hidden");

  practiceWorkspace.querySelector("#practice-workspace-back")?.addEventListener("click", closePracticeWorkspace);
  practiceWorkspace.querySelector("#practice-info-form")?.addEventListener("submit", savePracticeInfo);
  practiceWorkspace.querySelectorAll("[data-practice-status]").forEach((btn) => {
    btn.addEventListener("click", () => setPracticeStatus(btn.dataset.practiceId, btn.dataset.practiceStatus));
  });
  practiceWorkspace.querySelector("[data-practice-try-draft]")?.addEventListener("click", async (e) => {
    const practiceId = e.currentTarget.dataset.practiceId;
    const { openDraftPreview } = await import("./forms.js");
    await openDraftPreview(practiceId);
  });
  renderProblems(practice.id);
  if (def?.current_version) renderDraftChanges(practice.id);
  practiceWorkspace.querySelector("[data-practice-delete]")?.addEventListener("click", (e) => {
    deletePractice(e.currentTarget.dataset.practiceId);
  });
  practiceWorkspace.querySelector("#practice-kind-form")?.addEventListener("submit", savePracticeKind);
  practiceWorkspace.querySelector("#practice-regression-form")?.addEventListener("submit", savePracticeRegressionFormulas);
  practiceWorkspace.querySelector("#practice-operators-form")?.addEventListener("submit", savePracticeOperatorCount);
  practiceWorkspace.querySelector("#new-quantity-form")?.addEventListener("submit", saveNewQuantity);
  practiceWorkspace.querySelector("#new-result-form")?.addEventListener("submit", saveNewResult);
  practiceWorkspace.querySelector('#new-result-form [name="section_id"]')?.addEventListener("change", (e) => {
    const section = def?.sections?.find((s) => s.id === e.target.value) ?? null;
    const select = e.target.form.elements.compare_with;
    select.innerHTML = compareOptionsHtml(compareOptionGroups(def, { section }, select.value), select.value);
    const help = e.target.form.querySelector(".formula-help");
    const open = help.open;
    help.outerHTML = formulaHelpFor(formulaScope(def, "result", { section }));
    e.target.form.querySelector(".formula-help").open = open;
  });
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
  practiceWorkspace.querySelectorAll("[data-layout-form]").forEach((form) => {
    form.addEventListener("submit", (event) => saveLayoutItem(event, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-layout-delete]").forEach((btn) => {
    btn.addEventListener("click", () => deleteLayoutItem(btn.dataset.layoutDelete, btn.dataset.id, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-part-kind]").forEach((select) => {
    select.addEventListener("change", () => setPartKind(select.dataset.id, select.value, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-section-select]").forEach((select) => {
    select.addEventListener("change", () => setItemSection(select.dataset.sectionSelect, select.dataset.id, select.value, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-move]").forEach((btn) => {
    btn.addEventListener("click", () => movePracticeItem(btn.dataset.move, btn.dataset.id, practice.id, btn.dataset.dir));
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
  practiceWorkspace.querySelectorAll("#new-quantity-form, [data-edit-quantity-form]").forEach((form) => {
    syncQuantityForm(form);
    form.addEventListener("input", () => syncQuantityForm(form));
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

const STATUS_LABELS = { borrador: "Borrador", publicada: "Publicada", archivada: "Archivada" };

function practiceStatusChip(status) {
  return `<span class="practice-state practice-state--${escapeHtml(status)}">${escapeHtml(STATUS_LABELS[status] ?? status)}</span>`;
}

function renderNewPracticeForm() {
  return `
    <form id="new-practice-form" class="detail-form detail-form-grid">
      <label>Nombre <input name="name" required placeholder="Péndulo simple" /></label>
      <label>Tipo de análisis
        <select name="analysis_kind">
          ${ANALYSIS_KINDS.map((k) => `<option value="${k}">${escapeHtml(analysisKindLabel(k))}</option>`).join("")}
        </select>
      </label>
      <label>Descripción <input name="description" placeholder="Qué se mide y qué se determina" /></label>
      <div class="detail-actions"><button type="submit">Crear borrador</button></div>
    </form>`;
}

function renderPracticeDirectory() {
  if (!practiceCatalog) return;

  const rows = state.practices.map((p) => `
    <tr>
      <td class="directory-primary"><strong>${escapeHtml(p.name)}</strong></td>
      <td>${practiceStatusChip(p.status)}</td>
      <td><span class="status-chip">${escapeHtml(analysisKindLabel(p.analysis_kind))}</span></td>
      <td>${escapeHtml(p.description)}</td>
      <td class="directory-actions">
        <button type="button" data-practice-open data-practice-id="${escapeHtml(p.id)}">Editar</button>
        <button type="button" data-practice-duplicate data-practice-id="${escapeHtml(p.id)}">Duplicar</button>
      </td>
    </tr>
  `);

  practiceCatalog.innerHTML = `
    <details class="add-panel" data-add-panel="practice">
      <summary>Nueva práctica</summary>
      <p class="submission-meta">Se crea como borrador: cargale magnitudes y mensurandos, y publicala para poder habilitarla en un curso.</p>
      ${renderNewPracticeForm()}
    </details>
    ${
      rows.length
        ? `
      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Práctica</th>
              <th>Estado</th>
              <th>Tipo de análisis</th>
              <th>Descripción</th>
              <th>Acciones</th>
            </tr>
          </thead>
          <tbody>${rows.join("")}</tbody>
        </table>
      </div>`
        : `<p class="submission-meta">No hay prácticas definidas.</p>`
    }
  `;

  practiceCatalog.querySelector("#new-practice-form")?.addEventListener("submit", createPractice);
  practiceCatalog.querySelectorAll("[data-practice-open]").forEach((btn) => {
    btn.addEventListener("click", () => openPracticeWorkspace(btn.dataset.practiceId));
  });
  practiceCatalog.querySelectorAll("[data-practice-duplicate]").forEach((btn) => {
    btn.addEventListener("click", () => duplicatePractice(btn.dataset.practiceId));
  });
}

async function refreshPractices() {
  const { loadAcademic } = await import("./academic.js");
  await loadAcademic();
}

async function createPractice(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    const created = await postJson("/api/practices", payload);
    await refreshPractices();
    await openPracticeWorkspace(created.id);
    state.practiceActionStatus = "Práctica creada en borrador";
    renderPracticesPage();
  } catch (error) {
    withPracticeStatus(error.message);
  }
}

async function duplicatePractice(practiceId) {
  try {
    const copy = await postJson(`/api/practices/${practiceId}/duplicate`, {});
    await refreshPractices();
    await openPracticeWorkspace(copy.id);
    state.practiceActionStatus = "Copia creada en borrador";
    renderPracticesPage();
  } catch (error) {
    withPracticeStatus(error.message);
  }
}

function practiceLifecycleActions(practice, def) {
  const id = escapeHtml(practice.id);
  const version = def?.current_version;
  const versionInfo = version ? `<span class="submission-meta">Versión ${version}</span>` : "";
  const buttons = {
    borrador: `<button type="button" data-practice-status="publicada" data-practice-id="${id}">Publicar</button>
      <button type="button" class="btn-danger" data-practice-delete data-practice-id="${id}">Eliminar</button>`,
    publicada: `<button type="button" data-practice-status="archivada" data-practice-id="${id}">Archivar</button>`,
    archivada: `<button type="button" data-practice-status="publicada" data-practice-id="${id}">Restaurar</button>`,
  };
  const publishedActions = practice.status === "borrador" ? "" : versionInfo;
  const tryDraft = `<button type="button" data-practice-try-draft data-practice-id="${id}">Probar el borrador</button>`;
  return `<div class="practice-lifecycle">${practiceStatusChip(practice.status)} ${buttons[practice.status] ?? ""} ${publishedActions} ${tryDraft}</div>
    <div id="practice-problems"></div>
    <div id="practice-draft-changes"></div>`;
}

function draftChangesMarkup(practiceId, { version, kind, changes, submissions, version_submissions: versionSubmissions }) {
  const id = escapeHtml(practiceId);
  const structural = kind === "structural";
  const list = changes.map((change) => `<li>${escapeHtml(change)}</li>`).join("");
  const explanation = structural
    ? `Publicar crea la versión ${version + 1}.${submissions ? ` Entregas existentes: ${submissions}; siguen con su versión.` : ""}`
    : `Solo cambian nombres, títulos, tolerancias o el orden de la lista: publicar actualiza la versión ${version}, sin crear otra.${list && versionSubmissions ? ` Las entregas de esa versión (${versionSubmissions}) se comparan con la tolerancia nueva.` : ""}`;
  return `
    <div class="edit-banner draft-changes">
      <div>
        <strong>Cambios sin publicar</strong>
        ${list ? `<ul>${list}</ul>` : ""}
        <p class="submission-meta">${explanation}</p>
      </div>
      <button type="button" data-practice-publish-version data-practice-id="${id}">${structural ? `Publicar versión ${version + 1}` : `Actualizar versión ${version}`}</button>
    </div>`;
}

const EDITABLE_KINDS = new Set(["quantity", "result", "curve", "intermediate", "pointResult", "aggregate"]);
let problemsRequest = 0;
let lastProblems = null;

function problemItemHtml({ message }, k) {
  return `<li><button type="button" class="link-button" data-problem="${k}">${escapeHtml(message)}</button></li>`;
}

function goToProblem({ kind, id }) {
  const parts = '[data-add-panel="part"]';
  let target = { axes: "#practice-regression-form", layout: parts }[kind] ?? `[data-add-panel="${kind}"]`;
  if (id && EDITABLE_KINDS.has(kind)) {
    state.editing = { kind, id };
    renderPracticesPage();
    const cell = practiceWorkspace.querySelector(".scale-edit-cell");
    if (cell) return cell.scrollIntoView({ block: "center" });
    state.editing = { kind: null, id: null };
    target = parts;
  }
  const el = practiceWorkspace.querySelector(target) ?? practiceWorkspace.querySelector(parts);
  if (el?.tagName === "DETAILS" && el.matches(`[data-add-panel="${kind}"]`)) el.open = true;
  (el?.closest("section") ?? el)?.scrollIntoView({ block: "start" });
}

async function renderProblems(practiceId) {
  const box = practiceWorkspace.querySelector("#practice-problems");
  if (!box) return;
  if (lastProblems?.practiceId === practiceId) paintProblems(box, lastProblems.problems);
  const request = ++problemsRequest;
  try {
    const problems = await fetchJson(`/api/practices/${practiceId}/problems`);
    if (request !== problemsRequest) return;
    lastProblems = { practiceId, problems };
    paintProblems(box, problems);
  } catch (error) {
    if (request === problemsRequest) box.innerHTML = `<p class="submission-meta">${escapeHtml(error.message)}</p>`;
  }
}

function paintProblems(box, problems) {
  if (!problems.length) {
    box.innerHTML = `<p class="submission-meta practice-problems-ok">Sin problemas: la definición está completa.</p>`;
    return;
  }
  box.innerHTML = `
    <div class="edit-banner practice-problems" role="status">
      <div>
        <strong>Para publicar falta resolver (${problems.length})</strong>
        <ul>${problems.map(problemItemHtml).join("")}</ul>
      </div>
    </div>`;
  box.querySelectorAll("[data-problem]").forEach((btn) => {
    btn.addEventListener("click", () => goToProblem(problems[Number(btn.dataset.problem)]));
  });
}

let lastDraftChanges = null;
let draftChangesRequest = 0;

function versionUpdateMarkup({ version, last_update: update }) {
  if (!update) return "";
  const by = update.updated_by ? ` por ${escapeHtml(update.updated_by)}` : "";
  return `<p class="submission-meta version-update">Versión ${version} actualizada${by} el ${escapeHtml(formatDate(update.updated_at))}.</p>`;
}

function paintDraftChanges(box, practiceId, changes) {
  if (!changes) {
    box.replaceChildren();
    return;
  }
  const pending = changes.kind === "none" ? "" : draftChangesMarkup(practiceId, changes);
  box.innerHTML = versionUpdateMarkup(changes) + pending;
  box.querySelector("[data-practice-publish-version]")?.addEventListener("click", () => {
    publishPracticeVersion(practiceId, changes.fingerprint);
  });
}

async function renderDraftChanges(practiceId) {
  const box = practiceWorkspace.querySelector("#practice-draft-changes");
  if (!box) return;
  if (lastDraftChanges?.practiceId === practiceId) paintDraftChanges(box, practiceId, lastDraftChanges.changes);
  const request = ++draftChangesRequest;
  try {
    const changes = await fetchJson(`/api/practices/${practiceId}/draft-changes`);
    if (request !== draftChangesRequest) return;
    lastDraftChanges = { practiceId, changes };
    paintDraftChanges(box, practiceId, changes);
  } catch (error) {
    box.innerHTML = `<p class="submission-meta">${escapeHtml(error.message)}</p>`;
  }
}

function renderPracticeInfoForm(practice) {
  return `
    <form id="practice-info-form" class="detail-form detail-form-grid">
      <input name="practice_id" type="hidden" value="${escapeHtml(practice.id)}" />
      <label>Nombre <input name="name" value="${escapeHtml(practice.name)}" required /></label>
      <label>Descripción <input name="description" value="${escapeHtml(practice.description)}" /></label>
      <div class="detail-actions"><button type="submit">Guardar</button></div>
    </form>`;
}

const STATUS_DONE = { publicada: "Práctica publicada", archivada: "Práctica archivada" };

async function setPracticeStatus(practiceId, status) {
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${practiceId}/status`, { status });
    await refreshPractices();
    if (state.practiceDefinition?.practice_id === practiceId) {
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    }
  }, STATUS_DONE[status] ?? "Estado actualizado");
}

async function publishPracticeVersion(practiceId, fingerprint) {
  lastDraftChanges = null;
  await runPracticeAction(async () => {
    const { number, new_version } = await postJson(`/api/practices/${practiceId}/versions`, { expected_fingerprint: fingerprint });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    return new_version
      ? `Versión ${number} publicada: las entregas nuevas la usan; las anteriores siguen con la suya.`
      : `Versión ${number} actualizada, sin crear otra.`;
  }, "Versión publicada");
}

async function savePracticeInfo(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${payload.practice_id}`, { name: payload.name, description: payload.description });
    await refreshPractices();
  }, "Datos de la práctica guardados");
}

async function deletePractice(practiceId) {
  if (!window.confirm("¿Eliminar este borrador con toda su definición? Esta accion no se puede deshacer.")) return;
  try {
    await deleteJson(`/api/practices/${practiceId}`);
    await refreshPractices();
    closePracticeWorkspace();
    withPracticeStatus("Borrador eliminado");
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
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
          ${ANALYSIS_KINDS.map((k) =>
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
      ${formulaFieldHtml("Fórmula eje X", `<input name="x_formula" value="${x}" placeholder="2*pi*f" />`, formulaScope(def, "axis"))}
      ${formulaFieldHtml("Fórmula eje Y", `<input name="y_formula" value="${y}" placeholder="b / math::sqrt(a*a - b*b)" />`, formulaScope(def, "axis"))}
      <p class="submission-meta">En los mensurandos, la pendiente del ajuste se referencia como <code>slope</code> y el intercepto como <code>intercept</code>.</p>
      <div class="detail-actions">
        <button type="submit">Guardar fórmulas</button>
      </div>
    </form>
  `;
}

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

function adminSection({ id, title, count, help = "", list, addLabel, form }) {
  return `
    <section class="panel workspace-panel admin-section">
      <h3 class="admin-section-title">${title} <span class="count-badge">${count}</span></h3>
      ${help ? `<p class="submission-meta">${help}</p>` : ""}
      ${list}
      <details class="add-panel" data-add-panel="${id}">
        <summary>${addLabel}</summary>
        ${form}
      </details>
    </section>`;
}

function symbolFormulaSection(kindKey, practice, def, { title, help, addLabel }) {
  const kind = SYMBOL_FORMULA_KINDS[kindKey];
  return adminSection({
    id: kindKey,
    title,
    count: kind.listOf(def).length,
    help,
    list: renderSymbolFormulaList(kind, def, practice.id),
    addLabel,
    form: renderSymbolFormulaForm(kind, null, practice.id),
  });
}

function renderPointResultsSection(practice, def) {
  return symbolFormulaSection("pointResult", practice, def, {
    title: "Magnitudes derivadas por punto",
    help: "Se calculan tras el ajuste, una por corrida (p. ej. Reynolds). La fórmula puede usar las magnitudes y las intermedias del punto, <code>slope</code>/<code>intercept</code> y los mensurandos. Sin incertidumbre.",
    addLabel: "Nueva derivada por punto",
  });
}

function renderAggregatesSection(practice, def) {
  return symbolFormulaSection("aggregate", practice, def, {
    title: "Mensurandos agregados",
    help: "Se calculan una vez tras el ajuste, un valor escalar (p. ej. Reynolds medio). La fórmula puede usar los escalares compartidos, <code>slope</code>/<code>intercept</code>, los mensurandos, los agregados anteriores y los extremos de cada magnitud por punto: <code>x_first</code>, <code>x_first2</code>, <code>x_last</code>, <code>x_last2</code>. Sin incertidumbre.",
    addLabel: "Nuevo agregado",
  });
}

function renderIntermediatesSection(practice, def) {
  return symbolFormulaSection("intermediate", practice, def, {
    title: "Magnitudes intermedias por punto",
    help: "Se evalúan por réplica de cada punto y se promedian (p. ej. Q = V/t por réplica → Q medio), o, con \"Calcular sobre los promedios\", una sola vez con las magnitudes ya promediadas (p. ej. v = dx/t con el tiempo medio). Una intermedia que usa otra anterior recibe su promedio. El símbolo queda disponible en las fórmulas de eje.",
    addLabel: "Nueva intermedia",
  });
}

function renderCurvesSection(practice, def) {
  return adminSection({
    id: "curve",
    title: "Curvas",
    count: def?.curves?.length ?? 0,
    help: "Una práctica de curva grafica una o varias series sobre el mismo barrido (p. ej. dos curvas en Filtros). Se grafican los puntos sin ajuste ni mensurandos derivados.",
    list: renderCurvesList(def, practice.id),
    addLabel: "Nueva curva",
    form: renderCurveForm(null, practice.id),
  });
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
      ${formulaFieldHtml("Fórmula eje X", `<input name="x_formula" value="${x}" required placeholder="math::log10(2*pi*f)" />`, formulaScope(state.practiceDefinition, "axis"))}
      ${formulaFieldHtml("Fórmula eje Y", `<input name="y_formula" value="${y}" required placeholder="VR / Vg" />`, formulaScope(state.practiceDefinition, "axis"))}
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
          ${moveButtonsHtml("curves", c.id, i, curves.length)}
          <button type="button" data-edit-curve data-cid="${escapeHtml(c.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" class="btn-danger" data-delete-curve data-cid="${escapeHtml(c.id)}">Eliminar</button>
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

const POINT_KINDS = ["regresion_lineal", "curva"];

function seriesPractice() {
  return POINT_KINDS.includes(state.practiceDefinition?.analysis_kind);
}

function pointBasedFor(qty) {
  const def = state.practiceDefinition;
  if (!seriesPractice()) return false;
  if (!def.parts?.length || !qty) return true;
  const section = def.sections?.find((sec) => sec.quantities?.includes(qty.symbol));
  if (!section?.part_id) return true;
  const part = def.parts.find((p) => p.id === section.part_id);
  return !part || POINT_KINDS.includes(part.effective_kind);
}

function roleSelectHtml(qty) {
  const pointBased = pointBasedFor(qty);
  const current = qty ? quantityRole(qty, pointBased) : pointBased ? "serie" : "replicas";
  const options = QUANTITY_ROLES.filter((r) => pointBased || !r.series).map((r) => [r.id, r.label]);
  return selectFieldHtml("¿Cómo se ingresa?", "role", options, current).replace("<label>", '<label class="wide-field">');
}

function renderQuantityForm(qty, practiceId) {
  const v = (f) => qty ? escapeHtml(String(qty[f] ?? "")) : "";
  const formId = qty ? "edit-quantity-form" : "new-quantity-form";
  const formAttr = qty ? `data-edit-quantity-form data-qid="${escapeHtml(qty.id)}"` : "";
  const kept = `data-per-point="${qty?.per_point === false ? 0 : 1}" data-repeated="${qty?.repeated ? 1 : 0}" data-given="${qty?.is_given ? 1 : 0}" data-replicas="${qty?.replicas_per_point ?? ""}"`;
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr} ${kept}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${qty ? `<input name="qid" type="hidden" value="${escapeHtml(qty.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="l" /></label>
      ${displaySymbolFieldHtml(qty)}
      <label>Nombre <input name="name" value="${v("name")}" required placeholder="Longitud del cordón" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="mm (vacío = adimensional)" /></label>
      <label>Magnitud física <input name="quantity" value="${v("quantity")}" placeholder="longitud" /></label>
      ${roleSelectHtml(qty)}
      <label data-for-roles="serie_replicas">Réplicas por punto
        <input name="replicas_per_point" type="number" min="1" step="1" required value="${v("replicas_per_point")}" placeholder="cantidad" />
      </label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="optional" ${qty?.optional ? "checked" : ""} />
        Opcional (puede quedar sin lecturas sin bloquear la entrega)
      </label>
      <label>Valor inicial
        <input name="default_value" type="number" step="any" value="${v("default_value")}" placeholder="vacío" title="Se precarga en el formulario del alumno" />
      </label>
      <label data-for-roles="catedra constante">Ranuras
        <input name="slot_count" type="number" min="1" step="1" value="${v("slot_count")}" placeholder="sin restricción" title="El valor es un entero de 1 a N, en la unidad de la magnitud" />
      </label>
      ${selectFieldHtml("Rótulo en el formulario", "label_mode", [
        ["nombre", "Nombre"],
        ["simbolo", "Símbolo"],
        ["nombre_simbolo", "Nombre y símbolo"],
      ], qty?.label_mode)}
      ${selectFieldHtml("Entrada del valor", "input_widget", [
        ["teclado", "Teclado"],
        ["cronometro", "Cronómetro (solo magnitud física tiempo)"],
      ], qty?.input_widget)}
      <div class="detail-actions">
        <button type="submit">${qty ? "Guardar" : "Agregar"}</button>
        ${qty ? `<button type="button" data-cancel-quantity>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderQuantitiesList(def, practiceId) {
  const quantities = def?.quantities ?? [];
  if (quantities.length === 0) return `<p class="submission-meta">Sin magnitudes. Agregá una con «Nueva magnitud».</p>`;

  const rows = quantities.flatMap((q, i) => {
    const editing = isEditing("quantity", q.id);
    const baseRow = `
      <tr>
        <td class="directory-primary">${adminSymbolHtml(q.symbol)}</td>
        <td>${inlineMathHtml(q.name)}</td>
        <td>${q.unit ? unitHtml(q.unit) : '<span class="submission-meta">adimensional</span>'}</td>
        <td>${q.quantity ? escapeHtml(q.quantity) : "-"}</td>
        <td>${QUANTITY_ROLES.find((r) => r.id === quantityRole(q, pointBasedFor(q))).short}</td>
        <td>${sectionSelectHtml(def, "quantities", q)}</td>
        <td class="directory-actions">
          ${moveButtonsHtml("quantities", q.id, i, quantities.length)}
          <button type="button" data-edit-quantity data-qid="${escapeHtml(q.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" class="btn-danger" data-delete-quantity data-qid="${escapeHtml(q.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="7" class="scale-edit-cell">${renderQuantityForm(q, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Magnitud</th><th>Cómo se ingresa</th><th>Sección</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function newResultSectionHtml(def) {
  const sections = def?.sections ?? [];
  if (!sections.length) return "";
  const options = [`<option value="">Sin sección (va a la parte de los datos que usa)</option>`]
    .concat(sections.map((s) => `<option value="${escapeHtml(s.id)}">${escapeHtml(sectionLabel(def, s))}</option>`))
    .join("");
  return `<label>Sección <select name="section_id">${options}</select></label>`;
}

function compareOptionsHtml(groups, current) {
  const option = (value, text = value) =>
    `<option value="${escapeHtml(value)}"${value === current ? " selected" : ""}>${escapeHtml(text)}</option>`;
  return option("", "— sin comparación —") + groups
    .map(([label, symbols]) => {
      const items = symbols.map((s) => option(s)).join("");
      return label == null ? items : `<optgroup label="${escapeHtml(label)}">${items}</optgroup>`;
    })
    .join("");
}

function renderResultForm(res, practiceId, def = null) {
  const v = (f) => res ? escapeHtml(String(res[f] ?? "")) : "";
  const formId = res ? "edit-result-form" : "new-result-form";
  const formAttr = res ? `data-edit-result-form data-rid="${escapeHtml(res.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${res ? `<input name="rid" type="hidden" value="${escapeHtml(res.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="Q" /></label>
      ${displaySymbolFieldHtml(res)}
      <label>Nombre <input name="name" value="${v("name")}" required placeholder="Área transversal" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" placeholder="mm2 (vacío = adimensional)" /></label>
      ${formulaFieldHtml("Fórmula", `<input name="formula" value="${v("formula")}" required placeholder="l*a + l*b" />`, formulaScope(def, "result", { symbol: res?.symbol ?? "" }))}
      <p class="submission-meta">La incertidumbre se propaga desde las magnitudes.</p>
      ${res ? "" : newResultSectionHtml(def)}
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
      <label>Comparar con (medido vs teórico)
        <select name="compare_with">${compareOptionsHtml(compareOptionGroups(def, { result: res?.symbol }, res?.compare_with ?? ""), res?.compare_with ?? "")}</select>
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
  if (results.length === 0) return `<p class="submission-meta">Sin mensurandos. Agregá uno con «Nuevo mensurando».</p>`;

  const rows = results.flatMap((r, i) => {
    const tolLabel = r.tolerance != null ? `${escapeHtml(String(r.tolerance))} %` : `<span class="submission-meta">—</span>`;
    const finalLabel = r.is_final ? "Sí" : "";
    const editing = isEditing("result", r.id);
    const baseRow = `
      <tr>
        <td class="directory-primary">${adminSymbolHtml(r.symbol)}</td>
        <td>${inlineMathHtml(r.name)}</td>
        <td>${r.unit ? unitHtml(r.unit) : '<span class="submission-meta">adimensional</span>'}</td>
        <td><code>${escapeHtml(r.formula)}</code></td>
        <td>${tolLabel}</td>
        <td>${finalLabel}</td>
        <td>${sectionSelectHtml(def, "results", r)}</td>
        <td class="directory-actions">
          ${moveButtonsHtml("results", r.id, i, results.length)}
          <button type="button" data-edit-result data-rid="${escapeHtml(r.id)}">${editing ? "Cerrar" : "Editar"}</button>
          <button type="button" class="btn-danger" data-delete-result data-rid="${escapeHtml(r.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = editing
      ? `<tr><td colspan="8" class="scale-edit-cell">${renderResultForm(r, practiceId, def)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Fórmula</th><th>Tolerancia (%)</th><th>Final</th><th>Sección</th><th>Acciones</th></tr>
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
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${payload.practice_id}/analysis-kind`, {
      analysis_kind: payload.analysis_kind,
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
    state.practices = await fetchJson("/api/practices");
  }, "Tipo de análisis guardado");
}

async function savePracticeOperatorCount(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  const count = Number(payload.operator_count) || 0;
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${payload.practice_id}/operator-count`, { count });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
  }, "Operadores actualizados");
}

async function savePracticeRegressionFormulas(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${payload.practice_id}/regression-formulas`, {
      x_formula: payload.x_formula ?? "",
      y_formula: payload.y_formula ?? "",
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
  }, "Fórmulas de ajuste guardadas");
}

function syncQuantityForm(form) {
  const { elements } = form;
  const role = elements.role.value;
  const given = roleFlags(role, seriesPractice()).is_given;
  const option = elements.input_widget.querySelector('option[value="cronometro"]');
  option.disabled = elements.quantity.value !== "tiempo" || given;
  if (option.disabled && option.selected) elements.input_widget.value = "teclado";
  form.querySelectorAll("[data-for-roles]").forEach((label) => {
    const applies = label.dataset.forRoles.split(" ").includes(role);
    label.hidden = !applies;
    label.querySelector("input").disabled = !applies;
  });
}

function quantityPayloadFromForm(form) {
  const raw = Object.fromEntries(new FormData(form).entries());
  const flags = roleFlags(raw.role, seriesPractice(), {
    per_point: form.dataset.perPoint === "1",
    repeated: form.dataset.repeated === "1",
    is_given: form.dataset.given === "1",
    replicas_per_point: Number(form.dataset.replicas) || null,
  });
  const replicas = Number(raw.replicas_per_point);
  const slots = raw.slot_count ? Number(raw.slot_count) : null;
  if (slots != null && !Number.isInteger(slots)) {
    throw new Error("Las ranuras tienen que ser un entero de 1 en adelante (vacío = sin restricción).");
  }
  return {
    symbol: raw.symbol,
    name: raw.name,
    unit: raw.unit,
    quantity: raw.quantity || null,
    ...flags,
    replicas_per_point: raw.replicas_per_point && replicas > 0 ? replicas : null,
    optional: "optional" in raw,
    slot_count: slots,
    display_symbol: raw.display_symbol,
    label_mode: raw.label_mode,
    input_widget: raw.input_widget,
    default_value: raw.default_value === "" || raw.default_value == null ? null : Number(raw.default_value),
  };
}

async function saveNewQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/quantities`, quantityPayloadFromForm(form));
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Magnitud agregada",
    { resetEditing: true },
  );
}

async function saveEditQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const qid = form.querySelector('[name="qid"]').value;
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/quantities/${qid}`, quantityPayloadFromForm(form));
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Magnitud actualizada",
    { resetEditing: true },
  );
}

async function deletePracticeQuantity(qid, practiceId) {
  if (!window.confirm("¿Quitar esta magnitud del borrador? Esta accion no se puede deshacer.")) return;
  await runPracticeAction(
    async () => {
      const { retired } = await deleteJson(`/api/practices/${practiceId}/quantities/${qid}`);
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
      return retired
        ? "Magnitud quitada del borrador: sigue en la versión vigente hasta que publiques la siguiente."
        : "Magnitud eliminada";
    },
    "Magnitud eliminada",
    { resetEditing: true },
  );
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
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/curves`, curvePayloadFromForm(form));
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Curva agregada",
    { resetEditing: true },
  );
}

async function saveEditCurve(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const cid = form.querySelector('[name="cid"]').value;
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/curves/${cid}`, curvePayloadFromForm(form));
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Curva actualizada",
    { resetEditing: true },
  );
}

function sectionLabel(def, section) {
  const part = (def?.parts ?? []).find((p) => p.id === section.part_id);
  return part ? `${part.title} — ${section.title}` : `${section.title} (todas las partes)`;
}

function sectionSelectHtml(def, list, item) {
  const sections = def?.sections ?? [];
  if (!sections.length) return `<span class="submission-meta">—</span>`;
  const current = sections.find((s) => s[list].includes(item.symbol))?.id ?? "";
  const options = [`<option value="">Sin sección (siempre visible)</option>`]
    .concat(sections.map((s) => `<option value="${escapeHtml(s.id)}" ${s.id === current ? "selected" : ""}>${escapeHtml(sectionLabel(def, s))}</option>`))
    .join("");
  return `<select data-section-select="${list}" data-id="${escapeHtml(item.id)}" aria-label="Sección">${options}</select>`;
}

function partOptionsHtml(def, selected) {
  return [`<option value="">Todas las partes (compartida)</option>`]
    .concat((def?.parts ?? []).map((p) => `<option value="${escapeHtml(p.id)}" ${p.id === selected ? "selected" : ""}>${escapeHtml(p.title)}</option>`))
    .join("");
}

function layoutTable(head, rows, empty) {
  return rows.length
    ? `<div class="data-table-wrap"><table class="data-table"><thead><tr>${head}</tr></thead><tbody>${rows.join("")}</tbody></table></div>`
    : `<p class="submission-meta">${empty}</p>`;
}

function renderLayoutSection(practice, def) {
  const parts = def?.parts ?? [];
  const sections = def?.sections ?? [];
  const partRows = parts.map((p, i) => `
    <tr>
      <td class="directory-primary">
        <form class="layout-inline-form" data-layout-form="parts" data-id="${escapeHtml(p.id)}">
          <input name="title" value="${escapeHtml(p.title)}" required aria-label="Título de la parte" />
          <button type="submit">Guardar</button>
        </form>
      </td>
      <td>
        <select data-part-kind data-id="${escapeHtml(p.id)}" aria-label="Análisis de la parte">
          ${ANALYSIS_KINDS.map((k) =>
            `<option value="${k}" ${k === p.effective_kind ? "selected" : ""}>${escapeHtml(analysisKindLabel(k))}</option>`
          ).join("")}
        </select>
      </td>
      <td class="directory-actions">
        ${moveButtonsHtml("parts", p.id, i, parts.length)}
        <button type="button" class="btn-danger" data-layout-delete="parts" data-id="${escapeHtml(p.id)}">Eliminar</button>
      </td>
    </tr>`);
  const sectionRows = sections.map((s, i) => `
    <tr>
      <td class="directory-primary">
        <form class="layout-inline-form" data-layout-form="sections" data-id="${escapeHtml(s.id)}">
          <input name="title" value="${escapeHtml(s.title)}" required aria-label="Título de la sección" />
          <select name="part_id" aria-label="Parte">${partOptionsHtml(def, s.part_id)}</select>
          <button type="submit">Guardar</button>
        </form>
      </td>
      <td>${s.quantities.length + s.results.length}</td>
      <td class="directory-actions">
        ${moveButtonsHtml("sections", s.id, i, sections.length)}
        <button type="button" class="btn-danger" data-layout-delete="sections" data-id="${escapeHtml(s.id)}">Eliminar</button>
      </td>
    </tr>`);
  return `
    <section class="panel workspace-panel admin-section">
      <h3 class="admin-section-title">Partes y secciones <span class="count-badge">${parts.length}</span></h3>
      <p class="submission-meta">Cada parte es una pestaña del formulario del alumno. Una sección agrupa magnitudes y resultados bajo un título; sin parte, se ve en todas las pestañas (datos compartidos). La sección de cada magnitud y mensurando se elige en sus tablas.</p>
      <h4>Partes</h4>
      <p class="submission-meta">Cada parte elige su análisis. Solo una puede ser de regresión o curva: es la que lleva la serie de datos (sus magnitudes por punto, intermedias, curvas y ajuste).</p>
      ${layoutTable("<th>Título</th><th>Análisis</th><th>Acciones</th>", partRows, "Sin partes: el formulario va en una sola pestaña.")}
      <details class="add-panel" data-add-panel="part">
        <summary>Nueva parte</summary>
        <form id="new-part-form" class="detail-form detail-form-grid" data-layout-form="parts">
          <label>Título <input name="title" required placeholder="Parte 1: Arquímedes" /></label>
          <div class="detail-actions"><button type="submit">Agregar</button></div>
        </form>
      </details>
      <h4>Secciones</h4>
      ${layoutTable("<th>Título y parte</th><th>Elementos</th><th>Acciones</th>", sectionRows, "Sin secciones.")}
      <details class="add-panel" data-add-panel="section">
        <summary>Nueva sección</summary>
        <form id="new-section-form" class="detail-form detail-form-grid" data-layout-form="sections">
          <label>Título <input name="title" required placeholder="Datos comunes" /></label>
          <label>Parte <select name="part_id">${partOptionsHtml(def, null)}</select></label>
          <div class="detail-actions"><button type="submit">Agregar</button></div>
        </form>
      </details>
    </section>`;
}

async function saveLayoutItem(event, practiceId) {
  event.preventDefault();
  const form = event.currentTarget;
  const list = form.dataset.layoutForm;
  const id = form.dataset.id;
  const payload = Object.fromEntries(new FormData(form).entries());
  const body = list === "parts" ? { title: payload.title } : { title: payload.title, part_id: payload.part_id || null };
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${practiceId}/${list}${id ? `/${id}` : ""}`, body);
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
  }, list === "parts" ? "Parte guardada" : "Sección guardada");
}

async function deleteLayoutItem(list, id, practiceId) {
  const what = list === "parts"
    ? "¿Eliminar esta parte? Sus secciones pasan a verse en todas las pestañas."
    : "¿Eliminar esta sección? Sus magnitudes y resultados quedan sin sección.";
  if (!window.confirm(what)) return;
  await runPracticeAction(async () => {
    await deleteJson(`/api/practices/${practiceId}/${list}/${id}`);
    [state.practiceDefinition, state.practices] = await Promise.all([
      fetchJson(`/api/practices/${practiceId}/definition`),
      list === "parts" ? fetchJson("/api/practices") : state.practices,
    ]);
  }, list === "parts" ? "Parte eliminada" : "Sección eliminada");
}

async function setPartKind(partId, kind, practiceId) {
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${practiceId}/parts/${partId}/analysis-kind`, { analysis_kind: kind });
    [state.practiceDefinition, state.practices] = await Promise.all([
      fetchJson(`/api/practices/${practiceId}/definition`),
      fetchJson("/api/practices"),
    ]);
  }, "Análisis de la parte guardado");
}

async function setItemSection(list, id, sectionId, practiceId) {
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${practiceId}/${list}/${id}/section`, { section_id: sectionId || null });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
  }, "Sección asignada");
}

function moveButtonsHtml(list, id, index, total) {
  const attrs = `data-move="${list}" data-id="${escapeHtml(id)}"`;
  return `
    <button type="button" ${attrs} data-dir="up" title="Subir" ${index === 0 ? "disabled" : ""}>▲</button>
    <button type="button" ${attrs} data-dir="down" title="Bajar" ${index === total - 1 ? "disabled" : ""}>▼</button>`;
}

async function movePracticeItem(list, id, practiceId, dir) {
  await runPracticeAction(async () => {
    await postJson(`/api/practices/${practiceId}/${list}/${id}/move`, { up: dir === "up" });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
  }, state.practiceActionStatus);
}

async function deletePracticeCurve(cid, practiceId) {
  if (!window.confirm("¿Eliminar esta curva? Esta accion no se puede deshacer.")) return;
  await runPracticeAction(
    async () => {
      await deleteJson(`/api/practices/${practiceId}/curves/${cid}`);
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Curva eliminada",
    { resetEditing: true },
  );
}

function parseTolerance(raw) {
  const v = parseFloat(raw);
  return isNaN(v) || raw.trim() === "" || v < 0 ? null : v;
}

async function saveNewResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const raw = Object.fromEntries(new FormData(form).entries());
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/results`, {
        symbol: raw.symbol,
        name: raw.name,
        unit: raw.unit,
        formula: raw.formula,
        tolerance: parseTolerance(raw.tolerance ?? ""),
        is_final: raw.is_final === "on",
        has_uncertainty: raw.has_uncertainty === "on",
        display_symbol: raw.display_symbol,
        compare_with: raw.compare_with ?? "",
        section_id: raw.section_id || null,
      });
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Mensurando agregado",
    { resetEditing: true },
  );
}

async function saveEditResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const rid = form.querySelector('[name="rid"]').value;
  const raw = Object.fromEntries(new FormData(form).entries());
  await runPracticeAction(
    async () => {
      await postJson(`/api/practices/${practiceId}/results/${rid}`, {
        symbol: raw.symbol,
        name: raw.name,
        unit: raw.unit,
        formula: raw.formula,
        tolerance: parseTolerance(raw.tolerance ?? ""),
        is_final: raw.is_final === "on",
        has_uncertainty: raw.has_uncertainty === "on",
        display_symbol: raw.display_symbol,
        compare_with: raw.compare_with ?? "",
      });
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Mensurando actualizado",
    { resetEditing: true },
  );
}

async function deletePracticeResult(rid, practiceId) {
  if (!window.confirm("¿Eliminar este mensurando? Esta accion no se puede deshacer.")) return;
  await runPracticeAction(
    async () => {
      await deleteJson(`/api/practices/${practiceId}/results/${rid}`);
      state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    },
    "Mensurando eliminado",
    { resetEditing: true },
  );
}

function withPracticeStatus(message) {
  if (practiceStatus) practiceStatus.textContent = message;
  if (message) window.setTimeout(() => { if (practiceStatus) practiceStatus.textContent = ""; }, 3000);
}
