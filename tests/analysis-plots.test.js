import { test } from "node:test";
import assert from "node:assert/strict";

import { regressionMarkup } from "../static/analysis-plots.js";

const base = { points: [[0, 1], [1, 3], [2, 5]], slope: 2, intercept: 1, u_slope: 0.1, u_intercept: 0.2, r_squared: 1, x_label: "x", y_label: "y" };

test("regressionMarkup muestra la U del ajuste y la total con las compartidas que incluye", () => {
  const html = regressionMarkup({ ...base, u_slope_total: 0.15, u_intercept_total: 0.25, shared: ["dx", "rho"] });
  assert.equal(html.match(/>ajuste</g).length, 2);
  assert.equal(html.match(/total \(incluye dx, rho\)/g).length, 2);
});

test("regressionMarkup sin compartidas muestra solo la U del ajuste", () => {
  const html = regressionMarkup(base);
  assert.ok(!html.includes(">ajuste<"));
  assert.ok(!html.includes("total (incluye"));
});
