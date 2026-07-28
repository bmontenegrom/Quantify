import { state } from "./state.js";
import { courseSelect, groupSelect, practiceSelect, studentComment } from "./dom.js";
import { collectMeasurements, collectFinalResults } from "./forms.js";

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

export function loadDraft() {
  try {
    const raw = localStorage.getItem(draftKey());
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function clearDraft() {
  try {
    localStorage.removeItem(draftKey());
  } catch {
    // no-op
  }
}

let draftSaveTimer = null;
export function scheduleDraftSave() {
  clearTimeout(draftSaveTimer);
  draftSaveTimer = setTimeout(saveDraft, 350);
}
