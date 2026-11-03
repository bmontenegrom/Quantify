export const PRACTICE_GROUPS = {
  "p3-relajacion": { group: "p3", label: "Parte 1: Relajacion directa", order: 1 },
  "p3-relajacion-desfasaje": { group: "p3", label: "Parte 2: Desfasaje", order: 2 },
};

export const PRACTICE_PARTS = {
  "p1-estadistica": [
    { id: "op1", label: "Operador 1" },
    { id: "op2", label: "Operador 2" },
    { id: "op3", label: "Operador 3" },
  ],
  "p2-cc": [
    { id: "serie", label: "Serie" },
    { id: "paralelo", label: "Paralelo" },
    { id: "potencia", label: "Curva de potencia" },
  ],
  hidrostatica: [
    { id: "arq", label: "Parte 1: Arquimedes" },
    { id: "ts", label: "Parte 2: Tension superficial" },
  ],
};

const MOHR_RANURAS = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const HIDRO_MEDIDAS = [1, 2, 3];

export const PRACTICE_SECTIONS = {
  "p1-estadistica": [
    { id: "op1", title: "Operador 1 — Períodos", symbols: ["T1"], results: ["g1"] },
    { id: "op2", title: "Operador 2 — Períodos (opcional)", symbols: ["T2"], results: ["g2"] },
    { id: "op3", title: "Operador 3 — Períodos (opcional)", symbols: ["T3"], results: ["g3"] },
    { title: "Medidas", symbols: ["L", "t_med"], results: ["gamma", "Q"] },
  ],
  "p2-cc": [
    { title: "Resistencias (medidas una vez, valen para las tres partes)", symbols: ["R1", "R2", "R3"] },
    {
      id: "serie",
      title: "Circuito serie",
      symbols: ["Vg_s", "RA_s", "VR1_s", "VR2_s", "VR3_s"],
      results: ["I_s", "VR1_s_t", "VR2_s_t", "VR3_s_t"],
    },
    {
      id: "paralelo",
      title: "Circuito paralelo",
      symbols: ["Vg_p", "RA_p", "VR1_p", "VR2_p", "VR3_p"],
      results: ["I_p", "VR1_p_t", "VR2_p_t", "VR3_p_t"],
    },
    {
      id: "potencia",
      title: "Curva de potencia — datos",
      symbols: ["Vg_c", "RA_c"],
      results: ["RP_max_t", "P_max_t", "P_max_e", "RP_max_e"],
    },
    { id: "potencia", series: true },
  ],
  hidrostatica: [
    {
      id: "arq",
      title: "Goma, fluido y brazos de la balanza",
      symbols: ["m_goma", "rho_f", "g_ac", "b_E", ...MOHR_RANURAS.map((i) => `b${i}`)],
    },
    ...HIDRO_MEDIDAS.map((k) => ({
      id: "arq",
      title: `Medida ${k} — masas por ranura (la ranura sin pesa queda en 0)`,
      symbols: MOHR_RANURAS.map((i) => `m${k}_${i}`),
      results: k === 3 ? ["E_medio", "rho_medio"] : [],
    })),
    { id: "ts", title: "Marco de Wilhelmy", symbols: ["l", "d"] },
    ...HIDRO_MEDIDAS.map((k) => ({
      id: "ts",
      title: `Medida ${k} — pesa que sustituye a la pelicula`,
      symbols: [`mw${k}`, `dw${k}`],
      results: k === 3 ? ["gamma_medio"] : [],
    })),
  ],
};

export const SERIES_LIVE_COLUMNS = {
  "p2-cc": [{ symbol: "P", unit: "W", inputs: ["R", "I"] }],
  "fluidos-1": [
    { symbol: "Q_1", unit: "m3/s" },
    { symbol: "Q_2", unit: "m3/s" },
    { symbol: "Q_medio", unit: "m3/s" },
  ],
  viscosidad: [{ symbol: "v_medio", unit: "m/s" }],
};

export const PRACTICES_WITHOUT_CHRONO_HELPER = new Set(["p3-relajacion"]);

export const SYMBOL_FIRST_QUANTITIES = new Set([
  ...PRACTICE_SECTIONS["p2-cc"].filter((sec) => sec.id).flatMap((sec) => sec.symbols ?? []),
  "Vg", "VRpp", "VCpp", "VLpp", "a_R", "b_R", "a_C", "b_C", "a_L", "b_L",
]);
