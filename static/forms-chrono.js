import { state } from "./state.js";
import { measurementFields, practiceSelect } from "./dom.js";
import { seriesStats, histogram, normalCurve } from "./lib.js";
import { PRACTICES_WITHOUT_CHRONO_HELPER } from "./constants.js";
import { Chronometer } from "./chronometer.js";

/** Markup interno (sin fieldset) del widget de cronómetro: display, controles y modo. */
function chronoWidgetInnerHtml() {
  return `
    <div class="chrono-widget">
      <div class="chrono-display">0.000 s</div>
      <div class="chrono-info"><span class="chrono-count">0 marcas</span></div>
      <div class="chrono-controls">
        <button type="button" class="chrono-start">▶ Iniciar</button>
        <button type="button" class="chrono-mark" disabled>● Marcar</button>
        <button type="button" class="chrono-stop" disabled>■ Detener</button>
        <button type="button" class="chrono-reset">↺ Reiniciar</button>
      </div>
      <label class="chrono-mode-label">Modo:
        <select class="chrono-mode">
          <option value="periodo">Período (pares t₂-t₁, t₄-t₃… → técnica de Estadística)</option>
          <option value="consecutivo">Consecutivo (una marca por período)</option>
          <option value="pares">Pares solapados (marca cada T/2)</option>
          <option value="absoluto">Absoluto (tiempos desde inicio)</option>
        </select>
      </label>
      <div class="chrono-readings-preview"></div>
    </div>
  `;
}

/**
 * Cronómetro suelto de apoyo (no atado a ninguna magnitud): ayuda a tomar el tiempo para
 * después tipearlo a mano en el input que corresponda. No entra en `collectMeasurements`
 * (usa `.measurement-section`, no `.measurement-row`).
 */
export function chronoHelperSectionHtml() {
  return `
    <div class="measurement-section chrono-helper" data-chrono-helper="1">
      <h4 class="measurement-section-title">Cronómetro <span class="submission-meta">— ayuda para tomar tiempos</span></h4>
      ${chronoWidgetInnerHtml()}
    </div>
  `;
}

/** Cablea todos los `.chrono-helper` presentes en el form con una clave única por instancia. */
export function wireChronoHelpers() {
  measurementFields.querySelectorAll(".chrono-helper").forEach((el, i) => {
    wireChronometerWidget(el, `__chrono_helper__${i}`);
  });
}

/** `true` si esta magnitud se mide a mano (sin cronómetro propio) pero es un tiempo, y la
 *  práctica no está en `PRACTICES_WITHOUT_CHRONO_HELPER` (instrumento con lectura propia,
 *  p. ej. osciloscopio: relajación exponencial no cronometra T_oc/tmedio a mano). */
export function needsChronoHelper(q) {
  if (PRACTICES_WITHOUT_CHRONO_HELPER.has(practiceSelect.value)) return false;
  return q.quantity === "tiempo" && !q.repeated && !q.is_given;
}

/** Clave del cronómetro de una fila: por operador (`qid#i`) si tiene `data-operator-index`. */
export function chronoKeyFor(row) {
  const op = row.dataset.operatorIndex;
  return op != null ? `${row.dataset.quantityId}#${op}` : row.dataset.quantityId;
}

// ── Cronómetro ────────────────────────────────────────────────────────────────

function formatElapsed(seconds) {
  const total = Math.max(0, seconds);
  const m = Math.floor(total / 60);
  const s = Math.floor(total % 60);
  const ms = Math.round((total % 1) * 1000);
  return m > 0
    ? `${m}:${String(s).padStart(2, "0")}.${String(ms).padStart(3, "0")} s`
    : `${s}.${String(ms).padStart(3, "0")} s`;
}

