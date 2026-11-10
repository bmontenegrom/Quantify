import { state } from "./state.js";
import { courseSelect, groupSelect, practiceSelect, studentComment } from "./dom.js";
import { collectMeasurements, collectFinalResults } from "./forms.js";
import { Chronometer } from "./chronometer.js";

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
    chrono: chronoDraft(),
    savedAt: Date.now(),
  };
  try {
    localStorage.setItem(draftKey(), JSON.stringify(draft));
  } catch {
  }
}

function chronoDraft() {
  const out = {};
  for (const [key, chrono] of state.chronometers) {
    if (!chrono.count) continue;
    const dbg = state.seriesDebug.get(key);
    out[key] = {
      ...chrono.toJSON(),
      mode: state.chronoModes.get(key) ?? null,
      discarded: dbg ? [...dbg.discarded] : [],
      bins: dbg?.bins ?? 0,
    };
  }
  return out;
}

export function restoreChronoDraft(chrono) {
  state.chronometers.clear();
  state.seriesDebug.clear();
  state.chronoModes.clear();
  for (const [key, saved] of Object.entries(chrono ?? {})) {
    state.chronometers.set(key, Chronometer.fromJSON(saved));
    if (saved.mode) state.chronoModes.set(key, saved.mode);
    if (saved.discarded?.length || saved.bins) {
      state.seriesDebug.set(key, { discarded: new Set(saved.discarded ?? []), bins: saved.bins ?? 0 });
    }
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
