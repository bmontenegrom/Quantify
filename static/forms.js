import { state } from "./state.js";
import {
  courseSelect, groupSelect, practiceSelect, tableSelect,
  measurementFields, latestResult, submitStatus, submitButton,
  practicaTitle, practicePartTabs, submissionForm, studentComment,
} from "./dom.js";
import { fetchJson, postJson, deleteJson } from "./api.js";
import {
  escapeHtml, symbolHtml, inlineMathHtml, unitHtml, canReview, format,
  compatibleInstruments, SI_PREFIXES, prefixFactor, pointPower,
  seriesStats, histogram, normalCurve, validateMeasurements,
  draftMeasurementsByQuantity, hasUncertainty,
} from "./lib.js";
import {
  PRACTICE_GROUPS, PRACTICE_PARTS, PRACTICE_SECTIONS, SERIES_LIVE_COLUMNS,
  SYMBOL_FIRST_QUANTITIES, PRACTICES_WITHOUT_CHRONO_HELPER,
} from "./constants.js";
import { Chronometer } from "./chronometer.js";
import { loadSubmissions, openSubmissionWorkspace } from "./submissions.js";

/** Agrupa `items` (con `.id`/`.symbol`) según `sections[].symbols`, en el mismo orden que las
 *  secciones. Devuelve, por sección, sus `rows` encontrados, y aparte los `items` que no entraron
 *  en ninguna sección (`rest`). Común al render de magnitudes (Motor D) y al de la serie (Motor E),
 *  que solo difieren en cómo pintan cada fila/bloque, no en el matching contra PRACTICE_SECTIONS. */
function groupBySections(items, sections) {
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

function quantityNameHtml(q) {
  const base = inlineMathHtml(q.name);
  if (SYMBOL_FIRST_QUANTITIES.has(q.symbol)) {
    return `${symbolHtml(q.symbol)} <span class="submission-meta">${base}</span>`;
  }
  // T_oc no tiene subíndice obvio en el nombre: se agrega el símbolo al final, sin duplicarlo
  // si el nombre ya lo menciona.
  if (q.symbol === "T_oc" && !/T_?oc/i.test(q.name)) {
    return `${base} ${symbolHtml(q.symbol)}`;
  }
  return base;
}

function formatSeriesStat(value) {
  return Number(value).toLocaleString("es-UY", { maximumSignificantDigits: 10 });
}

export function renderStudentSelectors() {
  const courses = state.academic.courses;
  courseSelect.innerHTML = courses.length
    ? courses
        .map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`)
        .join("")
    : `<option value="">Sin cursos asignados</option>`;

  // Pre-seleccionar curso/grupo del perfil del alumno si hay default_group_id
  const defaultGroupId = state.user?.default_group_id;
  if (defaultGroupId) {
    const defaultCourse = courses.find((c) => c.groups.some((g) => g.id === defaultGroupId));
    if (defaultCourse) courseSelect.value = defaultCourse.id;
  }

  updateStudentSelectors();

  // Seleccionar el grupo por defecto después de actualizar los selects del curso
  if (defaultGroupId && groupSelect.querySelector(`option[value="${CSS.escape(defaultGroupId)}"]`)) {
    groupSelect.value = defaultGroupId;
    updateTableSelector();
  }
}

export function updateStudentSelectors({ autoLoad = true } = {}) {
  const course = selectedCourse();
  groupSelect.innerHTML = course?.groups.length
    ? course.groups.map((group) => `<option value="${escapeHtml(group.id)}">${escapeHtml(group.name)}</option>`).join("")
    : `<option value="">Sin grupos</option>`;
  practiceSelect.innerHTML = course?.practices.length
    ? course.practices
        .map((practice) => `<option value="${escapeHtml(practice.id)}">${escapeHtml(practice.name)}</option>`)
        .join("")
    : `<option value="">Sin practicas habilitadas</option>`;
  updateTableSelector();
  if (autoLoad) loadSubmissionForm();
}

export function updateTableSelector() {
  if (!tableSelect) return;
  const group = selectedCourse()?.groups.find((item) => item.id === groupSelect.value);
  const assignment = selectedTableAssignment();
  // Mesa por defecto del perfil, solo si es el grupo por defecto del alumno
  const isDefaultGroup = groupSelect.value === (state.user?.default_group_id ?? "");
  const profileTable = isDefaultGroup ? (state.user?.default_table_number ?? null) : null;
  const tableCount = group?.table_count ?? 0;
  tableSelect.innerHTML = tableCount
    ? Array.from({ length: tableCount }, (_, index) => {
        const tableNumber = index + 1;
        const selected =
          assignment?.table_number === tableNumber ||
          (!assignment && tableNumber === profileTable);
        return `<option value="${tableNumber}" ${selected ? "selected" : ""}>Mesa ${tableNumber}</option>`;
      }).join("")
    : `<option value="">Sin mesas</option>`;
  tableSelect.disabled = !tableCount;
}

export function selectedCourse() {
  return state.academic?.courses.find((course) => course.id === courseSelect.value);
}

export function selectedTableAssignment() {
  const course = selectedCourse();
  return course?.table_assignments?.find(
    (assignment) =>
      assignment.user_id === state.user?.id &&
      assignment.group_id === groupSelect.value &&
      assignment.practice_id === practiceSelect.value,
  );
}

export async function loadSubmissionForm() {
  if (!measurementFields) return;
  if (canReview(state.user)) return;
  latestResult.classList.add("hidden");
  submitStatus.textContent = "";
  // El textarea de observaciones vive fuera de #measurement-fields (no se destruye al cambiar
  // de práctica): hay que vaciarlo a mano, salvo que se esté editando (ahí lo prellena applyPrefill).
  if (!state.editingSubmissionId && studentComment) studentComment.value = "";
  const practiceId = practiceSelect.value;
  const courseId = courseSelect.value;
  if (practicaTitle) {
    const practiceName =
      selectedCourse()?.practices.find((p) => p.id === practiceId)?.name ?? "Nueva entrega";
    practicaTitle.textContent = state.editingSubmissionId ? `Editar — ${practiceName}` : practiceName;
  }
  if (submitButton) submitButton.textContent = state.editingSubmissionId ? "Guardar cambios" : "Entregar";
  renderPartTabs(practiceId);
  if (!practiceId || !courseId) {
    state.practiceForm = null;
    measurementFields.innerHTML = "";
    return;
  }

  // Guard: si ya existe un informe para (práctica, grupo, mesa) mostrar aviso en lugar del form.
  if (!state.editingSubmissionId) {
    const blocked = await checkExistingReport(practiceId);
    if (blocked) return;
  }

  try {
    const [definition, instruments] = await Promise.all([
      fetchJson(`/api/practices/${encodeURIComponent(practiceId)}/definition`),
      fetchJson(`/api/instruments?course_id=${encodeURIComponent(courseId)}`),
    ]);
    state.practiceForm = { definition, instruments };
    // Form nuevo: descartá cronómetros/depuración de la práctica anterior para no dejar
    // instancias huérfanas (p. ej. claves `qid#i` de una config de operadores distinta).
    state.chronometers.clear();
    state.seriesDebug.clear();
    renderMeasurementFields();
    applyPrefill();
    applyDraftPrefill();
    applyPartVisibility();
  } catch (error) {
    state.practiceForm = null;
    measurementFields.innerHTML = `<p class="submission-meta">${escapeHtml(error.message)}</p>`;
  }
}

/** Verifica si ya existe un informe para la (práctica, grupo, mesa) seleccionada.
 *  Muestra el aviso correspondiente y devuelve `true` si el form debe bloquearse. */
