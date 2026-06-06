import { state } from "./state.js";
import { escapeHtml, format, canReview, measureText, regressionPlot, compareResults, cssEscape } from "./lib.js";
import { postJson } from "./api.js";
import { submissionHeader, teacherCommentMarkup, editBannerMarkup, renderReviewForm, saveReview } from "./submissions.js";
import { openSubmissionWorkspace } from "./submissions.js";

export function renderAnalysis(target, submission, includeReview = false, definition = null) {
  target.classList.remove("detail-empty");

  if (submission.entry_mode === "form") {
    const isTeacher = canReview(state.user);
    const studentResults = submission.student_results ?? [];
    let body = "";
    if (submission.analysis) {
      body += formAnalysisMarkup(submission.analysis);
      if (studentResults.length) {
        body += comparisonMarkup(submission.analysis.derived ?? [], studentResults);
      }
    } else {
      body += `<p class="submission-meta">El docente todavia no habilito los resultados de esta entrega.</p>`;
    }
    body += measurementMetaMarkup(submission, definition);
    if (!isTeacher) {
      body += studentResultsFormMarkup(submission, definition);
    }
    target.innerHTML = `
      ${submissionHeader(submission)}
      ${teacherCommentMarkup(submission)}
      ${!isTeacher ? editBannerMarkup(submission) : ""}
      ${body}
      ${includeReview ? renderReviewForm(submission) : ""}
    `;
    target
      .querySelector(".edit-submission-btn")
      ?.addEventListener("click", () =>
        import("./forms.js").then(({ startEditSubmission }) => startEditSubmission(submission))
      );
    const reviewForm = target.querySelector(".review-form");
    if (reviewForm) reviewForm.addEventListener("submit", (event) => saveReview(event, submission.id));
    const studentForm = target.querySelector(".student-results-form");
    if (studentForm && !submission.results_visible_to_student) {
      studentForm.addEventListener("submit", (event) => saveStudentResults(event, submission.id));
    }
    return;
  }

  // Entregas CSV (legacy): estadística por columna + regresión.
  const analysis = submission.analysis;
  const regression = analysis.regression;
  target.innerHTML = `
    ${submissionHeader(submission)}
    ${teacherCommentMarkup(submission)}

    <div class="metrics">
      <div class="metric">
        <div class="metric-label">Filas</div>
        <div class="metric-value">${analysis.row_count}</div>
      </div>
      <div class="metric">
        <div class="metric-label">Columnas numericas</div>
        <div class="metric-value">${analysis.numeric_columns.length}</div>
      </div>
      <div class="metric">
        <div class="metric-label">Advertencias</div>
        <div class="metric-value">${analysis.warnings.length}</div>
      </div>
    </div>

    ${renderStats(analysis.numeric_columns)}
    ${regression ? renderRegression(regression) : ""}
    ${renderWarnings(analysis.warnings)}
    ${includeReview ? renderReviewForm(submission) : ""}
  `;

  const reviewForm = target.querySelector(".review-form");
  if (reviewForm) reviewForm.addEventListener("submit", (event) => saveReview(event, submission.id));
}

function renderStats(columns) {
  if (columns.length === 0) return `<p class="submission-meta">No se detectaron columnas numericas.</p>`;
  return `
    <h3>Estadistica por columna</h3>
    <div class="metrics">
      ${columns
        .map(
          (column) => `
          <div class="metric">
            <div class="metric-label">${escapeHtml(column.name)}</div>
            <div>n=${column.count}</div>
            <div>media=${format(column.mean)}</div>
            <div>sd=${format(column.std_dev)}</div>
            <div>min=${format(column.min)} max=${format(column.max)}</div>
          </div>
        `,
        )
        .join("")}
    </div>
  `;
}

function renderRegression(regression) {
  return `
    <h3>Ajuste lineal automatico</h3>
    <div class="metric">
      <div>${escapeHtml(regression.y_column)} = ${format(regression.slope)} * ${escapeHtml(regression.x_column)} + ${format(regression.intercept)}</div>
      <div class="submission-meta">R2 = ${format(regression.r_squared)}</div>
    </div>
  `;
}

function renderWarnings(warnings) {
  if (warnings.length === 0) return "";
  return `
    <div class="warnings">
      <strong>Advertencias</strong>
      ${warnings.slice(0, 8).map((warning) => `<span>${escapeHtml(warning)}</span>`).join("")}
      ${warnings.length > 8 ? `<span>${warnings.length - 8} mas...</span>` : ""}
    </div>
  `;
}

