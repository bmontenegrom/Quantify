import { state } from "./state.js";
import { adminStatus, userStatus } from "./dom.js";
import { fetchJson } from "./api.js";
import { canReview } from "./lib.js";
import { renderGradeCourseOptions } from "./gradebook.js";
import { renderStudentsPage } from "./students.js";
import { renderGroupsPage } from "./groups.js";
import { renderCoursesPage, renderCourseDirectory } from "./courses.js";
import { renderUsers } from "./users.js";

export async function loadAcademic() {
  state.academic = await fetchJson("/api/academic/context");
  state.practices = state.academic.practices;

  // Importar lazily: estos módulos no existen aún (se crearán pronto)
  const { renderStudentSelectors } = await import("./forms.js");
  const { renderPracticeNav } = await import("./navigation.js");
  const { renderInstrumentCourseOptions } = await import("./instruments.js");

  renderStudentSelectors();
  renderPracticeNav();
  if (canReview(state.user)) {
    renderAdmin();
    renderStudentsPage();
    renderGroupsPage();
    renderCoursesPage();
  }
  if (canReview(state.user)) renderGradeCourseOptions();
  if (canReview(state.user)) renderInstrumentCourseOptions();
}

export async function refreshAcademic(message) {
  await loadAcademic();
  adminStatus.textContent = message;
  userStatus.textContent = message;
  window.setTimeout(() => {
    adminStatus.textContent = "";
    userStatus.textContent = "";
  }, 2500);
}

export async function withAdminError(action) {
  try {
    adminStatus.textContent = "";
    userStatus.textContent = "";
    await action();
  } catch (error) {
    adminStatus.textContent = error.message;
    userStatus.textContent = error.message;
  }
}

export function renderAdmin() {
  renderCourseDirectory(state.academic.courses);
  renderUsers();
}
