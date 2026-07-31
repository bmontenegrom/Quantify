// Notificaciones transitorias para los puntos de feedback mas visibles (login, entregas,
// acciones de admin). Div simple con position:fixed, no Popover API: el contenedor cuelga
// directo de <body> sin ancestro con overflow/transform que lo recorte, asi que el top-layer
// no aporta nada aca y evitamos el polyfill que pide Popover en navegadores viejos.
let region = null;

function getRegion() {
  if (region && document.body.contains(region)) return region;
  region = document.createElement("div");
  region.className = "toast-region";
  region.setAttribute("role", "status");
  region.setAttribute("aria-live", "polite");
  document.body.appendChild(region);
  return region;
}

/**
 * Muestra una notificacion transitoria. `kind` es "info" (default), "success" o "error".
 * Se auto-cierra a los `duration` ms; el usuario tambien puede cerrarla a mano.
 */
export function showToast(message, kind = "info", duration = 4000) {
  const el = document.createElement("div");
  el.className = "toast";
  el.dataset.kind = kind;
  el.innerHTML = `<span></span><button type="button" class="toast-close" aria-label="Cerrar">✕</button>`;
  el.querySelector("span").textContent = message;

  const dismiss = () => {
    el.classList.remove("toast-visible");
    el.addEventListener("transitionend", () => el.remove(), { once: true });
    setTimeout(() => el.remove(), 300);
  };
  el.querySelector(".toast-close").addEventListener("click", dismiss);

  getRegion().appendChild(el);
  requestAnimationFrame(() => el.classList.add("toast-visible"));
  setTimeout(dismiss, duration);
}
