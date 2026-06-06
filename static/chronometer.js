// Cronómetro de lógica pura (sin DOM, sin efectos globales).
// Pensado para toma de múltiples marcas de tiempo (p. ej. período de péndulo).
// El reloj es inyectable para testear sin timers reales.
// Tests en tests/chronometer.test.js.

export class Chronometer {
  #state = "idle";
  #marks = [];
  #startedAt = null;
  #stoppedAt = null;
  #clockFn;

  /** @param {() => number} clockFn - función de reloj en ms; por defecto Date.now */
  constructor(clockFn = () => Date.now()) {
    this.#clockFn = clockFn;
  }

  /** @returns {'idle'|'running'|'stopped'} */
  get state() {
    return this.#state;
  }

  /** Número de marcas registradas. */
  get count() {
    return this.#marks.length;
  }

  /** Copia de los timestamps absolutos de las marcas (ms desde epoch). */
  get marks() {
    return [...this.#marks];
  }

  /** Tiempo transcurrido en segundos desde el inicio (hasta la parada o hasta ahora). */
  get elapsed() {
    if (this.#startedAt === null) return 0;
    const end = this.#stoppedAt ?? this.#clockFn();
    return (end - this.#startedAt) / 1000;
  }

  /** Inicia el cronómetro. Ignorado si ya está corriendo o detenido. */
  start() {
    if (this.#state !== "idle") return;
    this.#startedAt = this.#clockFn();
    this.#state = "running";
  }

  /** Registra una marca de tiempo. Ignorado si no está corriendo. */
  mark() {
    if (this.#state !== "running") return;
    this.#marks.push(this.#clockFn());
  }

  /** Detiene el cronómetro. Ignorado si no está corriendo. */
  stop() {
    if (this.#state !== "running") return;
    this.#stoppedAt = this.#clockFn();
    this.#state = "stopped";
  }

  /** Reinicia a estado inicial. */
  reset() {
    this.#state = "idle";
    this.#marks = [];
    this.#startedAt = null;
    this.#stoppedAt = null;
  }

  /**
   * Lecturas derivadas de las marcas, en segundos.
   *
   * - `'absoluto'`:    tiempo de cada marca desde el inicio.
   * - `'consecutivo'`: diferencia entre marcas contiguas (T[i] = m[i+1] - m[i]).
   *                    Útil cuando se marca en cada período completo.
   * - `'pares'`:       diferencia entre marcas separadas por 2 (T[i] = m[i+2] - m[i]).
   *                    Útil para péndulo donde se marca en cada paso alternando dirección:
   *                    cada T[i] abarca un período completo aunque los marks son cada T/2.
   * - `'periodo'`:     diferencia entre pares NO solapados (T[k] = m[2k+1] - m[2k]).
   *                    Técnica de Estadística (Física 103): se registra el tiempo en cada
   *                    paso por el equilibrio en el mismo sentido y cada par consecutivo de
   *                    marcas es un período completo independiente. 200 marcas → 100 períodos.
   *
   * @param {'absoluto'|'consecutivo'|'pares'|'periodo'} mode
   * @returns {number[]} lecturas en segundos
   */
  readings(mode = "consecutivo") {
    if (this.#startedAt === null || this.#marks.length === 0) return [];
    switch (mode) {
      case "absoluto":
        return this.#marks.map((t) => (t - this.#startedAt) / 1000);
      case "consecutivo":
        if (this.#marks.length < 2) return [];
        return this.#marks.slice(1).map((t, i) => (t - this.#marks[i]) / 1000);
      case "pares":
        if (this.#marks.length < 3) return [];
        // T[i] = mark[i+2] - mark[i]; avanza de a 1 → n-2 lecturas solapadas.
        return this.#marks.slice(2).map((t, i) => (t - this.#marks[i]) / 1000);
      case "periodo": {
        // T[k] = mark[2k+1] - mark[2k]; avanza de a 2 → floor(n/2) lecturas independientes.
        const out = [];
        for (let i = 0; i + 1 < this.#marks.length; i += 2) {
          out.push((this.#marks[i + 1] - this.#marks[i]) / 1000);
        }
        return out;
      }
      default:
        return [];
    }
  }
}
