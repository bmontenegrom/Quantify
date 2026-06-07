import { state } from "./state.js";
import { sidebar, navToggle, practiceNavChildren, courseSelect, practiceSelect } from "./dom.js";
import { escapeHtml, canReview } from "./lib.js";
import { PRACTICE_GROUPS } from "./constants.js";
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
  if (view === "submissions") import("./submissions.js").then(({ loadSubmissions }) => loadSubmissions());
  if (view === "gradebook") loadGrades().then(renderGradebookAdmin);
  if (view === "students") {
    loadGrades().then(renderStudentsPage);
    renderStudentsPage();
  }
  if (view === "groups") renderGroupsPage();
  if (view === "courses") renderCoursesPage();
  if (view === "instruments") import("./instruments.js").then(({ renderInstrumentsPage, refreshInstruments }) => { renderInstrumentsPage(); refreshInstruments(); });
  if (view === "practices") import("./practices-admin.js").then(({ renderPracticesPage }) => renderPracticesPage());
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
  const all = [...seen.values()];
  const shownGroups = new Set();
  const items = [];
  for (const practice of all) {
    const group = PRACTICE_GROUPS[practice.id]?.group;
    if (group) {
      if (shownGroups.has(group)) continue;
      shownGroups.add(group);
      const rep = all
        .filter((p) => PRACTICE_GROUPS[p.id]?.group === group)
        .sort((a, b) => PRACTICE_GROUPS[a.id].order - PRACTICE_GROUPS[b.id].order)[0];
      items.push(rep);
    } else {
      items.push(practice);
    }
  }

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
      import("./forms.js").then(({ exitEditMode }) => exitEditMode());
      selectPracticeFromNav(btn.dataset.practiceId);
    });
  });
}

export function selectPracticeFromNav(practiceId) {
  const course = state.academic?.courses.find((c) =>
    (c.practices ?? []).some((p) => p.id === practiceId)
  );
  if (course && course.id !== courseSelect.value) {
    courseSelect.value = course.id;
    import("./forms.js").then(({ updateStudentSelectors }) => updateStudentSelectors());
  }
  practiceSelect.value = practiceId;
  practiceSelect.dispatchEvent(new Event("change", { bubbles: true }));
  selectView("practica");
}

export function highlightPracticeNav() {
  if (!practiceNavChildren) return;
  const current = practiceSelect.value;
  const currentGroup = PRACTICE_GROUPS[current]?.group;
  practiceNavChildren.querySelectorAll(".nav-child").forEach((btn) => {
    const id = btn.dataset.practiceId;
    const match = currentGroup ? PRACTICE_GROUPS[id]?.group === currentGroup : id === current;
    btn.classList.toggle("active", match);
  });
}

// --- Listeners top-level ---

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    closeSidebarOnMobile();
    import("./forms.js").then(({ exitEditMode }) => exitEditMode());
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
      import("./instruments.js").then(({ closeInstrumentWorkspace }) => closeInstrumentWorkspace());
      selectView("instruments");
      return;
    }
    if (tab.dataset.view === "practices" && state.activePracticeId) {
      import("./practices-admin.js").then(({ closePracticeWorkspace }) => closePracticeWorkspace());
      selectView("practices");
      return;
    }
    if (tab.dataset.view === "submissions" && state.activeSubmissionId) {
      import("./submissions.js").then(({ closeSubmissionWorkspace }) => closeSubmissionWorkspace());
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