async function checkExistingReport(practiceId) {
  const groupId = groupSelect.value;
  const tableNum = Number(tableSelect.value);
  if (!groupId || !tableNum) return false;
  try {
    const existing = await fetchJson(
      `/api/submissions/existing?practice_id=${encodeURIComponent(practiceId)}&group_id=${encodeURIComponent(groupId)}&table_number=${tableNum}`,
    );
    if (!existing) return false;
    const { submission_id, is_member, can_accept } = existing;
    if (is_member) {
      state.practiceForm = null;
      measurementFields.innerHTML = `
        <div class="edit-banner">
          <div>Ya sos miembro del informe de esta mesa.</div>
          <button type="button" class="view-existing-btn" data-id="${escapeHtml(submission_id)}">Ver informe</button>
        </div>`;
      measurementFields.querySelector(".view-existing-btn")?.addEventListener("click", (e) => {
        // Capturar el id ANTES del import(): currentTarget es null una vez despachado el evento.
        const id = e.currentTarget.dataset.id;
        import("./submissions.js").then(({ openSubmissionWorkspace }) =>
          openSubmissionWorkspace(id),
        );
      });
      return true;
    }
    if (can_accept) {
      state.practiceForm = null;
      measurementFields.innerHTML = `
        <div class="edit-banner">
          <div>Hay un informe para esta mesa. Podés aceptar la invitación para ver las medidas.</div>
          <button type="button" class="accept-existing-btn" data-id="${escapeHtml(submission_id)}">Aceptar invitación</button>
        </div>`;
      measurementFields.querySelector(".accept-existing-btn")?.addEventListener("click", async (e) => {
        const id = e.currentTarget.dataset.id;
        const { acceptInvitation } = await import("./invitations.js");
        await acceptInvitation(id);
        await loadSubmissionForm();
      });
      return true;
    }
    // Hay informe pero el alumno no está invitado ni es miembro
    state.practiceForm = null;
    measurementFields.innerHTML = `
      <div class="edit-banner">
        <div>Esta mesa ya tiene un informe. Si corresponde, pedile al docente que te agregue.</div>
      </div>`;
    return true;
  } catch {
    return false; // si falla el check, no bloquear
  }
}

// Parte temática activa de una práctica con PRACTICE_PARTS (tabs que solo alternan secciones
// de la misma definición, sin cambiar de práctica ni de entrega).
let activePart = null;

export function renderPartTabs(practiceId) {
  if (!practicePartTabs) return;

  // Partes internas de UNA práctica: las tabs muestran/ocultan secciones, no cambian de práctica.
  const innerParts = PRACTICE_PARTS[practiceId];
  if (innerParts) {
    if (!innerParts.some((p) => p.id === activePart)) activePart = innerParts[0].id;
    practicePartTabs.classList.remove("hidden");
    practicePartTabs.innerHTML = innerParts
      .map(
        (p) =>
          `<button type="button" class="part-tab ${p.id === activePart ? "active" : ""}" data-part-id="${escapeHtml(p.id)}">${escapeHtml(p.label)}</button>`
      )
      .join("");
    practicePartTabs.querySelectorAll(".part-tab").forEach((tab) => {
      tab.addEventListener("click", () => {
        activePart = tab.dataset.partId;
        practicePartTabs
          .querySelectorAll(".part-tab")
          .forEach((t) => t.classList.toggle("active", t === tab));
        applyPartVisibility();
      });
    });
    return;
  }

  const group = PRACTICE_GROUPS[practiceId]?.group;
  const enabled = selectedCourse()?.practices ?? [];
  const parts = enabled
    .filter((p) => PRACTICE_GROUPS[p.id]?.group === group && group)
    .sort((a, b) => PRACTICE_GROUPS[a.id].order - PRACTICE_GROUPS[b.id].order);

  if (parts.length < 2) {
    practicePartTabs.classList.add("hidden");
    practicePartTabs.innerHTML = "";
    return;
  }

  practicePartTabs.classList.remove("hidden");
  practicePartTabs.innerHTML = parts
    .map(
      (p) =>
        `<button type="button" class="part-tab ${p.id === practiceId ? "active" : ""}" data-practice-id="${escapeHtml(p.id)}">${escapeHtml(PRACTICE_GROUPS[p.id].label)}</button>`
    )
    .join("");

  practicePartTabs.querySelectorAll(".part-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      if (tab.dataset.practiceId === practiceSelect.value) return;
      exitEditMode();
      practiceSelect.value = tab.dataset.practiceId;
      practiceSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });
  });
}

/** Muestra solo los bloques `[data-section]` de la parte activa; los sin sección quedan siempre. */
function applyPartVisibility() {
  if (!PRACTICE_PARTS[practiceSelect.value]) return;
  measurementFields.querySelectorAll("[data-section]").forEach((el) => {
    el.hidden = el.dataset.section !== activePart;
  });
}

