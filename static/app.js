const state = {
  user: null,
  practices: [],
  submissions: [],
  selectedId: null,
};

const loginScreen = document.querySelector("#login-screen");
const appShell = document.querySelector("#app-shell");
const loginForm = document.querySelector("#login-form");
const loginStatus = document.querySelector("#login-status");
const sessionUser = document.querySelector("#session-user");
const logoutButton = document.querySelector("#logout-button");
const practiceSelect = document.querySelector("#practice-select");
const submissionForm = document.querySelector("#submission-form");
const submitStatus = document.querySelector("#submit-status");
const latestResult = document.querySelector("#latest-result");
const submissionList = document.querySelector("#submission-list");
const submissionDetail = document.querySelector("#submission-detail");

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

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((item) => item.classList.remove("active"));
    document.querySelectorAll(".view").forEach((item) => item.classList.remove("active"));
    tab.classList.add("active");
    document.querySelector(`#${tab.dataset.view}-view`).classList.add("active");
    if (tab.dataset.view === "teacher") loadSubmissions();
  });
});

document.querySelector("#refresh-submissions").addEventListener("click", loadSubmissions);

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

  selectView("student");
  state.practices = await fetchJson("/api/practices");
  practiceSelect.innerHTML = state.practices
    .map((practice) => `<option value="${escapeHtml(practice.id)}">${escapeHtml(practice.name)}</option>`)
    .join("");

  if (canReview()) await loadSubmissions();
}

function selectView(view) {
  document.querySelectorAll(".tab").forEach((item) => item.classList.toggle("active", item.dataset.view === view));
  document.querySelectorAll(".view").forEach((item) => item.classList.remove("active"));
  document.querySelector(`#${view}-view`).classList.add("active");
}

async function loadSubmissions() {
  if (!canReview()) return;
  state.submissions = await fetchJson("/api/submissions");
  renderSubmissionList();
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