function measurementMetaMarkup(submission, definition) {
  const meta = submission.measurement_meta;
  if (!meta || typeof meta !== "object") return "";
  const labelFor = (qid) => {
    const fromAnalysis = (submission.analysis?.quantities ?? []).find((q) => q.quantity_id === qid);
    if (fromAnalysis) return `${fromAnalysis.name} (${fromAnalysis.symbol})`;
    const fromDef = (definition?.quantities ?? []).find((q) => q.id === qid);
    if (fromDef) return `${fromDef.name} (${fromDef.symbol})`;
    return qid;
  };
  const blocks = Object.entries(meta)
    .map(([qid, m]) => {
      const discarded = m?.discarded ?? [];
      const bins = m?.bins;
      if (!discarded.length && !bins) return "";
      return `<div class="meta-block">
        <strong>${escapeHtml(labelFor(qid))}</strong>${bins ? ` <span class="submission-meta">· ${escapeHtml(String(bins))} intervalos</span>` : ""}
        ${
          discarded.length
            ? `<div class="submission-meta">Puntos descartados (${discarded.length}): ${discarded.map((v) => Number(v).toFixed(3)).join(", ")}</div>`
            : `<div class="submission-meta">Sin puntos descartados.</div>`
        }
      </div>`;
    })
    .filter(Boolean)
    .join("");
  if (!blocks) return "";
  return `<section class="panel meta-panel"><h4>Depuración de series</h4>${blocks}</section>`;
}

function formAnalysisMarkup(analysis) {
  const quantities = analysis.quantities ?? [];
  const derived = analysis.derived ?? [];
  const quantitiesTable = quantities.length
    ? `
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
          <thead>
            <tr><th>Magnitud</th><th>n</th><th>media</th><th>s</th><th>u_A</th><th>u_B</th><th>u_c</th><th>U</th></tr>
          </thead>
          <tbody>
            ${quantities
              .map(
                (q) => `
                <tr>
                  <td class="directory-primary"><strong>${escapeHtml(q.symbol)}</strong> <span class="submission-meta">${escapeHtml(q.unit)}</span></td>
                  <td>${q.result.n}</td>
                  <td>${format(q.result.mean)}</td>
                  <td>${format(q.result.s)}</td>
                  <td>${format(q.result.u_a)}</td>
                  <td>${format(q.result.u_b)}</td>
                  <td>${format(q.result.u_c)}</td>
                  <td>${format(q.result.u_expanded)}</td>
                </tr>`,
              )
              .join("")}
          </tbody>
        </table>
      </div>`
    : `<p class="submission-meta">Sin magnitudes cargadas.</p>`;

  const derivedBlock = derived.length
    ? `
      <h3>Mensurandos</h3>
      <div class="metrics">
        ${derived
          .map(
            (d) => `
            <div class="metric">
              <div class="metric-label">${escapeHtml(d.symbol)} (${escapeHtml(d.unit)})</div>
              <div class="metric-value metric-text">${escapeHtml(measureText(d.value, d.u_expanded))}</div>
              <div class="submission-meta">${escapeHtml(d.formula)}</div>
            </div>`,
          )
          .join("")}
      </div>`
    : "";

  if (analysis.regression) {
    return `
      <h3>Ajuste lineal</h3>
      ${regressionMarkup(analysis.regression)}
      ${derivedBlock}
      ${renderWarnings(analysis.warnings ?? [])}
    `;
  }

  return `
    <h3>Incertidumbres por magnitud</h3>
    ${quantitiesTable}
    ${derivedBlock}
    ${renderWarnings(analysis.warnings ?? [])}
  `;
}

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

function regressionSvg(plot, xLabel = "x", yLabel = "y") {
  const f = (n) => n.toFixed(1);
  const points = plot.scatter
    .map((p) => `<circle cx="${f(p.cx)}" cy="${f(p.cy)}" r="3" class="reg-point" />`)
    .join("");
  const axisY = plot.height - plot.pad;
  return `
    <svg class="reg-plot" viewBox="0 0 ${plot.width} ${plot.height}" role="img" aria-label="Gráfico del ajuste lineal de ${escapeHtml(yLabel)} contra ${escapeHtml(xLabel)}">
      <line class="reg-axis" x1="${plot.pad}" y1="${axisY}" x2="${plot.width - plot.pad}" y2="${axisY}" />
      <line class="reg-axis" x1="${plot.pad}" y1="${plot.pad}" x2="${plot.pad}" y2="${axisY}" />
      <line class="reg-line" x1="${f(plot.line.x1)}" y1="${f(plot.line.y1)}" x2="${f(plot.line.x2)}" y2="${f(plot.line.y2)}" />
      ${points}
      <text class="reg-label" x="${plot.width - plot.pad}" y="${plot.height - 8}" text-anchor="end">x: ${escapeHtml(xLabel)}</text>
      <text class="reg-label" x="${plot.pad}" y="${plot.pad - 12}" text-anchor="start">y: ${escapeHtml(yLabel)}</text>
    </svg>
  `;
}

