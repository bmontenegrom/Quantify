use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::window;

// ---------- LÓGICA NUMÉRICA / AUXILIAR ----------

/// Convierte milisegundos a "HH:MM:SS.mmm"
fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let millis = ms % 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

#[derive(Clone, Copy, PartialEq)]
struct Stats {
    mean: f64,
    std: f64,
    sn: f64,
    n: usize,
}

#[derive(Clone, PartialEq)]
struct HistData {
    min: f64,
    max: f64,
    bin_width: f64,
    counts: Vec<usize>,
    max_count: usize,
}

/// A partir de las marcas en ms devuelve períodos independientes en segundos:
/// (t1 - t0), (t3 - t2), ...
fn compute_periodos(marcas: &[u64]) -> Vec<f64> {
    let mut res = Vec::new();
    for par in marcas.chunks(2) {
        if par.len() == 2 {
            let dt_ms = par[1].saturating_sub(par[0]);
            let dt_s = dt_ms as f64 / 1000.0;
            res.push(dt_s);
        }
    }
    res
}

/// Estadísticos: media, desvío estándar muestral s, desvío estándar de la media Sn = s / √n
fn compute_stats(periodos_activos: &[f64]) -> Option<Stats> {
    if periodos_activos.is_empty() {
        return None;
    }

    let n = periodos_activos.len() as f64;
    let mean = periodos_activos.iter().sum::<f64>() / n;

    if n > 1.0 {
        let var = periodos_activos
            .iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>()
            / (n - 1.0);
        let std = var.sqrt();
        let sn = std / n.sqrt();
        Some(Stats {
            mean,
            std,
            sn,
            n: periodos_activos.len(),
        })
    } else {
        Some(Stats {
            mean: periodos_activos[0],
            std: 0.0,
            sn: 0.0,
            n: 1,
        })
    }
}

/// Calcula datos de histograma a partir de períodos activos.
/// Usa un número de bins adaptativo según la cantidad de datos.
fn compute_histogram(periodos_activos: &[f64]) -> Option<HistData> {
    let n = periodos_activos.len();
    if n < 5 {
        return None;
    }

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &p in periodos_activos {
        if p < min {
            min = p;
        }
        if p > max {
            max = p;
        }
    }

    if !min.is_finite() || !max.is_finite() || min == max {
        return None;
    }

    // número de bins ~ sqrt(n), acotado entre 4 y 12
    let mut bins = (n as f64).sqrt().round() as usize;
    if bins < 4 {
        bins = 4;
    }
    if bins > 12 {
        bins = 12;
    }

    let width = max - min;
    let bin_width = width / bins as f64;
    let mut counts = vec![0usize; bins];

    for &p in periodos_activos {
        let mut idx = ((p - min) / bin_width) as usize;
        if idx >= bins {
            idx = bins - 1;
        }
        counts[idx] += 1;
    }

    let max_count = counts.iter().copied().max().unwrap_or(1);

    Some(HistData {
        min,
        max,
        bin_width,
        counts,
        max_count,
    })
}

/// Aproximación de erf(x)
fn erf_approx(x: f64) -> f64 {
    // Aproximación razonable para fines docentes
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let a = 0.147;
    let term = 1.0 + a * x * x;
    let num = 4.0 / std::f64::consts::PI + a * x * x;
    let inner = -x * x * num / term;
    let erf = 1.0 - (-inner).exp().sqrt();
    sign * erf
}

/// CDF de N(mean, std^2)
fn normal_cdf(x: f64, mean: f64, std: f64) -> f64 {
    if std <= 0.0 {
        return 0.0;
    }
    let z = (x - mean) / (std * (2.0_f64).sqrt());
    0.5 * (1.0 + erf_approx(z))
}

/// PDF de una normal N(mean, std^2)
fn normal_pdf(x: f64, mean: f64, std: f64) -> f64 {
    if std <= 0.0 {
        return 0.0;
    }
    let z = (x - mean) / std;
    let norm = (2.0 * std::f64::consts::PI).sqrt() * std;
    (-0.5 * z * z).exp() / norm
}

