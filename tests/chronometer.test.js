import { test } from "node:test";
import assert from "node:assert/strict";
import { Chronometer } from "../static/chronometer.js";

function clock(times) {
  let i = 0;
  return () => times[i++];
}

test("Chronometer: estado inicial es idle, count 0, elapsed 0", () => {
  const c = new Chronometer(clock([1000]));
  assert.equal(c.state, "idle");
  assert.equal(c.count, 0);
  assert.equal(c.elapsed, 0);
  assert.deepEqual(c.marks, []);
});

test("Chronometer: start cambia estado a running", () => {
  const c = new Chronometer(clock([1000, 2000]));
  c.start();
  assert.equal(c.state, "running");
});

test("Chronometer: mark registra timestamps; count y marks reflejan el estado", () => {
  const c = new Chronometer(clock([0, 1000, 2000, 3000]));
  c.start();
  c.mark();
  c.mark();
  c.mark();
  assert.equal(c.count, 3);
  assert.deepEqual(c.marks, [1000, 2000, 3000]);
});

test("Chronometer: stop cambia estado a stopped; elapsed usa la hora de stop", () => {
  const c = new Chronometer(clock([0, 5000]));
  c.start();
  c.stop();
  assert.equal(c.state, "stopped");
  assert.ok(Math.abs(c.elapsed - 5) < 1e-9);
});

test("Chronometer: elapsed en running consulta el reloj en tiempo real", () => {
  const times = [0, 3000];
  let idx = 0;
  const c = new Chronometer(() => times[idx++]);
  c.start();
  assert.ok(Math.abs(c.elapsed - 3) < 1e-9);
});

test("Chronometer: mark ignorado fuera de running", () => {
  const c = new Chronometer(clock([0, 999, 999]));
  c.mark();
  assert.equal(c.count, 0);
  c.start();
  c.stop();
  c.mark();
  assert.equal(c.count, 0);
});

test("Chronometer: start ignorado si ya no es idle", () => {
  const c = new Chronometer(clock([0, 1000, 2000]));
  c.start();
  c.start();
  c.stop();
  c.start();
  assert.equal(c.state, "stopped");
});

test("Chronometer: reset vuelve a idle y borra marcas", () => {
  const c = new Chronometer(clock([0, 1000, 2000]));
  c.start();
  c.mark();
  c.stop();
  c.reset();
  assert.equal(c.state, "idle");
  assert.equal(c.count, 0);
  assert.equal(c.elapsed, 0);
});

test("Chronometer: readings('absoluto') da tiempos desde inicio en segundos", () => {
  const c = new Chronometer(clock([0, 1000, 3000, 6000]));
  c.start();
  c.mark();
  c.mark();
  c.mark();
  assert.deepEqual(c.readings("absoluto"), [1, 3, 6]);
});

test("Chronometer: readings('consecutivo') da diferencias entre marcas contiguas", () => {
  const c = new Chronometer(clock([0, 1000, 1500, 2200]));
  c.start();
  c.mark();
  c.mark();
  c.mark();
  const r = c.readings("consecutivo");
  assert.equal(r.length, 2);
  assert.ok(Math.abs(r[0] - 0.5) < 1e-9);
  assert.ok(Math.abs(r[1] - 0.7) < 1e-9);
});

test("Chronometer: readings('consecutivo') requiere ≥2 marcas", () => {
  const c = new Chronometer(clock([0, 1000]));
  c.start();
  c.mark();
  assert.deepEqual(c.readings("consecutivo"), []);
});

test("Chronometer: readings('pares') da T[i]=mark[i+2]-mark[i], n-2 lecturas", () => {
  // Péndulo: marcas cada T/2; T=2s → marks a 1,2,3,4,5 s
  const ms = [0, 1000, 2000, 3000, 4000, 5000];
  const c = new Chronometer(clock(ms));
  c.start();
  ms.slice(1).forEach(() => c.mark());
  const r = c.readings("pares");
  // T[i] = mark[i+2]-mark[i]: (3-1)=2, (4-2)=2, (5-3)=2 → 3 lecturas de 2s c/u
  assert.equal(r.length, 3);
  r.forEach((t) => assert.ok(Math.abs(t - 2) < 1e-9));
});

test("Chronometer: readings('pares') requiere ≥3 marcas", () => {
  const c = new Chronometer(clock([0, 1000, 2000]));
  c.start();
  c.mark();
  c.mark();
  assert.deepEqual(c.readings("pares"), []);
});

test("Chronometer: readings('periodo') da T[k]=mark[2k+1]-mark[2k], pares no solapados", () => {
  // Técnica del péndulo: dt1=t1-t0, dt2=t3-t2. Marcas a 1, 2.1, 3, 4.05 s desde inicio.
  const ms = [0, 1000, 2100, 3000, 4050];
  const c = new Chronometer(clock(ms));
  c.start();
  ms.slice(1).forEach(() => c.mark()); // 4 marcas
  const r = c.readings("periodo");
  // pares (m0,m1),(m2,m3): (2100-1000)=1.1, (4050-3000)=1.05 → 2 períodos
  assert.equal(r.length, 2);
  assert.ok(Math.abs(r[0] - 1.1) < 1e-9);
  assert.ok(Math.abs(r[1] - 1.05) < 1e-9);
});

test("Chronometer: readings('periodo') con marca impar descarta la última suelta", () => {
  const ms = [0, 1000, 2000, 3000]; // 3 marcas
  const c = new Chronometer(clock(ms));
  c.start();
  ms.slice(1).forEach(() => c.mark());
  const r = c.readings("periodo");
  // solo el par (m0,m1) → 1 período; la marca m2 queda suelta
  assert.equal(r.length, 1);
  assert.ok(Math.abs(r[0] - 1) < 1e-9);
});

test("Chronometer: readings vacío sin marcas ni inicio", () => {
  const c = new Chronometer();
  assert.deepEqual(c.readings("absoluto"), []);
  assert.deepEqual(c.readings("consecutivo"), []);
  assert.deepEqual(c.readings("pares"), []);
  assert.deepEqual(c.readings("periodo"), []);
});
