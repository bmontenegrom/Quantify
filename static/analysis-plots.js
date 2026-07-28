import { escapeHtml, measureText, regressionPlot, scatterPlot } from "./lib.js";

export function regressionMarkup(regression) {
  const plot = regressionPlot(regression.points ?? [], regression.slope, regression.intercept);
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
  `;
}

/**
 * Markup SVG común a los gráficos de ajuste y de dispersión: ejes, puntos y rótulos.
 * `lineMarkup` inyecta la recta del ajuste (vacío para scatter); `xText`/`yLabel`/`ariaLabel`
 * deben venir ya escapados por el llamador.
 */
function plotSvg(plot, { ariaLabel, lineMarkup = "", xText, yLabel }) {
  const f = (n) => n.toFixed(1);
  const points = plot.scatter
    .map((p) => `<circle cx="${f(p.cx)}" cy="${f(p.cy)}" r="3" class="reg-point" />`)
    .join("");
  const axisY = plot.height - plot.pad;
  return `
    <svg class="reg-plot" viewBox="0 0 ${plot.width} ${plot.height}" role="img" aria-label="${ariaLabel}">
      <line class="reg-axis" x1="${plot.pad}" y1="${axisY}" x2="${plot.width - plot.pad}" y2="${axisY}" />
      <line class="reg-axis" x1="${plot.pad}" y1="${plot.pad}" x2="${plot.pad}" y2="${axisY}" />
      ${lineMarkup}
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
