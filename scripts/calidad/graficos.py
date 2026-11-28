import csv
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

CALIDAD = Path(__file__).resolve().parents[2] / "docs" / "calidad"

FIGURAS = {
    "tamano": ("Tamaño (líneas de código)", {
        "loc_rust": "Rust", "loc_js": "JS", "loc_css": "CSS", "loc_html": "HTML"}),
    "tests": ("Tests", {
        "tests_rust": "Rust", "tests_doctests": "Doctests", "tests_node": "node --test", "tests_e2e": "E2E"}),
    "cobertura": ("Cobertura (%)", {
        "cov_rust_lineas": "Rust líneas", "cov_rust_regiones": "Rust regiones",
        "cov_js_lineas": "JS líneas (cargados)", "sonar_coverage": "Global (Sonar)", "mutation_score": "Mutation score"}),
    "complejidad": ("Funciones sobre el umbral", {
        "rust_ciclo_gt10": "Rust ciclomática > 10", "rust_cogn_gt15": "Rust cognitiva > 15",
        "js_ciclo_gt10": "JS ciclomática > 10", "js_cogn_gt15": "JS cognitiva > 15"}),
    "lint": ("Avisos", {
        "clippy": "clippy", "clippy_pedantic": "clippy pedantic", "sonar_code_smells": "Sonar code smells"}),
    "ux": ("Lighthouse (mediana)", {
        "lh_perf_min": "Performance (peor página)", "lh_a11y_min": "Accesibilidad (peor página)"}),
}


def num(v):
    try:
        return float(v)
    except ValueError:
        return None


def main():
    with open(CALIDAD / "historial.csv", encoding="utf-8") as f:
        filas = list(csv.DictReader(f))
    etiquetas = [f"{r['fecha']}\n{r['commit']}" for r in filas]
    (CALIDAD / "figuras").mkdir(exist_ok=True)
    for nombre, (titulo, series) in FIGURAS.items():
        fig, ax = plt.subplots(figsize=(7, 3.5))
        for col, label in series.items():
            ys = [num(r.get(col, "")) for r in filas]
            if any(y is not None for y in ys):
                ax.plot(etiquetas, ys, marker="o", label=label)
        ax.set_title(titulo)
        ax.grid(alpha=0.3)
        ax.legend(fontsize=8)
        fig.tight_layout()
        fig.savefig(CALIDAD / "figuras" / f"{nombre}.png", dpi=120)
        plt.close(fig)


main()
