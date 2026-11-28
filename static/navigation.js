import { showError } from "./toast.js";
import { state } from "./state.js";
import { sidebar, navToggle, practiceNavChildren, courseSelect, practiceSelect } from "./dom.js";
import { escapeHtml, canReview } from "./lib.js";
import { renderAccount } from "./auth.js";
import { loadGrades, renderGradebookAdmin } from "./gradebook.js";
import { renderStudentsPage } from "./students.js";
import { renderGroupsPage } from "./groups.js";
import { renderCoursesPage } from "./courses.js";
import { closeStudentWorkspace } from "./students.js";
import { closeGroupWorkspace } from "./groups.js";
import { closeCourseWorkspace } from "./courses.js";

export function selectView(view) {
  document.querySelectorAll(".tab").forEach((item) => item.classList.toggle("active", item.dataset.view === view));
  document.querySelectorAll(".view").forEach((item) => item.classList.remove("active"));
  document.querySelector(`#${view}-view`).classList.add("active");
  if (view === "submissions") import("./submissions.js").then(({ loadSubmissions }) => loadSubmissions()).catch(showError);
  if (view === "gradebook") loadGrades().then(renderGradebookAdmin).catch(showError);
  if (view === "students") {
    loadGrades().then(renderStudentsPage).catch(showError);
    renderStudentsPage();
  }
  if (view === "groups") renderGroupsPage();
  if (view === "courses") renderCoursesPage();
  if (view === "instruments") import("./instruments.js").then(({ renderInstrumentsPage, refreshInstruments }) => { renderInstrumentsPage(); return refreshInstruments(); }).catch(showError);
  if (view === "practices") import("./practices-admin.js").then(({ renderPracticesPage }) => renderPracticesPage()).catch(showError);
  if (view === "practica") highlightPracticeNav();
  if (view === "account") renderAccount();
}

export function closeSidebarOnMobile() {
  sidebar?.classList.remove("sidebar-open");
  navToggle?.setAttribute("aria-expanded", "false");
}

export function renderPracticeNav() {
  if (!practiceNavChildren) return;
  if (canReview(state.user)) {
    practiceNavChildren.innerHTML = "";
    return;
  }
  const seen = new Map();
  for (const course of state.academic?.courses ?? []) {
    for (const practice of course.practices ?? []) {
      if (!seen.has(practice.id)) seen.set(practice.id, practice);
    }
  }
  const items = [...seen.values()];

  practiceNavChildren.innerHTML = items.length
    ? items
        .map(
          (p) =>
            `<button class="tab nav-child" data-view="practica" data-practice-id="${escapeHtml(p.id)}">${escapeHtml(p.name)}</button>`
        )
        .join("")
    : `<p class="nav-empty submission-meta">Sin practicas habilitadas</p>`;

  practiceNavChildren.querySelectorAll(".nav-child").forEach((btn) => {
    btn.addEventListener("click", () => {
      closeSidebarOnMobile();
      import("./forms.js").then(({ exitEditMode }) => exitEditMode()).catch(showError);
      selectPracticeFromNav(btn.dataset.practiceId).catch(showError);
    });
  });
}

export async function selectPracticeFromNav(practiceId, courseId = null) {
  const withPractice = (state.academic?.courses ?? []).filter((c) =>
    (c.practices ?? []).some((p) => p.id === practiceId)
  );
  const course =
    withPractice.find((c) => c.id === courseId) ??
    withPractice.find((c) => c.id === courseSelect.value) ??
    withPractice[0];
  if (course && course.id !== courseSelect.value) {
    courseSelect.value = course.id;
    const { updateStudentSelectors } = await import("./forms.js");
    updateStudentSelectors({ autoLoad: false });
  }
  practiceSelect.value = practiceId;
  practiceSelect.dispatchEvent(new Event("change", { bubbles: true }));
  selectView("practica");
}

export function highlightPracticeNav() {
  if (!practiceNavChildren) return;
  const current = practiceSelect.value;
  practiceNavChildren.querySelectorAll(".nav-child").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.practiceId === current);
  });
}

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    closeSidebarOnMobile();
    import("./forms.js").then(({ exitEditMode }) => exitEditMode()).catch(showError);
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
      import("./instruments.js").then(({ closeInstrumentWorkspace }) => closeInstrumentWorkspace()).catch(showError);
      selectView("instruments");
      return;
    }
    if (tab.dataset.view === "practices" && state.activePracticeId) {
      const onPractices = document.querySelector("#practices-view").classList.contains("active");
      import("./practices-admin.js").then(({ resetPracticeWorkspace, confirmDiscardForms }) => {
        if (onPractices && confirmDiscardForms()) resetPracticeWorkspace();
        selectView("practices");
      }).catch(showError);
      return;
    }
    if (tab.dataset.view === "submissions" && state.activeSubmissionId) {
      import("./submissions.js").then(({ closeSubmissionWorkspace }) => closeSubmissionWorkspace()).catch(showError);
      selectView("submissions");
      return;
    }
    selectView(tab.dataset.view);
  });
});

navToggle?.addEventListener("click", () => {
  const open = sidebar?.classList.toggle("sidebar-open");
  navToggle.setAttribute("aria-expanded", String(!!open));
});
