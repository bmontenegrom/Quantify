import { state } from "./state.js";
import { groupDirectory, groupWorkspace } from "./dom.js";
import { postJson } from "./api.js";
import { escapeHtml, allGroups, studentCourses, studentGroups, studentTotals, renderGroupType } from "./lib.js";
import { refreshAcademic } from "./academic.js";
import { selectView } from "./navigation.js";
import { openStudentWorkspace, renderStudentPoints } from "./students.js";

export function renderGroupDirectory() {
  if (!groupDirectory || !state.academic) return;
  const groups = allGroups(state.academic);

  const rows = groups.map(
    (group) => `
      <tr>
        <td class="directory-primary">
          <strong>${escapeHtml(group.name)}</strong>
        </td>
        <td>
          <strong>${escapeHtml(group.courseName)}</strong>
          <div class="submission-meta">${escapeHtml(group.courseTerm)}</div>
        </td>
        <td>
          <strong>${group.members.length}</strong>
          <div class="submission-meta">estudiantes</div>
        </td>
        <td>
          <strong>${group.table_count ?? 4}</strong>
          <div class="submission-meta">${renderGroupType(group.group_type)}</div>
        </td>
        <td>
          <div class="directory-listing">
            ${group.members.map((member) => `<span class="inline-pill">${escapeHtml(member.display_name)}</span>`).join("") || `<span class="submission-meta">Sin estudiantes</span>`}
          </div>
        </td>
        <td class="directory-actions">
          <button type="button" data-group-open data-group-id="${escapeHtml(group.id)}">Editar</button>
        </td>
      </tr>`,
  );

  groupDirectory.innerHTML = rows.length
    ? `
        <div class="directory-table-wrap">
          <table class="grade-table directory-data-table">
            <thead>
              <tr>
                <th>Grupo</th>
                <th>Curso</th>
                <th>Cantidad</th>
                <th>Mesas</th>
                <th>Estudiantes</th>
                <th>Acciones</th>
              </tr>
            </thead>
            <tbody>
              ${rows.join("")}
            </tbody>
          </table>
        </div>
      `
    : `<p class="submission-meta">No hay grupos creados.</p>`;

  groupDirectory.querySelectorAll("[data-group-open]").forEach((button) => {
    button.addEventListener("click", () => openGroupWorkspace(button.dataset.groupId));
  });
}

