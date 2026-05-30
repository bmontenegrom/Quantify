import {
  escapeHtml,
  cssEscape,
  format,
  formatDate,
  groupBy,
  renderGroupType,
  scalePayload,
  canReview,
  studentCourses,
  studentGroups,
  studentTotals,
  availableCoursesForStudent,
  availableGroupsForStudent,
  allStudents,
  allGroups,
  analysisKindLabel,
} from "./lib.js";

const state = {
  user: null,
  academic: null,
  practices: [],
  submissions: [],
  gradebooks: [],
  selectedId: null,
  activeStudentId: null,
  studentDetailSection: "overview",
  studentActionStatus: "",
  editingUserId: null,
  userActionStatus: "",
  activeGroupId: null,
  groupActionStatus: "",
  activeCourseId: null,
  courseActionStatus: "",
  highlightedStudentId: null,
  highlightedCourseId: null,
  instruments: [],
  activeInstrumentId: null,
  instrumentActionStatus: "",
  instrumentCourseId: null,
  editingScaleId: null,
  activePracticeId: null,
  practiceDefinition: null,
  practiceActionStatus: "",
  editingQuantityId: null,
  editingResultId: null,
};

const loginScreen = document.querySelector("#login-screen");
const appShell = document.querySelector("#app-shell");
const loginForm = document.querySelector("#login-form");
const loginStatus = document.querySelector("#login-status");
const sessionUser = document.querySelector("#session-user");
const accountProfileForm = document.querySelector("#account-profile-form");
const accountDisplayName = document.querySelector("#account-display-name");
const accountEmail = document.querySelector("#account-email");
const accountRole = document.querySelector("#account-role");
const accountStatus = document.querySelector("#account-status");
const passwordForm = document.querySelector("#password-form");
const passwordStatus = document.querySelector("#password-status");
const logoutButton = document.querySelector("#logout-button");
const courseSelect = document.querySelector("#course-select");
const groupSelect = document.querySelector("#group-select");
const practiceSelect = document.querySelector("#practice-select");
const tableSelect = document.querySelector("#table-select");
const submissionForm = document.querySelector("#submission-form");
const submitStatus = document.querySelector("#submit-status");
const latestResult = document.querySelector("#latest-result");
const submissionsTitle = document.querySelector("#submissions-title");
const submissionsSubtitle = document.querySelector("#submissions-subtitle");
const submissionsListTitle = document.querySelector("#submissions-list-title");
const submissionList = document.querySelector("#submission-list");
const submissionDetail = document.querySelector("#submission-detail");
const userForm = document.querySelector("#user-form");
const courseMemberForm = document.querySelector("#course-member-form");
const memberForm = document.querySelector("#member-form");
const courseCatalog = document.querySelector("#course-catalog");
const courseWorkspace = document.querySelector("#course-workspace");
const userList = document.querySelector("#user-list");
const adminStatus = document.querySelector("#admin-status");
const userStatus = document.querySelector("#user-status");
const gradeComponentForm = document.querySelector("#grade-component-form");
const gradeCourseSelect = document.querySelector("#grade-course-select");
const gradebookCourseFilter = document.querySelector("#gradebook-course-filter");
const teacherGradebook = document.querySelector("#teacher-gradebook");
const gradeStatus = document.querySelector("#grade-status");
const studentDirectory = document.querySelector("#student-directory");
const studentWorkspace = document.querySelector("#student-workspace");
const groupDirectory = document.querySelector("#group-directory");
const groupWorkspace = document.querySelector("#group-workspace");
const instrumentCatalog = document.querySelector("#instrument-catalog");
const instrumentWorkspace = document.querySelector("#instrument-workspace");
const instrumentCourseFilter = document.querySelector("#instrument-course-filter");
const instrumentStatus = document.querySelector("#instrument-status");
const practiceCatalog = document.querySelector("#practice-catalog");
const practiceWorkspace = document.querySelector("#practice-workspace");
const practiceStatus = document.querySelector("#practice-status");

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
    await startApp();
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

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    if (tab.dataset.view === "students" && state.activeStudentId) {
      closeStudentWorkspace();
      selectView("students");
      return;
    }
    if (tab.dataset.view === "groups" && state.activeGroupId) {
      closeGroupWorkspace();
      selectView("groups");
      return;
    }
    if (tab.dataset.view === "courses" && state.activeCourseId) {
      closeCourseWorkspace();
      selectView("courses");
      return;
    }
    if (tab.dataset.view === "instruments" && state.activeInstrumentId) {
      closeInstrumentWorkspace();
      selectView("instruments");
      return;
    }
    if (tab.dataset.view === "practices" && state.activePracticeId) {
      closePracticeWorkspace();
      selectView("practices");
      return;
    }
    selectView(tab.dataset.view);
  });
});

document.querySelector("#refresh-submissions").addEventListener("click", loadSubmissions);
courseSelect.addEventListener("change", updateStudentSelectors);
groupSelect.addEventListener("change", updateTableSelector);
practiceSelect.addEventListener("change", updateTableSelector);
gradebookCourseFilter.addEventListener("change", renderGradebookAdmin);
instrumentCourseFilter.addEventListener("change", () => {
  state.instrumentCourseId = instrumentCourseFilter.value;
  state.activeInstrumentId = null;
  state.editingScaleId = null;
  refreshInstruments();
});

gradeComponentForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withGradeError(async () => {
    const payload = Object.fromEntries(new FormData(gradeComponentForm).entries());
    payload.max_points = Number(payload.max_points);
    payload.weight_points = Number(payload.weight_points);
    await postJson("/api/grades/components", payload);
    gradeComponentForm.reset();
    await loadGrades();
    renderGradebookAdmin();
    gradeStatus.textContent = "Componente creado";
  });
});

userForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    await postJson("/api/users", Object.fromEntries(new FormData(userForm).entries()));
    userForm.reset();
    await refreshAcademic("Usuario creado");
  });
});

submissionForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  submitStatus.textContent = "Subiendo...";

  try {
    const formData = new FormData(submissionForm);
    await postJson(`/api/academic/groups/${formData.get("group_id")}/practice-table`, {
      practice_id: formData.get("practice_id"),
      table_number: Number(formData.get("table_number")),
    });
    const response = await fetch("/api/submissions", {
      method: "POST",
      body: formData,
    });

    if (!response.ok) throw new Error(await errorText(response));

    const submission = await response.json();
    submitStatus.textContent = "Entrega guardada";
    submissionForm.reset();
    await loadAcademic();
    renderAnalysis(latestResult, submission);
    latestResult.classList.remove("hidden");
    await loadSubmissions();
  } catch (error) {
    submitStatus.textContent = error.message;
  }
});

async function init() {
  try {
    const body = await fetchJson("/api/auth/me");
    state.user = body.user;
    await startApp();
  } catch {
    loginScreen.classList.remove("hidden");
    appShell.classList.add("hidden");
  }
}

async function startApp() {
  loginScreen.classList.add("hidden");
  appShell.classList.remove("hidden");
  renderSessionUser();
  renderAccount();

  document.querySelectorAll(".teacher-only").forEach((element) => {
    element.classList.toggle("hidden", !canReview(state.user));
  });

  selectView("submissions");
  await loadAcademic();
  await loadSubmissions();
}

