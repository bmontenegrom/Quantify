import { state } from "./state.js";
import {
  gradeCourseSelect, gradebookCourseFilter, teacherGradebook,
  gradeStatus, gradeComponentForm,
} from "./dom.js";
import { fetchJson, postJson } from "./api.js";
import { escapeHtml, format, canReview } from "./lib.js";

export async function loadGrades() {
  state.gradebooks = await fetchJson("/api/grades");
  if (canReview(state.user)) renderGradeCourseOptions();
}

export function renderGradeCourseOptions() {
  const options = state.academic.courses
    .map((course) => `<option value="${escapeHtml(course.id)}">${escapeHtml(course.name)} (${escapeHtml(course.term)})</option>`)
    .join("");
  gradeCourseSelect.innerHTML = options;
  gradebookCourseFilter.innerHTML = options;
}

export function renderKindTotals(summary) {
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

export function renderStudentGradeTable(summary) {
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

export function renderGradebookAdmin() {
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
      .querySelector(`[data-student-id="${CSS.escape(state.highlightedStudentId)}"]`)
      ?.scrollIntoView({ block: "center", behavior: "smooth" });
  }
}

export function renderStudentGradeCard(summary, components) {
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

export function renderGradeField(summary, component) {
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

export async function saveGradeInput(input) {
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
    // Importar lazily para evitar ciclo de inicialización
    const { renderStudentsPage } = await import("./students.js");
    if (state.activeStudentId) renderStudentsPage();
    gradeStatus.textContent = "Nota guardada";
  });
}

export async function withGradeError(action) {
  try {
    gradeStatus.textContent = "";
    await action();
  } catch (error) {
    gradeStatus.textContent = error.message;
  }
}

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

gradebookCourseFilter.addEventListener("change", renderGradebookAdmin);
