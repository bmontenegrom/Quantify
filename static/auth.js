import { state } from "./state.js";
import {
  loginScreen, appShell, loginForm, loginStatus,
  sessionUser, accountProfileForm, accountDisplayName,
  accountEmail, accountRole, accountStatus,
  passwordForm, passwordStatus, logoutButton,
} from "./dom.js";
import { postJson, errorText } from "./api.js";

export function renderSessionUser() {
  sessionUser.textContent = `${state.user.display_name} (${state.user.role})`;
}

export function renderAccount() {
  if (!state.user) return;
  accountDisplayName.value = state.user.display_name;
  accountEmail.value = state.user.email;
  accountRole.value = state.user.role;
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
      const payload = Object.fromEntries(new FormData(accountProfileForm).entries());
      const user = await postJson("/api/auth/profile", {
        display_name: payload.display_name,
        email: payload.email,
        role: state.user.role,
      });
      state.user = user;
      renderSessionUser();
      renderAccount();
      accountStatus.textContent = "Cambios guardados";
    } catch (error) {
      accountStatus.textContent = error.message;
    }
  });
}
