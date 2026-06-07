import { state } from "./state.js";
import { fetchJson, postJson } from "./api.js";
import { escapeHtml } from "./lib.js";
import { invitationBanner } from "./dom.js";
import { loadSubmissions } from "./submissions.js";

/** Carga las invitaciones vigentes del alumno autenticado y las renderiza. */
export async function loadInvitations() {
  if (!invitationBanner) return;
  if (state.user?.role !== "estudiante") {
    invitationBanner.classList.add("hidden");
    return;
  }
  try {
    state.invitations = await fetchJson("/api/submissions/invitations");
    renderInvitations();
  } catch {
    // no bloquear si falla
  }
}

/** Renderiza el banner con una tarjeta por invitación vigente. */
export function renderInvitations() {
  if (!invitationBanner) return;
  const inv = state.invitations ?? [];
  if (inv.length === 0) {
    invitationBanner.classList.add("hidden");
    invitationBanner.innerHTML = "";
    return;
  }
  const titulo =
    inv.length === 1 ? "Tenés una invitación a un informe compartido" : `Tenés ${inv.length} invitaciones a informes compartidos`;
  invitationBanner.classList.remove("hidden");
  invitationBanner.innerHTML = `
    <p class="invitation-banner-title">${titulo}</p>
    ${inv.map(cardHtml).join("")}
  `;
  invitationBanner.querySelectorAll(".accept-invitation-btn").forEach((btn) => {
    btn.addEventListener("click", () => acceptInvitation(btn.dataset.id));
  });
}

function cardHtml(inv) {
  const expires = new Date(inv.expires_at);
  const remainingMs = expires.getTime() - Date.now();
  const mins = Math.max(0, Math.floor(remainingMs / 60000));
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  const left = h > 0 ? `${h} h ${m} min` : `${m} min`;
  const tableLabel = inv.table_number != null ? ` · Mesa ${inv.table_number}` : "";
  return `
    <div class="edit-banner">
      <div>
        <strong>${escapeHtml(inv.practice_name)}</strong> —
        ${escapeHtml(inv.course)} · ${escapeHtml(inv.group_name)}${tableLabel}<br />
        <span class="submission-meta">Invitado por ${escapeHtml(inv.owner_name)} · vence en ${left}</span>
      </div>
      <button type="button" class="accept-invitation-btn" data-id="${escapeHtml(inv.submission_id)}">
        Aceptar
      </button>
    </div>
  `;
}

/** Acepta la invitación al informe `submissionId` y recarga lista e invitaciones. */
export async function acceptInvitation(submissionId) {
  try {
    await postJson(`/api/submissions/${encodeURIComponent(submissionId)}/accept`, {});
    await Promise.all([loadInvitations(), loadSubmissions()]);
  } catch (error) {
    // mostrar el mensaje amigable del servidor directamente
    const banner = document.createElement("p");
    banner.className = "submission-meta";
    banner.textContent = error.message;
    invitationBanner?.prepend(banner);
    setTimeout(() => banner.remove(), 6000);
  }
}