/// Test de Kolmogorov–Smirnov para normalidad.
/// Devuelve (D, Dcrit, pasa?)
fn ks_test_normal(data: &[f64], mean: f64, std: f64) -> Option<(f64, f64, bool)> {
    if data.len() < 5 || std <= 0.0 {
        return None;
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let n = sorted.len() as f64;
    let mut d_max = 0.0;

    for (i, &x) in sorted.iter().enumerate() {
        let fi = (i as f64 + 1.0) / n;         // CDF empírica
        let f_theo = normal_cdf(x, mean, std); // CDF teórica normal
        let diff = (fi - f_theo).abs();
        if diff > d_max {
            d_max = diff;
        }
    }

    // valor crítico aproximado para α = 0.05
    let d_crit = 1.36 / n.sqrt();
    let passes = d_max < d_crit;
    Some((d_max, d_crit, passes))
}

// ---------- VISTAS AUXILIARES (UI PEQUEÑA) ----------

fn render_stats(stats: Option<Stats>) -> AnyView {
    if let Some(Stats { mean, std, sn, n }) = stats {
        view! {
            <div class="periodos-stats">
                <p>
                    "n = " {n}
                    ", ⟨T⟩ = " {format!("{:.4} s", mean)}
                </p>
                <p>
                    "Desvío estándar s = " {format!("{:.4} s", std)}
                </p>
                <p>
                    "Desvío estándar de la media Sₙ = " {format!("{:.4} s", sn)}
                </p>
            </div>
        }.into_any()
    } else {
        view! {
            <p>"Aún no hay suficientes datos para estadísticas."</p>
        }.into_any()
    }
}

fn render_histograma(stats: Option<Stats>, hist: Option<HistData>) -> AnyView {
    if let (Some(Stats { mean, std, .. }), Some(hist)) = (stats, hist) {
        let HistData {
            min,
            max,
            bin_width: _,
            counts,
            max_count,
        } = hist;

        let width = 400.0;
        let height = 200.0;
        let bins = counts.len();
        let bin_px = width / bins as f64;

        // puntos de la curva gaussiana
        let mut gauss_points = Vec::new();
        if std > 0.0 && max_count > 0 {
            let steps = 100;
            let max_pdf = normal_pdf(mean, mean, std);
            for i in 0..=steps {
                let x = min + (max - min) * (i as f64 / steps as f64);
                let pdf = normal_pdf(x, mean, std);
                let y_count_equiv =
                    if max_pdf > 0.0 {
                        pdf / max_pdf * (max_count as f64 * 0.9)
                    } else {
                        0.0
                    };
                let svg_x = ((x - min) / (max - min)) * width;
                let svg_y =
                    height - (y_count_equiv / max_count as f64) * (height - 20.0);
                gauss_points.push((svg_x, svg_y));
            }
        }

        view! {
            <svg
                width=width
                height=height
                style="border: 1px solid #ccc; background-color: #fafafa;"
            >
                // barras del histograma
                {
                    counts
                        .into_iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let x = i as f64 * bin_px;
                            let bar_h = if max_count > 0 {
                                (c as f64 / max_count as f64) * (height - 20.0)
                            } else {
                                0.0
                            };
                            let y = height - bar_h;
                            view! {
                                <rect
                                    x=x
                                    y=y
                                    width=bin_px - 2.0
                                    height=bar_h
                                    fill="#88c"
                                />
                            }
                        })
                        .collect_view()
                }

                // curva gaussiana
                {
                    let d = if !gauss_points.is_empty() {
                        let mut s = String::new();
                        for (i, (x, y)) in gauss_points.iter().enumerate() {
                            if i == 0 {
                                s.push_str(&format!("M {} {}", x, y));
                            } else {
                                s.push_str(&format!(" L {} {}", x, y));
                            }
                        }
                        s
                    } else {
                        String::new()
                    };

                    view! {
                        <path
                            d=d
                            fill="none"
                            stroke="#c44"
                            stroke-width="2"
                        />
                    }
                }
            </svg>
        }.into_any()
    } else {
        view! {
            <p>"Se necesitan al menos algunos datos activos para dibujar el histograma y la gaussiana."</p>
        }.into_any()
    }
}

