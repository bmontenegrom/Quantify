import { state } from "./state.js";
import { loginScreen, appShell } from "./dom.js";
import { fetchJson, setCsrfToken } from "./api.js";
import { canReview } from "./lib.js";
import { renderSessionUser, renderAccount, setupAuth } from "./auth.js";
import { selectView } from "./navigation.js";
import { loadAcademic } from "./academic.js";
import { loadSubmissions } from "./submissions.js";
import { loadInvitations } from "./invitations.js";

// Side-effect imports: register top-level event listeners in each domain module.
import "./gradebook.js";
import "./users.js";
import "./students.js";
import "./groups.js";
import "./courses.js";
import "./analysis.js";
import "./forms.js";
import "./instruments.js";
import "./practices-admin.js";
import "./theme.js";

async function init() {
  try {
    const body = await fetchJson("/api/auth/me");
    state.user = body.user;
    setCsrfToken(body.csrf_token);
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

  document.querySelectorAll(".teacher-only").forEach((el) => {
    el.classList.toggle("hidden", !canReview(state.user));
  });
  document.querySelectorAll(".student-only").forEach((el) => {
    el.classList.toggle("hidden", canReview(state.user));
  });

  selectView("submissions");
  await loadAcademic();
  renderAccount(); // re-renderiza con grupos disponibles
  await loadSubmissions();
  await loadInvitations();
}

setupAuth(startApp);
init();
