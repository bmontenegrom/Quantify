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

/// Una serie de datos de la práctica del péndulo.
/// Cada serie tiene sus propias marcas y flags de descarte.
#[derive(Clone, PartialEq)]
struct SerieData {
    id: usize,
    label: String,
    marcas: Vec<u64>,
    descartados: Vec<bool>,
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
                style="border: 1px solid #333; background-color: #191919;"
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
                                    fill="#4f7cff"
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
                            stroke="#ff6b6b"
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
                            <p class="normality-pass">
                                "D < D₍crit₎ → No se rechaza la hipótesis de normalidad al 5%."
                            </p>
                        }.into_any()
                    } else {
                        view! {
                            <p class="normality-fail">
                                "D ≥ D₍crit₎ → Se rechaza la hipótesis de normalidad al 5%."
                            </p>
                        }.into_any()
                    }
                }}

                <div class="normality-interpretation">
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
    // --- Cronómetro (compartido entre series) ---
    let (elapsed_ms, set_elapsed_ms) = signal(0u64);
    let (running, set_running) = signal(false);

    // --- Series de datos ---
    let (series, set_series) = signal(vec![SerieData {
        id: 0,
        label: "Serie 1".to_string(),
        marcas: Vec::new(),
        descartados: Vec::new(),
    }]);

    // Índice de serie actual
    let (current_idx, set_current_idx) = signal(0usize);

    // --- Helper: marcas de la serie actual ---
    let marcas = Memo::new({
        let series = series.clone();
        let current_idx = current_idx.clone();
        move |_| {
            let idx = current_idx.get();
            series.with(|vec| {
                vec.get(idx)
                    .map(|s| s.marcas.clone())
                    .unwrap_or_default()
            })
        }
    });

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

    // --- Períodos independientes de la serie actual ---
    let periodos = Memo::new({
        let series = series.clone();
        let current_idx = current_idx.clone();
        move |_| {
            let idx = current_idx.get();
            series.with(|vec| {
                if let Some(s) = vec.get(idx) {
                    compute_periodos(&s.marcas)
                } else {
                    Vec::new()
                }
            })
        }
    });

    // Sincronizar longitud de descartados con los períodos de la serie actual
    Effect::new({
        let periodos = periodos.clone();
        let set_series = set_series.clone();
        let current_idx = current_idx.clone();
        move |_| {
            let len = periodos.get().len();
            let idx = current_idx.get();
            set_series.update(|vec| {
                if let Some(s) = vec.get_mut(idx) {
                    if s.descartados.len() != len {
                        s.descartados.resize(len, false); // false = no descartado
                    }
                }
            });
        }
    });

    // --- Períodos activos (no descartados) de la serie actual ---
    let active_periods = Memo::new({
        let series = series.clone();
        let current_idx = current_idx.clone();
        let periodos = periodos.clone();
        move |_| {
            let ps = periodos.get();
            let idx = current_idx.get();
            series.with(|vec| {
                if let Some(s) = vec.get(idx) {
                    let mut res = Vec::new();
                    for (i, p) in ps.into_iter().enumerate() {
                        if s.descartados.get(i).copied().unwrap_or(false) == false {
                            res.push(p);
                        }
                    }
                    res
                } else {
                    Vec::new()
                }
            })
        }
    });

    // --- Estadísticas, histograma, KS ---
    let stats = Memo::new({
        let active_periods = active_periods.clone();
        move |_| compute_stats(&active_periods.get())
    });

    let hist_data = Memo::new({
        let active_periods = active_periods.clone();
        move |_| compute_histogram(&active_periods.get())
    });

    let ks_result = Memo::new({
        let stats = stats.clone();
        let active_periods = active_periods.clone();
        move |_| {
            let st_opt = stats.get();
            let acts = active_periods.get();
            if let Some(st) = st_opt {
                ks_test_normal(&acts, st.mean, st.std)
            } else {
                None
            }
        }
    });

    // --- Parámetro de outliers: múltiplos de sigma ---
    let (sigma_threshold, set_sigma_threshold) = signal(3.0f64);

    // --- Flags de outlier (paralelo a periodos, serie actual) ---
    let outliers = Memo::new({
        let series = series.clone();
        let current_idx = current_idx.clone();
        let periodos = periodos.clone();
        let stats = stats.clone();
        let sigma_threshold = sigma_threshold.clone();
        move |_| {
            let ps = periodos.get();
            let st_opt = stats.get();
            let sigma = sigma_threshold.get();
            let idx = current_idx.get();

            series.with(|vec| {
                let mut flags = vec![false; ps.len()];
                if let (Some(s), Some(st)) = (vec.get(idx), st_opt) {
                    if st.std > 0.0 && sigma > 0.0 {
                        for (i, &p) in ps.iter().enumerate() {
                            if s.descartados.get(i).copied().unwrap_or(false) {
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
            })
        }
    });

    // --- Índices de períodos ordenados de menor a mayor (serie actual) ---
    let periodos_sorted_idx = Memo::new({
        let periodos = periodos.clone();
        move |_| {
            let ps = periodos.get();
            let mut idxs: Vec<usize> = (0..ps.len()).collect();
            idxs.sort_by(|&a, &b| ps[a].partial_cmp(&ps[b]).unwrap());
            idxs
        }
    });

    // --- Handlers de cronómetro ---
    let on_start = move |_| set_running.set(true);
    let on_pause = move |_| set_running.set(false);
    let on_reset = {
        let set_series = set_series.clone();
        let current_idx = current_idx.clone();
        move |_| {
            set_running.set(false);
            set_elapsed_ms.set(0);
            let idx = current_idx.get_untracked();
            set_series.update(|vec| {
                if let Some(s) = vec.get_mut(idx) {
                    s.marcas.clear();
                    s.descartados.clear();
                }
            });
        }
    };

    // --- Registrar marca en la serie actual (sin límite artificial) ---
    let on_registrar_marca = {
        let set_series = set_series.clone();
        let current_idx = current_idx.clone();
        move |_| {
            let current = elapsed_ms.get_untracked();
            let idx = current_idx.get_untracked();
            set_series.update(|vec| {
                if let Some(s) = vec.get_mut(idx) {
                    s.marcas.push(current);
                }
            });
        }
    };

    // --- Botón "Descartar outliers" para la serie actual ---
    let on_discard_outliers = {
        let outliers_for_button = outliers.clone();
        let set_series = set_series.clone();
        let current_idx = current_idx.clone();
        move |_| {
            let outs = outliers_for_button.get();
            let idx = current_idx.get_untracked();
            set_series.update(|vec| {
                if let Some(s) = vec.get_mut(idx) {
                    for (i, is_out) in outs.iter().enumerate() {
                        if *is_out && i < s.descartados.len() {
                            s.descartados[i] = true;
                        }
                    }
                }
            });
        }
    };

    // --- Agregar nueva serie ---
    let on_add_serie = {
        let series_for_add = series.clone();
        let set_series = set_series.clone();
        let set_current_idx = set_current_idx.clone();
        move |_| {
            let new_idx = series_for_add.get_untracked().len();
            set_series.update(|vec| {
                vec.push(SerieData {
                    id: new_idx,
                    label: format!("Serie {}", new_idx + 1),
                    marcas: Vec::new(),
                    descartados: Vec::new(),
                });
            });
            set_current_idx.set(new_idx);
        }
    };

    view! {
        <div class="practica-pendulo">
            <h3>"Medición del período de un péndulo"</h3>

            // ---------- Selector de series ----------
            <div class="series-selector">
                <span>"Series de datos: "</span>
                <For
                    each=move || series.get()
                    key=|s: &SerieData| s.id
                    children=move |s: SerieData| {
                        let current_idx = current_idx.clone();
                        let set_current_idx = set_current_idx.clone();
                        view! {
                            <button
                                class=move || {
                                    if current_idx.get() == s.id {
                                        "serie-button active"
                                    } else {
                                        "serie-button"
                                    }
                                }
                                on:click=move |_| set_current_idx.set(s.id)
                            >
                                {s.label.clone()}
                            </button>
                        }
                    }
                />
                <button class="serie-button add" on:click=on_add_serie>
                    "+ Serie"
                </button>
            </div>

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
                        "Reiniciar (tiempo y datos de la serie actual)"
                    </button>
                </div>
            </div>

            // ---------- Marcas (TIEMPOS EN SEGUNDOS, GRID MULTICOLUMNA) ----------
            <div class="marcas-panel">
                <h4>
                    "Marcas de tiempo "
                    {"("} {move || marcas.get().len()} {")"}
                </h4>

                <button on:click=on_registrar_marca>
                    "Registrar marca"
                </button>

                <div class="marcas-grid">
                    <For
                        each=move || {
                            marcas
                                .get()
                                .into_iter()
                                .enumerate()
                                .collect::<Vec<_>>()
                        }
                        key=|item: &(usize, u64)| item.0
                        children=move |(_i, t_ms): (usize, u64)| {
                            let t_s = t_ms as f64 / 1000.0;
                            view! {
                                <div class="marca-card">
                                    <span>{format!("{:.3} s", t_s)}</span>
                                </div>
                            }
                        }
                    />
                </div>
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

                // Grilla de períodos (sin índice; botón al costado del valor)
                <div class="periodos-grid">
                    <For
                        each=move || periodos_sorted_idx.get()
                        key=|i: &usize| *i
                        children=move |i: usize| {
                            let periodos_sig = periodos.clone();
                            let outliers_sig = outliers.clone();
                            let series_sig = series.clone();
                            let current_idx_sig = current_idx.clone();
                            let set_series_sig = set_series.clone();

                            view! {
                                <div
                                    class=move || {
                                        let is_out = outliers_sig
                                            .get()
                                            .get(i)
                                            .copied()
                                            .unwrap_or(false);

                                        let is_desc = series_sig.with(|vec| {
                                            let idx = current_idx_sig.get();
                                            vec.get(idx)
                                                .and_then(|s| s.descartados.get(i).copied())
                                                .unwrap_or(false)
                                        });

                                        if is_desc {
                                            "period-card discarded"
                                        } else if is_out {
                                            "period-card outlier"
                                        } else {
                                            "period-card"
                                        }
                                    }
                                >
                                    <div class="period-info">
                                        <div class="period-value">
                                            {move || format!("{:.4} s", periodos_sig.get()[i])}
                                        </div>
                                        <div class="period-status">
                                            {move || {
                                                series_sig.with(|vec| {
                                                    let idx = current_idx_sig.get();
                                                    if let Some(s) = vec.get(idx) {
                                                        if s.descartados.get(i).copied().unwrap_or(false) {
                                                            "Descartado".to_string()
                                                        } else {
                                                            "Usado".to_string()
                                                        }
                                                    } else {
                                                        "-".to_string()
                                                    }
                                                })
                                            }}
                                        </div>
                                    </div>
                                    <button
                                        class="period-toggle"
                                        on:click=move |_| {
                                            let idx_sel = current_idx_sig.get_untracked();
                                            set_series_sig.update(|vec| {
                                                if let Some(s) = vec.get_mut(idx_sel) {
                                                    if i < s.descartados.len() {
                                                        s.descartados[i] = !s.descartados[i];
                                                    }
                                                }
                                            });
                                        }
                                    >
                                        {move || {
                                            series_sig.with(|vec| {
                                                let idx = current_idx_sig.get();
                                                if let Some(s) = vec.get(idx) {
                                                    if s.descartados.get(i).copied().unwrap_or(false) {
                                                        "Restaurar".to_string()
                                                    } else {
                                                        "Descartar".to_string()
                                                    }
                                                } else {
                                                    "".to_string()
                                                }
                                            })
                                        }}
                                    </button>
                                </div>
                            }
                        }
                    />
                </div>

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