export function renderMeasurementFields() {
  if (!state.practiceForm) {
    measurementFields.innerHTML = "";
    return;
  }
  const { definition, instruments } = state.practiceForm;
  // El formulario arranca habilitado; los guards de abajo lo deshabilitan si la práctica no está
  // lista para entregar (p. ej. una curva sin curvas definidas).
  if (submitButton) submitButton.disabled = false;
  if (definition.quantities.length === 0) {
    measurementFields.innerHTML = `<p class="submission-meta">Esta practica todavia no tiene magnitudes definidas.</p>`;
    return;
  }

  // Una curva necesita al menos una curva definida; si no, no hay nada para graficar ni entregar.
  if (definition.analysis_kind === "curva" && (definition.curves?.length ?? 0) === 0) {
    measurementFields.innerHTML = `<p class="submission-meta">Esta práctica de curva todavía no tiene curvas definidas. Pedile al docente que las configure antes de entregar.</p>`;
    if (submitButton) submitButton.disabled = true;
    return;
  }

  if (definition.analysis_kind === "regresion_lineal" || definition.analysis_kind === "curva") {
    renderSeriesTable(definition);
    measurementFields.insertAdjacentHTML("beforeend", finalResultSectionHtml(definition));
    return;
  }

  // Motor D: en el estadístico, una práctica puede declarar N operadores. Las magnitudes repetidas
  // (tipo A) se cargan por operador; las dadas o de medida única se comparten.
  const operatorCount =
    definition.analysis_kind == null || definition.analysis_kind === "estadistico"
      ? definition.operator_count ?? 0
      : 0;
  const useOperators = operatorCount >= 2;
  const isPerOperator = (q) => useOperators && q.repeated && !q.is_given;
  const legendHtml = (q) => quantityNameHtml(q);

  // `opIndex` (número) marca el bloque de un operador; `null` para magnitudes compartidas.
  const measurementRowHtml = (q, opIndex) => {
    const opAttr = opIndex != null ? ` data-operator-index="${opIndex}"` : "";
    if (q.is_given) {
      // Sin has_uncertainty: dato de tabla sin incertidumbre propia (p. ej. un tiempo de
      // semiamplitud leído de una lectura única) — pide solo "Valor", sin instrumento ni U.
      const uField = !hasUncertainty(q)
        ? ""
        : `<label>Incertidumbre U (expandida)
              <div class="replica-input-wrap">
                ${prefixSelectHtml()}
                <input class="measure-given-u" type="number" step="any" min="0" placeholder="U" />
                <span class="replica-unit">${unitHtml(q.unit)}</span>
              </div>
            </label>`;
      return `
        <fieldset class="measurement-row measurement-row--given" data-quantity-id="${escapeHtml(q.id)}" data-is-given="1">
          <legend>${legendHtml(q)}</legend>
          <div class="form-grid">
            <label>Valor
              <div class="replica-input-wrap">
                ${prefixSelectHtml()}
                <input class="measure-given-value" type="number" step="any" placeholder="valor" />
                <span class="replica-unit">${unitHtml(q.unit)}</span>
              </div>
            </label>
            ${uField}
          </div>
        </fieldset>
      `;
    }
    if (q.repeated && q.quantity === "tiempo" && !state.editingSubmissionId) {
      const chronoOpts = compatibleInstruments(instruments, q.quantity);
      const defaultInst = chronoOpts.find((i) => /cron[oó]metro/i.test(i.name)) ?? chronoOpts[0];
      const chronoInstrumentOptions = [`<option value="">— sin instrumento —</option>`]
        .concat(
          chronoOpts.map(
            (i) =>
              `<option value="${escapeHtml(i.id)}" ${defaultInst && i.id === defaultInst.id ? "selected" : ""}>${escapeHtml(i.name)}</option>`
          )
        )
        .join("");
      return `
        <fieldset class="measurement-row measurement-row--chrono"
                  data-quantity-id="${escapeHtml(q.id)}" data-is-chrono="1"${opAttr}>
          <legend>${legendHtml(q)}</legend>
          <div class="measure-selectors" style="margin-bottom:8px;">
            <select class="measure-instrument" title="Instrumento" aria-label="Instrumento">${chronoInstrumentOptions}</select>
            <select class="measure-scale" title="Escala" aria-label="Escala"><option value="">sin escala</option></select>
          </div>
          ${chronoWidgetInnerHtml()}
          <div class="series-debug"></div>
        </fieldset>
      `;
    }
    const options = compatibleInstruments(instruments, q.quantity);
    const instrumentOptions = [`<option value="">— sin instrumento —</option>`]
      .concat(options.map((i) => `<option value="${escapeHtml(i.id)}">${escapeHtml(i.name)}</option>`))
      .join("");
    return `
      <fieldset class="measurement-row" data-quantity-id="${escapeHtml(q.id)}"${opAttr}>
        <legend>${legendHtml(q)}</legend>
        <div class="measure-body${q.repeated ? " measure-body--stacked" : ""}">
          <div class="measure-selectors">
            <select class="measure-instrument" title="Instrumento" aria-label="Instrumento">${instrumentOptions}</select>
            <select class="measure-scale" title="Escala" aria-label="Escala"><option value="">sin escala</option></select>
          </div>
          <div class="measure-sep"></div>
          <div class="measure-right">
            <div class="measure-values" data-repeated="${q.repeated ? "1" : "0"}">
              ${renderReplicaInput(q.unit)}
            </div>
            ${q.repeated ? `<button type="button" class="add-replica">＋ agregar réplica</button>` : ""}
          </div>
        </div>
      </fieldset>
    `;
  };

  // Render de una magnitud: por-operador (N bloques etiquetados) o una sola fila compartida.
  const quantityRowHtml = (q) => {
    if (!isPerOperator(q)) return measurementRowHtml(q, null);
    const blocks = Array.from(
      { length: operatorCount },
      (_, i) =>
        `<div class="operator-block"><h5 class="operator-label">Operador ${i + 1}</h5>${measurementRowHtml(q, i)}</div>`
    ).join("");
    return `
      <div class="operator-quantity" data-quantity-id="${escapeHtml(q.id)}">
        <h4 class="measurement-section-title">${quantityNameHtml(q)} <span class="submission-meta">— por operador</span></h4>
        ${blocks}
      </div>
    `;
  };

  const sections = PRACTICE_SECTIONS[practiceSelect.value];
  if (sections) {
    const { grouped, rest } = groupBySections(definition.quantities, sections);
    // Los resultados finales de una sección (p. ej. g1 en "Operador 1") se incrustan ahí mismo,
    // junto a la magnitud de la que salen (T1), en vez de amontonarse aparte al final.
    const allFinals = (definition.results ?? []).filter((r) => r.is_final);
    const embedded = new Set();
    const blocks = grouped.map(({ sec, rows }) => {
      if (rows.length === 0) return "";
      const helper = rows.some(needsChronoHelper) ? chronoHelperSectionHtml() : "";
      const secAttr = sec.id ? ` data-section="${escapeHtml(sec.id)}"` : "";
      // `!embedded.has` evita reventarlo dos veces si un símbolo quedara en el `results` de más
      // de una sección: gana la primera (mismo orden que PRACTICE_SECTIONS).
      const secFinals = allFinals.filter((r) => !embedded.has(r.symbol) && (sec.results ?? []).includes(r.symbol));
      secFinals.forEach((r) => embedded.add(r.symbol));
      // Se pasa `sec.id` explícito: es la sección donde esta fila realmente queda en el DOM, que
      // puede no coincidir con lo que `partForResult` (usada por el fallback de `leftoverFinals`)
      // encontraría si la sección "dueña" del símbolo en PRACTICE_SECTIONS no llegó a renderizar.
      const finalsHtml = secFinals.length
        ? `<h5 class="measurement-section-subtitle">Resultado final <span class="submission-meta">— opcional</span></h5>
           ${secFinals.map((r) => finalResultRowHtml(r, sec.id ?? null)).join("")}`
        : "";
      return `<div class="measurement-section"${secAttr}>
          <h4 class="measurement-section-title">${escapeHtml(sec.title)}</h4>
          ${rows.map(quantityRowHtml).join("")}
          ${helper}
          ${finalsHtml}
        </div>`;
    });
    const leftoverFinals = allFinals.filter((r) => !embedded.has(r.symbol));
    measurementFields.innerHTML =
      blocks.join("") + rest.map(quantityRowHtml).join("") + finalResultSectionHtml(definition, leftoverFinals);
  } else {
    const helper = definition.quantities.some(needsChronoHelper) ? chronoHelperSectionHtml() : "";
    measurementFields.innerHTML =
      definition.quantities.map(quantityRowHtml).join("") + helper + finalResultSectionHtml(definition);
  }
  wireChronoHelpers();

  measurementFields.querySelectorAll(".measurement-row:not([data-final-result])").forEach((row) => {
    if (row.dataset.isChrono === "1") {
      const chronoInstrument = row.querySelector(".measure-instrument");
      if (chronoInstrument) {
        chronoInstrument.addEventListener("change", () => populateScaleOptions(row));
        populateScaleOptions(row);
      }
      wireChronometerWidget(row, chronoKeyFor(row));
      return;
    }
    if (row.dataset.isGiven === "1") return;
    const instrumentSelect = row.querySelector(".measure-instrument");
    instrumentSelect.addEventListener("change", () => populateScaleOptions(row));
    row.querySelector(".add-replica")?.addEventListener("click", () => {
      const unit = row.querySelector(".measure-value")?.dataset.unit ?? "";
      row.querySelector(".measure-values").insertAdjacentHTML("beforeend", renderReplicaInput(unit));
      wireRemoveReplica(row);
    });
    wireRemoveReplica(row);
  });
}

/** Markup interno (sin fieldset) del widget de cronómetro: display, controles y modo. */
function chronoWidgetInnerHtml() {
  return `
    <div class="chrono-widget">
      <div class="chrono-display">0.000 s</div>
      <div class="chrono-info"><span class="chrono-count">0 marcas</span></div>
      <div class="chrono-controls">
        <button type="button" class="chrono-start">▶ Iniciar</button>
        <button type="button" class="chrono-mark" disabled>● Marcar</button>
        <button type="button" class="chrono-stop" disabled>■ Detener</button>
        <button type="button" class="chrono-reset">↺ Reiniciar</button>
      </div>
      <label class="chrono-mode-label">Modo:
        <select class="chrono-mode">
          <option value="periodo">Período (pares t₂-t₁, t₄-t₃… → técnica de Estadística)</option>
          <option value="consecutivo">Consecutivo (una marca por período)</option>
          <option value="pares">Pares solapados (marca cada T/2)</option>
          <option value="absoluto">Absoluto (tiempos desde inicio)</option>
        </select>
      </label>
      <div class="chrono-readings-preview"></div>
    </div>
  `;
}

/**
 * Cronómetro suelto de apoyo (no atado a ninguna magnitud): ayuda a tomar el tiempo para
 * después tipearlo a mano en el input que corresponda. No entra en `collectMeasurements`
 * (usa `.measurement-section`, no `.measurement-row`).
 */
function chronoHelperSectionHtml() {
  return `
    <div class="measurement-section chrono-helper" data-chrono-helper="1">
      <h4 class="measurement-section-title">Cronómetro <span class="submission-meta">— ayuda para tomar tiempos</span></h4>
      ${chronoWidgetInnerHtml()}
    </div>
  `;
}

/** Cablea todos los `.chrono-helper` presentes en el form con una clave única por instancia. */
function wireChronoHelpers() {
  measurementFields.querySelectorAll(".chrono-helper").forEach((el, i) => {
    wireChronometerWidget(el, `__chrono_helper__${i}`);
  });
}

/** `true` si esta magnitud se mide a mano (sin cronómetro propio) pero es un tiempo, y la
 *  práctica no está en `PRACTICES_WITHOUT_CHRONO_HELPER` (instrumento con lectura propia,
 *  p. ej. osciloscopio: relajación exponencial no cronometra T_oc/tmedio a mano). */
function needsChronoHelper(q) {
  if (PRACTICES_WITHOUT_CHRONO_HELPER.has(practiceSelect.value)) return false;
  return q.quantity === "tiempo" && !q.repeated && !q.is_given;
}