function renderSessionUser() {
  sessionUser.textContent = `${state.user.display_name} (${state.user.role})`;
}

function renderAccount() {
  if (!state.user) return;
  accountDisplayName.value = state.user.display_name;
  accountEmail.value = state.user.email;
  accountRole.value = state.user.role;
}

function selectView(view) {
  document.querySelectorAll(".tab").forEach((item) => item.classList.toggle("active", item.dataset.view === view));
  document.querySelectorAll(".view").forEach((item) => item.classList.remove("active"));
  document.querySelector(`#${view}-view`).classList.add("active");
  if (view === "submissions") loadSubmissions();
  if (view === "gradebook") {
    loadGrades().then(() => {
      renderGradebookAdmin();
    });
  }
  if (view === "students") {
    loadGrades().then(renderStudentsPage);
    renderStudentsPage();
  }
  if (view === "groups") renderGroupsPage();
  if (view === "courses") renderCoursesPage();
  if (view === "instruments") {
    renderInstrumentsPage();
    refreshInstruments();
  }
  if (view === "practices") {
    renderPracticesPage();
  }
  if (view === "account") renderAccount();
}

async function loadSubmissions() {
  state.submissions = await fetchJson("/api/submissions");
  renderSubmissionsPage();
}

async function loadAcademic() {
  state.academic = await fetchJson("/api/academic/context");
  state.practices = state.academic.practices;
  renderStudentSelectors();
  if (canReview(state.user)) {
    renderAdmin();
    renderStudentsPage();
    renderGroupsPage();
    renderCoursesPage();
  }
  if (canReview(state.user)) renderGradeCourseOptions();
  if (canReview(state.user)) renderInstrumentCourseOptions();
}

async function loadGrades() {
  state.gradebooks = await fetchJson("/api/grades");
  if (canReview(state.user)) renderGradeCourseOptions();
}

