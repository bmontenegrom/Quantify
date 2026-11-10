
export class Chronometer {
  #state = "idle";
  #marks = [];
  #startedAt = null;
  #stoppedAt = null;
  #clockFn;

  constructor(clockFn = () => Date.now()) {
    this.#clockFn = clockFn;
  }

  static fromJSON(data, clockFn) {
    const c = new Chronometer(clockFn);
    const marks = Array.isArray(data?.marks) ? data.marks.filter(Number.isFinite) : [];
    const startedAt = Number.isFinite(data?.startedAt) ? data.startedAt : null;
    if (startedAt === null || !marks.length || !["running", "stopped"].includes(data.state)) return c;
    c.#marks = marks;
    c.#startedAt = startedAt;
    c.#state = "stopped";
    c.#stoppedAt = data.state === "stopped" && Number.isFinite(data.stoppedAt) ? data.stoppedAt : marks.at(-1);
    return c;
  }

  toJSON() {
    return { state: this.#state, marks: [...this.#marks], startedAt: this.#startedAt, stoppedAt: this.#stoppedAt };
  }

  get state() {
    return this.#state;
  }

  get count() {
    return this.#marks.length;
  }

  get marks() {
    return [...this.#marks];
  }

  get elapsed() {
    if (this.#startedAt === null) return 0;
    const end = this.#stoppedAt ?? this.#clockFn();
    return (end - this.#startedAt) / 1000;
  }

  start() {
    if (this.#state !== "idle") return;
    this.#startedAt = this.#clockFn();
    this.#state = "running";
  }

  mark() {
    if (this.#state !== "running") return;
    this.#marks.push(this.#clockFn());
  }

  stop() {
    if (this.#state !== "running") return;
    this.#stoppedAt = this.#clockFn();
    this.#state = "stopped";
  }

  reset() {
    this.#state = "idle";
    this.#marks = [];
    this.#startedAt = null;
    this.#stoppedAt = null;
  }

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
        return this.#marks.slice(2).map((t, i) => (t - this.#marks[i]) / 1000);
      case "periodo": {
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
