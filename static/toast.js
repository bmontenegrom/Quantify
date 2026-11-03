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