function renderGradeCourseOptions() {
  const options = state.academic.courses
    .map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`)
    .join("");
  gradeCourseSelect.innerHTML = options;
  gradebookCourseFilter.innerHTML = options;
}

function renderKindTotals(summary) {
  return `
    <div class="metrics">
      ${summary.totals_by_kind
        .map(
          (total) => `
            <div class="metric">
              <div class="metric-label">${escapeHtml(total.kind)}</div>
              <div class="metric-value">${format(total.points)} / ${format(total.possible)}</div>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}

function renderStudentGradeTable(summary) {
  return `
    <div class="grade-table-wrap">
      <table class="grade-table">
        <thead>
          <tr>
            <th>Tipo</th>
            <th>Item</th>
            <th>Puntaje</th>
            <th>Aporte</th>
            <th>Comentario</th>
          </tr>
        </thead>
        <tbody>
          ${summary.scores
            .map(
              (score) => `
                <tr>
                  <td>${escapeHtml(score.kind)}</td>
                  <td>${escapeHtml(score.name)}</td>
                  <td>${score.raw_points ?? "-"} / ${format(score.max_points)}</td>
                  <td>${format(score.normalized_points)} / ${format(score.weight_points)}</td>
                  <td>${escapeHtml(score.comment ?? "")}</td>
                </tr>
              `,
            )
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}

function renderGradebookAdmin() {
  if (state.gradebooks.length === 0) {
    teacherGradebook.innerHTML = `<section class="detail-empty">No hay cursos cargados.</section>`;
    return;
  }

  const selectedId = state.highlightedCourseId || gradebookCourseFilter.value || state.gradebooks[0].course.id;
  const book = state.gradebooks.find((item) => item.course.id === selectedId) ?? state.gradebooks[0];
  gradebookCourseFilter.value = book.course.id;
  state.highlightedCourseId = book.course.id;

  if (book.components.length === 0) {
    teacherGradebook.innerHTML = `
      <section class="detail-empty">
        Crea componentes para ${escapeHtml(book.course.name)} antes de cargar notas.
      </section>
    `;
    return;
  }

  teacherGradebook.innerHTML = `
    <section class="grade-course">
      <div>
        <h3>${escapeHtml(book.course.name)} (${escapeHtml(book.course.term)})</h3>
        <p class="submission-meta">${book.components.length} componentes - ${book.students.length} estudiantes</p>
      </div>
      <div class="student-grade-list">
        ${book.students.map((summary) => renderStudentGradeCard(summary, book.components)).join("")}
      </div>
    </section>
  `;

  teacherGradebook.querySelectorAll(".grade-input").forEach((input) => {
    input.addEventListener("change", () => saveGradeInput(input));
  });

  if (state.highlightedStudentId) {
    teacherGradebook
      .querySelector(`[data-student-id="${cssEscape(state.highlightedStudentId)}"]`)
      ?.scrollIntoView({ block: "center", behavior: "smooth" });
  }
}

function renderStudentGradeCard(summary, components) {
  return `
    <article class="student-grade-card ${summary.student.id === state.highlightedStudentId ? "highlighted" : ""}" data-student-id="${escapeHtml(summary.student.id)}">
      <div class="student-grade-head">
        <div>
          <h4>${escapeHtml(summary.student.display_name)}</h4>
          <p class="submission-meta">${escapeHtml(summary.student.email)}</p>
        </div>
        <div class="grade-total">${format(summary.total_points)} / ${format(summary.total_possible)}</div>
      </div>
      ${renderKindTotals(summary)}
      <div class="student-grade-fields">
        ${components.map((component) => renderGradeField(summary, component)).join("")}
      </div>
    </article>
  `;
}

function renderGradeField(summary, component) {
  const score = summary.scores.find((item) => item.component_id === component.id);
  return `
    <label class="grade-field">
      <span>${escapeHtml(component.name)}</span>
      <small>${escapeHtml(component.kind)} - sobre ${format(component.max_points)} - vale ${format(component.weight_points)}</small>
      <input class="grade-input" type="number" min="0" max="${component.max_points}" step="0.01"
        value="${score?.raw_points ?? ""}"
        data-component-id="${escapeHtml(component.id)}"
        data-student-id="${escapeHtml(summary.student.id)}" />
      <div class="submission-meta">${format(score?.normalized_points ?? 0)} / ${format(component.weight_points)}</div>
    </label>
  `;
}

async function saveGradeInput(input) {
  if (input.value === "") return;
  await withGradeError(async () => {
    await postJson("/api/grades/scores", {
      component_id: input.dataset.componentId,
      student_id: input.dataset.studentId,
      raw_points: Number(input.value),
      comment: null,
    });
    await loadGrades();
    renderGradebookAdmin();
    if (state.activeStudentId) renderStudentsPage();
    gradeStatus.textContent = "Nota guardada";
  });
}

async function withGradeError(action) {
  try {
    gradeStatus.textContent = "";
    await action();
  } catch (error) {
    gradeStatus.textContent = error.message;
  }
}

async function refreshAcademic(message) {
  await loadAcademic();
  adminStatus.textContent = message;
  userStatus.textContent = message;
  window.setTimeout(() => {
    adminStatus.textContent = "";
    userStatus.textContent = "";
  }, 2500);
}

async function withAdminError(action) {
  try {
    adminStatus.textContent = "";
    userStatus.textContent = "";
    await action();
  } catch (error) {
    adminStatus.textContent = error.message;
    userStatus.textContent = error.message;
  }
}

function renderStudentSelectors() {
  const courses = state.academic.courses;
  courseSelect.innerHTML = courses.length
    ? courses.map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`).join("")
    : `<option value="">Sin cursos asignados</option>`;
  updateStudentSelectors();
}

function updateStudentSelectors() {
  const course = selectedCourse();
  groupSelect.innerHTML = course?.groups.length
    ? course.groups.map((group) => `<option value="${escapeHtml(group.id)}">${escapeHtml(group.name)}</option>`).join("")
    : `<option value="">Sin grupos</option>`;
  practiceSelect.innerHTML = course?.practices.length
    ? course.practices.map((practice) => `<option value="${escapeHtml(practice.id)}">${escapeHtml(practice.name)}</option>`).join("")
    : `<option value="">Sin practicas habilitadas</option>`;
  updateTableSelector();
}

function updateTableSelector() {
  if (!tableSelect) return;
  const group = selectedCourse()?.groups.find((item) => item.id === groupSelect.value);
  const assignment = selectedTableAssignment();
  const tableCount = group?.table_count ?? 0;
  tableSelect.innerHTML = tableCount
    ? Array.from({ length: tableCount }, (_, index) => {
        const tableNumber = index + 1;
        return `<option value="${tableNumber}" ${assignment?.table_number === tableNumber ? "selected" : ""}>Mesa ${tableNumber}</option>`;
      }).join("")
    : `<option value="">Sin mesas</option>`;
  tableSelect.disabled = !tableCount;
}

function renderAdmin() {
  const courses = state.academic.courses;
  renderCourseDirectory(courses);
  renderUsers();
}

function renderUsers() {
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

function renderStudentDirectory() {
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

function renderStudentPoints(totals) {
  if (!totals) return `<span class="submission-meta">Sin notas cargadas</span>`;
  return `
    <strong>${format(totals.points)} / ${format(totals.possible)}</strong>
    <div class="submission-meta">Puntos acumulados</div>
  `;
}

function renderStudentsPage() {
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

function renderStudentProfileForm(student) {
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

function renderStudentEnrollmentPanel(student) {
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

function renderStudentGradeEditor(student) {
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

function openStudentWorkspace(studentId, section = "overview") {
  state.activeStudentId = studentId;
  state.studentDetailSection = section;
  renderStudentsPage();
  selectView("students");
}

function closeStudentWorkspace() {
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

function renderGroupDirectory() {
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

function renderGroupsPage() {
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

function openGroupWorkspace(groupId) {
  state.activeGroupId = groupId;
  state.groupActionStatus = "";
  renderGroupsPage();
  selectView("groups");
}

function closeGroupWorkspace() {
  state.activeGroupId = null;
  state.groupActionStatus = "";
  renderGroupsPage();
}

function toggleUserAction(userId) {
  state.userActionStatus = "";
  state.editingUserId = state.editingUserId === userId ? null : userId;
  renderUsers();
}

function clearUserAction() {
  state.editingUserId = null;
  state.userActionStatus = "";
  renderUsers();
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

async function removeGroupMember(groupId, studentId, origin) {
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

async function saveUserReset(event) {
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

function renderCoursesPage() {
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

function renderCourseDirectory(courses) {
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

function openCourseWorkspace(courseId) {
  state.activeCourseId = courseId;
  state.courseActionStatus = "";
  renderCoursesPage();
  selectView("courses");
}

function closeCourseWorkspace() {
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

function renderUserDetail(user) {
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

function selectedCourse() {
  return state.academic?.courses.find((course) => course.id === courseSelect.value);
}

function selectedTableAssignment() {
  const course = selectedCourse();
  return course?.table_assignments?.find(
    (assignment) =>
      assignment.user_id === state.user?.id &&
      assignment.group_id === groupSelect.value &&
      assignment.practice_id === practiceSelect.value,
  );
}

function renderSubmissionsPage() {
  const teacher = canReview(state.user);
  submissionsTitle.textContent = teacher ? "Entregas" : "Mis entregas";
  submissionsSubtitle.textContent = teacher
    ? "Todas las entregas organizadas por curso y grupo."
    : "Tus entregas y el estado de correccion.";
  submissionsListTitle.textContent = teacher ? "Entregas por curso y grupo" : "Mis entregas";
  submissionForm.classList.toggle("hidden", teacher);
  latestResult.classList.toggle("hidden", teacher || latestResult.innerHTML.trim() === "");
  renderSubmissionList();
}

function renderSubmissionList() {
  if (state.submissions.length === 0) {
    submissionList.innerHTML = `<p class="submission-meta">Todavia no hay entregas.</p>`;
    submissionDetail.classList.add("hidden");
    return;
  }

  submissionList.innerHTML = canReview(state.user) ? renderTeacherSubmissionGroups() : renderStudentSubmissionRows();

  submissionList.querySelectorAll(".submission-item").forEach((item) => {
    item.addEventListener("click", () => loadSubmissionDetail(item.dataset.id));
  });
}

function renderStudentSubmissionRows() {
  return state.submissions
    .map(
      (item) => `
        <article class="submission-item ${item.id === state.selectedId ? "active" : ""}" data-id="${escapeHtml(item.id)}">
          <strong>${escapeHtml(item.practice_name)}</strong>
          <div class="submission-meta">${escapeHtml(item.course)} - Grupo ${escapeHtml(item.group_name)}</div>
          <div class="submission-meta">${formatDate(item.submitted_at)}</div>
          <span class="status ${escapeHtml(item.status)}">${escapeHtml(item.status)}</span>
        </article>
      `,
    )
    .join("");
}

function renderTeacherSubmissionGroups() {
  const byCourse = groupBy(state.submissions, (item) => item.course);
  return Object.entries(byCourse)
    .map(([course, courseItems]) => {
      const byGroup = groupBy(courseItems, (item) => item.group_name);
      return `
        <section class="submission-group">
          <h4>${escapeHtml(course)}</h4>
          ${Object.entries(byGroup)
            .map(
              ([group, groupItems]) => `
                <div class="submission-course-group">
                  <div class="list-head compact-list-head">
                    <strong>Grupo ${escapeHtml(group)}</strong>
                    <span class="submission-meta">${groupItems.length} entregas</span>
                  </div>
                  ${groupItems
                    .map(
                      (item) => `
                        <article class="submission-item ${item.id === state.selectedId ? "active" : ""}" data-id="${escapeHtml(item.id)}">
                          <strong>${escapeHtml(item.student_name)}</strong>
                          <div class="submission-meta">${escapeHtml(item.practice_name)} - ${formatDate(item.submitted_at)}</div>
                          <span class="status ${escapeHtml(item.status)}">${escapeHtml(item.status)}</span>
                        </article>
                      `,
                    )
                    .join("")}
                </div>
              `,
            )
            .join("")}
        </section>
      `;
    })
    .join("");
}

async function loadSubmissionDetail(id) {
  state.selectedId = id;
  renderSubmissionList();
  const submission = await fetchJson(`/api/submissions/${id}`);
  submissionDetail.classList.remove("hidden");
  renderAnalysis(submissionDetail, submission, canReview(state.user));
}

function renderAnalysis(target, submission, includeReview = false) {
  const analysis = submission.analysis;
  const regression = analysis.regression;
  target.classList.remove("detail-empty");
  target.innerHTML = `
    <div>
      <h3>${escapeHtml(submission.practice_name)}</h3>
      <p class="submission-meta">
        ${escapeHtml(submission.student_name)} - Grupo ${escapeHtml(submission.group_name)} - ${escapeHtml(submission.course)}
      </p>
      <span class="status ${escapeHtml(submission.status)}">${escapeHtml(submission.status)}</span>
    </div>

    <div class="metrics">
      <div class="metric">
        <div class="metric-label">Filas</div>
        <div class="metric-value">${analysis.row_count}</div>
      </div>
      <div class="metric">
        <div class="metric-label">Columnas numericas</div>
        <div class="metric-value">${analysis.numeric_columns.length}</div>
      </div>
      <div class="metric">
        <div class="metric-label">Advertencias</div>
        <div class="metric-value">${analysis.warnings.length}</div>
      </div>
    </div>

    ${renderStats(analysis.numeric_columns)}
    ${regression ? renderRegression(regression) : ""}
    ${renderWarnings(analysis.warnings)}
    ${includeReview ? renderReviewForm(submission) : ""}
  `;

  const reviewForm = target.querySelector(".review-form");
  if (reviewForm) reviewForm.addEventListener("submit", (event) => saveReview(event, submission.id));
}

function renderStats(columns) {
  if (columns.length === 0) return `<p class="submission-meta">No se detectaron columnas numericas.</p>`;
  return `
    <h3>Estadistica por columna</h3>
    <div class="metrics">
      ${columns
        .map(
          (column) => `
          <div class="metric">
            <div class="metric-label">${escapeHtml(column.name)}</div>
            <div>n=${column.count}</div>
            <div>media=${format(column.mean)}</div>
            <div>sd=${format(column.std_dev)}</div>
            <div>min=${format(column.min)} max=${format(column.max)}</div>
          </div>
        `,
        )
        .join("")}
    </div>
  `;
}

function renderRegression(regression) {
  return `
    <h3>Ajuste lineal automatico</h3>
    <div class="metric">
      <div>${escapeHtml(regression.y_column)} = ${format(regression.slope)} * ${escapeHtml(regression.x_column)} + ${format(regression.intercept)}</div>
      <div class="submission-meta">R2 = ${format(regression.r_squared)}</div>
    </div>
  `;
}

function renderWarnings(warnings) {
  if (warnings.length === 0) return "";
  return `
    <div class="warnings">
      <strong>Advertencias</strong>
      ${warnings.slice(0, 8).map((warning) => `<span>${escapeHtml(warning)}</span>`).join("")}
      ${warnings.length > 8 ? `<span>${warnings.length - 8} mas...</span>` : ""}
    </div>
  `;
}

function renderReviewForm(submission) {
  return `
    <form class="review-form">
      <div class="review-row">
        <label>
          Estado
          <select name="status">
            ${["pendiente", "observada", "aprobada"]
              .map(
                (status) =>
                  `<option value="${status}" ${status === submission.status ? "selected" : ""}>${status}</option>`,
              )
              .join("")}
          </select>
        </label>
        <label>
          Nota
          <input name="score" type="number" min="0" max="100" step="0.1" value="${submission.score ?? ""}" />
        </label>
      </div>
      <label>
        Comentario docente
        <textarea name="teacher_comment">${escapeHtml(submission.teacher_comment ?? "")}</textarea>
      </label>
      <div class="review-actions">
        <button type="submit">Guardar correccion</button>
        <span class="submission-meta">${submission.reviewed_at ? `Revisada: ${new Date(submission.reviewed_at).toLocaleString()}` : ""}</span>
      </div>
    </form>
  `;
}

async function saveReview(event, id) {
  event.preventDefault();
  const form = event.currentTarget;
  const payload = Object.fromEntries(new FormData(form).entries());
  payload.score = payload.score === "" ? null : Number(payload.score);

  const response = await fetch(`/api/submissions/${id}/review`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    alert(await errorText(response));
    return;
  }

  const updated = await response.json();
  renderAnalysis(submissionDetail, updated, true);
  await loadSubmissions();
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

async function postJson(url, payload) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

async function errorText(response) {
  try {
    const body = await response.json();
    return body.error ?? response.statusText;
  } catch {
    return response.statusText;
  }
}

// ── Instrumentos ──────────────────────────────────────────────────────────────

function renderInstrumentCourseOptions() {
  const options = state.academic.courses
    .map((c) => `<option value="${escapeHtml(c.id)}">${escapeHtml(c.name)} (${escapeHtml(c.term)})</option>`)
    .join("");
  instrumentCourseFilter.innerHTML = options;
  if (!state.instrumentCourseId && state.academic.courses.length > 0) {
    state.instrumentCourseId = state.academic.courses[0].id;
  }
  instrumentCourseFilter.value = state.instrumentCourseId ?? "";
}

async function loadInstruments() {
  const courseId = state.instrumentCourseId || state.academic?.courses[0]?.id;
  if (!courseId) { state.instruments = []; return; }
  state.instrumentCourseId = courseId;
  state.instruments = await fetchJson(`/api/instruments?course_id=${encodeURIComponent(courseId)}`);
}

// Carga los instrumentos y re-renderiza, mostrando el error en el status si la carga falla.
async function refreshInstruments() {
  try {
    await loadInstruments();
    renderInstrumentsPage();
  } catch (error) {
    state.instruments = [];
    renderInstrumentsPage();
    withInstrumentStatus(error.message);
  }
}

function renderInstrumentsPage() {
  renderInstrumentDirectory();
  if (!instrumentWorkspace) return;

  const item = state.instruments.find((i) => i.id === state.activeInstrumentId);
  if (!item) {
    instrumentWorkspace.innerHTML = "";
    instrumentWorkspace.classList.add("hidden");
    instrumentCatalog.closest(".panel")?.classList.remove("hidden");
    return;
  }

  instrumentWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="instrument-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(item.name)}</h3>
        <p class="submission-meta">${escapeHtml(item.quantity)} · ${escapeHtml(item.unit)} · <span class="status-chip">${escapeHtml(item.kind)}</span></p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Escalas</div>
          <div class="metric-value">${item.scales.length}</div>
        </div>
      </div>
    </div>

    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Datos del instrumento</h3>
        ${renderInstrumentProfileForm(item)}
      </section>
      <section class="panel workspace-panel">
        <h3>Nueva escala</h3>
        ${renderScaleForm(null, item.id)}
      </section>
    </div>

    <section class="panel workspace-panel">
      <div class="list-head">
        <h3>Escalas</h3>
        <span class="submission-meta">${escapeHtml(state.instrumentActionStatus)}</span>
      </div>
      ${renderScalesList(item)}
    </section>
  `;

  instrumentWorkspace.classList.remove("hidden");
  instrumentCatalog.closest(".panel")?.classList.add("hidden");

  instrumentWorkspace.querySelector("#instrument-workspace-back")?.addEventListener("click", closeInstrumentWorkspace);
  instrumentWorkspace.querySelector("#instrument-profile-form")?.addEventListener("submit", saveInstrumentEdit);
  const newScaleForm = instrumentWorkspace.querySelector("#new-scale-form");
  if (newScaleForm) {
    wireScaleBModelToggle(newScaleForm);
    newScaleForm.addEventListener("submit", saveNewScale);
  }
  instrumentWorkspace.querySelectorAll("[data-edit-scale]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingScaleId = state.editingScaleId === btn.dataset.scaleId ? null : btn.dataset.scaleId;
      renderInstrumentsPage();
    });
  });
  instrumentWorkspace.querySelectorAll("[data-delete-scale]").forEach((btn) => {
    btn.addEventListener("click", () => deleteScale(btn.dataset.scaleId, item.id));
  });
  instrumentWorkspace.querySelectorAll("[data-cancel-scale]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingScaleId = null; renderInstrumentsPage(); });
  });
  instrumentWorkspace.querySelectorAll("[data-edit-scale-form]").forEach((form) => {
    wireScaleBModelToggle(form);
    form.addEventListener("submit", saveEditScale);
  });
}

