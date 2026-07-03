// Grupos de prácticas hermanas: varias prácticas reales que el menú muestra como un solo ítem
// con tabs para saltar entre ellas (cada tab cambia de práctica y de entrega).
export const PRACTICE_GROUPS = {
  "p3-relajacion": { group: "p3", label: "Parte 1: Relajacion directa", order: 1 },
  "p3-relajacion-desfasaje": { group: "p3", label: "Parte 2: Desfasaje", order: 2 },
};

// Partes temáticas DENTRO de una misma práctica (una sola definición y una sola entrega):
// las tabs solo muestran/ocultan las secciones con data-section correspondiente.
export const PRACTICE_PARTS = {
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
    { title: "1) Determinación de períodos", symbols: ["T"] },
    { title: "2) Amortiguamiento (γ, Q)", symbols: ["t_med"] },
    { title: "3) Determinación de g", symbols: ["L"] },
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

// Resultados finales que se entregan SIN incertidumbre: el form y las tablas de análisis
// omiten el campo/columna U para estos símbolos.
export const RESULTS_WITHOUT_U = new Set(["P_max_e", "P_max_t", "RP_max_e", "RP_max_t"]);

// Columnas calculadas en vivo en la tabla de series (solo lectura, el cliente las computa al
// tipear): `inputs` son los símbolos de las columnas de entrada, en el orden que espera `fn`
// de lib.js (hoy solo pointPower).
export const SERIES_LIVE_COLUMNS = {
  "p2-cc": [{ symbol: "P", unit: "W", inputs: ["R", "I"] }],
};