export function wireChronometerWidget(row, quantityId) {
  if (!state.chronometers.has(quantityId)) {
    state.chronometers.set(quantityId, new Chronometer());
  }
  const chrono = state.chronometers.get(quantityId);

  const display = row.querySelector(".chrono-display");
  const countEl = row.querySelector(".chrono-count");
  const startBtn = row.querySelector(".chrono-start");
  const markBtn = row.querySelector(".chrono-mark");
  const stopBtn = row.querySelector(".chrono-stop");
  const resetBtn = row.querySelector(".chrono-reset");
  const modeSelect = row.querySelector(".chrono-mode");
  const preview = row.querySelector(".chrono-readings-preview");

  let rafId = null;

  function updateButtons() {
    const s = chrono.state;
    startBtn.disabled = s !== "idle";
    markBtn.disabled = s !== "running";
    stopBtn.disabled = s !== "running";
    resetBtn.disabled = s === "running";
  }

  function updatePreview() {
    const mode = modeSelect.value;
    const r = chrono.readings(mode);
    countEl.textContent = `${chrono.count} marca${chrono.count !== 1 ? "s" : ""} → ${r.length} lectura${r.length !== 1 ? "s" : ""}`;
    if (r.length === 0) {
      preview.textContent = "";
      return;
    }
    const shown = r.slice(0, 8).map((v) => v.toFixed(3)).join(", ");
    preview.textContent = r.length > 8 ? `${shown} … (+${r.length - 8} más)` : shown;
  }

  function tick() {
    display.textContent = formatElapsed(chrono.elapsed);
    updatePreview();
    if (chrono.state === "running") {
      rafId = requestAnimationFrame(tick);
    }
  }

  function stopRaf() {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  }

  const debugContainer = row.querySelector(".series-debug");
  function refreshDebug() {
    renderSeriesDebug(row, quantityId, chrono.readings(modeSelect.value));
  }

  display.textContent = formatElapsed(chrono.elapsed);
  updateButtons();
  updatePreview();
  if (chrono.state === "running") rafId = requestAnimationFrame(tick);
  else refreshDebug();

  startBtn.addEventListener("click", () => {
    chrono.start();
    updateButtons();
    if (debugContainer) debugContainer.innerHTML = "";
    rafId = requestAnimationFrame(tick);
  });
  markBtn.addEventListener("click", () => {
    chrono.mark();
    updatePreview();
  });
  stopBtn.addEventListener("click", () => {
    chrono.stop();
    stopRaf();
    display.textContent = formatElapsed(chrono.elapsed);
    updateButtons();
    updatePreview();
    refreshDebug();
  });
  resetBtn.addEventListener("click", () => {
    chrono.reset();
    stopRaf();
    display.textContent = formatElapsed(0);
    updateButtons();
    updatePreview();
    state.seriesDebug.delete(quantityId);
    if (debugContainer) debugContainer.innerHTML = "";
  });
  modeSelect.addEventListener("change", () => {
    state.seriesDebug.delete(quantityId);
    updatePreview();
    if (chrono.state !== "running") refreshDebug();
  });

  row._chronoKeyHandler = (e) => {
    if (e.code === "Space" && e.target.tagName !== "BUTTON" && e.target.tagName !== "SELECT") {
      e.preventDefault();
      chrono.mark();
      updatePreview();
    }
  };
  document.addEventListener("keydown", row._chronoKeyHandler);

  new MutationObserver(() => {
    if (!document.contains(row)) {
      document.removeEventListener("keydown", row._chronoKeyHandler);
      stopRaf();
    }
  }).observe(measurementFields, { childList: true, subtree: false });
}

