import { state } from "./state.js";
import {
  submissionList, submissionWorkspace, submissionsTitle,
  submissionsSubtitle, submissionsListTitle,
} from "./dom.js";
import { fetchJson, errorText } from "./api.js";
import { escapeHtml, canReview, formatDate, groupBy } from "./lib.js";

export async function loadSubmissions() {
  state.submissions = await fetchJson("/api/submissions");
  renderSubmissionsPage();
}

export function renderSubmissionsPage() {
  const teacher = canReview(state.user);
  submissionsTitle.textContent = teacher ? "Entregas" : "Mis entregas";
  submissionsSubtitle.textContent = teacher
    ? "Todas las entregas organizadas por curso y grupo."
    : "Tus entregas y el estado de correccion.";
  submissionsListTitle.textContent = teacher ? "Entregas por curso y grupo" : "Mis entregas";
  renderSubmissionList();
}

export function renderSubmissionList() {
  const hasList = state.submissions.length > 0;
  const catalogPanel = submissionList.closest(".catalog-panel");

  if (state.activeSubmissionId) {
    catalogPanel?.classList.add("hidden");
    submissionWorkspace.classList.remove("hidden");
    return;
  }

  submissionWorkspace.classList.add("hidden");
  catalogPanel?.classList.remove("hidden");

  if (!hasList) {
    submissionList.innerHTML = `<p class="submission-meta">Todavia no hay entregas.</p>`;
    return;
  }

  submissionList.innerHTML = canReview(state.user)
    ? renderTeacherSubmissionGroups()
    : renderStudentSubmissionRows();

  submissionList.querySelectorAll(".submission-item").forEach((item) => {
    item.addEventListener("click", () => openSubmissionWorkspace(item.dataset.id));
  });
}

function renderStudentSubmissionRows() {
  return state.submissions
    .map(
      (item) => `
        <article class="submission-item ${item.id === state.activeSubmissionId ? "active" : ""}" data-id="${escapeHtml(item.id)}">
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
                        <article class="submission-item ${item.id === state.activeSubmissionId ? "active" : ""}" data-id="${escapeHtml(item.id)}">
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

export async function openSubmissionWorkspace(id) {
  state.activeSubmissionId = id;
  renderSubmissionsPage();
  submissionWorkspace.innerHTML = `<p class="submission-meta">Cargando...</p>`;

  const submission = await fetchJson(`/api/submissions/${id}`);
  state.activeSubmission = submission;

  let definition = null;
  if (submission.entry_mode === "form" && !canReview(state.user)) {
    try {
      definition = await fetchJson(`/api/practices/${encodeURIComponent(submission.practice_id)}/definition`);
    } catch {
      definition = null;
    }
  }

  submissionWorkspace.innerHTML = `
    <button type="button" class="back-link" id="submission-workspace-back">Volver al listado</button>
    <div id="submission-detail-body"></div>
  `;
  submissionWorkspace.querySelector("#submission-workspace-back").addEventListener("click", closeSubmissionWorkspace);
  const detailBody = submissionWorkspace.querySelector("#submission-detail-body");
  const { renderAnalysis } = await import("./analysis.js");
  renderAnalysis(detailBody, submission, canReview(state.user), definition);
}

export function closeSubmissionWorkspace() {
  state.activeSubmissionId = null;
  state.activeSubmission = null;
  renderSubmissionsPage();
}

export function submissionHeader(submission) {
  return `
    <div>
      <h3>${escapeHtml(submission.practice_name)}</h3>
      <p class="submission-meta">
        ${escapeHtml(submission.student_name)} - Grupo ${escapeHtml(submission.group_name)} - ${escapeHtml(submission.course)}
      </p>
      <span class="status ${escapeHtml(submission.status)}">${escapeHtml(submission.status)}</span>
    </div>`;
}

export function teacherCommentMarkup(submission) {
  const comment = (submission.teacher_comment ?? "").trim();
  if (!comment) return "";
  const score = submission.score != null
    ? ` <span class="teacher-comment-score">Nota: ${escapeHtml(String(submission.score))}</span>`
    : "";
  return `
    <div class="teacher-comment">
      <div class="teacher-comment-head">Comentario del docente${score}</div>
      <p class="teacher-comment-body">${escapeHtml(comment)}</p>
    </div>`;
}

export function editBannerMarkup(submission) {
  if (!submission.can_edit || !submission.editable_until) return "";
  const until = new Date(submission.editable_until);
  const remainingMs = until.getTime() - Date.now();
  if (remainingMs <= 0) return "";
  const mins = Math.floor(remainingMs / 60000);
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  const left = h > 0 ? `${h} h ${m} min` : `${m} min`;
  return `<div class="edit-banner">
    <div>Podés editar esta entrega hasta el ${escapeHtml(formatDate(submission.editable_until))} — te quedan ${left}.</div>
    <button type="button" class="edit-submission-btn">Editar entrega</button>
  </div>`;
}

export function renderReviewForm(submission) {
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
      <label class="review-visibility">
        <input type="checkbox" name="results_visible" ${submission.results_visible_to_student ? "checked" : ""} />
        Mostrar el calculo automatico al estudiante
      </label>
      <div class="review-actions">
        <button type="submit">Guardar correccion</button>
        <span class="submission-meta">${submission.reviewed_at ? `Revisada: ${new Date(submission.reviewed_at).toLocaleString()}` : ""}</span>
      </div>
    </form>
  `;
}

export async function saveReview(event, id) {
  event.preventDefault();
  const form = event.currentTarget;
  const payload = Object.fromEntries(new FormData(form).entries());
  payload.score = payload.score === "" ? null : Number(payload.score);
  payload.results_visible = form.querySelector('[name="results_visible"]').checked;

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
  state.activeSubmission = updated;
  const detailBody = submissionWorkspace?.querySelector("#submission-detail-body");
  if (detailBody) {
    const { renderAnalysis } = await import("./analysis.js");
    renderAnalysis(detailBody, updated, true);
  }
  await loadSubmissions();
}

document.querySelector("#refresh-submissions")?.addEventListener("click", loadSubmissions);
