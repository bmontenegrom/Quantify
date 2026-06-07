import { state } from "./state.js";
import {
  loginScreen, appShell, loginForm, loginStatus,
  sessionUser, accountProfileForm, accountDisplayName,
  accountEmail, accountRole, accountStatus,
  passwordForm, passwordStatus, logoutButton,
} from "./dom.js";
import { postJson, errorText } from "./api.js";
import { escapeHtml, canReview } from "./lib.js";

const accountDefaultGroup = () => document.querySelector("#account-default-group");
const accountDefaultTable = () => document.querySelector("#account-default-table");

export function renderSessionUser() {
  sessionUser.textContent = `${state.user.display_name} (${state.user.role})`;
}

export function renderAccount() {
  if (!state.user) return;
  accountDisplayName.value = state.user.display_name;
  accountEmail.value = state.user.email;
  accountRole.value = state.user.role;

  if (!canReview(state.user)) {
    renderDefaultGroupSelect();
  }
}

function renderDefaultGroupSelect() {
  const groupEl = accountDefaultGroup();
  const tableEl = accountDefaultTable();
  if (!groupEl || !tableEl) return;

  // Grupos del alumno en todos sus cursos
  const groups = (state.academic?.courses ?? []).flatMap((c) =>
    c.groups
      .filter((g) => g.members?.some((m) => m.id === state.user.id))
      .map((g) => ({ id: g.id, label: `${g.name} (${c.name})`, tableCount: g.table_count ?? 0 })),
  );

  const currentGroupId = state.user.default_group_id ?? "";
  groupEl.innerHTML =
    `<option value="">— sin grupo —</option>` +
    groups
      .map(
        (g) =>
          `<option value="${escapeHtml(g.id)}" ${g.id === currentGroupId ? "selected" : ""}>${escapeHtml(g.label)}</option>`,
      )
      .join("");

  renderDefaultTableSelect(groups);

  if (!groupEl.dataset.wired) {
    groupEl.dataset.wired = "1";
    groupEl.addEventListener("change", () => renderDefaultTableSelect(groups));
  }
}

function renderDefaultTableSelect(groups) {
  const groupEl = accountDefaultGroup();
  const tableEl = accountDefaultTable();
  if (!groupEl || !tableEl) return;

  const selectedGroup = groups.find((g) => g.id === groupEl.value);
  const tableCount = selectedGroup?.tableCount ?? 0;
  const currentTable = state.user.default_table_number;
  tableEl.innerHTML =
    `<option value="">— sin mesa —</option>` +
    (tableCount
      ? Array.from({ length: tableCount }, (_, i) => {
          const n = i + 1;
          return `<option value="${n}" ${n === currentTable && groupEl.value === (state.user.default_group_id ?? "") ? "selected" : ""}>Mesa ${n}</option>`;
        }).join("")
      : "");
  tableEl.disabled = !tableCount;
}

export function setupAuth(onLogin) {
  loginForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    loginStatus.textContent = "Entrando...";
    try {
      const payload = Object.fromEntries(new FormData(loginForm).entries());
      const response = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!response.ok) throw new Error(await errorText(response));
      const body = await response.json();
      state.user = body.user;
      loginForm.reset();
      loginStatus.textContent = "";
      await onLogin();
    } catch (error) {
      loginStatus.textContent = error.message;
    }
  });

  logoutButton.addEventListener("click", async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    state.user = null;
    appShell.classList.add("hidden");
    loginScreen.classList.remove("hidden");
  });

  passwordForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      await postJson("/api/auth/password", Object.fromEntries(new FormData(passwordForm).entries()));
      passwordForm.reset();
      passwordStatus.textContent = "Contrasena actualizada. Volve a iniciar sesion.";
    } catch (error) {
      passwordStatus.textContent = error.message;
    }
  });

  accountProfileForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      accountStatus.textContent = "";
      const raw = Object.fromEntries(new FormData(accountProfileForm).entries());
      const payload = {
        display_name: raw.display_name,
        email: raw.email,
        role: state.user.role,
      };
      if (!canReview(state.user)) {
        if (raw.default_group_id) {
          payload.default_group_id = raw.default_group_id;
          const tableNum = Number(raw.default_table_number);
          if (tableNum) payload.default_table_number = tableNum;
        }
      }
      const user = await postJson("/api/auth/profile", payload);
      state.user = user;
      renderSessionUser();
      renderAccount();
      accountStatus.textContent = "Cambios guardados";
    } catch (error) {
      accountStatus.textContent = error.message;
    }
  });
}
