use leptos::prelude::*;
use crate::math::linear_regression::{linear_regression, LinearRegressionResult};

/// Panel de regresión lineal no ponderada:
/// - Recibe señales con los datos x e y
/// - Calcula la recta y = a x + b
/// - Muestra a, b, σ_a, σ_b, R² y n
/// - Dibuja un gráfico simple (puntos + recta)
#[component]
pub fn LinearRegressionPanel(
    /// Valores de x (en la misma unidad en la que querés ajustar)
    xs: Signal<Vec<f64>>,
    /// Valores de y (en la misma unidad)
    ys: Signal<Vec<f64>>,
    /// Título opcional a mostrar arriba del panel
    #[prop(optional)]
    title: Option<String>,
) -> impl IntoView {
    // Memo que recalcula la regresión cuando cambian xs o ys
    let result = Memo::new({
        let xs = xs.clone();
        let ys = ys.clone();
        move |_| {
            let vx = xs.get();
            let vy = ys.get();
            linear_regression(&vx, &vy)
        }
    });

    // Vista de resumen numérico
    let summary_view = move || render_regression_summary(result.get());

    // Vista de gráfico (puntos + recta)
    let plot_view = move || {
        let vx = xs.get();
        let vy = ys.get();
        let res = result.get();
        render_regression_plot(&vx, &vy, res)
    };

    view! {
        <div class="regression-panel">
            {
                if let Some(t) = title.clone() {
                    view! { <h4 class="regression-title">{t}</h4> }.into_any()
                } else {
                    view! { <h4 class="regression-title">"Regresión lineal"</h4> }.into_any()
                }
            }

            <div class="regression-content">
                <div class="regression-summary">
                    {summary_view}
                </div>
                <div class="regression-plot">
                    {plot_view}
                </div>
            </div>
        </div>
    }
}

/// Render de la parte numérica (a, b, σ, R², etc.)
fn render_regression_summary(res: Option<LinearRegressionResult>) -> AnyView {
    if let Some(r) = res {
        view! {
            <div class="regression-summary-box">
                <p>
                    "n = "
                    {r.n}
                </p>
                <p>
                    "a (pendiente) = "
                    {format!("{:.6}", r.slope)}
                    " ± "
                    {format!("{:.6}", r.slope_err)}
                </p>
                <p>
                    "b (ordenada) = "
                    {format!("{:.6}", r.intercept)}
                    " ± "
                    {format!("{:.6}", r.intercept_err)}
                </p>
                <p>
                    "R² = "
                    {format!("{:.6}", r.r2)}
                </p>
                <p class="regression-means">
                    "⟨x⟩ = "
                    {format!("{:.4}", r.x_mean)}
                    ", ⟨y⟩ = "
                    {format!("{:.4}", r.y_mean)}
                </p>
            </div>
        }.into_any()
    } else {
        view! {
            <div class="regression-summary-box">
                <p>
                    "No se pudo ajustar la recta. Revisá que haya al menos 2 puntos con valores de x distintos."
                </p>
            </div>
        }.into_any()
    }
}

/// Render del gráfico: puntos + recta ajustada.
/// Escala automáticamente el SVG al rango de datos.
fn render_regression_plot(
    xs: &[f64],
    ys: &[f64],
    res: Option<LinearRegressionResult>,
) -> AnyView {
    if xs.len() < 2 || ys.len() != xs.len() {
        return view! {
            <p class="regression-plot-placeholder">
                "Se necesitan al menos 2 puntos para dibujar el gráfico."
            </p>
        }.into_any();
    }

    // Encontrar min y max de x e y para escalar
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (&x, &y) in xs.iter().zip(ys.iter()) {
        if x < min_x { min_x = x; }
        if x > max_x { max_x = x; }
        if y < min_y { min_y = y; }
        if y > max_y { max_y = y; }
    }

    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite() {
        return view! {
            <p class="regression-plot-placeholder">
                "Datos no válidos para dibujar el gráfico."
            </p>
        }.into_any();
    }

    // Si todos los x o y son iguales, agregamos un pequeño padding
    if (max_x - min_x).abs() < 1e-12 {
        min_x -= 0.5;
        max_x += 0.5;
    }
    if (max_y - min_y).abs() < 1e-12 {
        min_y -= 0.5;
        max_y += 0.5;
    }

    let width = 360.0;
    let height = 240.0;
    let padding = 30.0;

    let plot_width = width - 2.0 * padding;
    let plot_height = height - 2.0 * padding;

    let dx = max_x - min_x;
    let dy = max_y - min_y;

    // Funciones para pasar de (x, y) de datos a coordenadas SVG
    let to_svg = |x: f64, y: f64| {
        let sx = padding + (x - min_x) / dx * plot_width;
        let sy = padding + plot_height - (y - min_y) / dy * plot_height;
        (sx, sy)
    };

    // Puntos como pequeños círculos
    let circles_view = xs
        .iter()
        .zip(ys.iter())
        .map(|(&x, &y)| {
            let (sx, sy) = to_svg(x, y);
            view! {
                <circle
                    cx=sx
                    cy=sy
                    r=3.0
                    fill="#66c"
                />
            }
        })
        .collect_view();

    // Recta ajustada, si existe
    let line_view = if let Some(r) = res {
        // Tomamos la recta en los extremos del rango de x
        let y1 = r.predict(min_x);
        let y2 = r.predict(max_x);
        let (x1_s, y1_s) = to_svg(min_x, y1);
        let (x2_s, y2_s) = to_svg(max_x, y2);

        view! {
            <line
                x1=x1_s
                y1=y1_s
                x2=x2_s
                y2=y2_s
                stroke="#e55"
                stroke-width="2"
            />
        }.into_any()
    } else {
        view! {}.into_any()
    };

    // Ejes simples (sin ticks para no complicar)
    let (x_axis_x1, x_axis_y1) = to_svg(min_x, min_y);
    let (x_axis_x2, x_axis_y2) = to_svg(max_x, min_y);
    let (y_axis_x1, y_axis_y1) = to_svg(min_x, min_y);
    let (y_axis_x2, y_axis_y2) = to_svg(min_x, max_y);

    view! {
        <svg
            class="regression-svg"
            width=width
            height=height
            style="border: 1px solid #444; background-color: #1b1b1b;"
        >
            // ejes
            <line
                x1=x_axis_x1
                y1=x_axis_y1
                x2=x_axis_x2
                y2=x_axis_y2
                stroke="#aaa"
                stroke-width="1"
            />
            <line
                x1=y_axis_x1
                y1=y_axis_y1
                x2=y_axis_x2
                y2=y_axis_y2
                stroke="#aaa"
                stroke-width="1"
            />

            // puntos
            {circles_view}

            // recta
            {line_view}
        </svg>
    }.into_any()
}
