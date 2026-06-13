import { state } from "./state.js";
import {
  courseSelect, groupSelect, practiceSelect, tableSelect,
  measurementFields, latestResult, submitStatus, submitButton,
  practicaTitle, practicePartTabs, submissionForm,
} from "./dom.js";
import { fetchJson, postJson } from "./api.js";
import {
  escapeHtml, canReview,
  compatibleInstruments, SI_PREFIXES, prefixFactor,
  seriesStats, histogram, normalCurve, validateMeasurements,
} from "./lib.js";
import { PRACTICE_GROUPS, PRACTICE_SECTIONS } from "./constants.js";
import { Chronometer } from "./chronometer.js";
import { loadSubmissions, openSubmissionWorkspace } from "./submissions.js";

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

export function updateStudentSelectors() {
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
  loadSubmissionForm();
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
    renderMeasurementFields();
    applyPrefill();
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
        import("./submissions.js").then(({ openSubmissionWorkspace }) =>
          openSubmissionWorkspace(e.currentTarget.dataset.id),
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
        const { acceptInvitation } = await import("./invitations.js");
        await acceptInvitation(e.currentTarget.dataset.id);
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

export function renderPartTabs(practiceId) {
  if (!practicePartTabs) return;
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

export function renderMeasurementFields() {
  if (!state.practiceForm) {
    measurementFields.innerHTML = "";
    return;
  }
  const { definition, instruments } = state.practiceForm;
  if (definition.quantities.length === 0) {
    measurementFields.innerHTML = `<p class="submission-meta">Esta practica todavia no tiene magnitudes definidas.</p>`;
    return;
  }

  if (definition.analysis_kind === "regresion_lineal") {
    renderSeriesTable(definition);
    return;
  }

  const quantityRowHtml = (q) => {
    if (q.is_given) {
      return `
        <fieldset class="measurement-row measurement-row--given" data-quantity-id="${escapeHtml(q.id)}" data-is-given="1">
          <legend>${escapeHtml(q.name)} <span class="submission-meta">(dato — ${escapeHtml(q.symbol)}, ${escapeHtml(q.unit)})</span></legend>
          <div class="form-grid">
            <label>Valor
              <div class="replica-input-wrap">
                ${prefixSelectHtml()}
                <input class="measure-given-value" type="number" step="any" placeholder="valor" />
                <span class="replica-unit">${escapeHtml(q.unit)}</span>
              </div>
            </label>
            <label>Incertidumbre U (expandida)
              <div class="replica-input-wrap">
                ${prefixSelectHtml()}
                <input class="measure-given-u" type="number" step="any" min="0" placeholder="U" />
                <span class="replica-unit">${escapeHtml(q.unit)}</span>
              </div>
            </label>
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
                  data-quantity-id="${escapeHtml(q.id)}" data-is-chrono="1">
          <legend>${escapeHtml(q.name)} <span class="submission-meta">(${escapeHtml(q.symbol)}, ${escapeHtml(q.unit)})</span></legend>
          <div class="measure-selectors" style="margin-bottom:8px;">
            <select class="measure-instrument" title="Instrumento" aria-label="Instrumento">${chronoInstrumentOptions}</select>
            <select class="measure-scale" title="Escala" aria-label="Escala"><option value="">sin escala</option></select>
          </div>
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
          <div class="series-debug"></div>
        </fieldset>
      `;
    }
    const options = compatibleInstruments(instruments, q.quantity);
    const instrumentOptions = [`<option value="">— sin instrumento —</option>`]
      .concat(options.map((i) => `<option value="${escapeHtml(i.id)}">${escapeHtml(i.name)}</option>`))
      .join("");
    return `
      <fieldset class="measurement-row" data-quantity-id="${escapeHtml(q.id)}">
        <legend>${escapeHtml(q.name)} <span class="submission-meta">(${escapeHtml(q.symbol)}, ${escapeHtml(q.unit)})</span></legend>
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

  const sections = PRACTICE_SECTIONS[practiceSelect.value];
  if (sections) {
    const used = new Set();
    const blocks = sections.map((sec) => {
      const rows = sec.symbols
        .map((sym) => definition.quantities.find((q) => q.symbol === sym))
        .filter(Boolean);
      rows.forEach((q) => used.add(q.id));
      if (rows.length === 0) return "";
      return `<div class="measurement-section">
          <h4 class="measurement-section-title">${escapeHtml(sec.title)}</h4>
          ${rows.map(quantityRowHtml).join("")}
        </div>`;
    });
    const rest = definition.quantities.filter((q) => !used.has(q.id));
    measurementFields.innerHTML = blocks.join("") + rest.map(quantityRowHtml).join("");
  } else {
    measurementFields.innerHTML = definition.quantities.map(quantityRowHtml).join("");
  }

  measurementFields.querySelectorAll(".measurement-row").forEach((row) => {
    if (row.dataset.isChrono === "1") {
      const chronoInstrument = row.querySelector(".measure-instrument");
      if (chronoInstrument) {
        chronoInstrument.addEventListener("change", () => populateScaleOptions(row));
        populateScaleOptions(row);
      }
      wireChronometerWidget(row, row.dataset.quantityId);
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

function prefixSelectHtml() {
  const opts = SI_PREFIXES.map(
    (p) => `<option value="${escapeHtml(p.label)}" ${p.label === "" ? "selected" : ""}>${p.label || "—"}</option>`
  ).join("");
  return `<select class="prefix-select" title="Prefijo SI">${opts}</select>`;
}

export function renderReplicaInput(unit) {
  return `
    <div class="replica">
      ${prefixSelectHtml()}
      <input class="measure-value" type="number" step="any" placeholder="lectura" data-unit="${escapeHtml(unit)}" />
      <span class="replica-unit">${escapeHtml(unit)}</span>
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
    const byQuantity = new Map(quantityIds.map((id) => [id, []]));
    seriesTable.querySelectorAll(".series-row").forEach((row) => {
      const cells = [...row.querySelectorAll(".series-cell")];
      const parsed = cells.map((cell) => {
        const raw = cell.querySelector(".series-value").value.trim();
        const factor = prefixFactor(cell.querySelector(".prefix-select").value);
        return raw === "" ? NaN : Number(raw) * factor;
      });
      if (parsed.some((n) => !Number.isFinite(n))) return;
      cells.forEach((cell, i) => byQuantity.get(cell.querySelector(".series-value").dataset.quantityId).push(parsed[i]));
    });
    return quantityIds.map((id) => ({
      quantity_id: id,
      instrument_id: null,
      scale_id: null,
      values: byQuantity.get(id),
      given_u: null,
    }));
  }

  return [...measurementFields.querySelectorAll(".measurement-row")].map((row) => {
    if (row.dataset.isGiven === "1") {
      const valInput = row.querySelector(".measure-given-value");
      const uInput = row.querySelector(".measure-given-u");
      const [valPrefix, uPrefix] = [...row.querySelectorAll(".prefix-select")].map((s) => s.value);
      const rawVal = valInput.value.trim();
      const rawU = uInput.value.trim();
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

    if (row.dataset.isChrono === "1") {
      const mode = row.querySelector(".chrono-mode")?.value ?? "consecutivo";
      const chrono = state.chronometers.get(row.dataset.quantityId);
      const all = chrono ? chrono.readings(mode) : [];
      const dbg = state.seriesDebug.get(row.dataset.quantityId);
      const values = dbg ? all.filter((_, i) => !dbg.discarded.has(i)) : all;
      return {
        quantity_id: row.dataset.quantityId,
        instrument_id: row.querySelector(".measure-instrument")?.value || null,
        scale_id: row.querySelector(".measure-scale")?.value || null,
        values,
        given_u: null,
      };
    }

    const values = [...row.querySelectorAll(".replica")].reduce((acc, replica) => {
      const raw = replica.querySelector(".measure-value").value.trim();
      if (raw === "") return acc;
      const factor = prefixFactor(replica.querySelector(".prefix-select").value);
      const n = Number(raw) * factor;
      if (Number.isFinite(n)) acc.push(n);
      return acc;
    }, []);
    return {
      quantity_id: row.dataset.quantityId,
      instrument_id: row.querySelector(".measure-instrument").value || null,
      scale_id: row.querySelector(".measure-scale").value || null,
      values,
      given_u: null,
    };
  });
}

function collectMeta() {
  const meta = {};
  measurementFields.querySelectorAll('.measurement-row[data-is-chrono="1"]').forEach((row) => {
    const qid = row.dataset.quantityId;
    const dbg = state.seriesDebug.get(qid);
    if (!dbg) return;
    const mode = row.querySelector(".chrono-mode")?.value ?? "consecutivo";
    const chrono = state.chronometers.get(qid);
    const all = chrono ? chrono.readings(mode) : [];
    const discarded = [...dbg.discarded].filter((i) => i < all.length).map((i) => all[i]);
    if (discarded.length > 0 || (dbg.bins && dbg.bins > 0)) {
      meta[qid] = { bins: dbg.bins || null, discarded };
    }
  });
  return Object.keys(meta).length ? meta : null;
}

function setSubmissionBusy(busy) {
  if (submitButton) submitButton.disabled = busy;
}

function buildMetaMap(measurements) {
  const map = {};
  for (const m of measurements) {
    const row = measurementFields.querySelector(`[data-quantity-id="${CSS.escape(m.quantity_id)}"]`);
    if (row) {
      map[m.quantity_id] = {
        name: row.querySelector("legend")?.textContent?.trim() ?? m.quantity_id,
        isGiven: row.dataset.isGiven === "1",
        isChrono: row.dataset.isChrono === "1",
      };
    }
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
    });
    submitStatus.textContent = "Entrega guardada";
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
  import("./navigation.js").then(({ selectPracticeFromNav }) => selectPracticeFromNav(submission.practice_id));
}

export function exitEditMode() {
  state.editingSubmissionId = null;
  state.editPrefill = null;
}

function editPrefillByQuantity() {
  const map = new Map();
  for (const m of state.editPrefill ?? []) {
    let e = map.get(m.quantity_id);
    if (!e) {
      e = { values: [], instrument_id: m.instrument_id, scale_id: m.scale_id, value_u: m.value_u };
      map.set(m.quantity_id, e);
    }
    e.values.push(m.value);
    if (m.value_u != null) e.value_u = m.value_u;
  }
  return map;
}

export function applyPrefill() {
  if (!state.editingSubmissionId) return;
  const byQ = editPrefillByQuantity();

  const seriesTable = measurementFields.querySelector(".series-table");
  if (seriesTable) {
    const qids = [...seriesTable.querySelectorAll("th[data-quantity-id]")].map((th) => th.dataset.quantityId);
    const nPoints = Math.max(...qids.map((id) => byQ.get(id)?.values.length ?? 0), 0);
    const cols = state.practiceForm.definition.quantities;
    const tbody = seriesTable.querySelector("tbody");
    tbody.innerHTML = Array.from({ length: Math.max(nPoints, 1) }, () => seriesRowHtml(cols)).join("");
    wireSeriesRemove();
    [...tbody.querySelectorAll(".series-row")].forEach((row, i) => {
      row.querySelectorAll(".series-value").forEach((input) => {
        const v = byQ.get(input.dataset.quantityId)?.values[i];
        if (v != null) input.value = v;
      });
    });
    return;
  }

  measurementFields.querySelectorAll(".measurement-row").forEach((row) => {
    const e = byQ.get(row.dataset.quantityId);
    if (!e) return;
    if (row.dataset.isGiven === "1") {
      const v = row.querySelector(".measure-given-value");
      const u = row.querySelector(".measure-given-u");
      if (v) v.value = e.values[0] ?? "";
      if (u && e.value_u != null) u.value = e.value_u;
      return;
    }
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
    while (container.querySelectorAll(".replica").length < e.values.length) {
      container.insertAdjacentHTML("beforeend", renderReplicaInput(unit));
    }
    wireRemoveReplica(row);
    container.querySelectorAll(".measure-value").forEach((input, i) => {
      if (e.values[i] != null) input.value = e.values[i];
    });
  });
}

function renderSeriesTable(definition) {
  const cols = definition.quantities;
  const header = cols
    .map((q) => `<th data-quantity-id="${escapeHtml(q.id)}">${escapeHtml(q.symbol)} <span class="submission-meta">(${escapeHtml(q.unit)})</span></th>`)
    .join("");
  const INITIAL_ROWS = 3;
  const body = Array.from({ length: INITIAL_ROWS }, () => seriesRowHtml(cols)).join("");
  measurementFields.innerHTML = `
    <p class="submission-meta">Cargá un punto por fila. Las filas incompletas se ignoran. Hacen falta al menos 2 puntos para el ajuste.</p>
    <div class="directory-table-wrap">
      <table class="series-table grade-table directory-data-table">
        <thead><tr>${header}<th></th></tr></thead>
        <tbody>${body}</tbody>
      </table>
    </div>
    <button type="button" class="add-series-row">＋ agregar punto</button>
    <section class="series-preview panel" aria-live="polite"></section>
  `;
  measurementFields.querySelector(".add-series-row").addEventListener("click", () => {
    measurementFields.querySelector(".series-table tbody").insertAdjacentHTML("beforeend", seriesRowHtml(cols));
    wireSeriesRemove();
    schedulePreview();
  });
  wireSeriesRemove();

  let previewTimer = null;
  const schedulePreview = () => {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(updateRegressionPreview, 350);
  };
  measurementFields.querySelector(".series-table").addEventListener("input", (e) => {
    if (e.target.classList.contains("series-value") || e.target.classList.contains("prefix-select")) {
      schedulePreview();
    }
  });
  measurementFields.querySelector(".series-table").addEventListener("change", schedulePreview);
}

async function updateRegressionPreview() {
  const container = measurementFields.querySelector(".series-preview");
  if (!container) return;
  const measurements = collectMeasurements();
  const points = measurements[0]?.values.length ?? 0;
  if (points < 2) {
    container.innerHTML = `<p class="submission-meta">Cargá al menos 2 puntos completos para ver el ajuste.</p>`;
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
    } else {
      container.innerHTML = "";
    }
  } catch {
    container.innerHTML = `<p class="submission-meta">No se pudo calcular la vista previa con los datos actuales.</p>`;
  }
}

function seriesRowHtml(cols) {
  const cells = cols
    .map((q) => `<td class="series-cell">${prefixSelectHtml()}<input class="series-value" type="number" step="any" data-quantity-id="${escapeHtml(q.id)}" placeholder="${escapeHtml(q.symbol)}" /></td>`)
    .join("");
  return `<tr class="series-row">${cells}<td><button type="button" class="remove-series-row" title="Quitar">✕</button></td></tr>`;
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
      <span class="submission-meta">n=${stats.n} · x̄=${Number.isFinite(stats.mean) ? stats.mean.toFixed(4) : "—"} s · s=${Number.isFinite(stats.std) ? stats.std.toFixed(4) : "—"} s · s/√n=${Number.isFinite(stats.stdMean) ? stats.stdMean.toFixed(4) : "—"} s</span>
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

// ── Listeners top-level ────────────────────────────────────────────────────────

// "Entregar" (submit del form / Enter): crea la entrega por formulario.
submissionForm.addEventListener("submit", (event) => {
  event.preventDefault();
  submitFormSubmission();
});
courseSelect.addEventListener("change", updateStudentSelectors);
groupSelect.addEventListener("change", updateTableSelector);
practiceSelect.addEventListener("change", () => {
  updateTableSelector();
  loadSubmissionForm();
});
