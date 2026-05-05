const state = {
  user: null,
  academic: null,
  practices: [],
  submissions: [],
  gradebooks: [],
  selectedId: null,
};

const loginScreen = document.querySelector("#login-screen");
const appShell = document.querySelector("#app-shell");
const loginForm = document.querySelector("#login-form");
const loginStatus = document.querySelector("#login-status");
const sessionUser = document.querySelector("#session-user");
const passwordForm = document.querySelector("#password-form");
const passwordStatus = document.querySelector("#password-status");
const logoutButton = document.querySelector("#logout-button");
const courseSelect = document.querySelector("#course-select");
const groupSelect = document.querySelector("#group-select");
const practiceSelect = document.querySelector("#practice-select");
const submissionForm = document.querySelector("#submission-form");
const submitStatus = document.querySelector("#submit-status");
const latestResult = document.querySelector("#latest-result");
const submissionList = document.querySelector("#submission-list");
const submissionDetail = document.querySelector("#submission-detail");
const userForm = document.querySelector("#user-form");
const courseForm = document.querySelector("#course-form");
const groupForm = document.querySelector("#group-form");
const courseMemberForm = document.querySelector("#course-member-form");
const memberForm = document.querySelector("#member-form");
const coursePracticeForm = document.querySelector("#course-practice-form");
const adminCourseSelect = document.querySelector("#admin-course-select");
const memberCourseSelect = document.querySelector("#member-course-select");
const adminGroupSelect = document.querySelector("#admin-group-select");
const courseMemberSelect = document.querySelector("#course-member-select");
const studentMemberSelect = document.querySelector("#student-member-select");
const practiceCourseSelect = document.querySelector("#practice-course-select");
const adminPracticeSelect = document.querySelector("#admin-practice-select");
const courseCatalog = document.querySelector("#course-catalog");
const userList = document.querySelector("#user-list");
const adminStatus = document.querySelector("#admin-status");
const userStatus = document.querySelector("#user-status");
const gradeComponentForm = document.querySelector("#grade-component-form");
const gradeCourseSelect = document.querySelector("#grade-course-select");
const gradebookCourseFilter = document.querySelector("#gradebook-course-filter");
const teacherGradebook = document.querySelector("#teacher-gradebook");
const studentGrades = document.querySelector("#student-grades");
const gradeStatus = document.querySelector("#grade-status");

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

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    selectView(tab.dataset.view);
  });
});

document.querySelector("#refresh-submissions").addEventListener("click", loadSubmissions);
courseSelect.addEventListener("change", updateStudentSelectors);
gradebookCourseFilter.addEventListener("change", renderGradebookAdmin);

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
    renderStudentGrades();
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

courseForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    await postJson("/api/academic/courses", Object.fromEntries(new FormData(courseForm).entries()));
    courseForm.reset();
    await refreshAcademic("Curso creado");
  });
});

groupForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    const data = Object.fromEntries(new FormData(groupForm).entries());
    await postJson(`/api/academic/courses/${data.course_id}/groups`, { name: data.name });
    groupForm.reset();
    await refreshAcademic("Grupo creado");
  });
});

courseMemberForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    const data = Object.fromEntries(new FormData(courseMemberForm).entries());
    await postJson(`/api/academic/courses/${data.course_id}/members`, { user_id: data.user_id });
    courseMemberForm.reset();
    await refreshAcademic("Estudiante inscrito");
  });
});

memberForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    const data = Object.fromEntries(new FormData(memberForm).entries());
    await postJson(`/api/academic/groups/${data.group_id}/members`, { user_id: data.user_id });
    memberForm.reset();
    await refreshAcademic("Estudiante agregado");
  });
});

coursePracticeForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await withAdminError(async () => {
    const data = Object.fromEntries(new FormData(coursePracticeForm).entries());
    await postJson(`/api/academic/courses/${data.course_id}/practices`, { practice_id: data.practice_id });
    await refreshAcademic("Practica habilitada");
  });
});

submissionForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  submitStatus.textContent = "Subiendo...";

  try {
    const response = await fetch("/api/submissions", {
      method: "POST",
      body: new FormData(submissionForm),
    });

    if (!response.ok) throw new Error(await errorText(response));

    const submission = await response.json();
    submitStatus.textContent = "Entrega guardada";
    submissionForm.reset();
    renderStudentSelectors();
    renderAnalysis(latestResult, submission);
    latestResult.classList.remove("hidden");
    if (canReview()) await loadSubmissions();
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
  sessionUser.textContent = `${state.user.display_name} (${state.user.role})`;

  document.querySelectorAll(".teacher-only").forEach((element) => {
    element.classList.toggle("hidden", !canReview());
  });

  selectView("submissions");
  await loadAcademic();

  if (canReview()) await loadSubmissions();
}

function selectView(view) {
  document.querySelectorAll(".tab").forEach((item) => item.classList.toggle("active", item.dataset.view === view));
  document.querySelectorAll(".view").forEach((item) => item.classList.remove("active"));
  document.querySelector(`#${view}-view`).classList.add("active");
  if (view === "reviews") loadSubmissions();
  if (view === "grades") {
    loadGrades().then(() => {
      renderStudentGrades();
      if (canReview()) renderGradebookAdmin();
    });
  }
  if (view === "gradebook") {
    loadGrades().then(() => {
      renderStudentGrades();
      renderGradebookAdmin();
    });
  }
}

async function loadSubmissions() {
  if (!canReview()) return;
  state.submissions = await fetchJson("/api/submissions");
  renderSubmissionList();
}

async function loadAcademic() {
  state.academic = await fetchJson("/api/academic/context");
  state.practices = state.academic.practices;
  renderStudentSelectors();
  if (canReview()) renderAdmin();
  if (canReview()) renderGradeCourseOptions();
}

async function loadGrades() {
  state.gradebooks = await fetchJson("/api/grades");
  if (canReview()) renderGradeCourseOptions();
}

function renderGradeCourseOptions() {
  const options = state.academic.courses
    .map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`)
    .join("");
  gradeCourseSelect.innerHTML = options;
  gradebookCourseFilter.innerHTML = options;
}

function renderStudentGrades() {
  if (state.gradebooks.length === 0) {
    studentGrades.innerHTML = `<section class="panel detail-empty">No hay cursos con notas disponibles.</section>`;
    return;
  }

  studentGrades.innerHTML = state.gradebooks
    .map((book) => {
      const summary = book.students[0];
      if (!summary) return "";
      return `
        <section class="panel grade-course">
          <div>
            <h3>${escapeHtml(book.course.name)} (${escapeHtml(book.course.term)})</h3>
            <p class="submission-meta">Total: ${format(summary.total_points)} / ${format(summary.total_possible)}</p>
          </div>
          ${renderKindTotals(summary)}
          ${renderStudentGradeTable(summary)}
        </section>
      `;
    })
    .join("");
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

  const selectedId = gradebookCourseFilter.value || state.gradebooks[0].course.id;
  const book = state.gradebooks.find((item) => item.course.id === selectedId) ?? state.gradebooks[0];
  gradebookCourseFilter.value = book.course.id;

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
}

function renderStudentGradeCard(summary, components) {
  return `
    <article class="student-grade-card">
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
}