/** Parte temática (id de PRACTICE_PARTS) a la que pertenece un resultado final, o `null`. */
function partForResult(symbol) {
  const sections = PRACTICE_SECTIONS[practiceSelect.value] ?? [];
  return sections.find((sec) => sec.id && (sec.results ?? []).includes(symbol))?.id ?? null;
}

/** Fila de un resultado final (valor ± U), p. ej. `g`. Los resultados con `has_uncertainty:
 *  false` se entregan sin incertidumbre (sin campo U). `sectionId` es la parte donde se está
 *  incrustando esta fila (si el caller ya sabe en qué `<div data-section>` la está poniendo);
 *  sin ese dato se cae a `partForResult`, que puede no coincidir con dónde termina embebida si
 *  una sección "dueña" del símbolo no llegó a renderizar (ver `renderMeasurementFields`). */
function finalResultRowHtml(r, sectionId) {
  const part = sectionId !== undefined ? sectionId : partForResult(r.symbol);
  const uField = !hasUncertainty(r)
    ? ""
    : `
        <label>Incertidumbre U (expandida)
          <div class="replica-input-wrap">
            ${prefixSelectHtml()}
            <input class="final-result-u" type="number" step="any" min="0" placeholder="U" />
            <span class="replica-unit">${unitHtml(r.unit)}</span>
          </div>
        </label>`;
  return `
    <fieldset class="measurement-row" data-final-result="1" data-symbol="${escapeHtml(r.symbol)}"${part ? ` data-section="${escapeHtml(part)}"` : ""}>
      <legend>${symbolHtml(r.symbol)} <span class="submission-meta">${inlineMathHtml(r.name)}${r.unit ? ` (${unitHtml(r.unit)})` : ""}</span></legend>
      <div class="form-grid">
        <label>Valor
          <div class="replica-input-wrap">
            ${prefixSelectHtml()}
            <input class="final-result-value" type="number" step="any" placeholder="valor" />
            <span class="replica-unit">${unitHtml(r.unit)}</span>
          </div>
        </label>${uField}
      </div>
    </fieldset>`;
}

/** Sección opcional para que el alumno cargue sus resultados finales (valor ± U). `finals`
 *  por defecto son todos los de la definición; se puede pasar un subconjunto (p. ej. los que
 *  no quedaron ya incrustados en una sección temática, ver `renderMeasurementFields`). */
function finalResultSectionHtml(definition, finals) {
  const rows = finals ?? (definition.results ?? []).filter((r) => r.is_final);
  if (!rows.length) return "";
  return `
    <div class="measurement-section final-results-section">
      <h4 class="measurement-section-title">Resultado final <span class="submission-meta">— opcional</span></h4>
      <p class="submission-meta">Si ya calculaste tu resultado, cargalo acá. Podés dejarlo para más adelante; el docente puede cargarlo después.</p>
      ${rows.map((r) => finalResultRowHtml(r)).join("")}
    </div>
  `;
}

/** Recolecta los resultados finales cargados por el alumno junto con la entrega (si los hay). */
function collectFinalResults() {
  return [...measurementFields.querySelectorAll('[data-final-result="1"]')].reduce((acc, row) => {
    const [valPrefix, uPrefix] = [...row.querySelectorAll(".prefix-select")].map((s) => s.value);
    const rawVal = row.querySelector(".final-result-value").value.trim();
    if (rawVal === "") return acc;
    const value = Number(rawVal) * prefixFactor(valPrefix);
    if (!Number.isFinite(value)) return acc;
    // Sin campo U (resultado con has_uncertainty: false) va sin incertidumbre.
    const rawU = row.querySelector(".final-result-u")?.value.trim() ?? "";
    const u = rawU === "" ? null : Number(rawU) * prefixFactor(uPrefix);
    acc.push({ symbol: row.dataset.symbol, value, u_expanded: u != null && Number.isFinite(u) ? u : null });
    return acc;
  }, []);
}

function prefixSelectHtml() {
  const opts = SI_PREFIXES.map(
    (p) => `<option value="${escapeHtml(p.label)}" ${p.label === "" ? "selected" : ""}>${p.label || "—"}</option>`
  ).join("");
  return `<select class="prefix-select" title="Prefijo SI">${opts}</select>`;
}

/** Clave del cronómetro de una fila: por operador (`qid#i`) si tiene `data-operator-index`. */
function chronoKeyFor(row) {
  const op = row.dataset.operatorIndex;
  return op != null ? `${row.dataset.quantityId}#${op}` : row.dataset.quantityId;
}

export function renderReplicaInput(unit) {
  return `
    <div class="replica">
      ${prefixSelectHtml()}
      <input class="measure-value" type="number" step="any" placeholder="valor" data-unit="${escapeHtml(unit)}" />
      <span class="replica-unit">${unitHtml(unit)}</span>
      <button type="button" class="remove-replica" title="Quitar">✕</button>
    </div>
  `;
}

