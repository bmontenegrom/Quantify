import { state } from "./state.js";
import { courseSelect, groupSelect, measurementFields, practiceSelect, studentComment } from "./dom.js";
import { collectMeasurements, collectFinalResults } from "./forms.js";
import { Chronometer } from "./chronometer.js";
import { chronoKeyFor } from "./forms-chrono.js";

const CHRONO_MODES = ["periodo", "consecutivo", "pares", "absoluto"];

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

function renderedChronoKeys() {
  const rows = [...measurementFields.querySelectorAll('.measurement-row[data-is-chrono="1"]')].map(chronoKeyFor);
  const helpers = [...measurementFields.querySelectorAll(".chrono-helper")].map((_, i) => `__chrono_helper__${i}`);
  return [...rows, ...helpers];
}

function chronoDraft() {
  const out = {};
  for (const key of renderedChronoKeys()) {
    const chrono = state.chronometers.get(key);
    if (!chrono?.count && !state.chronoModes.has(key)) continue;
    const dbg = state.seriesDebug.get(key);
    out[key] = {
      ...(chrono ?? new Chronometer()).toJSON(),
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
  for (const [key, saved] of Object.entries(chrono && typeof chrono === "object" ? chrono : {})) {
    if (!saved || typeof saved !== "object") continue;
    const restored = Chronometer.fromJSON(saved);
    if (restored.count) state.chronometers.set(key, restored);
    if (CHRONO_MODES.includes(saved.mode)) state.chronoModes.set(key, saved.mode);
    const discarded = Array.isArray(saved.discarded) ? saved.discarded.filter(Number.isInteger) : [];
    const bins = Number.isInteger(saved.bins) && saved.bins > 0 ? saved.bins : 0;
    if (restored.count && (discarded.length || bins)) {
      state.seriesDebug.set(key, { discarded: new Set(discarded), bins });
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
