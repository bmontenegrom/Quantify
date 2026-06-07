import { state } from "./state.js";
import { userList, userStatus, userForm, adminStatus } from "./dom.js";
import { postJson } from "./api.js";
import { escapeHtml, format, studentCourses, studentGroups, studentTotals } from "./lib.js";
import { refreshAcademic, withAdminError } from "./academic.js";

export function renderUsers() {
  const rows = state.academic.users.flatMap((user) => {
    const courses = studentCourses(state.academic, user.id);
    const groups = studentGroups(state.academic, user.id);
    const totals = studentTotals(state.gradebooks, user.id);
    const baseRow = `
      <tr>
        <td class="directory-primary">
          <strong>${escapeHtml(user.display_name)}</strong>
          <div class="submission-meta">${escapeHtml(user.email)}</div>
        </td>
        <td><span class="status-chip">${escapeHtml(user.role)}</span></td>
        <td>
          <strong>${courses.length}</strong>
          <div class="submission-meta">${courses.map((course) => escapeHtml(course.name)).join(", ") || "-"}</div>
        </td>
        <td>
          <strong>${groups.length}</strong>
          <div class="submission-meta">${groups.map((group) => escapeHtml(group.name)).join(", ") || "-"}</div>
        </td>
        <td>
          <strong>${totals ? `${format(totals.points)} / ${format(totals.possible)}` : "-"}</strong>
          <div class="submission-meta">Puntos acumulados</div>
        </td>
        <td class="directory-actions">
          <button type="button" data-user-action="reset" data-user-id="${escapeHtml(user.id)}">Reset password</button>
        </td>
      </tr>`;
    const detailRow =
      state.editingUserId === user.id
        ? `
            <tr class="directory-detail-row">
              <td class="directory-detail-cell" colspan="6">
                ${renderUserDetail(user)}
              </td>
            </tr>
          `
        : "";
    return [baseRow, detailRow];
  });

  userList.innerHTML = rows.length
    ? `
        <div class="directory-table-wrap">
          <table class="grade-table directory-data-table">
            <thead>
              <tr>
                <th>Usuario</th>
                <th>Rol</th>
                <th>Cursos</th>
                <th>Grupos</th>
                <th>Puntos</th>
                <th>Acciones</th>
              </tr>
            </thead>
            <tbody>
              ${rows.join("")}
            </tbody>
          </table>
        </div>
      `
    : `<p class="submission-meta">No hay usuarios cargados.</p>`;

  userList.querySelectorAll("[data-user-action]").forEach((button) => {
    button.addEventListener("click", () => toggleUserAction(button.dataset.userId));
  });
  userList.querySelectorAll("[data-user-form='reset']").forEach((form) => {
    form.addEventListener("submit", saveUserReset);
  });
  userList.querySelectorAll("[data-user-cancel]").forEach((button) => {
    button.addEventListener("click", clearUserAction);
  });
}

export function renderUserDetail(user) {
  return `
    <form class="detail-form compact" data-user-form="reset">
      <input name="id" type="hidden" value="${escapeHtml(user.id)}" />
      <label>
        Nueva contrasena
        <input name="password" type="password" required minlength="8" placeholder="Minimo 8 caracteres" />
      </label>
      <div class="detail-actions">
        <button type="submit">Guardar</button>
        <button type="button" data-user-cancel>Cancelar</button>
        <span class="submission-meta">${escapeHtml(state.userActionStatus)}</span>
      </div>
    </form>
  `;
}

export function toggleUserAction(userId) {
  state.userActionStatus = "";
  state.editingUserId = state.editingUserId === userId ? null : userId;
  renderUsers();
}

export function clearUserAction() {
  state.editingUserId = null;
  state.userActionStatus = "";
  renderUsers();
}

export async function saveUserReset(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.userActionStatus = "";
    await postJson(`/api/users/${payload.id}/password`, { password: payload.password });
    state.userActionStatus = "Contrasena reseteada";
    await refreshAcademic("Contrasena reseteada");
  } catch (error) {
    state.userActionStatus = error.message;
    renderUsers();
  }
}

userForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    await postJson("/api/users", Object.fromEntries(new FormData(userForm).entries()));
    userForm.reset();
    await refreshAcademic("Usuario creado");
  });
});
