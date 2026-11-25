import { fetchJson } from "./api.js";
import { escapeHtml } from "./lib.js";

export const MAGNITUDES_LIST = "magnitudes-list";

let loading = null;

export function ensureMagnitudesList() {
  if (document.getElementById(MAGNITUDES_LIST)) return Promise.resolve();
  loading ??= fetchJson("/api/magnitudes")
    .then((names) => {
      if (document.getElementById(MAGNITUDES_LIST)) return;
      document.body.insertAdjacentHTML(
        "beforeend",
        `<datalist id="${MAGNITUDES_LIST}">${names.map((n) => `<option value="${escapeHtml(n)}"></option>`).join("")}</datalist>`,
      );
    })
    .catch(() => {});
  return loading;
}
