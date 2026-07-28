import { escapeHtml, symbolHtml, unitHtml, measureText, format, num, pct, compareResults, compareMeasuredVsTheoretical } from "./lib.js";

/** Tabla "Medido vs teórico": magnitudes medidas (`X`) contra su derivado automático (`X_t`). */
export function measuredVsTheoreticalMarkup(quantities, derived) {
  const rows = compareMeasuredVsTheoretical(quantities, derived);
  if (!rows.length) return "";
  return `
    <h3>Medido vs teórico (automático)</h3>
    <p class="submission-meta">Cada magnitud medida comparada con el valor teórico que calcula el programa (con su U propagada).</p>
    <div class="data-table-wrap">
      <table class="data-table compare-table">
        <thead>
          <tr>
            <th>Magnitud</th><th>Medido (±U)</th><th>Teórico (±U)</th>
            <th>Δ valor</th><th>Δ valor (%)</th><th>Δ U</th><th>Δ U (%)</th>
          </tr>
        </thead>
        <tbody>
          ${rows
            .map(
              (r) => `
            <tr>
              <td class="directory-primary"><strong>${symbolHtml(r.symbol)}</strong> <span class="submission-meta">${unitHtml(r.unit)}</span></td>
              <td>${escapeHtml(measureText(r.exp.value, r.exp.u))}</td>
              <td>${escapeHtml(measureText(r.teo.value, r.teo.u))}</td>
              <td>${num(r.dValue)}</td>
              <td>${pct(r.dValuePct)}</td>
              <td>${num(r.dU)}</td>
              <td>${pct(r.dUPct)}</td>
            </tr>`,
            )
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}

export function comparisonMarkup(autoDerived, studentResults, tolerances = {}) {
  const rows = compareResults(autoDerived, studentResults, tolerances);
  if (!rows.length) return "";
  const hasVerdicts = rows.some((r) => r.verdict != null);
  const verdictCell = (r) => {
    if (!hasVerdicts) return "";
    if (r.verdict === "pass") return `<td class="verdict-pass">✓</td>`;
    if (r.verdict === "fail") return `<td class="verdict-fail">✗</td>`;
    return `<td class="verdict-none">—</td>`;
  };
  return `
    <h3>Comparación: tus cálculos vs automático</h3>
    <div class="data-table-wrap">
      <table class="data-table compare-table">
        <thead>
          <tr>
            <th>Mensurando</th><th>Automático</th><th>Tus cálculos</th>
            <th>Δ valor</th><th>Δ valor (%)</th><th>Δ U</th><th>Δ U (%)</th>
            ${hasVerdicts ? "<th>Veredicto</th>" : ""}
          </tr>
        </thead>
        <tbody>
          ${rows
            .map(
              (r) => `
            <tr>
              <td class="directory-primary"><strong>${symbolHtml(r.symbol)}</strong> <span class="submission-meta">${unitHtml(r.unit)}</span></td>
              <td>${escapeHtml(measureText(r.auto.value, r.hasUncertainty ? r.auto.u : null))}</td>
              <td>${r.student ? escapeHtml(measureText(r.student.value, r.hasUncertainty ? r.student.u : null)) : "—"}</td>
              <td>${num(r.dValue)}</td>
              <td>${pct(r.dValuePct)}</td>
              <td>${num(r.dU)}</td>
              <td>${pct(r.dUPct)}</td>
              ${verdictCell(r)}
            </tr>`,
            )
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}

export function pointResultsComparisonMarkup(analysis, studentResults) {
  const pointResults = analysis.point_results ?? [];
  if (!pointResults.length) return "";
  const byStudent = new Map((studentResults ?? []).map((s) => [s.symbol, s]));
  const n = Math.max(0, ...pointResults.map((p) => p.values.length));
  const hasAny = pointResults.some((p) => p.values.some((_, k) => byStudent.has(`${p.symbol}#${k}`)));
  if (!hasAny) return "";
  const num = (v) => (v == null || !Number.isFinite(v) ? "—" : escapeHtml(format(v)));
  const headCells = pointResults
    .map((p) => `<th>${symbolHtml(p.symbol)} auto</th><th>tuyo</th><th>Δ %</th>`)
    .join("");
  const rows = Array.from({ length: n }, (_, k) => {
    const cells = pointResults
      .map((p) => {
        const auto = p.values[k];
        const s = byStudent.get(`${p.symbol}#${k}`);
        const sv = s ? s.value : null;
        const pct = sv != null && auto ? ((sv - auto) / auto) * 100 : null;
        return `<td>${num(auto)}</td><td>${num(sv)}</td><td>${pct == null ? "—" : `${num(pct)} %`}</td>`;
      })
      .join("");
    return `<tr><td class="directory-primary">Corrida ${k + 1}</td>${cells}</tr>`;
  }).join("");
  return `
    <h3>Comparación por corrida: tus cálculos vs automático</h3>
    <div class="data-table-wrap">
      <table class="data-table compare-table">
        <thead><tr><th>Corrida</th>${headCells}</tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}
