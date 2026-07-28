import { state } from "./state.js";
import { postJson } from "./api.js";
import { escapeHtml, formatDate, allStudents } from "./lib.js";
import { openSubmissionWorkspace } from "./submissions.js";

export function membersEditorMarkup(submission) {
  const members = submission.members ?? [];
  if (!members.length) return "";
  const students = allStudents(state.academic);
  const memberIds = new Set(members.map((m) => m.user_id));
  const available = students.filter((s) => !memberIds.has(s.id));
  const rows = members
    .map(
      (m) => `
      <tr>
        <td class="directory-primary">${escapeHtml(m.display_name)}</td>
        <td>${m.role === "owner" ? "★ owner" : "miembro"}</td>
        <td><span class="status ${escapeHtml(m.status)}">${escapeHtml(m.status)}</span></td>
        <td class="submission-meta">${m.accepted_at ? escapeHtml(formatDate(m.accepted_at)) : "—"}</td>
        <td><button type="button" class="remove-member-btn" data-user-id="${escapeHtml(m.user_id)}">Quitar</button></td>
      </tr>`,
    )
    .join("");
  const addOptions = available.length
    ? available.map((s) => `<option value="${escapeHtml(s.id)}">${escapeHtml(s.display_name)}</option>`).join("")
    : `<option value="" disabled>Sin alumnos disponibles</option>`;
  return `
    <section class="panel members-editor">
      <h4>Integrantes del informe</h4>
      <div class="data-table-wrap">
        <table class="data-table">
          <thead><tr><th>Nombre</th><th>Rol</th><th>Estado</th><th>Aceptado</th><th></th></tr></thead>
          <tbody>${rows}</tbody>
        </table>
      </div>
      <div class="members-add-row">
        <select class="add-member-select">
          <option value="">— Agregar alumno —</option>
          ${addOptions}
        </select>
        <button type="button" class="add-member-btn">Agregar</button>
        <span class="members-status submission-meta"></span>
      </div>
    </section>
  `;
}

export function wireMembersEditor(target, submissionId) {
  const editor = target.querySelector(".members-editor");
  if (!editor) return;
  const statusEl = editor.querySelector(".members-status");
  const setStatus = (msg) => { if (statusEl) statusEl.textContent = msg; };

  editor.querySelectorAll(".remove-member-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      btn.disabled = true;
      setStatus("Quitando...");
      try {
        await postJson(`/api/submissions/${submissionId}/members/remove`, { user_id: btn.dataset.userId });
        await openSubmissionWorkspace(submissionId);
      } catch (error) {
        setStatus(error.message);
        btn.disabled = false;
      }
    });
  });

  editor.querySelector(".add-member-btn")?.addEventListener("click", async () => {
    const select = editor.querySelector(".add-member-select");
    const userId = select?.value;
    if (!userId) { setStatus("Seleccioná un alumno."); return; }
    setStatus("Agregando...");
    try {
      await postJson(`/api/submissions/${submissionId}/members`, { user_id: userId, force_accept: true });
      await openSubmissionWorkspace(submissionId);
    } catch (error) {
      setStatus(error.message);
    }
  });
}
