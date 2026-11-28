import { fetchJsonOnce } from "./api.js";
import { escapeHtml } from "./lib.js";

export const MAGNITUDES_LIST = "magnitudes-list";

export function ensureMagnitudesList() {
  return fetchJsonOnce("/api/magnitudes").then((names) => {
    if (!names || document.getElementById(MAGNITUDES_LIST)) return;
    document.body.insertAdjacentHTML(
      "beforeend",
      `<datalist id="${MAGNITUDES_LIST}">${names.map((n) => `<option value="${escapeHtml(n)}"></option>`).join("")}</datalist>`,
    );
  });
}
