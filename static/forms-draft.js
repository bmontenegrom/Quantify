import { state } from "./state.js";
import { courseSelect, groupSelect, practiceSelect, studentComment } from "./dom.js";
import { collectMeasurements, collectFinalResults } from "./forms.js";

function draftKey() {
  const uid = state.user?.id ?? "anon";
  return `quantify-draft:${uid}:${courseSelect.value}:${groupSelect.value}:${practiceSelect.value}`;
}

export function saveDraft() {
  if (state.draftPreviewPracticeId || state.editingSubmissionId || state.restoringCancelledSubmission || !state.practiceForm) return;
  const draft = {
    measurements: collectMeasurements(),
    finalResults: collectFinalResults(),
    comment: studentComment?.value ?? "",
    savedAt: Date.now(),
  };
  try {
    localStorage.setItem(draftKey(), JSON.stringify(draft));
  } catch {
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
  }
}

let draftSaveTimer = null;
export function scheduleDraftSave() {
  clearTimeout(draftSaveTimer);
  draftSaveTimer = setTimeout(saveDraft, 350);
}