function renderSeriesDebug(row, quantityId, readings) {
  const container = row.querySelector(".series-debug");
  if (!container) return;
  if (!readings || readings.length === 0) {
    container.innerHTML = "";
    return;
  }
  let dbg = state.seriesDebug.get(quantityId);
  if (!dbg) {
    dbg = { discarded: new Set(), bins: 0 };
    state.seriesDebug.set(quantityId, dbg);
  }
  dbg.discarded = new Set([...dbg.discarded].filter((i) => i < readings.length));

  const kept = readings.filter((_, i) => !dbg.discarded.has(i));
  const stats = seriesStats(kept);
  const defaultBins = Math.max(1, Math.min(20, Math.round(Math.sqrt(kept.length || 1))));
  const bins = dbg.bins && dbg.bins > 0 ? dbg.bins : defaultBins;
  const hist = kept.length > 0 ? histogram(kept, bins) : null;

  const ordered = readings.map((v, i) => ({ v, i })).sort((a, b) => a.v - b.v);
  const items = ordered
    .map(({ v, i }) => {
      const off = dbg.discarded.has(i);
      return `<li class="series-point ${off ? "discarded" : ""}">
        <span class="series-point-value">${v.toFixed(3)} s</span>
        <button type="button" class="series-point-toggle" data-index="${i}">${off ? "restaurar" : "descartar"}</button>
      </li>`;
    })
    .join("");

  container.innerHTML = `
    <div class="series-debug-head">
      <strong>Depuración de la serie</strong>
      <span class="submission-meta">n=${stats.n} · x̄=${Number.isFinite(stats.mean) ? stats.mean.toLocaleString("es-UY", { maximumSignificantDigits: 10 }) : "—"} s · s=${Number.isFinite(stats.std) ? stats.std.toLocaleString("es-UY", { maximumSignificantDigits: 10 }) : "—"} s · s/√n=${Number.isFinite(stats.stdMean) ? stats.stdMean.toLocaleString("es-UY", { maximumSignificantDigits: 10 }) : "—"} s</span>
    </div>
    <div class="series-debug-grid">
      <div class="series-hist">
        <label class="hist-bins-label">Intervalos (bins):
          <input type="number" class="hist-bins" min="1" max="40" value="${bins}" />
        </label>
        ${hist ? histogramSvg(hist, stats.mean, stats.std, kept.length) : `<p class="submission-meta">Sin datos conservados.</p>`}
      </div>
      <ol class="series-point-list">${items}</ol>
    </div>
  `;

  container.querySelector(".hist-bins")?.addEventListener("change", (e) => {
    const n = Math.round(Number(e.target.value));
    dbg.bins = Number.isFinite(n) && n >= 1 ? n : 0;
    renderSeriesDebug(row, quantityId, readings);
  });
  container.querySelectorAll(".series-point-toggle").forEach((btn) => {
    btn.addEventListener("click", () => {
      const i = Number(btn.dataset.index);
      if (dbg.discarded.has(i)) dbg.discarded.delete(i);
      else dbg.discarded.add(i);
      renderSeriesDebug(row, quantityId, readings);
    });
  });
}

function histogramSvg(hist, mean, std, n) {
  const W = 340;
  const H = 180;
  const pad = 28;
  const innerW = W - 2 * pad;
  const innerH = H - 2 * pad;
  const { min, max, width, counts } = hist;
  const curve = std > 0 ? normalCurve(mean, std, min, max, 80) : [];
  const curveCounts = curve.map(([x, y]) => [x, y * n * width]);
  const maxCount = Math.max(...counts, ...curveCounts.map((p) => p[1]), 1);
  const spanX = max - min || 1;
  const sx = (x) => pad + ((x - min) / spanX) * innerW;
  const sy = (c) => H - pad - (c / maxCount) * innerH;
  const bars = counts
    .map((c, i) => {
      const x0 = sx(min + i * width);
      const x1 = sx(min + (i + 1) * width);
      const y = sy(c);
      const w = Math.max(0, x1 - x0 - 1);
      return `<rect x="${x0.toFixed(1)}" y="${y.toFixed(1)}" width="${w.toFixed(1)}" height="${(H - pad - y).toFixed(1)}" class="hist-bar" />`;
    })
    .join("");
  const poly = curveCounts.map(([x, c]) => `${sx(x).toFixed(1)},${sy(c).toFixed(1)}`).join(" ");
  const curveEl = poly ? `<polyline points="${poly}" class="normal-curve" fill="none" />` : "";
  return `<svg viewBox="0 0 ${W} ${H}" class="histogram" role="img" aria-label="Histograma con curva normal">
    ${bars}${curveEl}
    <line x1="${pad}" y1="${H - pad}" x2="${W - pad}" y2="${H - pad}" class="hist-axis" />
  </svg>`;
}