function wireRemoveReplica(row) {
  const replicas = row.querySelectorAll(".replica");
  row.querySelectorAll(".remove-replica").forEach((btn) => {
    btn.onclick = () => {
      if (row.querySelectorAll(".replica").length <= 1) return;
      btn.closest(".replica").remove();
    };
  });
  if (replicas.length === 1) {
    const only = replicas[0].querySelector(".remove-replica");
    if (only) only.style.visibility = "hidden";
  } else {
    row.querySelectorAll(".remove-replica").forEach((b) => (b.style.visibility = "visible"));
  }
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

export function collectMeasurements() {
  const seriesTable = measurementFields.querySelector(".series-table");
  if (seriesTable) {
    const quantityIds = [...seriesTable.querySelectorAll("th[data-quantity-id]")].map((th) => th.dataset.quantityId);
    // Magnitudes con grilla de réplicas por punto (tienen inputs .series-replica).
    const replicaIds = new Set(
      [...seriesTable.querySelectorAll(".series-replica")].map((i) => i.dataset.quantityId),
    );
    const singleValues = new Map(quantityIds.map((id) => [id, []]));
    const replicaPoints = new Map([...replicaIds].map((id) => [id, []]));
    seriesTable.querySelectorAll(".series-row").forEach((row) => {
      const cells = [...row.querySelectorAll(".series-cell")];
      // Parsea cada celda a un valor único o a una lista de réplicas; marca si está completa.
      const parsed = cells.map((cell) => {
        const replicaInput = cell.querySelector(".series-replica");
        if (replicaInput) {
          const reps = cellReplicaValues(cell);
          return {
            id: replicaInput.dataset.quantityId,
            replicas: reps,
            ok: reps.length > 0 && reps.every(Number.isFinite),
          };
        }
        const input = cell.querySelector(".series-value");
        const raw = input.value.trim();
        const factor = prefixFactor(cell.querySelector(".prefix-select").value);
        const v = raw === "" ? NaN : Number(raw) * factor;
        return { id: input.dataset.quantityId, value: v, ok: Number.isFinite(v) };
      });
      if (parsed.some((p) => !p.ok)) return; // fila incompleta: se ignora el punto
      parsed.forEach((p) => {
        if (p.replicas) replicaPoints.get(p.id).push(p.replicas);
        else singleValues.get(p.id).push(p.value);
      });
    });
    const series = quantityIds.map((id) =>
      replicaIds.has(id)
        ? { quantity_id: id, instrument_id: null, scale_id: null, values: [], given_u: null, point_replicas: replicaPoints.get(id) }
        : { quantity_id: id, instrument_id: null, scale_id: null, values: singleValues.get(id), given_u: null },
    );
    // Motor E: escalares compartidos (datos de cátedra / medida única), cargados una vez fuera de
    // la serie. Se recolectan como filas sueltas y se suman a las magnitudes por punto.
    const shared = [...measurementFields.querySelectorAll(".measurement-row:not([data-final-result])")].map(collectStandaloneRow);
    return [...series, ...shared];
  }

  // Motor D: magnitudes por operador → operator_replicas (una serie por bloque de operador).
  const out = [...measurementFields.querySelectorAll(".operator-quantity")].map((container) => {
    const rows = [...container.querySelectorAll(".measurement-row")].sort(
      (a, b) => Number(a.dataset.operatorIndex) - Number(b.dataset.operatorIndex)
    );
    return {
      quantity_id: container.dataset.quantityId,
      instrument_id: null,
      scale_id: null,
      values: [],
      given_u: null,
      operator_replicas: rows.map(rowSeriesValues),
    };
  });

  // Filas sueltas (compartidas o sin operadores): no están dentro de un contenedor por operador.
  const standalone = [...measurementFields.querySelectorAll(".measurement-row:not([data-final-result])")].filter(
    (row) => !row.closest(".operator-quantity")
  );
  for (const row of standalone) {
    out.push(collectStandaloneRow(row));
  }
  return out;
}

/// Recolecta una fila suelta: dato dado (valor ± U) o medida única/réplicas (instrumento/escala +
/// lecturas). Usada por el estadístico y por la sección de escalares compartidos de regresión.
function collectStandaloneRow(row) {
  if (row.dataset.isGiven === "1") {
    const valInput = row.querySelector(".measure-given-value");
    const uInput = row.querySelector(".measure-given-u");
    const [valPrefix, uPrefix] = [...row.querySelectorAll(".prefix-select")].map((s) => s.value);
    const rawVal = valInput.value.trim();
    // Sin campo U (magnitud `has_uncertainty: false`, p. ej. t_med): no hay nada que leer, va null.
    const rawU = uInput?.value.trim() ?? "";
    const value = rawVal === "" ? null : Number(rawVal) * prefixFactor(valPrefix);
    const given_u = rawU === "" ? null : Number(rawU) * prefixFactor(uPrefix);
    return {
      quantity_id: row.dataset.quantityId,
      instrument_id: null,
      scale_id: null,
      values: value != null && Number.isFinite(value) ? [value] : [],
      given_u: given_u != null && Number.isFinite(given_u) ? given_u : null,
    };
  }
  return {
    quantity_id: row.dataset.quantityId,
    instrument_id: row.querySelector(".measure-instrument")?.value || null,
    scale_id: row.querySelector(".measure-scale")?.value || null,
    values: rowSeriesValues(row),
    given_u: null,
  };
}

/** Lecturas numéricas de una fila de medición (cronómetro con descartes, o inputs de réplica). */
function rowSeriesValues(row) {
  if (row.dataset.isChrono === "1") {
    const mode = row.querySelector(".chrono-mode")?.value ?? "consecutivo";
    const key = chronoKeyFor(row);
    const chrono = state.chronometers.get(key);
    const all = chrono ? chrono.readings(mode) : [];
    const dbg = state.seriesDebug.get(key);
    return dbg ? all.filter((_, i) => !dbg.discarded.has(i)) : all;
  }
  return [...row.querySelectorAll(".replica")].reduce((acc, replica) => {
    const raw = replica.querySelector(".measure-value").value.trim();
    if (raw === "") return acc;
    const factor = prefixFactor(replica.querySelector(".prefix-select").value);
    const n = Number(raw) * factor;
    if (Number.isFinite(n)) acc.push(n);
    return acc;
  }, []);
}

function collectMeta() {
  const meta = {};
  measurementFields.querySelectorAll('.measurement-row[data-is-chrono="1"]').forEach((row) => {
    // Por operador, la clave de cronómetro/depuración es `qid#i` (ver chronoKeyFor).
    const key = chronoKeyFor(row);
    const dbg = state.seriesDebug.get(key);
    if (!dbg) return;
    const mode = row.querySelector(".chrono-mode")?.value ?? "consecutivo";
    const chrono = state.chronometers.get(key);
    const all = chrono ? chrono.readings(mode) : [];
    const discarded = [...dbg.discarded].filter((i) => i < all.length).map((i) => all[i]);
    if (discarded.length > 0 || (dbg.bins && dbg.bins > 0)) {
      meta[key] = { bins: dbg.bins || null, discarded };
    }
  });
  return Object.keys(meta).length ? meta : null;
}

function setSubmissionBusy(busy) {
  if (submitButton) submitButton.disabled = busy;
}

function buildMetaMap(measurements) {
  const map = {};
  const quantities = state.practiceForm?.definition?.quantities ?? [];
  for (const m of measurements) {
    // El nombre sale de la definición (robusto: las magnitudes por operador no tienen una única
    // fila con `legend`, sino N bloques bajo un contenedor). isGiven/isChrono salen de una fila real.
    const def = quantities.find((q) => q.id === m.quantity_id);
    const row = measurementFields.querySelector(
      `.measurement-row[data-quantity-id="${CSS.escape(m.quantity_id)}"]`
    );
    map[m.quantity_id] = {
      name: def?.name ?? row?.querySelector("legend")?.textContent?.trim() ?? m.quantity_id,
      isGiven: def?.is_given ?? row?.dataset.isGiven === "1",
      isChrono: row?.dataset.isChrono === "1",
      // En regresión/curva: las magnitudes con per_point=false (o dadas) son escalares compartidos.
      perPoint: def?.per_point ?? true,
      hasUncertainty: hasUncertainty(def),
      // Puede quedar sin lecturas sin bloquear el envío (p. ej. operador 2/3 opcional).
      optional: def?.optional ?? false,
    };
  }
  return map;
}

export async function submitFormSubmission() {
  if (!practiceSelect.value) return;

  const measurements = collectMeasurements();
  const analysisKind = state.practiceForm?.definition?.analysis_kind ?? "";
  const validationError = validateMeasurements(measurements, analysisKind, buildMetaMap(measurements));
  if (validationError) {
    submitStatus.textContent = validationError;
    return;
  }

  setSubmissionBusy(true);
  const editingId = state.editingSubmissionId;
  submitStatus.textContent = editingId ? "Guardando cambios..." : "Entregando...";
  try {
    if (editingId) {
      await postJson(`/api/submissions/${editingId}/edit`, {
        measurements,
        meta: collectMeta(),
        student_results: collectFinalResults(),
        student_comment: studentComment?.value.trim() || null,
      });
      submitStatus.textContent = "Cambios guardados";
      exitEditMode();
      await loadSubmissions();
      openSubmissionWorkspace(editingId);
      return;
    }
    const groupId = groupSelect.value;
    if (tableSelect.value) {
      await postJson(`/api/academic/groups/${groupId}/practice-table`, {
        practice_id: practiceSelect.value,
        table_number: Number(tableSelect.value),
      });
    }
    const submission = await postJson("/api/submissions/form", {
      course_id: courseSelect.value,
      group_id: groupId,
      practice_id: practiceSelect.value,
      measurements,
      meta: collectMeta(),
      student_results: collectFinalResults(),
      student_comment: studentComment?.value.trim() || null,
    });
    submitStatus.textContent = "Entrega guardada";
    clearDraft();
    const { renderAnalysis } = await import("./analysis.js");
    renderAnalysis(latestResult, submission);
    latestResult.classList.remove("hidden");
    await loadSubmissions();
  } catch (error) {
    submitStatus.textContent = error.message;
  } finally {
    setSubmissionBusy(false);
  }
}

export function startEditSubmission(submission) {
  state.editingSubmissionId = submission.id;
  state.editPrefill = submission.measurements ?? [];
  state.editPrefillStudentResults = submission.student_results ?? [];
  state.editPrefillComment = submission.student_comment ?? "";
  import("./navigation.js").then(({ selectPracticeFromNav }) => selectPracticeFromNav(submission.practice_id));
}

export function exitEditMode() {
  state.editingSubmissionId = null;
  state.editPrefillComment = null;
  state.editPrefill = null;
  state.editPrefillStudentResults = null;
}

/** Cancela una entrega dentro de la ventana de edición: la borra del servidor y devuelve al
 *  alumno al formulario de carga con todos los valores puestos, para que siga editando y vuelva
 *  a entregar sin re-tipear nada. Pide confirmación antes de borrar. `banner` es el `.edit-banner`
 *  que contiene el botón clickeado, para mostrar un error ahí mismo si falla el borrado. */
export async function cancelSubmission(submission, banner) {
  const confirmed = window.confirm(
    "¿Cancelar esta entrega? Se va a borrar del servidor; tus valores quedan cargados en el " +
      "formulario para que sigas editando. Esta acción no se puede deshacer.",
  );
  if (!confirmed) return;

  try {
    await deleteJson(`/api/submissions/${submission.id}`);
  } catch (error) {
    const status = banner?.querySelector(".edit-banner-status");
    if (status) status.textContent = error.message;
    return;
  }

  state.restoringCancelledSubmission = true;
  state.editPrefill = submission.measurements ?? [];
  state.editPrefillStudentResults = submission.student_results ?? [];
  state.editPrefillComment = submission.student_comment ?? "";

  const { selectView } = await import("./navigation.js");
  selectView("practica");
  courseSelect.value = submission.course_id ?? courseSelect.value;
  updateStudentSelectors({ autoLoad: false });
  groupSelect.value = submission.group_id ?? groupSelect.value;
  practiceSelect.value = submission.practice_id;
  updateTableSelector();
  if (submission.table_number != null) tableSelect.value = String(submission.table_number);
  await loadSubmissionForm();

  state.restoringCancelledSubmission = false;
  state.editPrefill = null;
  state.editPrefillStudentResults = null;
  state.editPrefillComment = null;

  await loadSubmissions();
}

function editPrefillByQuantity() {
  const map = new Map();
  for (const m of state.editPrefill ?? []) {
    let e = map.get(m.quantity_id);
    if (!e) {
      e = {
        points: new Map(),
        operators: new Map(),
        instrument_id: m.instrument_id,
        scale_id: m.scale_id,
        value_u: m.value_u,
      };
      map.set(m.quantity_id, e);
    }
    const pidx = m.point_index ?? 0;
    if (!e.points.has(pidx)) e.points.set(pidx, []);
    e.points.get(pidx).push(m.value);
    const oidx = m.operator_index ?? 0;
    if (!e.operators.has(oidx)) e.operators.set(oidx, []);
    e.operators.get(oidx).push(m.value);
    if (m.value_u != null) e.value_u = m.value_u;
  }
  // Normaliza a `pointGroups` (réplicas por punto) y `operatorGroups` (réplicas por operador),
  // ambas ordenadas por índice; `values` es la lista plana (estadístico de una sola serie).
  for (const e of map.values()) {
    const pIdx = [...e.points.keys()].sort((a, b) => a - b);
    e.pointGroups = pIdx.map((i) => e.points.get(i));
    const oIdx = [...e.operators.keys()].sort((a, b) => a - b);
    e.operatorGroups = oIdx.map((i) => e.operators.get(i));
    e.values = e.pointGroups.flat();
    delete e.points;
    delete e.operators;
  }
  return map;
}

/** Prellena el bloque opcional "Resultado final" con `results` (lista `{symbol, value, u_expanded}`). */
function applyFinalResultsPrefillFrom(results) {
  const saved = new Map((results ?? []).map((s) => [s.symbol, s]));
  measurementFields.querySelectorAll('[data-final-result="1"]').forEach((row) => {
    const s = saved.get(row.dataset.symbol);
    if (!s) return;
    row.querySelector(".final-result-value").value = s.value;
    const uInput = row.querySelector(".final-result-u");
    if (uInput && s.u_expanded != null) uInput.value = s.u_expanded;
  });
}

/** Restaura una entrega en edición (`applyPrefill`) desde `state.editPrefill*`. */
export function applyPrefill() {
  if (!state.editingSubmissionId && !state.restoringCancelledSubmission) return;
  applyMeasurementPrefill(
    editPrefillByQuantity(),
    state.editPrefillStudentResults,
    state.editPrefillComment,
  );
}

/** Restaura el borrador local guardado para la (curso, grupo, mesa, práctica) actual, si hay uno
 *  y no se está editando/restaurando una entrega existente (esos casos los maneja `applyPrefill`). */
function applyDraftPrefill() {
  if (state.editingSubmissionId || state.restoringCancelledSubmission) return;
  const draft = loadDraft();
  if (!draft) return;
  applyMeasurementPrefill(
    draftMeasurementsByQuantity(draft.measurements),
    draft.finalResults,
    draft.comment,
  );
}

/** Pinta en el DOM un `byQ` (Map quantity_id -> {pointGroups, operatorGroups, values, value_u,
 *  instrument_id, scale_id}) ya armado, sin importar si viene de una entrega guardada
 *  (`editPrefillByQuantity`) o de un borrador local (`draftMeasurementsByQuantity`). */
function applyMeasurementPrefill(byQ, finalResults, comment) {
  applyFinalResultsPrefillFrom(finalResults);
  if (studentComment) studentComment.value = comment ?? "";

  const seriesTable = measurementFields.querySelector(".series-table");
  if (seriesTable) {
    const qids = [...seriesTable.querySelectorAll("th[data-quantity-id]")].map((th) => th.dataset.quantityId);
    const nPoints = Math.max(...qids.map((id) => byQ.get(id)?.pointGroups.length ?? 0), 0);
    // Solo las columnas por punto (las compartidas se rellenan aparte, abajo).
    const cols = state.practiceForm.definition.quantities.filter((q) => q.per_point && !q.is_given);
    const tbody = seriesTable.querySelector("tbody");
    tbody.innerHTML = Array.from({ length: Math.max(nPoints, 1) }, () => seriesRowHtml(cols)).join("");
    wireSeriesRemove();
    [...tbody.querySelectorAll(".series-row")].forEach((row, i) => {
      // Columnas de un valor por punto.
      row.querySelectorAll(".series-value").forEach((input) => {
        const v = byQ.get(input.dataset.quantityId)?.pointGroups[i]?.[0];
        if (v != null) input.value = v;
      });
      // Columnas con grilla de réplicas: rellena cada input del punto i.
      row.querySelectorAll(".series-cell--replicas").forEach((cell) => {
        const id = cell.querySelector(".series-replica")?.dataset.quantityId;
        const reps = byQ.get(id)?.pointGroups[i] ?? [];
        const group = cell.querySelector(".series-replica-group");
        // Si la entrega guardó más réplicas que el ancho actual de la grilla (el docente redujo
        // replicas_per_point luego de cargarse), agrega inputs para no perder datos al editar.
        let inputs = [...cell.querySelectorAll(".series-replica")];
        while (group && inputs.length < reps.length) {
          group.insertAdjacentHTML("beforeend", replicaInputHtml(id, inputs.length));
          inputs = [...cell.querySelectorAll(".series-replica")];
        }
        inputs.forEach((input, k) => {
          if (reps[k] != null) input.value = reps[k];
        });
      });
    });
    updateSeriesMeans();
    updateSeriesLive();
    // Escalares compartidos (Motor E): se rellenan como filas sueltas fuera de la serie.
    measurementFields
      .querySelectorAll(".shared-quantities .measurement-row")
      .forEach((row) => prefillStandaloneRow(row, byQ));
    return;
  }

  // Motor D: magnitudes por operador → rellena cada bloque con la serie de ese operador.
  measurementFields.querySelectorAll(".operator-quantity").forEach((groupEl) => {
    const e = byQ.get(groupEl.dataset.quantityId);
    if (!e) return;
    const blocks = [...groupEl.querySelectorAll(".measurement-row")].sort(
      (a, b) => Number(a.dataset.operatorIndex) - Number(b.dataset.operatorIndex)
    );
    blocks.forEach((row, i) => fillReplicaRow(row, e, e.operatorGroups[i] ?? []));
  });

  // Filas sueltas (compartidas o sin operadores).
  const standalone = [...measurementFields.querySelectorAll(".measurement-row:not([data-final-result])")].filter(
    (row) => !row.closest(".operator-quantity")
  );
  for (const row of standalone) {
    prefillStandaloneRow(row, byQ);
  }
}

/// Rellena una fila suelta (dato dado o medida única/réplicas) desde el prefill de edición.
function prefillStandaloneRow(row, byQ) {
  const e = byQ.get(row.dataset.quantityId);
  if (!e) return;
  if (row.dataset.isGiven === "1") {
    const v = row.querySelector(".measure-given-value");
    const u = row.querySelector(".measure-given-u");
    if (v) v.value = e.values[0] ?? "";
    if (u && e.value_u != null) u.value = e.value_u;
    return;
  }
  fillReplicaRow(row, e, e.values);
}

/** Rellena una fila de réplicas con `values`, restaurando instrumento/escala desde el prefill. */
function fillReplicaRow(row, e, values) {
  const inst = row.querySelector(".measure-instrument");
  if (inst && e.instrument_id) {
    inst.value = e.instrument_id;
    populateScaleOptions(row);
  }
  const scale = row.querySelector(".measure-scale");
  if (scale && e.scale_id) scale.value = e.scale_id;
  const container = row.querySelector(".measure-values");
  if (!container) return;
  const unit = row.querySelector(".measure-value")?.dataset.unit ?? "";
  while (container.querySelectorAll(".replica").length < values.length) {
    container.insertAdjacentHTML("beforeend", renderReplicaInput(unit));
  }
  wireRemoveReplica(row);
  container.querySelectorAll(".measure-value").forEach((input, i) => {
    if (values[i] != null) input.value = values[i];
  });
}

function renderSeriesTable(definition) {
  // Motor E: separa las magnitudes que se miden por punto (van en la serie) de los escalares
  // compartidos (datos de cátedra / medida única), que se cargan una sola vez.
  const cols = definition.quantities.filter((q) => q.per_point && !q.is_given);
  const shared = definition.quantities.filter((q) => !q.per_point || q.is_given);
  const liveCols = SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [];
  const header = cols
    .map((q) => `<th data-quantity-id="${escapeHtml(q.id)}">${symbolHtml(q.symbol)}${q.unit ? ` <span class="submission-meta">(${unitHtml(q.unit)})</span>` : ""}</th>`)
    .join("") + liveCols
    .map((c) => `<th>${symbolHtml(c.symbol)}${c.unit ? ` <span class="submission-meta">(${unitHtml(c.unit)})</span>` : ""}</th>`)
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
      blocks.push(`<div class="shared-quantities"><h4>Datos compartidos</h4>${rest.map((q) => sharedRowHtml(q)).join("")}</div>`);
    }
    sharedSection = blocks.join("");
  } else if (shared.length) {
    sharedSection = `<div class="shared-quantities"><h4>Datos compartidos</h4>${shared.map((q) => sharedRowHtml(q)).join("")}</div>`;
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
      <p class="submission-meta">Cargá un punto por fila. Las filas incompletas se ignoran. Hacen falta al menos 2 puntos para el ajuste.</p>
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
  measurementFields.querySelectorAll(".shared-quantities").forEach((sharedEl) => {
    sharedEl.addEventListener("input", schedulePreview);
    sharedEl.addEventListener("change", schedulePreview);
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

function seriesRowHtml(cols) {
  const cells = cols
    .map((q) => {
      const n = q.repeated ? Number(q.replicas_per_point) || 0 : 0;
      if (n > 0) {
        const inputs = Array.from({ length: n }, (_, k) => replicaInputHtml(q.id, k)).join("");
        return `<td class="series-cell series-cell--replicas"><div class="series-input-wrap">${prefixSelectHtml()}<div class="series-replica-group">${inputs}</div></div><span class="series-mean submission-meta">x̄ —</span></td>`;
      }
      return `<td class="series-cell"><div class="series-input-wrap">${prefixSelectHtml()}<input class="series-value" type="number" step="any" data-quantity-id="${escapeHtml(q.id)}" placeholder="valor" /></div></td>`;
    })
    .join("");
  // Columnas calculadas en vivo: solo lectura y sin clase `series-cell`, para que
  // collectMeasurements no las cuente como parte del punto.
  const liveCells = (SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [])
    .map((c) => `<td class="series-live" data-live-symbol="${escapeHtml(c.symbol)}"><span class="series-live-value submission-meta">—</span></td>`)
    .join("");
  return `<tr class="series-row">${cells}${liveCells}<td><button type="button" class="remove-series-row" title="Quitar">✕</button></td></tr>`;
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

/** HTML de un input de réplica (índice 0-based `k`) para la magnitud `quantityId`. */
function replicaInputHtml(quantityId, k) {
  return `<input class="series-replica" type="number" step="any" data-quantity-id="${escapeHtml(quantityId)}" placeholder="valor ${k + 1}" />`;
}

/** Lee las réplicas no vacías de una celda de réplicas, aplicando el prefijo SI de la celda. */
function cellReplicaValues(cell) {
  const factor = prefixFactor(cell.querySelector(".prefix-select").value);
  return [...cell.querySelectorAll(".series-replica")]
    .map((input) => input.value.trim())
    .filter((raw) => raw !== "")
    .map((raw) => Number(raw) * factor);
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

/** Recalcula las columnas en vivo (p. ej. P = I²·R) de cada fila de la tabla de series. */
function updateSeriesLive() {
  const liveCols = SERIES_LIVE_COLUMNS[practiceSelect.value] ?? [];
  if (!liveCols.length) return;
  const quantities = state.practiceForm?.definition?.quantities ?? [];
  const idBySymbol = new Map(quantities.map((q) => [q.symbol, q.id]));
  measurementFields.querySelectorAll(".series-row").forEach((row) => {
    for (const col of liveCols) {
      const cell = row.querySelector(`.series-live[data-live-symbol="${CSS.escape(col.symbol)}"]`);
      const out = cell?.querySelector(".series-live-value");
      if (!out) continue;
      const args = col.inputs.map((sym) => seriesCellValue(row, idBySymbol.get(sym) ?? ""));
      const value = args.every(Number.isFinite) ? pointPower(...args) : NaN;
      out.textContent = Number.isFinite(value) ? format(value) : "—";
    }
  });
}

/** Actualiza el promedio (x̄) mostrado en cada celda de réplicas de la tabla de series. */
function updateSeriesMeans() {
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

function wireSeriesRemove() {
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

// ── Cronómetro ────────────────────────────────────────────────────────────────

function formatElapsed(seconds) {
  const total = Math.max(0, seconds);
  const m = Math.floor(total / 60);
  const s = Math.floor(total % 60);
  const ms = Math.round((total % 1) * 1000);
  return m > 0
    ? `${m}:${String(s).padStart(2, "0")}.${String(ms).padStart(3, "0")} s`
    : `${s}.${String(ms).padStart(3, "0")} s`;
}

function wireChronometerWidget(row, quantityId) {
  if (!state.chronometers.has(quantityId)) {
    state.chronometers.set(quantityId, new Chronometer());
  }
  const chrono = state.chronometers.get(quantityId);

  const display = row.querySelector(".chrono-display");
  const countEl = row.querySelector(".chrono-count");
  const startBtn = row.querySelector(".chrono-start");
  const markBtn = row.querySelector(".chrono-mark");
  const stopBtn = row.querySelector(".chrono-stop");
  const resetBtn = row.querySelector(".chrono-reset");
  const modeSelect = row.querySelector(".chrono-mode");
  const preview = row.querySelector(".chrono-readings-preview");

  let rafId = null;

  function updateButtons() {
    const s = chrono.state;
    startBtn.disabled = s !== "idle";
    markBtn.disabled = s !== "running";
    stopBtn.disabled = s !== "running";
    resetBtn.disabled = s === "running";
  }

  function updatePreview() {
    const mode = modeSelect.value;
    const r = chrono.readings(mode);
    countEl.textContent = `${chrono.count} marca${chrono.count !== 1 ? "s" : ""} → ${r.length} lectura${r.length !== 1 ? "s" : ""}`;
    if (r.length === 0) {
      preview.textContent = "";
      return;
    }
    const shown = r.slice(0, 8).map((v) => v.toFixed(3)).join(", ");
    preview.textContent = r.length > 8 ? `${shown} … (+${r.length - 8} más)` : shown;
  }

  function tick() {
    display.textContent = formatElapsed(chrono.elapsed);
    updatePreview();
    if (chrono.state === "running") {
      rafId = requestAnimationFrame(tick);
    }
  }

  function stopRaf() {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  }

  const debugContainer = row.querySelector(".series-debug");
  function refreshDebug() {
    renderSeriesDebug(row, quantityId, chrono.readings(modeSelect.value));
  }

  display.textContent = formatElapsed(chrono.elapsed);
  updateButtons();
  updatePreview();
  if (chrono.state === "running") rafId = requestAnimationFrame(tick);
  else refreshDebug();

  startBtn.addEventListener("click", () => {
    chrono.start();
    updateButtons();
    if (debugContainer) debugContainer.innerHTML = "";
    rafId = requestAnimationFrame(tick);
  });
  markBtn.addEventListener("click", () => {
    chrono.mark();
    updatePreview();
  });
  stopBtn.addEventListener("click", () => {
    chrono.stop();
    stopRaf();
    display.textContent = formatElapsed(chrono.elapsed);
    updateButtons();
    updatePreview();
    refreshDebug();
  });
  resetBtn.addEventListener("click", () => {
    chrono.reset();
    stopRaf();
    display.textContent = formatElapsed(0);
    updateButtons();
    updatePreview();
    state.seriesDebug.delete(quantityId);
    if (debugContainer) debugContainer.innerHTML = "";
  });
  modeSelect.addEventListener("change", () => {
    state.seriesDebug.delete(quantityId);
    updatePreview();
    if (chrono.state !== "running") refreshDebug();
  });

  row._chronoKeyHandler = (e) => {
    if (e.code === "Space" && e.target.tagName !== "BUTTON" && e.target.tagName !== "SELECT") {
      e.preventDefault();
      chrono.mark();
      updatePreview();
    }
  };
  document.addEventListener("keydown", row._chronoKeyHandler);

  new MutationObserver(() => {
    if (!document.contains(row)) {
      document.removeEventListener("keydown", row._chronoKeyHandler);
      stopRaf();
    }
  }).observe(measurementFields, { childList: true, subtree: false });
}

function renderSeriesDebug(row, quantityId, readings) {
  const container = row.querySelector(".series-debug");
  if (!container) return;
  if (!readings || readings.length === 0) {
    container.innerHTML = "";
    return;
  }
  let dbg = state.seriesDebug.get(quantityId);
  if (!dbg) {
    dbg = { discarded: new Set(), bins: 0 };
    state.seriesDebug.set(quantityId, dbg);
  }
  dbg.discarded = new Set([...dbg.discarded].filter((i) => i < readings.length));

  const kept = readings.filter((_, i) => !dbg.discarded.has(i));
  const stats = seriesStats(kept);
  const defaultBins = Math.max(1, Math.min(20, Math.round(Math.sqrt(kept.length || 1))));
  const bins = dbg.bins && dbg.bins > 0 ? dbg.bins : defaultBins;
  const hist = kept.length > 0 ? histogram(kept, bins) : null;

  const ordered = readings.map((v, i) => ({ v, i })).sort((a, b) => a.v - b.v);
  const items = ordered
    .map(({ v, i }) => {
      const off = dbg.discarded.has(i);
      return `<li class="series-point ${off ? "discarded" : ""}">
        <span class="series-point-value">${v.toFixed(3)} s</span>
        <button type="button" class="series-point-toggle" data-index="${i}">${off ? "restaurar" : "descartar"}</button>
      </li>`;
    })
    .join("");

  container.innerHTML = `
    <div class="series-debug-head">
      <strong>Depuración de la serie</strong>
      <span class="submission-meta">n=${stats.n} · x̄=${Number.isFinite(stats.mean) ? formatSeriesStat(stats.mean) : "—"} s · s=${Number.isFinite(stats.std) ? formatSeriesStat(stats.std) : "—"} s · s/√n=${Number.isFinite(stats.stdMean) ? formatSeriesStat(stats.stdMean) : "—"} s</span>
    </div>
    <div class="series-debug-grid">
      <div class="series-hist">
        <label class="hist-bins-label">Intervalos (bins):
          <input type="number" class="hist-bins" min="1" max="40" value="${bins}" />
        </label>
        ${hist ? histogramSvg(hist, stats.mean, stats.std, kept.length) : `<p class="submission-meta">Sin datos conservados.</p>`}
      </div>
      <ol class="series-point-list">${items}</ol>
    </div>
  `;

  container.querySelector(".hist-bins")?.addEventListener("change", (e) => {
    const n = Math.round(Number(e.target.value));
    dbg.bins = Number.isFinite(n) && n >= 1 ? n : 0;
    renderSeriesDebug(row, quantityId, readings);
  });
  container.querySelectorAll(".series-point-toggle").forEach((btn) => {
    btn.addEventListener("click", () => {
      const i = Number(btn.dataset.index);
      if (dbg.discarded.has(i)) dbg.discarded.delete(i);
      else dbg.discarded.add(i);
      renderSeriesDebug(row, quantityId, readings);
    });
  });
}

function histogramSvg(hist, mean, std, n) {
  const W = 340;
  const H = 180;
  const pad = 28;
  const innerW = W - 2 * pad;
  const innerH = H - 2 * pad;
  const { min, max, width, counts } = hist;
  const curve = std > 0 ? normalCurve(mean, std, min, max, 80) : [];
  const curveCounts = curve.map(([x, y]) => [x, y * n * width]);
  const maxCount = Math.max(...counts, ...curveCounts.map((p) => p[1]), 1);
  const spanX = max - min || 1;
  const sx = (x) => pad + ((x - min) / spanX) * innerW;
  const sy = (c) => H - pad - (c / maxCount) * innerH;
  const bars = counts
    .map((c, i) => {
      const x0 = sx(min + i * width);
      const x1 = sx(min + (i + 1) * width);
      const y = sy(c);
      const w = Math.max(0, x1 - x0 - 1);
      return `<rect x="${x0.toFixed(1)}" y="${y.toFixed(1)}" width="${w.toFixed(1)}" height="${(H - pad - y).toFixed(1)}" class="hist-bar" />`;
    })
    .join("");
  const poly = curveCounts.map(([x, c]) => `${sx(x).toFixed(1)},${sy(c).toFixed(1)}`).join(" ");
  const curveEl = poly ? `<polyline points="${poly}" class="normal-curve" fill="none" />` : "";
  return `<svg viewBox="0 0 ${W} ${H}" class="histogram" role="img" aria-label="Histograma con curva normal">
    ${bars}${curveEl}
    <line x1="${pad}" y1="${H - pad}" x2="${W - pad}" y2="${H - pad}" class="hist-axis" />
  </svg>`;
}

// ── Borrador local ───────────────────────────────────────────────────────────
// Autoguarda lo que el alumno va tipeando en una entrega NUEVA (no enviada aún) para que un
// cambio de práctica/curso/grupo accidental, o un refresh de página, no pierda los valores.
// No aplica mientras se edita/restaura una entrega existente (`applyPrefill` cubre esos casos).

function draftKey() {
  // Sin la mesa: `updateTableSelector()` reconstruye #table-select en cada cambio de práctica y
  // vuelve al valor por defecto (asignación/perfil), así que no es estable mientras se compone.
  const uid = state.user?.id ?? "anon";
  return `quantify-draft:${uid}:${courseSelect.value}:${groupSelect.value}:${practiceSelect.value}`;
}

function saveDraft() {
  if (state.editingSubmissionId || state.restoringCancelledSubmission || !state.practiceForm) return;
  const draft = {
    measurements: collectMeasurements(),
    finalResults: collectFinalResults(),
    comment: studentComment?.value ?? "",
    savedAt: Date.now(),
  };
  try {
    localStorage.setItem(draftKey(), JSON.stringify(draft));
  } catch {
    // localStorage puede fallar (cuota, modo privado); el borrador es best-effort.
  }
}

function loadDraft() {
  try {
    const raw = localStorage.getItem(draftKey());
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function clearDraft() {
  try {
    localStorage.removeItem(draftKey());
  } catch {
    // no-op
  }
}

let draftSaveTimer = null;
function scheduleDraftSave() {
  clearTimeout(draftSaveTimer);
  draftSaveTimer = setTimeout(saveDraft, 350);
}

// ── Listeners top-level ────────────────────────────────────────────────────────

// "Entregar" (submit del form / Enter): crea la entrega por formulario.
submissionForm.addEventListener("submit", (event) => {
  event.preventDefault();
  submitFormSubmission();
});
// `measurementFields` es un nodo estable (solo se reemplaza su innerHTML, nunca el nodo): un
// único listener delegado alcanza para autoguardar el borrador sin tocar los renders.
measurementFields.addEventListener("input", scheduleDraftSave);
measurementFields.addEventListener("change", scheduleDraftSave);
studentComment?.addEventListener("input", scheduleDraftSave);
courseSelect.addEventListener("change", updateStudentSelectors);
groupSelect.addEventListener("change", updateTableSelector);
practiceSelect.addEventListener("change", () => {
  updateTableSelector();
  loadSubmissionForm();
});