function renderAdmin() {
  const courses = state.academic.courses;
  const allGroups = courses.flatMap((course) => course.groups.map((group) => ({ ...group, courseName: course.name })));

  const courseOptions = courses
    .map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`)
    .join("");
  adminCourseSelect.innerHTML = courseOptions;
  memberCourseSelect.innerHTML = courseOptions;
  practiceCourseSelect.innerHTML = courseOptions;
  adminGroupSelect.innerHTML = allGroups
    .map((group) => `<option value="${escapeHtml(group.id)}">${escapeHtml(group.courseName)} - ${escapeHtml(group.name)}</option>`)
    .join("");
  courseMemberSelect.innerHTML = state.academic.students
    .map((student) => `<option value="${escapeHtml(student.id)}">${escapeHtml(student.display_name)} (${escapeHtml(student.email)})</option>`)
    .join("");
  studentMemberSelect.innerHTML = state.academic.students
    .map((student) => `<option value="${escapeHtml(student.id)}">${escapeHtml(student.display_name)} (${escapeHtml(student.email)})</option>`)
    .join("");
  adminPracticeSelect.innerHTML = state.practices
    .map((practice) => `<option value="${escapeHtml(practice.id)}">${escapeHtml(practice.name)}</option>`)
    .join("");

  courseCatalog.innerHTML = courses.length
    ? courses.map(renderCourseCard).join("")
    : `<p class="submission-meta">No hay cursos creados.</p>`;
  renderUsers();
}

function renderUsers() {
  userList.innerHTML = state.academic.users
    .map(
      (user) => `
        <article class="user-item" data-user-id="${escapeHtml(user.id)}">
          <div>
            <strong>${escapeHtml(user.display_name)}</strong>
            <div class="submission-meta">${escapeHtml(user.email)} - ${escapeHtml(user.role)}</div>
          </div>
          <form class="user-reset">
            <input name="password" type="password" required minlength="8" placeholder="Nueva contrasena" />
            <button type="submit">Reset</button>
          </form>
        </article>
      `,
    )
    .join("");

  userList.querySelectorAll(".user-reset").forEach((form) => {
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      await withAdminError(async () => {
        const item = form.closest(".user-item");
        const data = Object.fromEntries(new FormData(form).entries());
        await postJson(`/api/users/${item.dataset.userId}/password`, data);
        form.reset();
        await refreshAcademic("Contrasena reseteada");
      });
    });
  });
}

function renderCourseCard(course) {
  const members = course.members
    .map((member) => `<span class="chip">${escapeHtml(member.display_name)}</span>`)
    .join("");
  return `
    <article class="course-card">
      <h4>${escapeHtml(course.name)} (${escapeHtml(course.term)})</h4>
      <div class="submission-meta">${course.members.length} inscritos - ${course.groups.length} grupos - ${course.practices.length} practicas habilitadas</div>
      <div class="chips">
        ${members || `<span class="chip">Sin inscritos</span>`}
      </div>
      <div class="chips">
        ${course.practices.map((practice) => `<span class="chip">${escapeHtml(practice.name)}</span>`).join("") || `<span class="chip">Sin practicas</span>`}
      </div>
      ${course.groups
        .map(
          (group) => `
            <div class="metric">
              <strong>${escapeHtml(group.name)}</strong>
              <div class="submission-meta">${group.members.map((member) => escapeHtml(member.display_name)).join(", ") || "Sin estudiantes"}</div>
            </div>
          `,
        )
        .join("")}
    </article>
  `;
}

function selectedCourse() {
  return state.academic?.courses.find((course) => course.id === courseSelect.value);
}

function renderSubmissionList() {
  if (state.submissions.length === 0) {
    submissionList.innerHTML = `<p class="submission-meta">Todavia no hay entregas.</p>`;
    return;
  }

  submissionList.innerHTML = state.submissions
    .map(
      (item) => `
        <article class="submission-item ${item.id === state.selectedId ? "active" : ""}" data-id="${escapeHtml(item.id)}">
          <strong>${escapeHtml(item.student_name)}</strong>
          <div class="submission-meta">${escapeHtml(item.group_name)} - ${escapeHtml(item.practice_name)}</div>
          <span class="status ${escapeHtml(item.status)}">${escapeHtml(item.status)}</span>
        </article>
      `,
    )
    .join("");

  submissionList.querySelectorAll(".submission-item").forEach((item) => {
    item.addEventListener("click", () => loadSubmissionDetail(item.dataset.id));
  });
}

async function loadSubmissionDetail(id) {
  state.selectedId = id;
  renderSubmissionList();
  const submission = await fetchJson(`/api/submissions/${id}`);
  renderAnalysis(submissionDetail, submission, true);
}

function renderAnalysis(target, submission, includeReview = false) {
  const analysis = submission.analysis;
  const regression = analysis.regression;
  target.classList.remove("detail-empty");
  target.innerHTML = `
    <div>
      <h3>${escapeHtml(submission.practice_name)}</h3>
      <p class="submission-meta">
        ${escapeHtml(submission.student_name)} - ${escapeHtml(submission.group_name)} - ${escapeHtml(submission.course)}
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

function canReview() {
  return state.user && ["docente", "admin"].includes(state.user.role);
}

function format(value) {
  return Number(value).toLocaleString("es-UY", { maximumSignificantDigits: 5 });
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

init();