function comparisonMarkup(autoDerived, studentResults) {
  const rows = compareResults(autoDerived, studentResults);
  if (!rows.length) return "";
  const num = (v) => (v == null ? "—" : escapeHtml(format(v)));
  const pct = (v) => (v == null ? "—" : `${escapeHtml(format(v))} %`);
  return `
    <h3>Comparación: tus cálculos vs automático</h3>
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table compare-table">
        <thead>
          <tr><th>Mensurando</th><th>Automático</th><th>Tus cálculos</th><th>Δ valor</th><th>Δ valor (%)</th><th>Δ U</th><th>Δ U (%)</th></tr>
        </thead>
        <tbody>
          ${rows
            .map(
              (r) => `
            <tr>
              <td class="directory-primary"><strong>${escapeHtml(r.symbol)}</strong> <span class="submission-meta">${escapeHtml(r.unit)}</span></td>
              <td>${escapeHtml(measureText(r.auto.value, r.auto.u))}</td>
              <td>${r.student ? escapeHtml(measureText(r.student.value, r.student.u)) : "—"}</td>
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

function studentResultsFormMarkup(submission, definition) {
  const measurands = definition?.results ?? [];
  if (!measurands.length) return "";
  const locked = submission.results_visible_to_student;
  const saved = new Map((submission.student_results ?? []).map((s) => [s.symbol, s]));
  const rows = measurands
    .map((m) => {
      const s = saved.get(m.symbol);
      const v = s ? escapeHtml(String(s.value)) : "";
      const u = s && s.u_expanded != null ? escapeHtml(String(s.u_expanded)) : "";
      const dis = locked ? "disabled" : "";
      return `
        <tr>
          <td class="directory-primary"><strong>${escapeHtml(m.symbol)}</strong> <span class="submission-meta">${escapeHtml(m.name)} (${escapeHtml(m.unit)})</span></td>
          <td><input class="student-value" data-symbol="${escapeHtml(m.symbol)}" type="number" step="any" value="${v}" ${dis} placeholder="valor" /></td>
          <td><input class="student-u" data-symbol="${escapeHtml(m.symbol)}" type="number" step="any" value="${u}" ${dis} placeholder="U" /></td>
        </tr>`;
    })
    .join("");
  return `
    <form class="student-results-form detail-form">
      <h3>Mis cálculos</h3>
      <p class="submission-meta">${
        locked
          ? "El docente habilitó los resultados; tus cálculos quedaron congelados."
          : "Ingresá tu valor y tu U para cada mensurando (calculados por tu cuenta). Podés editarlos hasta que el docente habilite los resultados."
      }</p>
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
          <thead><tr><th>Mensurando</th><th>Valor</th><th>U</th></tr></thead>
          <tbody>${rows}</tbody>
        </table>
      </div>
      <span class="student-results-status submission-meta"></span>
      ${locked ? "" : `<div class="detail-actions"><button type="submit">Guardar mis cálculos</button></div>`}
    </form>
  `;
}

async function saveStudentResults(event, submissionId) {
  event.preventDefault();
  const form = event.currentTarget;
  const results = [];
  form.querySelectorAll(".student-value").forEach((input) => {
    const value = input.value.trim();
    if (value === "") return;
    const symbol = input.dataset.symbol;
    const uInput = form.querySelector(`.student-u[data-symbol="${cssEscape(symbol)}"]`);
    const u = uInput && uInput.value.trim() !== "" ? Number(uInput.value) : null;
    results.push({ symbol, value: Number(value), u_expanded: u });
  });
  try {
    await postJson(`/api/submissions/${submissionId}/student-results`, { results });
    await openSubmissionWorkspace(submissionId);
  } catch (error) {
    const note = form.querySelector(".student-results-status");
    if (note) note.textContent = error.message;
  }
}