function renderInstrumentDirectory() {
  if (!instrumentCatalog || !state.academic) return;

  const rows = state.instruments.map((item) => `
    <tr>
      <td class="directory-primary">
        <strong>${escapeHtml(item.name)}</strong>
      </td>
      <td><span class="status-chip">${escapeHtml(item.kind)}</span></td>
      <td>${escapeHtml(item.quantity)}</td>
      <td>${escapeHtml(item.unit)}</td>
      <td><strong>${item.scales.length}</strong></td>
      <td class="directory-actions">
        <button type="button" data-instrument-open data-instrument-id="${escapeHtml(item.id)}">Editar</button>
        <button type="button" data-instrument-delete data-instrument-id="${escapeHtml(item.id)}">Eliminar</button>
      </td>
    </tr>
  `);

  const courseId = state.instrumentCourseId ?? "";
  const exportImportBar = `
    <div class="detail-actions instrument-toolbar">
      <button type="button" id="instrument-export-btn">Exportar JSON</button>
      <button type="button" id="instrument-import-btn">Importar JSON</button>
      <input type="file" id="instrument-import-file" accept=".json,application/json" class="hidden" />
      <span id="instrument-import-status" class="submission-meta"></span>
    </div>
    <div class="panel instrument-new-panel">
      <h3>Nuevo instrumento</h3>
      <form id="new-instrument-form" class="detail-form detail-form-grid">
        <input name="course_id" type="hidden" value="${escapeHtml(courseId)}" />
        <label>Nombre <input name="name" required placeholder="Tester A830L" /></label>
        <label>Tipo
          <select name="kind" required>
            <option value="digital">digital</option>
            <option value="analogico">analogico</option>
          </select>
        </label>
        <label>Magnitud <input name="quantity" required placeholder="corriente" /></label>
        <label>Unidad <input name="unit" required placeholder="A" /></label>
        <div class="detail-actions">
          <button type="submit">Crear instrumento</button>
          <span id="new-instrument-status" class="submission-meta"></span>
        </div>
      </form>
    </div>
  `;

  instrumentCatalog.innerHTML = exportImportBar + (rows.length
    ? `
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
          <thead>
            <tr>
              <th>Nombre</th>
              <th>Tipo</th>
              <th>Magnitud</th>
              <th>Unidad</th>
              <th>Escalas</th>
              <th>Acciones</th>
            </tr>
          </thead>
          <tbody>${rows.join("")}</tbody>
        </table>
      </div>
    `
    : `<p class="submission-meta">No hay instrumentos para este curso.</p>`);

  instrumentCatalog.querySelectorAll("[data-instrument-open]").forEach((btn) => {
    btn.addEventListener("click", () => openInstrumentWorkspace(btn.dataset.instrumentId));
  });
  instrumentCatalog.querySelectorAll("[data-instrument-delete]").forEach((btn) => {
    btn.addEventListener("click", () => deleteInstrument(btn.dataset.instrumentId));
  });
  instrumentCatalog.querySelector("#new-instrument-form")?.addEventListener("submit", saveNewInstrument);
  instrumentCatalog.querySelector("#instrument-export-btn")?.addEventListener("click", exportInstruments);
  instrumentCatalog.querySelector("#instrument-import-btn")?.addEventListener("click", () => {
    instrumentCatalog.querySelector("#instrument-import-file")?.click();
  });
  instrumentCatalog.querySelector("#instrument-import-file")?.addEventListener("change", importInstruments);
}

