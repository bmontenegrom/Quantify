import { fetchJson } from "./api.js";
import { escapeHtml } from "./lib.js";

let constants = null;
let loading = null;

const optionsHtml = () =>
  (constants ?? [])
    .map((c, i) => `<option value="${i}">${escapeHtml(`${c.symbol} — ${c.name} · ${c.source}`)}</option>`)
    .join("");

export function constantPickerHtml() {
  return `<label class="wide-field">Desde el catálogo
    <select data-constant-picker><option value="">Elegí una constante…</option>${optionsHtml()}</select>
  </label>`;
}

export function ensureConstants() {
  loading ??= fetchJson("/constants.json")
    .then((list) => {
      constants = list;
      document.querySelectorAll("[data-constant-picker]").forEach((select) => select.insertAdjacentHTML("beforeend", optionsHtml()));
    })
    .catch(() => {});
  return loading;
}

export function applyConstant(form, index) {
  const c = constants?.[index];
  if (!c) return;
  const set = (name, value) => {
    const el = form.elements[name];
    if (el.type === "checkbox") el.checked = value;
    else el.value = value;
    for (const type of ["input", "change"]) el.dispatchEvent(new Event(type, { bubbles: true }));
  };
  set("role", c.u == null ? "constante" : "catedra");
  set("symbol", c.symbol);
  set("name", c.name);
  set("unit", c.unit);
  set("quantity", c.quantity ?? "");
  set("default_value", String(c.value));
  if (c.u != null) set("default_u", String(c.u));
  set("fixed", true);
}