fn render_normality_test(
    ks_res: Option<(f64, f64, bool)>,
    stats: Option<Stats>,
) -> AnyView {
    if let (Some((d, dcrit, passes)), Some(st)) = (ks_res, stats) {
        let n = st.n;
        view! {
            <div class="normality-test">
                <h5>"Test de normalidad KS (Kolmogorov–Smirnov, α = 0.05)"</h5>
                <p>
                    "D observado = "
                    {format!("{:.4}", d)}
                    ", D crítico = "
                    {format!("{:.4}", dcrit)}
                </p>
                {move || {
                    if passes {
                        view! {
                            <p style="color: green;">
                                "D < D₍crit₎ → No se rechaza la hipótesis de normalidad al 5%."
                            </p>
                        }.into_any()
                    } else {
                        view! {
                            <p style="color: red;">
                                "D ≥ D₍crit₎ → Se rechaza la hipótesis de normalidad al 5%."
                            </p>
                        }.into_any()
                    }
                }}

                <div class="normality-interpretation" style="margin-top: 0.5rem; font-size: 0.9rem;">
                    <h6>"Interpretación sugerida para el informe"</h6>
                    {move || {
                        if passes {
                            view! {
                                <p>
                                    {
                                        format!(
                                            "Se aplicó un test de Kolmogorov–Smirnov sobre los {} períodos no descartados. \
                                            El estadístico resultó D = {:.4}, menor que el valor crítico D₍crit₎ = {:.4} para α = 0.05. \
                                            Por lo tanto, no se rechaza la hipótesis de que los períodos medidos se distribuyen aproximadamente en forma normal. \
                                            Esto es consistente con la idea de que las fluctuaciones del período surgen de la superposición de múltiples fuentes de ruido independientes.",
                                            n, d, dcrit
                                        )
                                    }
                                </p>
                            }.into_any()
                        } else {
                            view! {
                                <p>
                                    {
                                        format!(
                                            "Se aplicó un test de Kolmogorov–Smirnov sobre los {} períodos no descartados. \
                                            El estadístico resultó D = {:.4}, mayor o igual que el valor crítico D₍crit₎ = {:.4} para α = 0.05. \
                                            En estas condiciones se rechaza la hipótesis de normalidad, lo que sugiere que la distribución experimental de los períodos \
                                            se desvía de una normal ideal. \
                                            Esto puede deberse a efectos sistemáticos en el montaje, presencia de outliers o a que la dispersión no está dominada solo por ruido aleatorio.",
                                            n, d, dcrit
                                        )
                                    }
                                </p>
                            }.into_any()
                        }
                    }}
                </div>
            </div>
        }.into_any()
    } else {
        view! {
            <div class="normality-test">
                <h5>"Test de normalidad KS"</h5>
                <p>
                    "No se pudo aplicar el test (muy pocos datos activos o dispersión prácticamente nula). \
                    Aumente el número de mediciones o revise el descarte de datos."
                </p>
            </div>
        }.into_any()
    }
}

// ---------- COMPONENTE PRINCIPAL ----------

