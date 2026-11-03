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
  "Vg_s", "RA_s", "VR1_s", "VR2_s", "VR3_s",
  "Vg_p", "RA_p", "VR1_p", "VR2_p", "VR3_p",
  "Vg_c", "RA_c",
  "Vg", "VRpp", "VCpp", "VLpp", "a_R", "b_R", "a_C", "b_C", "a_L", "b_L",
]);
