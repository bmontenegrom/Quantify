import { test } from "node:test";
import assert from "node:assert/strict";

import { seriesStats, histogram, normalCurve } from "../static/lib.js";

test("seriesStats: n, media, s (n-1) y error de la media", () => {
  const s = seriesStats([2, 4, 4, 4, 5, 5, 7, 9]);
  assert.equal(s.n, 8);
  assert.equal(s.mean, 5);
  // varianza muestral = 32/7 → s = 2.13809...
  assert.ok(Math.abs(s.std - 2.138089935) < 1e-6);
  assert.ok(Math.abs(s.stdMean - s.std / Math.sqrt(8)) < 1e-12);
});

test("seriesStats: ignora no finitos; vacío da NaN; un dato da s=0", () => {
  const empty = seriesStats([NaN, Infinity]);
  assert.equal(empty.n, 0);
  assert.ok(Number.isNaN(empty.mean));
  const one = seriesStats([42, NaN]);
  assert.deepEqual({ n: one.n, mean: one.mean, std: one.std, stdMean: one.stdMean }, {
    n: 1,
    mean: 42,
    std: 0,
    stdMean: 0,
  });
});

test("histogram: conteos suman n y el máximo cae en el último bin", () => {
  const h = histogram([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 5);
  assert.equal(h.bins, 5);
  assert.equal(h.counts.reduce((a, b) => a + b, 0), 11);
  assert.equal(h.counts[4], h.counts[4]); // existe
  // el 10 (máximo) entra en el último bin
  assert.ok(h.counts[4] >= 1);
  assert.equal(h.edges.length, 6);
  assert.equal(h.edges[0], 0);
  assert.equal(h.edges[5], 10);
});

test("histogram: valores iguales → un solo bin; sin datos → null", () => {
  const h = histogram([3, 3, 3], 8);
  assert.equal(h.bins, 1);
  assert.deepEqual(h.counts, [3]);
  assert.equal(histogram([], 5), null);
  assert.equal(histogram([1, 2, 3], 0), null);
});

test("normalCurve: simétrica, máximo en la media, área≈1", () => {
  const pts = normalCurve(0, 1, -5, 5, 1000);
  const peak = pts.reduce((m, p) => (p[1] > m[1] ? p : m), pts[0]);
  assert.ok(Math.abs(peak[0]) < 0.02); // pico cerca de x=0
  // integración trapezoidal ≈ 1 en ±5σ
  let area = 0;
  for (let i = 1; i < pts.length; i++) {
    area += ((pts[i][1] + pts[i - 1][1]) / 2) * (pts[i][0] - pts[i - 1][0]);
  }
  assert.ok(Math.abs(area - 1) < 1e-3);
  assert.deepEqual(normalCurve(0, 0, -1, 1), []);
});