#[component]
pub fn PracticaPendulo() -> impl IntoView {
    // --- Cronómetro ---
    let (elapsed_ms, set_elapsed_ms) = signal(0u64);
    let (running, set_running) = signal(false);

    // --- Marcas de tiempo (en ms) ---
    let (marcas, set_marcas) = signal(Vec::<u64>::new());

    // Intervalo: cada 10 ms, si running, incrementa elapsed_ms
    {
        let running_sig = running.clone();
        let elapsed_sig = set_elapsed_ms.clone();

        let cb = Closure::wrap(Box::new(move || {
            if running_sig.get_untracked() {
                elapsed_sig.update(|ms| *ms += 10);
            }
        }) as Box<dyn FnMut()>);

        window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                10,
            )
            .unwrap();

        cb.forget();
    }

    // --- Períodos independientes ---
    let periodos = Memo::new(move |_| compute_periodos(&marcas.get()));

    // --- Vector de "descartados" ---
    let (descartados, set_descartados) = signal(Vec::<bool>::new());

    // Sincronizar longitud de descartados con periodos
    Effect::new({
        let periodos = periodos.clone();
        let set_descartados = set_descartados.clone();
        move |_| {
            let len = periodos.get().len();
            set_descartados.update(|v| {
                if v.len() != len {
                    v.resize(len, false); // false = no descartado
                }
            });
        }
    });

    // --- Períodos activos (no descartados) ---
    let active_periods = Memo::new(move |_| {
        let ps = periodos.get();
        let ds = descartados.get();
        let mut res = Vec::new();
        for (i, p) in ps.into_iter().enumerate() {
            if ds.get(i).copied().unwrap_or(false) == false {
                res.push(p);
            }
        }
        res
    });

    // --- Estadísticas, histograma, KS ---
    let stats = Memo::new(move |_| compute_stats(&active_periods.get()));
    let hist_data = Memo::new(move |_| compute_histogram(&active_periods.get()));

    let ks_result = Memo::new(move |_| {
        let st_opt = stats.get();
        let acts = active_periods.get();
        if let Some(st) = st_opt {
            ks_test_normal(&acts, st.mean, st.std)
        } else {
            None
        }
    });

    // --- Parámetro de outliers: múltiplos de sigma ---
    let (sigma_threshold, set_sigma_threshold) = signal(3.0f64);

    // --- Flags de outlier (paralelo a periodos) ---
    let outliers = Memo::new(move |_| {
        let ps = periodos.get();
        let ds = descartados.get();
        let st_opt = stats.get();
        let sigma = sigma_threshold.get();

        let mut flags = vec![false; ps.len()];

        if let Some(st) = st_opt {
            if st.std > 0.0 && sigma > 0.0 {
                for (i, &p) in ps.iter().enumerate() {
                    if ds.get(i).copied().unwrap_or(false) {
                        continue;
                    }
                    let z = (p - st.mean) / st.std;
                    if z.abs() > sigma {
                        flags[i] = true;
                    }
                }
            }
        }

        flags
    });

    // --- Índices de períodos ordenados de menor a mayor ---
    let periodos_sorted_idx = Memo::new(move |_| {
        let ps = periodos.get();
        let mut idxs: Vec<usize> = (0..ps.len()).collect();
        idxs.sort_by(|&a, &b| ps[a].partial_cmp(&ps[b]).unwrap());
        idxs
    });

    // --- Handlers de cronómetro ---
    let on_start = move |_| set_running.set(true);
    let on_pause = move |_| set_running.set(false);
    let on_reset = move |_| {
        set_running.set(false);
        set_elapsed_ms.set(0);
        set_marcas.set(Vec::new());
        set_descartados.set(Vec::new());
    };

    // --- Registrar marca ---
    let on_registrar_marca = move |_| {
        let current = elapsed_ms.get_untracked();
        set_marcas.update(|v| {
            if v.len() < 200 {
                v.push(current);
            }
        });
    };

    // --- Botón "Descartar outliers" ---
    let outliers_for_button = outliers.clone();
    let on_discard_outliers = move |_| {
        let outs = outliers_for_button.get();
        set_descartados.update(|v| {
            for (i, is_out) in outs.iter().enumerate() {
                if *is_out && i < v.len() {
                    v[i] = true;
                }
            }
        });
    };

    view! {
        <div class="practica-pendulo">
            <h3>"Medición del período de un péndulo"</h3>

            // ---------- Cronómetro ----------
            <div class="cronometro-panel">
                <div class="cronometro-display">
                    {move || format_time(elapsed_ms.get())}
                </div>

                <div class="cronometro-buttons">
                    <button
                        on:click=on_pause
                        style=move || if running.get() { "" } else { "display:none" }
                    >
                        "Pausar"
                    </button>
                    <button
                        on:click=on_start
                        style=move || if running.get() { "display:none" } else { "" }
                    >
                        {move || if elapsed_ms.get() == 0 {
                            "Iniciar".to_string()
                        } else {
                            "Reanudar".to_string()
                        }}
                    </button>
                    <button on:click=on_reset>
                        "Reiniciar (tiempo y datos)"
                    </button>
                </div>
            </div>

            // ---------- Marcas ----------
            <div class="marcas-panel">
                <h4>
                    "Marcas de tiempo "
                    {"("} {move || marcas.get().len()} {"/ 200)"}
                </h4>

                <button on:click=on_registrar_marca>
                    "Registrar marca"
                </button>

                <table class="marcas-table">
                    <thead>
                        <tr>
                            <th>"#"</th>
                            <th>"Tiempo [ms]"</th>
                            <th>"Tiempo [s]"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || {
                                marcas
                                    .get()
                                    .into_iter()
                                    .enumerate()
                                    .collect::<Vec<_>>()
                            }
                            key=|item: &(usize, u64)| item.0
                            children=move |(i, t_ms): (usize, u64)| {
                                let t_s = t_ms as f64 / 1000.0;
                                view! {
                                    <tr>
                                        <td>{i}</td>
                                        <td>{t_ms}</td>
                                        <td>{format!("{:.4}", t_s)}</td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>
            </div>

            // ---------- Períodos ----------
            <div class="periodos-panel">
                <h4>
                    "Períodos independientes "
                    {"("} {move || periodos.get().len()} {")"}
                </h4>

                <div class="outlier-controls">
                    <span>"Umbral para outliers: "</span>
                    <button on:click=move |_| set_sigma_threshold.set(2.0)>"2σ"</button>
                    <button on:click=move |_| set_sigma_threshold.set(3.0)>"3σ"</button>
                    <span>
                        {move || format!(" (actual: {:.1}σ)", sigma_threshold.get())}
                    </span>
                    <button on:click=on_discard_outliers>
                        "Descartar outliers"
                    </button>
                </div>

                <table class="periodos-table">
                    <thead>
                        <tr>
                            <th>"# índice"</th>
                            <th>"T [s]"</th>
                            <th>"Estado"</th>
                            <th>"Acción"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            // usamos índices ordenados por período
                            each=move || periodos_sorted_idx.get()
                            key=|i: &usize| *i
                            children=move |i: usize| {
                                let desc_sig = descartados.clone();
                                let set_desc = set_descartados.clone();
                                let outliers_sig = outliers.clone();
                                let periodos_sig = periodos.clone();

                                view! {
                                    <tr
                                        class=move || {
                                            let is_out = outliers_sig
                                                .get()
                                                .get(i)
                                                .copied()
                                                .unwrap_or(false);
                                            if is_out { "outlier-row" } else { "" }
                                        }
                                    >
                                        <td>{i}</td>
                                        <td>{format!("{:.4}", periodos_sig.get()[i])}</td>
                                        <td>
                                            {move || {
                                                if desc_sig.get().get(i).copied().unwrap_or(false) {
                                                    "Descartado"
                                                } else {
                                                    "Usado"
                                                }
                                            }}
                                        </td>
                                        <td>
                                            <button
                                                on:click=move |_| {
                                                    set_desc.update(|v| {
                                                        if i < v.len() {
                                                            v[i] = !v[i];
                                                        }
                                                    });
                                                }
                                            >
                                                {move || {
                                                    if desc_sig.get().get(i).copied().unwrap_or(false) {
                                                        "Restaurar"
                                                    } else {
                                                        "Descartar"
                                                    }
                                                }}
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>

                {move || render_stats(stats.get())}
            </div>

            // ---------- Histograma + Gaussiana ----------
            <div class="histograma-panel">
                <h4>"Histograma de períodos (no descartados) y gaussiana estimada"</h4>
                {move || render_histograma(stats.get(), hist_data.get())}
            </div>

            // ---------- Test de normalidad KS ----------
            <div class="normality-panel">
                {move || render_normality_test(ks_result.get(), stats.get())}
            </div>
        </div>
    }
}