export function renderGroupsPage() {
  renderGroupDirectory();
  if (!groupWorkspace) return;

  const group = allGroups(state.academic).find((item) => item.id === state.activeGroupId);
  if (!group) {
    groupWorkspace.innerHTML = "";
    groupWorkspace.classList.add("hidden");
    groupDirectory.closest(".panel")?.classList.remove("hidden");
    return;
  }

  groupWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="group-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(group.name)}</h3>
        <p class="submission-meta">${escapeHtml(group.courseName)} (${escapeHtml(group.courseTerm)})</p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Estudiantes</div>
          <div class="metric-value">${group.members.length}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Mesas</div>
          <div class="metric-value">${group.table_count ?? 4}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Tipo</div>
          <div class="metric-value metric-text">${renderGroupType(group.group_type)}</div>
        </div>
      </div>
    </div>

    <section class="panel workspace-panel group-profile-panel">
      <h3>Datos del grupo</h3>
      ${renderGroupProfileForm(group)}
    </section>

    <section class="panel workspace-panel">
      <h3>Mesas del grupo</h3>
      ${renderGroupTablesPanel(group)}
    </section>

    <section class="panel workspace-panel">
      <h3>Estudiantes del grupo</h3>
      ${renderGroupMembersPanel(group)}
    </section>
  `;

  groupWorkspace.classList.remove("hidden");
  groupDirectory.closest(".panel")?.classList.add("hidden");
  groupWorkspace.querySelector("#group-workspace-back")?.addEventListener("click", closeGroupWorkspace);
  groupWorkspace.querySelector("#group-profile-form")?.addEventListener("submit", saveGroupEdit);
  groupWorkspace.querySelectorAll("[data-student-open]").forEach((button) => {
    button.addEventListener("click", () => openStudentWorkspace(button.dataset.studentId, button.dataset.studentOpen));
  });
  groupWorkspace.querySelectorAll("[data-remove-group-member]").forEach((button) => {
    button.addEventListener("click", () => removeGroupMember(button.dataset.groupId, button.dataset.studentId, "group"));
  });
}

function renderGroupProfileForm(group) {
  return `
    <form id="group-profile-form" class="detail-form detail-form-grid">
      <input name="id" type="hidden" value="${escapeHtml(group.id)}" />
      <label>
        Nombre
        <input name="name" value="${escapeHtml(group.name)}" required />
      </label>
      <label>
        Mesas
        <input name="table_count" type="number" min="1" max="24" step="1" value="${escapeHtml(group.table_count ?? 4)}" required />
      </label>
      <label>
        Tipo
        <select name="group_type">
          <option value="regular" ${group.group_type === "recuperacion" ? "" : "selected"}>Regular</option>
          <option value="recuperacion" ${group.group_type === "recuperacion" ? "selected" : ""}>Recuperacion</option>
        </select>
      </label>
      <div class="detail-actions">
        <button type="submit">Guardar cambios</button>
        <span class="submission-meta">${escapeHtml(state.groupActionStatus)}</span>
      </div>
    </form>
  `;
}

function renderGroupTablesPanel(group) {
  const tables = Array.from({ length: group.table_count ?? 4 }, (_, index) => index + 1);
  return `
    <div class="directory-listing">
      ${tables.map((tableNumber) => `<span class="inline-pill">Mesa ${tableNumber}</span>`).join("")}
    </div>
  `;
}

function renderGroupMembersPanel(group) {
  if (group.members.length === 0) {
    return `<p class="submission-meta">Sin estudiantes asignados.</p>`;
  }

  const rows = group.members.map((student) => {
    const courses = studentCourses(state.academic, student.id);
    const groups = studentGroups(state.academic, student.id);
    const totals = studentTotals(state.gradebooks, student.id);
    return `
      <tr>
        <td class="directory-primary">
          <strong>${escapeHtml(student.display_name)}</strong>
          <div class="submission-meta">${escapeHtml(student.email)}</div>
        </td>
        <td><span class="status-chip">${escapeHtml(student.role)}</span></td>
        <td>
          <strong>${courses.length}</strong>
          <div class="submission-meta">${courses.map((course) => escapeHtml(course.name)).join(", ") || "Sin cursos"}</div>
        </td>
        <td>
          <strong>${groups.length}</strong>
          <div class="submission-meta">${groups.map((item) => `${escapeHtml(item.courseName)} / ${escapeHtml(item.name)}`).join(", ") || "Sin grupos"}</div>
        </td>
        <td>${renderStudentPoints(totals)}</td>
        <td class="directory-actions">
          <button type="button" data-student-open="overview" data-student-id="${escapeHtml(student.id)}">Editar</button>
          <button type="button" data-remove-group-member data-group-id="${escapeHtml(group.id)}" data-student-id="${escapeHtml(student.id)}">Quitar</button>
        </td>
      </tr>
    `;
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead>
          <tr>
            <th>Estudiante</th>
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
  `;
}

export function openGroupWorkspace(groupId) {
  state.activeGroupId = groupId;
  state.groupActionStatus = "";
  renderGroupsPage();
  selectView("groups");
}

export function closeGroupWorkspace() {
  state.activeGroupId = null;
  state.groupActionStatus = "";
  renderGroupsPage();
}

export async function removeGroupMember(groupId, studentId, origin) {
  try {
    state.studentActionStatus = "";
    state.groupActionStatus = "";
    await postJson(`/api/academic/groups/${groupId}/members/remove`, { user_id: studentId });
    if (origin === "student") {
      state.studentActionStatus = "Estudiante removido del grupo";
    } else {
      state.groupActionStatus = "Estudiante removido del grupo";
    }
    await refreshAcademic("Estudiante removido del grupo");
  } catch (error) {
    if (origin === "student") {
      const { renderStudentsPage } = await import("./students.js");
      state.studentActionStatus = error.message;
      renderStudentsPage();
    } else {
      state.groupActionStatus = error.message;
      renderGroupsPage();
    }
  }
}

async function saveGroupEdit(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.groupActionStatus = "";
    await postJson(`/api/academic/groups/${payload.id}`, {
      name: payload.name,
      table_count: Number(payload.table_count),
      group_type: payload.group_type,
    });
    state.groupActionStatus = "Cambios guardados";
    await refreshAcademic("Grupo actualizado");
  } catch (error) {
    state.groupActionStatus = error.message;
    renderGroupsPage();
  }
}
