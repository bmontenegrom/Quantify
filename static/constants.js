// Grupos de prácticas hermanas: varias prácticas reales que el menú muestra como un solo ítem
// con tabs para saltar entre ellas (cada tab cambia de práctica y de entrega).
export const PRACTICE_GROUPS = {
  "p3-relajacion": { group: "p3", label: "Parte 1: Relajacion directa", order: 1 },
  "p3-relajacion-desfasaje": { group: "p3", label: "Parte 2: Desfasaje", order: 2 },
};

// Partes temáticas DENTRO de una misma práctica (una sola definición y una sola entrega):
// las tabs solo muestran/ocultan las secciones con data-section correspondiente.
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
};

// Secciones temáticas del formulario. `symbols` agrupa magnitudes bajo un título; `results`
// asigna resultados finales a la sección; `series: true` marca dónde va la tabla por punto.
// `id` vincula la sección a una parte de PRACTICE_PARTS (sin `id` ⇒ siempre visible).
export const PRACTICE_SECTIONS = {
  "p1-estadistica": [
    { id: "op1", title: "Operador 1 — Períodos", symbols: ["T1"], results: ["g1"] },
    { id: "op2", title: "Operador 2 — Períodos (opcional)", symbols: ["T2"], results: ["g2"] },
    { id: "op3", title: "Operador 3 — Períodos (opcional)", symbols: ["T3"], results: ["g3"] },
    { title: "Datos compartidos", symbols: ["L", "t_med"], results: ["gamma", "Q"] },
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
};

// Columnas calculadas en vivo en la tabla de series (solo lectura, el cliente las computa al
// tipear): `inputs` son los símbolos de las columnas de entrada, en el orden que espera `fn`
// de lib.js (hoy solo pointPower).
export const SERIES_LIVE_COLUMNS = {
  "p2-cc": [{ symbol: "P", unit: "W", inputs: ["R", "I"] }],
};

// p2-cc: mismo orden que el "Resultado final" (símbolo primero, nombre como aclaración muted),
// para las magnitudes cuyo símbolo no es obvio a simple vista o que ya se comparan 1 a 1 con su
// teórica (VR1 medida vs VR1 teórica). Se derivan de las secciones por parte (serie/paralelo/
// potencia, identificadas por su `id`) en vez de mantener una lista aparte: son exactamente esas
// magnitudes, no las compartidas (Resistencias) que quedan fuera de una parte.
export const SYMBOL_FIRST_QUANTITIES = new Set(
  PRACTICE_SECTIONS["p2-cc"].filter((sec) => sec.id).flatMap((sec) => sec.symbols ?? []),
);