function renderInstrumentProfileForm(item) {
  return `
    <form id="instrument-profile-form" class="detail-form detail-form-grid">
      <input name="id" type="hidden" value="${escapeHtml(item.id)}" />
      <label>Nombre <input name="name" value="${escapeHtml(item.name)}" required /></label>
      <label>Tipo
        <select name="kind" required>
          ${["digital", "analogico"].map((k) => `<option value="${k}" ${k === item.kind ? "selected" : ""}>${k}</option>`).join("")}
        </select>
      </label>
      <label>Magnitud <input name="quantity" value="${escapeHtml(item.quantity)}" required /></label>
      <label>Unidad <input name="unit" value="${escapeHtml(item.unit)}" required /></label>
      <div class="detail-actions">
        <button type="submit">Guardar cambios</button>
        <span class="submission-meta">${escapeHtml(state.instrumentActionStatus)}</span>
      </div>
    </form>
  `;
}

function renderScaleForm(scale, instrumentId) {
  const v = (field) => scale ? escapeHtml(String(scale[field] ?? "")) : "";
  const bModel = scale?.b_model ?? "resolucion";
  const isApre = bModel === "apreciacion";
  const isFab = bModel === "fabricante";
  const formId = scale ? "edit-scale-form" : "new-scale-form";
  const formAttr = scale ? `data-edit-scale-form data-scale-id="${escapeHtml(scale.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="instrument_id" type="hidden" value="${escapeHtml(instrumentId)}" />
      ${scale ? `<input name="scale_id" type="hidden" value="${escapeHtml(scale.id)}" />` : ""}
      <label>Etiqueta <input name="label" value="${v("label")}" required placeholder="200 uA" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" required placeholder="A" /></label>
      <label>Modelo de incertidumbre tipo B
        <select name="b_model" required>
          ${["resolucion", "apreciacion", "fabricante"].map((m) => `<option value="${m}" ${m === bModel ? "selected" : ""}>${m}</option>`).join("")}
        </select>
      </label>
      <label>Paso / Resolución <input name="step" type="number" step="any" value="${v("step")}" required placeholder="0.1e-6" /></label>
      <label>Fondo de escala <input name="full_scale" type="number" step="any" value="${v("full_scale")}" placeholder="200e-6" /></label>
      <label class="scale-field-apre ${isApre ? "" : "hidden"}">
        Apreciación <input name="appreciation" type="number" step="any" value="${v("appreciation")}" placeholder="0.5" />
      </label>
      <div class="scale-fields-fab ${isFab ? "" : "hidden"}">
        <label>Espec. % lectura <input name="spec_pct_reading" type="number" step="any" value="${v("spec_pct_reading")}" placeholder="1.0" /></label>
        <label>Espec. coef. paso <input name="spec_step_coeff" type="number" step="any" value="${v("spec_step_coeff")}" placeholder="5.0" /></label>
        <label>Espec. fijo <input name="spec_fixed" type="number" step="any" value="${v("spec_fixed")}" placeholder="0.0" /></label>
        <label>Res. interna (Ω) <input name="internal_res" type="number" step="any" value="${v("internal_res")}" /></label>
        <label>Incert. Res. interna <input name="internal_res_u" type="number" step="any" value="${v("internal_res_u")}" /></label>
      </div>
      <div class="detail-actions">
        <button type="submit">${scale ? "Guardar" : "Agregar escala"}</button>
        ${scale ? `<button type="button" data-cancel-scale>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function wireScaleBModelToggle(form) {
  const select = form.querySelector('[name="b_model"]');
  if (!select) return;
  const update = () => {
    const val = select.value;
    form.querySelector(".scale-field-apre")?.classList.toggle("hidden", val !== "apreciacion");
    form.querySelector(".scale-fields-fab")?.classList.toggle("hidden", val !== "fabricante");
  };
  select.addEventListener("change", update);
}

function renderScalesList(item) {
  if (item.scales.length === 0) return `<p class="submission-meta">Sin escalas. Agrega una desde el panel superior.</p>`;

  const rows = item.scales.flatMap((scale) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(scale.label)}</strong></td>
        <td><span class="status-chip">${escapeHtml(scale.b_model)}</span></td>
        <td>${format(scale.step)}</td>
        <td>${scale.full_scale != null ? format(scale.full_scale) : "-"}</td>
        <td>${escapeHtml(scale.unit)}</td>
        <td class="directory-actions">
          <button type="button" data-edit-scale data-scale-id="${escapeHtml(scale.id)}">${state.editingScaleId === scale.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-scale data-scale-id="${escapeHtml(scale.id)}">Eliminar</button>
        </td>
      </tr>
    `;
    const editRow = state.editingScaleId === scale.id
      ? `<tr><td colspan="6" class="scale-edit-cell">${renderScaleForm(scale, item.id)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead>
          <tr>
            <th>Etiqueta</th>
            <th>Modelo</th>
            <th>Paso</th>
            <th>Fondo</th>
            <th>Unidad</th>
            <th>Acciones</th>
          </tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function openInstrumentWorkspace(instrumentId) {
  state.activeInstrumentId = instrumentId;
  state.instrumentActionStatus = "";
  state.editingScaleId = null;
  renderInstrumentsPage();
  selectView("instruments");
}

function closeInstrumentWorkspace() {
  state.activeInstrumentId = null;
  state.instrumentActionStatus = "";
  state.editingScaleId = null;
  renderInstrumentsPage();
}

async function saveNewInstrument(event) {
  event.preventDefault();
  const status = instrumentCatalog.querySelector("#new-instrument-status");
  try {
    if (status) status.textContent = "";
    const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
    await postJson("/api/instruments", payload);
    event.currentTarget.reset();
    event.currentTarget.querySelector('[name="course_id"]').value = state.instrumentCourseId ?? "";
    await loadInstruments();
    renderInstrumentsPage();
    withInstrumentStatus("Instrumento creado");
  } catch (error) {
    if (status) status.textContent = error.message;
    else withInstrumentStatus(error.message);
  }
}

async function saveInstrumentEdit(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    state.instrumentActionStatus = "";
    await postJson(`/api/instruments/${payload.id}`, {
      name: payload.name,
      kind: payload.kind,
      quantity: payload.quantity,
      unit: payload.unit,
    });
    state.instrumentActionStatus = "Cambios guardados";
    await loadInstruments();
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

function scalePayloadFromForm(form) {
  return scalePayload(Object.fromEntries(new FormData(form).entries()));
}

async function saveNewScale(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const instrumentId = form.querySelector('[name="instrument_id"]').value;
  try {
    await postJson(`/api/instruments/${instrumentId}/scales`, scalePayloadFromForm(form));
    await loadInstruments();
    state.editingScaleId = null;
    state.instrumentActionStatus = "Escala agregada";
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

async function saveEditScale(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const instrumentId = form.querySelector('[name="instrument_id"]').value;
  const scaleId = form.querySelector('[name="scale_id"]').value;
  try {
    await postJson(`/api/instruments/${instrumentId}/scales/${scaleId}`, scalePayloadFromForm(form));
    await loadInstruments();
    state.editingScaleId = null;
    state.instrumentActionStatus = "Escala actualizada";
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

async function deleteScale(scaleId, instrumentId) {
  if (!window.confirm("¿Eliminar esta escala? Esta accion no se puede deshacer.")) return;
  try {
    const response = await fetch(`/api/instruments/${instrumentId}/scales/${scaleId}`, { method: "DELETE" });
    if (!response.ok) throw new Error(await errorText(response));
    await loadInstruments();
    state.editingScaleId = null;
    state.instrumentActionStatus = "Escala eliminada";
    renderInstrumentsPage();
  } catch (error) {
    state.instrumentActionStatus = error.message;
    renderInstrumentsPage();
  }
}

async function deleteInstrument(instrumentId) {
  const item = state.instruments.find((i) => i.id === instrumentId);
  const extra = item?.scales.length ? ` y sus ${item.scales.length} escala(s)` : "";
  if (!window.confirm(`¿Eliminar el instrumento "${item?.name ?? ""}"${extra}? Esta accion no se puede deshacer.`)) return;
  try {
    withInstrumentStatus("");
    const response = await fetch(`/api/instruments/${instrumentId}`, { method: "DELETE" });
    if (!response.ok) throw new Error(await errorText(response));
    await loadInstruments();
    renderInstrumentsPage();
    withInstrumentStatus("Instrumento eliminado");
  } catch (error) {
    withInstrumentStatus(error.message);
  }
}

async function exportInstruments() {
  try {
    withInstrumentStatus("");
    const courseId = state.instrumentCourseId;
    const data = await fetchJson(`/api/instruments/export?course_id=${encodeURIComponent(courseId)}`);
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "instrumentos.json";
    a.click();
    URL.revokeObjectURL(url);
    withInstrumentStatus("Catalogo exportado");
  } catch (error) {
    withInstrumentStatus(error.message);
  }
}

async function importInstruments(event) {
  const file = event.target.files?.[0];
  if (!file) return;
  const importStatus = instrumentCatalog.querySelector("#instrument-import-status");
  try {
    if (importStatus) importStatus.textContent = "Importando...";
    const text = await file.text();
    const catalog = JSON.parse(text);
    await postJson("/api/instruments/import", {
      course_id: state.instrumentCourseId,
      instruments: catalog.instruments,
    });
    event.target.value = "";
    await loadInstruments();
    renderInstrumentsPage();
    withInstrumentStatus(`${catalog.instruments?.length ?? 0} instrumentos importados`);
  } catch (error) {
    if (importStatus) importStatus.textContent = error.message;
    else withInstrumentStatus(error.message);
    event.target.value = "";
  }
}

function withInstrumentStatus(message) {
  if (instrumentStatus) instrumentStatus.textContent = message;
  if (message) window.setTimeout(() => { if (instrumentStatus) instrumentStatus.textContent = ""; }, 3000);
}

// ── Prácticas ─────────────────────────────────────────────────────────────────

function renderPracticesPage() {
  renderPracticeDirectory();
  if (!practiceWorkspace) return;

  const practice = state.practices.find((p) => p.id === state.activePracticeId);
  if (!practice) {
    practiceWorkspace.innerHTML = "";
    practiceWorkspace.classList.add("hidden");
    practiceCatalog?.closest(".panel")?.classList.remove("hidden");
    return;
  }

  const def = state.practiceDefinition;
  practiceWorkspace.innerHTML = `
    <div class="workspace-head">
      <div>
        <button type="button" class="back-link" id="practice-workspace-back">Volver al listado</button>
        <h3>${escapeHtml(practice.name)}</h3>
        <p class="submission-meta">${escapeHtml(practice.description)}</p>
      </div>
      <div class="metrics compact-metrics">
        <div class="metric">
          <div class="metric-label">Tipo</div>
          <div class="metric-value metric-text">${escapeHtml(analysisKindLabel(def?.analysis_kind))}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Magnitudes</div>
          <div class="metric-value">${def?.quantities?.length ?? 0}</div>
        </div>
        <div class="metric">
          <div class="metric-label">Mensurandos</div>
          <div class="metric-value">${def?.results?.length ?? 0}</div>
        </div>
      </div>
    </div>

    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Tipo de análisis</h3>
        ${renderAnalysisKindForm(practice, def)}
      </section>
      <section class="panel workspace-panel">
        <h3>Nueva magnitud</h3>
        ${renderQuantityForm(null, practice.id)}
      </section>
    </div>

    <section class="panel workspace-panel">
      <div class="list-head">
        <h3>Magnitudes de entrada</h3>
        <span class="submission-meta">${escapeHtml(state.practiceActionStatus)}</span>
      </div>
      ${renderQuantitiesList(def, practice.id)}
    </section>

    <div class="workspace-grid">
      <section class="panel workspace-panel">
        <h3>Nuevo mensurando</h3>
        ${renderResultForm(null, practice.id)}
      </section>
      <section class="panel workspace-panel">
        <h3>Mensurandos derivados</h3>
        ${renderResultsList(def, practice.id)}
      </section>
    </div>
  `;

  practiceWorkspace.classList.remove("hidden");
  practiceCatalog?.closest(".panel")?.classList.add("hidden");

  practiceWorkspace.querySelector("#practice-workspace-back")?.addEventListener("click", closePracticeWorkspace);
  practiceWorkspace.querySelector("#practice-kind-form")?.addEventListener("submit", savePracticeKind);
  practiceWorkspace.querySelector("#new-quantity-form")?.addEventListener("submit", saveNewQuantity);
  practiceWorkspace.querySelector("#new-result-form")?.addEventListener("submit", saveNewResult);

  practiceWorkspace.querySelectorAll("[data-edit-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingQuantityId = state.editingQuantityId === btn.dataset.qid ? null : btn.dataset.qid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeQuantity(btn.dataset.qid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-quantity]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingQuantityId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-quantity-form]").forEach((form) => {
    form.addEventListener("submit", saveEditQuantity);
  });
  practiceWorkspace.querySelectorAll("[data-edit-result]").forEach((btn) => {
    btn.addEventListener("click", () => {
      state.editingResultId = state.editingResultId === btn.dataset.rid ? null : btn.dataset.rid;
      renderPracticesPage();
    });
  });
  practiceWorkspace.querySelectorAll("[data-delete-result]").forEach((btn) => {
    btn.addEventListener("click", () => deletePracticeResult(btn.dataset.rid, practice.id));
  });
  practiceWorkspace.querySelectorAll("[data-cancel-result]").forEach((btn) => {
    btn.addEventListener("click", () => { state.editingResultId = null; renderPracticesPage(); });
  });
  practiceWorkspace.querySelectorAll("[data-edit-result-form]").forEach((form) => {
    form.addEventListener("submit", saveEditResult);
  });
}

function renderPracticeDirectory() {
  if (!practiceCatalog) return;

  const rows = state.practices.map((p) => `
    <tr>
      <td class="directory-primary"><strong>${escapeHtml(p.name)}</strong></td>
      <td><span class="status-chip">${escapeHtml(analysisKindLabel(p.analysis_kind))}</span></td>
      <td>${escapeHtml(p.description)}</td>
      <td class="directory-actions">
        <button type="button" data-practice-open data-practice-id="${escapeHtml(p.id)}">Editar</button>
      </td>
    </tr>
  `);

  practiceCatalog.innerHTML = rows.length
    ? `
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
          <thead>
            <tr>
              <th>Práctica</th>
              <th>Tipo de análisis</th>
              <th>Descripción</th>
              <th>Acciones</th>
            </tr>
          </thead>
          <tbody>${rows.join("")}</tbody>
        </table>
      </div>
    `
    : `<p class="submission-meta">No hay prácticas definidas.</p>`;

  practiceCatalog.querySelectorAll("[data-practice-open]").forEach((btn) => {
    btn.addEventListener("click", () => openPracticeWorkspace(btn.dataset.practiceId));
  });
}

function renderAnalysisKindForm(practice, def) {
  const current = def?.analysis_kind ?? "";
  return `
    <form id="practice-kind-form" class="detail-form detail-form-grid">
      <input name="practice_id" type="hidden" value="${escapeHtml(practice.id)}" />
      <label>Tipo de análisis
        <select name="analysis_kind" required>
          ${["estadistico", "regresion_lineal", "relajacion_exponencial"].map((k) =>
            `<option value="${k}" ${k === current ? "selected" : ""}>${escapeHtml(analysisKindLabel(k))}</option>`
          ).join("")}
        </select>
      </label>
      <div class="detail-actions">
        <button type="submit">Guardar</button>
      </div>
    </form>
  `;
}

function renderQuantityForm(qty, practiceId) {
  const v = (f) => qty ? escapeHtml(String(qty[f] ?? "")) : "";
  const formId = qty ? "edit-quantity-form" : "new-quantity-form";
  const formAttr = qty ? `data-edit-quantity-form data-qid="${escapeHtml(qty.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${qty ? `<input name="qid" type="hidden" value="${escapeHtml(qty.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="l" /></label>
      <label>Nombre <input name="name" value="${v("name")}" required placeholder="Longitud del cordón" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" required placeholder="mm" /></label>
      <label>Magnitud física <input name="quantity" value="${v("quantity")}" placeholder="longitud" /></label>
      <label class="detail-form-checkbox">
        <input type="checkbox" name="repeated" ${qty ? (qty.repeated ? "checked" : "") : "checked"} />
        Admite réplicas (tipo A)
      </label>
      <div class="detail-actions">
        <button type="submit">${qty ? "Guardar" : "Agregar"}</button>
        ${qty ? `<button type="button" data-cancel-quantity>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderQuantitiesList(def, practiceId) {
  const quantities = def?.quantities ?? [];
  if (quantities.length === 0) return `<p class="submission-meta">Sin magnitudes. Agrega una desde el panel lateral.</p>`;

  const rows = quantities.flatMap((q) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(q.symbol)}</strong></td>
        <td>${escapeHtml(q.name)}</td>
        <td>${escapeHtml(q.unit)}</td>
        <td>${q.quantity ? escapeHtml(q.quantity) : "-"}</td>
        <td>${q.repeated ? "Sí" : "No"}</td>
        <td class="directory-actions">
          <button type="button" data-edit-quantity data-qid="${escapeHtml(q.id)}">${state.editingQuantityId === q.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-quantity data-qid="${escapeHtml(q.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingQuantityId === q.id
      ? `<tr><td colspan="6" class="scale-edit-cell">${renderQuantityForm(q, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Magnitud</th><th>Réplicas</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

function renderResultForm(res, practiceId) {
  const v = (f) => res ? escapeHtml(String(res[f] ?? "")) : "";
  const formId = res ? "edit-result-form" : "new-result-form";
  const formAttr = res ? `data-edit-result-form data-rid="${escapeHtml(res.id)}"` : "";
  return `
    <form id="${formId}" class="detail-form detail-form-grid" ${formAttr}>
      <input name="practice_id" type="hidden" value="${escapeHtml(practiceId)}" />
      ${res ? `<input name="rid" type="hidden" value="${escapeHtml(res.id)}" />` : ""}
      <label>Símbolo <input name="symbol" value="${v("symbol")}" required placeholder="Q" /></label>
      <label>Nombre <input name="name" value="${v("name")}" required placeholder="Área transversal" /></label>
      <label>Unidad <input name="unit" value="${v("unit")}" required placeholder="mm2" /></label>
      <label>Fórmula <input name="formula" value="${v("formula")}" required placeholder="l*a + l*b" /></label>
      <div class="detail-actions">
        <button type="submit">${res ? "Guardar" : "Agregar"}</button>
        ${res ? `<button type="button" data-cancel-result>Cancelar</button>` : ""}
      </div>
    </form>
  `;
}

function renderResultsList(def, practiceId) {
  const results = def?.results ?? [];
  if (results.length === 0) return `<p class="submission-meta">Sin mensurandos. Agrega uno desde el panel lateral.</p>`;

  const rows = results.flatMap((r) => {
    const baseRow = `
      <tr>
        <td class="directory-primary"><strong>${escapeHtml(r.symbol)}</strong></td>
        <td>${escapeHtml(r.name)}</td>
        <td>${escapeHtml(r.unit)}</td>
        <td><code>${escapeHtml(r.formula)}</code></td>
        <td class="directory-actions">
          <button type="button" data-edit-result data-rid="${escapeHtml(r.id)}">${state.editingResultId === r.id ? "Cerrar" : "Editar"}</button>
          <button type="button" data-delete-result data-rid="${escapeHtml(r.id)}">Eliminar</button>
        </td>
      </tr>`;
    const editRow = state.editingResultId === r.id
      ? `<tr><td colspan="5" class="scale-edit-cell">${renderResultForm(r, practiceId)}</td></tr>`
      : "";
    return [baseRow, editRow];
  });

  return `
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
        <thead>
          <tr><th>Símbolo</th><th>Nombre</th><th>Unidad</th><th>Fórmula</th><th>Acciones</th></tr>
        </thead>
        <tbody>${rows.join("")}</tbody>
      </table>
    </div>
  `;
}

async function openPracticeWorkspace(practiceId) {
  state.activePracticeId = practiceId;
  state.practiceActionStatus = "";
  state.editingQuantityId = null;
  state.editingResultId = null;
  state.practiceDefinition = null;
  renderPracticesPage();
  selectView("practices");
  try {
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    renderPracticesPage();
  } catch (error) {
    withPracticeStatus(error.message);
  }
}

function closePracticeWorkspace() {
  state.activePracticeId = null;
  state.practiceDefinition = null;
  state.practiceActionStatus = "";
  state.editingQuantityId = null;
  state.editingResultId = null;
  renderPracticesPage();
}

async function savePracticeKind(event) {
  event.preventDefault();
  const payload = Object.fromEntries(new FormData(event.currentTarget).entries());
  try {
    await postJson(`/api/practices/${payload.practice_id}/analysis-kind`, {
      analysis_kind: payload.analysis_kind,
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${payload.practice_id}/definition`);
    // Refrescar state.practices para que el directorio muestre el nuevo analysis_kind.
    state.practices = await fetchJson("/api/practices");
    state.practiceActionStatus = "Tipo de análisis guardado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

function quantityPayloadFromForm(form) {
  const raw = Object.fromEntries(new FormData(form).entries());
  return {
    symbol: raw.symbol,
    name: raw.name,
    unit: raw.unit,
    quantity: raw.quantity || null,
    repeated: "repeated" in raw,
  };
}

async function saveNewQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/quantities`, quantityPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingQuantityId = null;
    state.practiceActionStatus = "Magnitud agregada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditQuantity(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const qid = form.querySelector('[name="qid"]').value;
  try {
    await postJson(`/api/practices/${practiceId}/quantities/${qid}`, quantityPayloadFromForm(form));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingQuantityId = null;
    state.practiceActionStatus = "Magnitud actualizada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeQuantity(qid, practiceId) {
  if (!window.confirm("¿Eliminar esta magnitud? Esta accion no se puede deshacer.")) return;
  try {
    const response = await fetch(`/api/practices/${practiceId}/quantities/${qid}`, { method: "DELETE" });
    if (!response.ok) throw new Error(await errorText(response));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingQuantityId = null;
    state.practiceActionStatus = "Magnitud eliminada";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveNewResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const raw = Object.fromEntries(new FormData(form).entries());
  try {
    await postJson(`/api/practices/${practiceId}/results`, {
      symbol: raw.symbol,
      name: raw.name,
      unit: raw.unit,
      formula: raw.formula,
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingResultId = null;
    state.practiceActionStatus = "Mensurando agregado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function saveEditResult(event) {
  event.preventDefault();
  const form = event.currentTarget;
  const practiceId = form.querySelector('[name="practice_id"]').value;
  const rid = form.querySelector('[name="rid"]').value;
  const raw = Object.fromEntries(new FormData(form).entries());
  try {
    await postJson(`/api/practices/${practiceId}/results/${rid}`, {
      symbol: raw.symbol,
      name: raw.name,
      unit: raw.unit,
      formula: raw.formula,
    });
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingResultId = null;
    state.practiceActionStatus = "Mensurando actualizado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

async function deletePracticeResult(rid, practiceId) {
  if (!window.confirm("¿Eliminar este mensurando? Esta accion no se puede deshacer.")) return;
  try {
    const response = await fetch(`/api/practices/${practiceId}/results/${rid}`, { method: "DELETE" });
    if (!response.ok) throw new Error(await errorText(response));
    state.practiceDefinition = await fetchJson(`/api/practices/${practiceId}/definition`);
    state.editingResultId = null;
    state.practiceActionStatus = "Mensurando eliminado";
    renderPracticesPage();
  } catch (error) {
    state.practiceActionStatus = error.message;
    renderPracticesPage();
  }
}

function withPracticeStatus(message) {
  if (practiceStatus) practiceStatus.textContent = message;
  if (message) window.setTimeout(() => { if (practiceStatus) practiceStatus.textContent = ""; }, 3000);
}

init();
