import { state } from "./state.js";
import { studentDirectory, studentWorkspace } from "./dom.js";
import { postJson } from "./api.js";
import {
  escapeHtml, format, canReview,
  studentCourses, studentGroups, studentTotals,
  availableCoursesForStudent, availableGroupsForStudent, allStudents,
} from "./lib.js";
import { refreshAcademic } from "./academic.js";
import { saveGradeInput, renderKindTotals, renderGradeField } from "./gradebook.js";
import { selectView } from "./navigation.js";
import { removeGroupMember } from "./groups.js";

export function renderStudentPoints(totals) {
  if (!totals) return `<span class="submission-meta">Sin notas cargadas</span>`;
  return `
    <strong>${format(totals.points)} / ${format(totals.possible)}</strong>
    <div class="submission-meta">Puntos acumulados</div>
  `;
}

export function renderStudentDirectory() {
  if (!studentDirectory || !state.academic) return;

  const rows = allStudents(state.academic).map((student) => {
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
          <div class="submission-meta">${groups.map((group) => `${escapeHtml(group.courseName)} / ${escapeHtml(group.name)}`).join(", ") || "Sin grupos"}</div>
        </td>
        <td>
          ${renderStudentPoints(totals)}
        </td>
        <td class="directory-actions">
          <button type="button" data-student-open="overview" data-student-id="${escapeHtml(student.id)}">Editar</button>
        </td>
      </tr>`;
  });

  studentDirectory.innerHTML = rows.length
    ? `
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
      `
    : `<p class="submission-meta">No hay estudiantes cargados.</p>`;
  studentDirectory.querySelectorAll("[data-student-open]").forEach((button) => {
    button.addEventListener("click", () => openStudentWorkspace(button.dataset.studentId, button.dataset.studentOpen));
  });
}

export function renderStudentProfileForm(student) {
  return `
    <form id="student-profile-form" class="detail-form detail-form-grid">
      <input name="id" type="hidden" value="${escapeHtml(student.id)}" />
      <label>
        Nombre
        <input name="display_name" value="${escapeHtml(student.display_name)}" required />
      </label>
      <label>
        Email
        <input name="email" type="email" value="${escapeHtml(student.email)}" required />
      </label>
      <label>
        Rol
        <select name="role" required>
          ${["estudiante", "docente", "admin"]
            .map((role) => `<option value="${role}" ${role === student.role ? "selected" : ""}>${role}</option>`)
            .join("")}
        </select>
      </label>
      <div class="detail-actions">
        <button type="submit">Guardar cambios</button>
        <span class="submission-meta">${escapeHtml(state.studentActionStatus)}</span>
      </div>
    </form>
  `;
}

export function renderStudentEnrollmentPanel(student) {
  const currentCourses = studentCourses(state.academic, student.id);
  const availableCourses = availableCoursesForStudent(state.academic, student.id);
  const currentGroups = studentGroups(state.academic, student.id);
  const groupOptions = availableGroupsForStudent(state.academic, student.id);

  return `
    <div class="stack-form">
      <div>
        <strong>Cursos actuales</strong>
        <div class="chips">
          ${currentCourses.map((course) => `<span class="chip">${escapeHtml(course.name)} (${escapeHtml(course.term)})</span>`).join("") || `<span class="chip">Sin cursos</span>`}
        </div>
      </div>

      <form id="student-course-form" class="detail-form compact">
        <input name="student_id" type="hidden" value="${escapeHtml(student.id)}" />
        <label>
          Inscribir en curso
          <select name="course_id" ${availableCourses.length ? "" : "disabled"}>
            ${
              availableCourses.length
                ? availableCourses.map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`).join("")
                : `<option value="">Sin cursos disponibles</option>`
            }
          </select>
        </label>
        <div class="detail-actions">
          <button type="submit" ${availableCourses.length ? "" : "disabled"}>Inscribir</button>
        </div>
      </form>

      <div>
        <strong>Grupos actuales</strong>
        <div class="stack-list">
          ${
            currentGroups.length
              ? currentGroups
                  .map(
                    (group) => `
                      <div class="inline-row">
                        <span class="chip">${escapeHtml(group.courseName)} - ${escapeHtml(group.name)}</span>
                        <button type="button" data-remove-group-member data-group-id="${escapeHtml(group.id)}" data-student-id="${escapeHtml(student.id)}">Quitar</button>
                      </div>
                    `,
                  )
                  .join("")
              : `<span class="chip">Sin grupos</span>`
          }
        </div>
      </div>

      <form id="student-group-form" class="detail-form compact">
        <input name="student_id" type="hidden" value="${escapeHtml(student.id)}" />
        <label>
          Asignar a grupo
          <select name="group_id" ${groupOptions.length ? "" : "disabled"}>
            ${
              groupOptions.length
                ? groupOptions
                    .map(
                      (group) =>
                        `<option value="${escapeHtml(group.id)}">${escapeHtml(group.courseName)} (${escapeHtml(group.courseTerm)}) - ${escapeHtml(group.name)}</option>`,
                    )
                    .join("")
                : `<option value="">Sin grupos disponibles</option>`
            }
          </select>
        </label>
        <div class="detail-actions">
          <button type="submit" ${groupOptions.length ? "" : "disabled"}>Asignar grupo</button>
          <span class="submission-meta">${escapeHtml(state.studentActionStatus)}</span>
        </div>
      </form>
    </div>
  `;
}

export function renderStudentGradeEditor(student) {
  const courses = studentCourses(state.academic, student.id);
  if (courses.length === 0) {
    return `<p class="submission-meta">El estudiante no esta inscrito en ningun curso.</p>`;
  }

  return courses
    .map((course) => {
      const book = state.gradebooks.find((item) => item.course.id === course.id);
      const summary = book?.students.find((item) => item.student.id === student.id);
      if (!book || !summary) {
        return `
          <section class="grade-course">
            <div>
              <h4>${escapeHtml(course.name)} (${escapeHtml(course.term)})</h4>
              <p class="submission-meta">Sin notas cargadas para este curso.</p>
            </div>
          </section>
        `;
      }
      if (book.components.length === 0) {
        return `
          <section class="grade-course">
            <div>
              <h4>${escapeHtml(course.name)} (${escapeHtml(course.term)})</h4>
              <p class="submission-meta">Este curso todavia no tiene componentes evaluables.</p>
            </div>
          </section>
        `;
      }
      return `
        <section class="grade-course">
          <div>
            <h4>${escapeHtml(course.name)} (${escapeHtml(course.term)})</h4>
            <p class="submission-meta">Total: ${format(summary.total_points)} / ${format(summary.total_possible)}</p>
          </div>
          ${renderKindTotals(summary)}
          <div class="student-grade-fields">
            ${book.components.map((component) => renderGradeField(summary, component)).join("")}
          </div>
        </section>
      `;
    })
    .join("");
}

export function renderStudentsPage() {
  renderStudentDirectory();
  if (!studentWorkspace) return;

  const student = allStudents(state.academic).find((item) => item.id === state.activeStudentId);
  if (!student) {
    studentWorkspace.innerHTML = "";
    studentWorkspace.classList.add("hidden");
    studentDirectory.closest(".panel")?.classList.remove("hidden");
    return;
  }

  const totals = studentTotals(state.gradebooks, student.id);
  const courses = studentCourses(state.academic, student.id);
  const groups = studentGroups(state.academic, student.id);
  studentWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="student-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(student.display_name)}</h3>
        <p class="submission-meta">${escapeHtml(student.email)} - ${escapeHtml(student.role)}</p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Cursos</div>
          <div class="metric-value">${courses.length}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Grupos</div>
          <div class="metric-value">${groups.length}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Puntos</div>
          <div class="metric-value">${totals ? `${format(totals.points)} / ${format(totals.possible)}` : "-"}</div>
        </div>
      </div>
    </div>

    <div class="workspace-grid">
      <section class="panel workspace-panel" id="student-profile-panel">
        <h3>Datos del estudiante</h3>
        ${renderStudentProfileForm(student)}
      </section>

      <section class="panel workspace-panel" id="student-groups-panel">
        <h3>Cursos y grupos</h3>
        ${renderStudentEnrollmentPanel(student)}
      </section>
    </div>

    <section class="panel workspace-panel" id="student-grades-panel">
      <h3>Notas del estudiante</h3>
      ${renderStudentGradeEditor(student)}
    </section>
  `;
  studentWorkspace.classList.remove("hidden");
  studentDirectory.closest(".panel")?.classList.add("hidden");

  document.querySelector("#student-workspace-back")?.addEventListener("click", closeStudentWorkspace);
  studentWorkspace.querySelector("#student-profile-form")?.addEventListener("submit", saveStudentEdit);
  studentWorkspace.querySelector("#student-course-form")?.addEventListener("submit", saveStudentEnrollment);
  studentWorkspace.querySelector("#student-group-form")?.addEventListener("submit", saveStudentGroup);
  studentWorkspace.querySelectorAll("[data-remove-group-member]").forEach((button) => {
    button.addEventListener("click", () => removeGroupMember(button.dataset.groupId, button.dataset.studentId, "student"));
  });
  studentWorkspace.querySelectorAll(".grade-input").forEach((input) => {
    input.addEventListener("change", () => saveGradeInput(input));
  });

  focusStudentSection();
}

export function openStudentWorkspace(studentId, section = "overview") {
  state.activeStudentId = studentId;
  state.studentDetailSection = section;
  renderStudentsPage();
  selectView("students");
}

export function closeStudentWorkspace() {
  state.activeStudentId = null;
  state.studentDetailSection = "overview";
  state.studentActionStatus = "";
  renderStudentsPage();
}

function focusStudentSection() {
  const map = {
    overview: "#student-profile-panel",
    groups: "#student-groups-panel",
    grades: "#student-grades-panel",
  };
  const target = studentWorkspace?.querySelector(map[state.studentDetailSection] ?? map.overview);
  target?.scrollIntoView({ block: "start", behavior: "smooth" });
}

async function saveStudentEdit(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.studentActionStatus = "";
    await postJson(`/api/users/${payload.id}`, {
      display_name: payload.display_name,
      email: payload.email,
      role: payload.role,
    });
    state.studentActionStatus = "Cambios guardados";
    await refreshAcademic("Estudiante actualizado");
  } catch (error) {
    state.studentActionStatus = error.message;
    renderStudentsPage();
  }
}

async function saveStudentEnrollment(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  if (!payload.course_id) return;
  try {
    state.studentActionStatus = "";
    await postJson(`/api/academic/courses/${payload.course_id}/members`, {
      user_id: payload.student_id,
    });
    state.studentActionStatus = "Estudiante inscrito";
    await refreshAcademic("Estudiante inscrito");
  } catch (error) {
    state.studentActionStatus = error.message;
    renderStudentsPage();
  }
}

async function saveStudentGroup(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  if (!payload.group_id) return;
  try {
    state.studentActionStatus = "";
    await postJson(`/api/academic/groups/${payload.group_id}/members`, {
      user_id: payload.student_id,
    });
    state.studentActionStatus = "Grupo asignado";
    await refreshAcademic("Estudiante asignado a grupo");
  } catch (error) {
    state.studentActionStatus = error.message;
    renderStudentsPage();
  }
}
