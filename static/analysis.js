import { state } from "./state.js";
import { escapeHtml, format, canReview, formatDate, measureText, regressionPlot, scatterPlot, compareResults, cssEscape, allStudents } from "./lib.js";
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
        body += comparisonMarkup(
          submission.analysis.derived ?? [],
          studentResults,
          submission.result_tolerances ?? {},
        );
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
      ${includeReview ? membersEditorMarkup(submission) : ""}
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
    wireMembersEditor(target, submission.id);
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

/** Tabla de incertidumbres por magnitud (n, media, s, u_A, u_B, u_c, U). */
function quantitiesTableMarkup(quantities) {
  if (!quantities.length) return `<p class="submission-meta">Sin magnitudes cargadas.</p>`;
  return `
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
    </div>`;
}

/** Bloque de mensurandos derivados (valor ± U + fórmula). `heading` controla el título opcional. */
function derivedBlockMarkup(derived, heading = "Mensurandos") {
  if (!derived.length) return "";
  return `
    ${heading ? `<h3>${escapeHtml(heading)}</h3>` : ""}
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
    </div>`;
}

function formAnalysisMarkup(analysis) {
  const quantities = analysis.quantities ?? [];
  const derived = analysis.derived ?? [];
  const quantitiesTable = quantitiesTableMarkup(quantities);
  const derivedBlock = derivedBlockMarkup(derived);

  if (analysis.regression) {
    return `
      <h3>Ajuste lineal</h3>
      ${regressionMarkup(analysis.regression)}
      ${derivedBlock}
      ${renderWarnings(analysis.warnings ?? [])}
    `;
  }

  const scatters = analysis.scatters ?? [];
  if (scatters.length) {
    const title = scatters.length > 1 ? "Curvas (puntos sin ajuste)" : "Curva (puntos sin ajuste)";
    const blocks = scatters
      .map((s) => {
        // Con varias curvas, encabeza cada una con "y vs x" para distinguirlas.
        const heading = scatters.length > 1
          ? `<h4>${escapeHtml(s.y_label)} vs ${escapeHtml(s.x_label)}${s.x_log ? " (x log)" : ""}</h4>`
          : "";
        return `${heading}${scatterMarkup(s)}`;
      })
      .join("");
    return `
      <h3>${title}</h3>
      ${blocks}
      ${renderWarnings(analysis.warnings ?? [])}
    `;
  }

  // Motor D: con operadores, las magnitudes compartidas van arriba y cada operador trae su propio
  // bloque (sus magnitudes repetidas + sus mensurandos), comparados lado a lado sin promedio.
  const operators = analysis.operators ?? [];
  if (operators.length) {
    const sharedTable = quantities.length
      ? `<h4>Magnitudes compartidas</h4>${quantitiesTable}`
      : "";
    const opBlocks = operators
      .map(
        (op) => `
        <section class="operator-result panel">
          <h4>${escapeHtml(op.label)}</h4>
          ${quantitiesTableMarkup(op.quantities ?? [])}
          ${derivedBlockMarkup(op.derived ?? [], "")}
        </section>`,
      )
      .join("");
    return `
      <h3>Incertidumbres por operador</h3>
      ${sharedTable}
      ${opBlocks}
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
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table">
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

function comparisonMarkup(autoDerived, studentResults, tolerances = {}) {
  const rows = compareResults(autoDerived, studentResults, tolerances);
  if (!rows.length) return "";
  const num = (v) => (v == null ? "—" : escapeHtml(format(v)));
  const pct = (v) => (v == null ? "—" : `${escapeHtml(format(v))} %`);
  const hasVerdicts = rows.some((r) => r.verdict != null);
  const verdictCell = (r) => {
    if (!hasVerdicts) return "";
    if (r.verdict === "pass") return `<td class="verdict-pass">✓</td>`;
    if (r.verdict === "fail") return `<td class="verdict-fail">✗</td>`;
    return `<td class="verdict-none">—</td>`;
  };
  return `
    <h3>Comparación: tus cálculos vs automático</h3>
    <div class="directory-table-wrap">
      <table class="grade-table directory-data-table compare-table">
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
              <td class="directory-primary"><strong>${escapeHtml(r.symbol)}</strong> <span class="submission-meta">${escapeHtml(r.unit)}</span></td>
              <td>${escapeHtml(measureText(r.auto.value, r.auto.u))}</td>
              <td>${r.student ? escapeHtml(measureText(r.student.value, r.student.u)) : "—"}</td>
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

function membersEditorMarkup(submission) {
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
      <div class="directory-table-wrap">
        <table class="grade-table directory-data-table">
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

function wireMembersEditor(target, submissionId) {
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
