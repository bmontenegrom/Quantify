import { escapeHtml, format, measureText, regressionPlot, scatterPlot } from "./lib.js";

let plotCount = 0;
const isClipped = (p) => [p.ex, p.ey].some((bar) => bar?.clipped.some(Boolean));

export function regressionMarkup(regression) {
  const plot = regressionPlot(regression.points ?? [], regression.slope, regression.intercept, { uPoints: regression.u_points ?? [] });
  return `
    <div class="metrics">
      <div class="metric">
        <div class="metric-label">Pendiente</div>
        <div class="metric-value metric-text">${escapeHtml(measureText(regression.slope, regression.u_slope))}</div>
      </div>
      <div class="metric">
        <div class="metric-label">Intercepto</div>
        <div class="metric-value metric-text">${escapeHtml(measureText(regression.intercept, regression.u_intercept))}</div>
      </div>
      <div class="metric">
        <div class="metric-label">R²</div>
        <div class="metric-value">${format(regression.r_squared)}</div>
      </div>
      <div class="metric">
        <div class="metric-label">Puntos</div>
        <div class="metric-value">${(regression.points ?? []).length}</div>
      </div>
    </div>
    ${plot ? regressionSvg(plot, regression.x_label, regression.y_label) : `<p class="submission-meta">No se puede graficar: el rango de los datos es nulo.</p>`}
    ${plot?.scatter.some(isClipped) ? `<p class="submission-meta">Las barras con flecha siguen fuera del gráfico: su incertidumbre es mayor que lo dibujado.</p>` : ""}
    ${plot?.scatter.some((p) => p.noU) ? `<p class="submission-meta">Los puntos huecos no tienen incertidumbre: falta la de alguna magnitud o la fórmula no es derivable ahí (ver avisos).</p>` : ""}
  `;
}

function plotSvg(plot, { ariaLabel, lineMarkup = "", xText, yLabel }) {
  const f = (n) => n.toFixed(1);
  const markerId = `reg-clip-${++plotCount}`;
  const half = (p, x, y, clipped) =>
    `<line class="reg-errbar" x1="${f(p.cx)}" y1="${f(p.cy)}" x2="${f(x)}" y2="${f(y)}"${clipped ? ` marker-end="url(#${markerId})"` : ""} />`;
  const bars = plot.scatter
    .map((p) =>
      [
        ...(p.ex ? p.ex.ends.map((x, k) => half(p, x, p.cy, p.ex.clipped[k])) : []),
        ...(p.ey ? p.ey.ends.map((y, k) => half(p, p.cx, y, p.ey.clipped[k])) : []),
      ].join(""),
    )
    .join("");
  const clipMarker = plot.scatter.some(isClipped)
    ? `<defs><marker id="${markerId}" viewBox="0 0 6 6" refX="6" refY="3" markerWidth="6" markerHeight="6" orient="auto"><path class="reg-clip-arrow" d="M0,0 L6,3 L0,6 z" /></marker></defs>`
    : "";
  const points = plot.scatter
    .map((p) => `<circle cx="${f(p.cx)}" cy="${f(p.cy)}" r="3" class="reg-point${p.noU ? " reg-point-nou" : ""}" />`)
    .join("");
  const axisY = plot.height - plot.pad;
  return `
    <svg class="reg-plot" viewBox="0 0 ${plot.width} ${plot.height}" role="img" aria-label="${ariaLabel}">
      <line class="reg-axis" x1="${plot.pad}" y1="${axisY}" x2="${plot.width - plot.pad}" y2="${axisY}" />
      <line class="reg-axis" x1="${plot.pad}" y1="${plot.pad}" x2="${plot.pad}" y2="${axisY}" />
      ${clipMarker}
      ${lineMarkup}
      ${bars}
      ${points}
      <text class="reg-label" x="${plot.width - plot.pad}" y="${plot.height - 8}" text-anchor="end">${xText}</text>
      <text class="reg-label" x="${plot.pad}" y="${plot.pad - 12}" text-anchor="start">y: ${yLabel}</text>
    </svg>
  `;
}

function regressionSvg(plot, xLabel = "x", yLabel = "y") {
  const f = (n) => n.toFixed(1);
  const lineMarkup = `<line class="reg-line" x1="${f(plot.line.x1)}" y1="${f(plot.line.y1)}" x2="${f(plot.line.x2)}" y2="${f(plot.line.y2)}" />`;
  return plotSvg(plot, {
    ariaLabel: `Gráfico del ajuste lineal de ${escapeHtml(yLabel)} contra ${escapeHtml(xLabel)}`,
    lineMarkup,
    xText: `x: ${escapeHtml(xLabel)}`,
    yLabel: escapeHtml(yLabel),
  });
}

export function scatterMarkup(scatter) {
  const points = scatter.points ?? [];
  const plot = scatterPlot(points, { xLog: scatter.x_log });
  const xHeader = scatter.x_log ? `${escapeHtml(scatter.x_label)} (log)` : escapeHtml(scatter.x_label);
  const table = `
    <div class="data-table-wrap">
      <table class="data-table">
        <thead>
          <tr><th>#</th><th>${xHeader}</th><th>${escapeHtml(scatter.y_label)}</th></tr>
        </thead>
        <tbody>
          ${points
            .map((p, i) => `<tr><td>${i + 1}</td><td>${format(p[0])}</td><td>${format(p[1])}</td></tr>`)
            .join("")}
        </tbody>
      </table>
    </div>`;
  const graph = plot
    ? scatterSvg(plot, scatter.x_label, scatter.y_label)
    : `<p class="submission-meta">No se puede graficar: el rango de los datos es nulo${scatter.x_log ? " o hay un x ≤ 0 con eje logarítmico" : ""}.</p>`;
  return `${graph}${table}`;
}

function scatterSvg(plot, xLabel = "x", yLabel = "y") {
  const xText = plot.xLog ? `x: ${escapeHtml(xLabel)} (log)` : `x: ${escapeHtml(xLabel)}`;
  return plotSvg(plot, {
    ariaLabel: `Gráfico de dispersión de ${escapeHtml(yLabel)} contra ${escapeHtml(xLabel)}`,
    xText,
    yLabel: escapeHtml(yLabel),
  });
}
