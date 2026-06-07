import { state } from "./state.js";
import { courseCatalog, courseWorkspace } from "./dom.js";
import { postJson } from "./api.js";
import { escapeHtml } from "./lib.js";
import { refreshAcademic } from "./academic.js";
import { selectView } from "./navigation.js";
import { openGroupWorkspace } from "./groups.js";

export function renderCoursesPage() {
  renderCourseDirectory(state.academic?.courses ?? []);
  if (!courseWorkspace) return;

  const course = state.academic?.courses.find((item) => item.id === state.activeCourseId);
  if (!course) {
    courseWorkspace.innerHTML = "";
    courseWorkspace.classList.add("hidden");
    courseCatalog.closest(".panel")?.classList.remove("hidden");
    return;
  }

  courseWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="course-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(course.name)}</h3>
        <p class="submission-meta">${escapeHtml(course.term)}</p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Estudiantes</div>
          <div class="metric-value">${course.members.length}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Grupos</div>
          <div class="metric-value">${course.groups.length}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Subgrupos</div>
          <div class="metric-value">${course.subgroups?.length ?? 0}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Practicas</div>
          <div class="metric-value">${course.practices.length}</div>
        </div>
      </div>
    </div>

    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Datos del curso</h3>
        ${renderCourseProfileForm(course)}
      </section>
      <section class="panel workspace-panel">
        <h3>Nuevo grupo</h3>
        ${renderCourseGroupForm(course)}
      </section>
    </div>

    <section class="panel workspace-panel group-profile-panel">
      <h3>Practicas habilitadas</h3>
      ${renderCoursePracticesPanel(course)}
    </section>

    <section class="panel workspace-panel">
      <h3>Grupos del curso</h3>
      ${renderCourseGroupsTable(course)}
    </section>

    <section class="panel workspace-panel">
      <h3>Subgrupos por practica</h3>
      ${renderCourseSubgroupsPanel(course)}
    </section>
  `;

  courseWorkspace.classList.remove("hidden");
  courseCatalog.closest(".panel")?.classList.add("hidden");
  courseWorkspace.querySelector("#course-workspace-back")?.addEventListener("click", closeCourseWorkspace);
  courseWorkspace.querySelector("#course-profile-form")?.addEventListener("submit", saveCourseEdit);
  courseWorkspace.querySelector("#course-group-form")?.addEventListener("submit", saveCourseGroup);
  courseWorkspace.querySelector("#course-subgroup-form")?.addEventListener("submit", saveCourseSubgroup);
  courseWorkspace.querySelector("#course-practice-form")?.addEventListener("submit", saveCoursePractice);
  courseWorkspace.querySelectorAll("[data-group-open]").forEach((button) => {
    button.addEventListener("click", () => openGroupWorkspace(button.dataset.groupId));
  });
}

export function renderCourseDirectory(courses) {
  if (!courseCatalog) return;
  const rows = courses.map(
    (course) => `
      <tr>
        <td class="directory-primary">
          <strong>${escapeHtml(course.name)}</strong>
          <div class="submission-meta">${escapeHtml(course.term)}</div>
        </td>
        <td><strong>${course.members.length}</strong></td>
        <td><strong>${course.groups.length}</strong></td>
        <td><strong>${course.subgroups?.length ?? 0}</strong></td>
        <td><strong>${course.practices.length}</strong></td>
        <td class="directory-actions">
          <button type="button" data-course-open data-course-id="${escapeHtml(course.id)}">Editar</button>
        </td>
      </tr>
    `,
  );

  courseCatalog.innerHTML = rows.length
    ? `
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
          <thead>
            <tr>
              <th>Curso</th>
              <th>Estudiantes</th>
              <th>Grupos</th>
              <th>Subgrupos</th>
              <th>Practicas</th>
              <th>Acciones</th>
            </tr>
          </thead>
          <tbody>${rows.join("")}</tbody>
        </table>
      </div>
    `
    : `<p class="submission-meta">No hay cursos creados.</p>`;

  courseCatalog.querySelectorAll("[data-course-open]").forEach((button) => {
    button.addEventListener("click", () => openCourseWorkspace(button.dataset.courseId));
  });
}

function renderCourseProfileForm(course) {
  return `
    <form id="course-profile-form" class="detail-form detail-form-grid">
      <input name="id" type="hidden" value="${escapeHtml(course.id)}" />
      <label>
        Nombre
        <input name="name" value="${escapeHtml(course.name)}" required />
      </label>
      <label>
        Periodo
        <input name="term" value="${escapeHtml(course.term)}" required />
      </label>
      <label>
        Horas de edición de entregas
        <input name="submission_edit_hours" type="number" min="0" max="72" step="0.5" value="${escapeHtml(String(course.submission_edit_hours ?? 4))}" required />
      </label>
      <div class="detail-actions">
        <button type="submit">Guardar cambios</button>
        <span class="submission-meta">${escapeHtml(state.courseActionStatus)}</span>
      </div>
    </form>
  `;
}

function renderCourseGroupForm(course) {
  return `
    <form id="course-group-form" class="detail-form detail-form-grid">
      <input name="course_id" type="hidden" value="${escapeHtml(course.id)}" />
      <label>
        Nombre
        <input name="name" required placeholder="Grupo 2" />
      </label>
      <label>
        Mesas
        <input name="table_count" type="number" min="1" max="24" step="1" value="4" required />
      </label>
      <label>
        Tipo
        <select name="group_type">
          <option value="regular" selected>Regular</option>
          <option value="recuperacion">Recuperacion</option>
        </select>
      </label>
      <div class="detail-actions">
        <button type="submit">Crear grupo</button>
      </div>
    </form>
  `;
}

function renderCoursePracticesPanel(course) {
  const enabled = new Set(course.practices.map((practice) => practice.id));
  const available = state.practices.filter((practice) => !enabled.has(practice.id));
  return `
    <div class="chips">
      ${course.practices.map((practice) => `<span class="chip">${escapeHtml(practice.name)}</span>`).join("") || `<span class="chip">Sin practicas</span>`}
    </div>
    <form id="course-practice-form" class="detail-form compact">
      <input name="course_id" type="hidden" value="${escapeHtml(course.id)}" />
      <label>
        Habilitar practica
        <select name="practice_id" ${available.length ? "" : "disabled"}>
          ${
            available.length
              ? available.map((practice) => `<option value="${escapeHtml(practice.id)}">${escapeHtml(practice.name)}</option>`).join("")
              : `<option value="">Sin practicas disponibles</option>`
          }
        </select>
      </label>
      <div class="detail-actions">
        <button type="submit" ${available.length ? "" : "disabled"}>Habilitar</button>
      </div>
    </form>
  `;
}

function renderCourseGroupsTable(course) {
  if (course.groups.length === 0) return `<p class="submission-meta">No hay grupos creados.</p>`;
  const rows = course.groups.map(
    (group) => `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(group.name)}</strong></td>
        <td><strong>${group.members.length}</strong></td>
        <td>
          <strong>${group.table_count ?? 4}</strong>
          <div class="submission-meta">${group.group_type === "recuperacion" ? "Recuperación" : "Regular"}</div>
        </td>
        <td>
          <div class="directory-listing">
            ${group.members.map((member) => `<span class="inline-pill">${escapeHtml(member.display_name)}</span>`).join("") || `<span class="submission-meta">Sin estudiantes</span>`}
          </div>
        </td>
        <td class="directory-actions">
          <button type="button" data-group-open data-group-id="${escapeHtml(group.id)}">Editar</button>
        </td>
      </tr>
    `,
  );
  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead>
          <tr>
            <th>Grupo</th>
            <th>Estudiantes</th>
            <th>Mesas</th>
            <th>Integrantes</th>
            <th>Acciones</th>
          </tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function renderCourseSubgroupsPanel(course) {
  const subgroups = course.subgroups ?? [];
  const canCreate = course.groups.length > 0 && course.practices.length > 0;
  const rows = subgroups.map(
    (subgroup) => `
      <tr>
        <td class="directory-primary">
          <strong>${escapeHtml(subgroup.name)}</strong>
        </td>
        <td>${escapeHtml(subgroup.practice.name)}</td>
        <td>${escapeHtml(subgroup.group.name)}</td>
        <td><strong>${subgroup.members.length}</strong></td>
        <td>
          <div class="directory-listing">
            ${subgroup.members.map((member) => `<span class="inline-pill">${escapeHtml(member.display_name)}</span>`).join("") || `<span class="submission-meta">Sin estudiantes</span>`}
          </div>
        </td>
      </tr>
    `,
  );

  return `
    <form id="course-subgroup-form" class="detail-form detail-form-grid">
      <input name="course_id" type="hidden" value="${escapeHtml(course.id)}" />
      <label>
        Practica
        <select name="practice_id" ${canCreate ? "" : "disabled"}>
          ${course.practices.map((practice) => `<option value="${escapeHtml(practice.id)}">${escapeHtml(practice.name)}</option>`).join("") || `<option value="">Sin practicas</option>`}
        </select>
      </label>
      <label>
        Grupo
        <select name="group_id" ${canCreate ? "" : "disabled"}>
          ${course.groups.map((group) => `<option value="${escapeHtml(group.id)}">${escapeHtml(group.name)}</option>`).join("") || `<option value="">Sin grupos</option>`}
        </select>
      </label>
      <label>
        Nombre del subgrupo
        <input name="name" required ${canCreate ? "" : "disabled"} placeholder="Subgrupo A" />
      </label>
      <div class="detail-actions">
        <button type="submit" ${canCreate ? "" : "disabled"}>Crear subgrupo</button>
      </div>
    </form>
    ${
      rows.length
        ? `
          <div class="directory-table-wrap">
            <table class="grade-table directory-data-table">
              <thead>
                <tr>
                  <th>Subgrupo</th>
                  <th>Practica</th>
                  <th>Grupo</th>
                  <th>Estudiantes</th>
                  <th>Integrantes</th>
                </tr>
              </thead>
              <tbody>${rows.join("")}</tbody>
            </table>
          </div>
        `
        : `<p class="submission-meta">No hay subgrupos creados.</p>`
    }
  `;
}

export function openCourseWorkspace(courseId) {
  state.activeCourseId = courseId;
  state.courseActionStatus = "";
  renderCoursesPage();
  selectView("courses");
}

export function closeCourseWorkspace() {
  state.activeCourseId = null;
  state.courseActionStatus = "";
  renderCoursesPage();
}

async function saveCourseEdit(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.courseActionStatus = "";
    await postJson(`/api/academic/courses/${payload.id}`, {
      name: payload.name,
      term: payload.term,
      submission_edit_hours:
        payload.submission_edit_hours === "" ? null : Number(payload.submission_edit_hours),
    });
    state.courseActionStatus = "Cambios guardados";
    await refreshAcademic("Curso actualizado");
  } catch (error) {
    state.courseActionStatus = error.message;
    renderCoursesPage();
  }
}

async function saveCourseGroup(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.courseActionStatus = "";
    await postJson(`/api/academic/courses/${payload.course_id}/groups`, {
      name: payload.name,
      table_count: Number(payload.table_count),
      group_type: payload.group_type,
    });
    state.courseActionStatus = "Grupo creado";
    await refreshAcademic("Grupo creado");
  } catch (error) {
    state.courseActionStatus = error.message;
    renderCoursesPage();
  }
}

async function saveCourseSubgroup(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.courseActionStatus = "";
    await postJson(`/api/academic/courses/${payload.course_id}/subgroups`, {
      practice_id: payload.practice_id,
      group_id: payload.group_id,
      name: payload.name,
    });
    state.courseActionStatus = "Subgrupo creado";
    await refreshAcademic("Subgrupo creado");
  } catch (error) {
    state.courseActionStatus = error.message;
    renderCoursesPage();
  }
}

async function saveCoursePractice(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.courseActionStatus = "";
    await postJson(`/api/academic/courses/${payload.course_id}/practices`, { practice_id: payload.practice_id });
    state.courseActionStatus = "Practica habilitada";
    await refreshAcademic("Practica habilitada");
  } catch (error) {
    state.courseActionStatus = error.message;
    renderCoursesPage();
  }
}
