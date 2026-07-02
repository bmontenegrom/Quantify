import { themeToggle, themeToggleLabel } from "./dom.js";

const STORAGE_KEY = "quantify-theme";

function preferredTheme() {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark") return stored;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(STORAGE_KEY, theme);
  if (themeToggle) themeToggle.setAttribute("aria-pressed", String(theme === "dark"));
  if (themeToggleLabel) themeToggleLabel.textContent = theme === "dark" ? "Tema oscuro" : "Tema claro";
}

applyTheme(preferredTheme());

themeToggle?.addEventListener("click", () => {
  const current = document.documentElement.dataset.theme === "dark" ? "dark" : "light";
  applyTheme(current === "dark" ? "light" : "dark");
});
