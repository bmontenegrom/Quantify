use leptos::ev;
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use js_sys::Array;
use web_sys::{window, HtmlAnchorElement, HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};

// ─────────────────────────────────────────────────────────────
// Helpers para leer valores de inputs/select/textarea
// ─────────────────────────────────────────────────────────────

fn input_value(ev: &ev::Event) -> String {
    ev.target()
        .unwrap()
        .unchecked_into::<HtmlInputElement>()
        .value()
}

fn textarea_value(ev: &ev::Event) -> String {
    ev.target()
        .unwrap()
        .unchecked_into::<HtmlTextAreaElement>()
        .value()
}

fn select_value(ev: &ev::Event) -> String {
    ev.target()
        .unwrap()
        .unchecked_into::<HtmlSelectElement>()
        .value()
}

// ─────────────────────────────────────────────────────────────
// Tipos de dominio UI para instrumentos
// ─────────────────────────────────────────────────────────────

/// Tipo de instrumento: analógico o digital.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UiInstrumentKind {
    Analogico,
    Digital,
}

/// Magnitud física principal que mide la escala
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UiQuantity {
    Tension,
    Corriente,
    Resistencia,
    Capacitancia,
    Inductancia,
    Temperatura,
    Masa,
    Volumen,
    Tiempo,
    Densidad,
    Distancia,
}

/// Unidad básica (sin prefijo)
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UiUnit {
    Volt,
    Ampere,
    Ohm,
    Farad,
    Henry,
    Celsius,
    Kilogramo,
    Gramo,
    Litro,
    Metro,
    Segundo,
    KgPorMetroCubico,
}

/// Prefijo independiente de la unidad (m, µ, k, etc.)
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UiPrefix {
    Nano,
    Micro,
    Mili,
    Ninguno,
    Kilo,
    Mega,
}

impl UiPrefix {
    pub fn label(&self) -> &'static str {
        match self {
            UiPrefix::Nano => "n",
            UiPrefix::Micro => "µ",
            UiPrefix::Mili => "m",
            UiPrefix::Ninguno => "",
            UiPrefix::Kilo => "k",
            UiPrefix::Mega => "M",
        }
    }

    pub fn factor(&self) -> f64 {
        match self {
            UiPrefix::Nano => 1e-9,
            UiPrefix::Micro => 1e-6,
            UiPrefix::Mili => 1e-3,
            UiPrefix::Ninguno => 1.0,
            UiPrefix::Kilo => 1e3,
            UiPrefix::Mega => 1e6,
        }
    }
}

/// Modelo genérico de incertidumbre (simplificado para UI).
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UiUncertaintyKind {
    /// Incertidumbre combinada en lectura y calibración (en unidades de la escala)
    LecturaCalibracion,
    /// ±(p% del valor medido + d dígitos de resolución)
    PorcentajeMasDigitos,
    /// Incertidumbre absoluta fija (en unidades de la escala)
    AbsolutaFija,
}

#[derive(Clone, Serialize)]
pub struct UiScale {
    pub id: u32,
    pub label: String,
    pub quantity: UiQuantity,
    pub unit: UiUnit,
    pub prefix: UiPrefix,
    pub range_min: f64,
    pub range_max: f64,
    pub resolution: f64,
    pub uncertainty_kind: UiUncertaintyKind,
    /// Para Lectura/Calibración y AbsolutaFija
    pub lectura_abs: Option<f64>,
    pub calibracion_abs: Option<f64>,
    /// Para Porcentaje+Digitos
    pub porcentaje: Option<f64>,
    pub digitos: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct UiInstrument {
    pub id: u32,
    pub nombre: String,
    pub kind: UiInstrumentKind,
    pub notas: String,
    pub escalas: Vec<UiScale>,
}

/// Objeto raíz para exportar a JSON
#[derive(Serialize)]
pub struct UiInstrumentCollection {
    pub version: String,
    pub instrumentos: Vec<UiInstrument>,
}

// ─────────────────────────────────────────────────────────────
// Utilidades
// ─────────────────────────────────────────────────────────────

/// Descargar un JSON como archivo desde el WebView
fn download_json(filename: &str, contents: &str) {
    if let Some(win) = window() {
        if let Some(doc) = win.document() {
            // Creamos un Blob con el contenido
            let array = Array::new();
            array.push(&JsValue::from_str(contents));

            let blob = web_sys::Blob::new_with_str_sequence(&array)
                .expect("no se pudo crear Blob");

            let url = web_sys::Url::create_object_url_with_blob(&blob)
                .expect("no se pudo crear object URL");

            // Creamos un <a href="blob:..." download="..."> y lo "clickeamos"
            let a = doc
                .create_element("a")
                .expect("no se pudo crear <a>")
                .dyn_into::<HtmlAnchorElement>()
                .expect("no es un anchor");

            a.set_href(&url);
            a.set_download(filename);

            if let Some(body) = doc.body() {
                let _ = body.append_child(&a);
                a.click();
                let _ = body.remove_child(&a);
            }

            // Limpiar el object URL
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    }
}

/// Etiquetas amigables para unidades
fn unit_label(u: UiUnit) -> &'static str {
    match u {
        UiUnit::Volt => "V",
        UiUnit::Ampere => "A",
        UiUnit::Ohm => "Ω",
        UiUnit::Farad => "F",
        UiUnit::Henry => "H",
        UiUnit::Celsius => "°C",
        UiUnit::Kilogramo => "kg",
        UiUnit::Gramo => "g",
        UiUnit::Litro => "L",
        UiUnit::Metro => "m",
        UiUnit::Segundo => "s",
        UiUnit::KgPorMetroCubico => "kg/m³",
    }
}


fn default_unit_for_quantity(q: UiQuantity) -> UiUnit {
    match q {
        UiQuantity::Tension => UiUnit::Volt,
        UiQuantity::Corriente => UiUnit::Ampere,
        UiQuantity::Resistencia => UiUnit::Ohm,
        UiQuantity::Capacitancia => UiUnit::Farad,
        UiQuantity::Inductancia => UiUnit::Henry,
        UiQuantity::Temperatura => UiUnit::Celsius,
        UiQuantity::Masa => UiUnit::Gramo,
        UiQuantity::Volumen => UiUnit::Litro,
        UiQuantity::Tiempo => UiUnit::Segundo,
        UiQuantity::Densidad => UiUnit::KgPorMetroCubico,
        UiQuantity::Distancia => UiUnit::Metro,
    }
}

// ─────────────────────────────────────────────────────────────
// Componente principal: InstrumentosPanel
// ─────────────────────────────────────────────────────────────

#[component]
pub fn InstrumentosPanel() -> impl IntoView {
    // Lista de instrumentos definidos en la sesión
    let (instrumentos, set_instrumentos) = signal(Vec::<UiInstrument>::new());

    // Campos del formulario de "nuevo instrumento"
    let (nuevo_nombre, set_nuevo_nombre) = signal(String::new());
    let (nuevo_kind, set_nuevo_kind) = signal(UiInstrumentKind::Digital);
    let (nuevo_notas, set_nuevo_notas) = signal(String::new());

    // Escalas del instrumento en edición
    let (escala_label, set_escala_label) = signal(String::new());
    let (escala_quantity, set_escala_quantity) = signal(UiQuantity::Tension);
    let (escala_unit, set_escala_unit) = signal(UiUnit::Volt);
    let (escala_prefix, set_escala_prefix) = signal(UiPrefix::Ninguno);
    let (escala_min, set_escala_min) = signal(String::from("0.0"));
    let (escala_max, set_escala_max) = signal(String::from("10.0"));
    let (escala_res, set_escala_res) = signal(String::from("0.01"));
    let (escala_unc_kind, set_escala_unc_kind) =
        signal(UiUncertaintyKind::LecturaCalibracion);
    let (escala_lectura, set_escala_lectura) = signal(String::from("0.0"));
    let (escala_calibracion, set_escala_calibracion) = signal(String::from("0.0"));
    let (escala_porcentaje, set_escala_porcentaje) = signal(String::from("1.0"));
    let (escala_digitos, set_escala_digitos) = signal(String::from("1.0"));

    // Escalas ya agregadas al instrumento en edición
    let (escalas_en_edicion, set_escalas_en_edicion) = signal(Vec::<UiScale>::new());

    // Generador simple de IDs locales
    let (next_instrument_id, set_next_instrument_id) = signal(1u32);
    let (next_scale_id, set_next_scale_id) = signal(1u32);

    // ---- Handlers ----

    // Agregar una escala al instrumento "en edición"
    let on_add_scale = move |_| {
        let id = next_scale_id.get_untracked();
        set_next_scale_id.set(id + 1);

        let label = if !escala_label.get_untracked().is_empty() {
            escala_label.get_untracked()
        } else {
            // label por defecto tipo "0–10 V"
            format!(
                "{} – {} {}{}",
                escala_min.get_untracked(),
                escala_max.get_untracked(),
                escala_prefix.get_untracked().label(),
                unit_label(escala_unit.get_untracked())
            )
        };

        let range_min = escala_min.get_untracked().parse::<f64>().unwrap_or(0.0);
        let range_max = escala_max.get_untracked().parse::<f64>().unwrap_or(0.0);
        let resolution = escala_res.get_untracked().parse::<f64>().unwrap_or(0.0);

        let unc_kind = escala_unc_kind.get_untracked();
        let lectura = escala_lectura.get_untracked().parse::<f64>().ok();
        let calibracion = escala_calibracion.get_untracked().parse::<f64>().ok();
        let porcentaje = escala_porcentaje.get_untracked().parse::<f64>().ok();
        let digitos = escala_digitos.get_untracked().parse::<f64>().ok();

        let (lectura_abs, calibracion_abs, porcentaje_opt, digitos_opt) = match unc_kind {
            UiUncertaintyKind::LecturaCalibracion => (lectura, calibracion, None, None),
            UiUncertaintyKind::PorcentajeMasDigitos => (None, None, porcentaje, digitos),
            UiUncertaintyKind::AbsolutaFija => (lectura, None, None, None),
        };

        let scale = UiScale {
            id,
            label,
            quantity: escala_quantity.get_untracked(),
            unit: escala_unit.get_untracked(),
            prefix: escala_prefix.get_untracked(),
            range_min,
            range_max,
            resolution,
            uncertainty_kind: unc_kind,
            lectura_abs,
            calibracion_abs,
            porcentaje: porcentaje_opt,
            digitos: digitos_opt,
        };

        set_escalas_en_edicion.update(|v| v.push(scale));
        set_escala_label.set(String::new());
    };

    // Guardar el instrumento completo en la lista
    let on_save_instrument = move |_| {
        let nombre = nuevo_nombre.get_untracked().trim().to_string();
        if nombre.is_empty() {
            web_sys::console::warn_1(&"Nombre de instrumento vacío".into());
            return;
        }

        let id = next_instrument_id.get_untracked();
        set_next_instrument_id.set(id + 1);

        let inst = UiInstrument {
            id,
            nombre,
            kind: nuevo_kind.get_untracked(),
            notas: nuevo_notas.get_untracked(),
            escalas: escalas_en_edicion.get_untracked(),
        };

        set_instrumentos.update(|v| v.push(inst));

        // Reset de formulario
        set_nuevo_nombre.set(String::new());
        set_nuevo_notas.set(String::new());
        set_escalas_en_edicion.set(Vec::new());
    };

    // Exportar a JSON (todos los instrumentos en memoria)
    let on_export_json = move |_| {
        let snapshot = instrumentos.get_untracked();
        let collection = UiInstrumentCollection {
            version: "0.1.0".to_string(),
            instrumentos: snapshot,
        };

        match serde_json::to_string_pretty(&collection) {
            Ok(json_str) => {
                download_json("instrumentos.json", &json_str);
            }
            Err(e) => {
                web_sys::console::error_1(
                    &format!("Error serializando instrumentos: {}", e).into(),
                );
            }
        }
    };

    view! {
        <div class="instrumentos-panel">
            <div class="instrumentos-header">
                <h3>"Instrumentos"</h3>
                <p class="instrumentos-subtitle">
                    "Definí instrumentos y sus escalas (unidad, prefijo, rango, resolución, incertidumbre)."
                </p>
            </div>

            <div class="instrumentos-layout">
                // Columna izquierda: lista de instrumentos definidos
                <section class="instrumentos-list-section">
                    <div class="instrumentos-list-header">
                        <h4>"Instrumentos definidos"</h4>
                        <button class="export-button" on:click=on_export_json>
                            "Exportar JSON"
                        </button>
                    </div>

                    <div class="instrumentos-list">
                        <Show
                            when=move || !instrumentos.get().is_empty()
                            fallback=|| view! {
                                <p class="placeholder-note">
                                    "Todavía no hay instrumentos. Usá el formulario de la derecha para agregar uno."
                                </p>
                            }
                        >
                            <For
                                each=move || instrumentos.get()
                                key=|inst: &UiInstrument| inst.id
                                children=move |inst: UiInstrument| {
                                    view! {
                                        <div class="instrument-card">
                                            <div class="instrument-card-header">
                                                <span class="instrument-name">{inst.nombre.clone()}</span>
                                                <span class="instrument-kind">
                                                    {match inst.kind {
                                                        UiInstrumentKind::Analogico => "Analógico",
                                                        UiInstrumentKind::Digital => "Digital",
                                                    }}
                                                </span>
                                            </div>
                                            <div class="instrument-card-body">
                                                <p class="instrument-notes">
                                                    {if inst.notas.trim().is_empty() {
                                                        "Sin notas adicionales.".to_string()
                                                    } else {
                                                        inst.notas.clone()
                                                    }}
                                                </p>
                                                <p class="instrument-scales-title">
                                                    {format!("Escalas: {}", inst.escalas.len())}
                                                </p>
                                                <ul class="instrument-scales-list">
                                                    {inst.escalas.iter().map(|s| {
                                                        let label = &s.label;
                                                        let unit_str = format!(
                                                            "{}{}",
                                                            s.prefix.label(),
                                                            unit_label(s.unit)
                                                        );
                                                        view! {
                                                            <li>
                                                                <span class="scale-label">{label.clone()}</span>
                                                                <span class="scale-unit">{unit_str}</span>
                                                            </li>
                                                        }
                                                    }).collect_view()}
                                                </ul>
                                            </div>
                                        </div>
                                    }
                                }
                            />
                        </Show>
                    </div>
                </section>

                // Columna derecha: formulario de nuevo instrumento
                <section class="instrumentos-form-section">
                    <h4>"Nuevo instrumento"</h4>

                    <div class="instrument-form-group">
                        <label>"Nombre"</label>
                        <input
                            type="text"
                            placeholder="Ej: Tester digital Fluke 115"
                            value=move || nuevo_nombre.get()
                            on:input=move |ev| set_nuevo_nombre.set(input_value(&ev))
                        />
                    </div>

                    <div class="instrument-form-group">
                        <label>"Tipo"</label>
                        <select
                            on:change=move |ev| {
                                let v = select_value(&ev);
                                let kind = if v == "analogico" {
                                    UiInstrumentKind::Analogico
                                } else {
                                    UiInstrumentKind::Digital
                                };
                                set_nuevo_kind.set(kind);
                            }
                        >
                            <option value="digital" selected>{ "Digital" }</option>
                            <option value="analogico">{ "Analógico" }</option>
                        </select>
                    </div>

                    <div class="instrument-form-group">
                        <label>"Notas"</label>
                        <textarea
                        rows=3
                        placeholder="Comentarios, número de serie, estado, etc."
                        prop:value=move || nuevo_notas.get()
                        on:input=move |ev| set_nuevo_notas.set(textarea_value(&ev))
                    />
                    </div>

                    <hr class="instrument-divider" />

                    <h5>"Escalas del instrumento (en edición)"</h5>

                    <div class="scale-form-grid">
                        <div class="instrument-form-group">
                            <label>"Etiqueta de la escala"</label>
                            <input
                                type="text"
                                placeholder="Ej: 0–20 V"
                                value=move || escala_label.get()
                                on:input=move |ev| set_escala_label.set(input_value(&ev))
                            />
                        </div>

                        <div class="instrument-form-group">
                            <label>"Magnitud"</label>
                            <select
                                on:change=move |ev| {
                                    let v = select_value(&ev);
                                    let q = match v.as_str() {
                                        "corriente" => UiQuantity::Corriente,
                                        "resistencia" => UiQuantity::Resistencia,
                                        "capacitancia" => UiQuantity::Capacitancia,
                                        "inductancia" => UiQuantity::Inductancia,
                                        "temperatura" => UiQuantity::Temperatura,
                                        "masa" => UiQuantity::Masa,
                                        "volumen" => UiQuantity::Volumen,
                                        "tiempo" => UiQuantity::Tiempo,
                                        "densidad" => UiQuantity::Densidad,
                                        _ => UiQuantity::Tension,
                                    };
                                    let u = default_unit_for_quantity(q);
                                    set_escala_quantity.set(q);
                                    set_escala_unit.set(u);
                                }
                            >
                                <option value="tension" selected>{ "Tensión" }</option>
                                <option value="corriente">{ "Corriente" }</option>
                                <option value="resistencia">{ "Resistencia" }</option>
                                <option value="capacitancia">{ "Capacitancia" }</option>
                                <option value="inductancia">{ "Inductancia" }</option>
                                <option value="temperatura">{ "Temperatura" }</option>
                                <option value="masa">{ "Masa" }</option>
                                <option value="volumen">{ "Volumen" }</option>
                                <option value="tiempo">{ "Tiempo" }</option>
                                <option value="densidad">{ "Densidad" }</option>
                            </select>
                        </div>

                        <div class="instrument-form-group">
                            <label>"Unidad (fijada por la magnitud)"</label>
                            <div class="unit-display">
                                {move || unit_label(escala_unit.get()).to_string()}
                            </div>
                        </div>

                        <div class="instrument-form-group">
                            <label>"Prefijo"</label>
                            <select
                                on:change=move |ev| {
                                    let v = select_value(&ev);
                                    let p = match v.as_str() {
                                        "nano" => UiPrefix::Nano,
                                        "micro" => UiPrefix::Micro,
                                        "mili" => UiPrefix::Mili,
                                        "kilo" => UiPrefix::Kilo,
                                        "mega" => UiPrefix::Mega,
                                        _ => UiPrefix::Ninguno,
                                    };
                                    set_escala_prefix.set(p);
                                }
                            >
                                <option value="none" selected>{ " (sin prefijo)" }</option>
                                <option value="mili">{ "m" }</option>
                                <option value="micro">{ "µ" }</option>
                                <option value="nano">{ "n" }</option>
                                <option value="kilo">{ "k" }</option>
                                <option value="mega">{ "M" }</option>
                            </select>
                        </div>

                        <div class="instrument-form-group">
                            <label>"Rango mínimo"</label>
                            <input
                                type="text"
                                value=move || escala_min.get()
                                on:input=move |ev| set_escala_min.set(input_value(&ev))
                            />
                        </div>

                        <div class="instrument-form-group">
                            <label>"Rango máximo"</label>
                            <input
                                type="text"
                                value=move || escala_max.get()
                                on:input=move |ev| set_escala_max.set(input_value(&ev))
                            />
                        </div>

                        <div class="instrument-form-group">
                            <label>"Resolución"</label>
                            <input
                                type="text"
                                value=move || escala_res.get()
                                on:input=move |ev| set_escala_res.set(input_value(&ev))
                            />
                        </div>

                        <div class="instrument-form-group">
                            <label>"Modelo de incertidumbre"</label>
                            <select
                                on:change=move |ev| {
                                    let v = select_value(&ev);
                                    let k = match v.as_str() {
                                        "lectura_cal" => UiUncertaintyKind::LecturaCalibracion,
                                        "p_digitos" => UiUncertaintyKind::PorcentajeMasDigitos,  
                                        "absoluta" => UiUncertaintyKind::AbsolutaFija,
                                        _ => UiUncertaintyKind::LecturaCalibracion
                                    };
                                    set_escala_unc_kind.set(k);
                                }
                            >
                                <option value="lectura_cal">"Lectura + Calibración"</option>
                                <option value="p_digitos">"p% del valor + d dígitos"</option>
                                <option value="absoluta">"Incertidumbre absoluta fija"</option>
                            </select>
                        </div>

                        // Campos específicos según modelo de incertidumbre
                        <Show
                            when=move || escala_unc_kind.get() == UiUncertaintyKind::LecturaCalibracion
                        >
                            <div class="instrument-form-group">
                                <label>"Incert. lectura (abs)"</label>
                                <input
                                    type="text"
                                    value=move || escala_lectura.get()
                                    on:input=move |ev| set_escala_lectura.set(input_value(&ev))
                                />
                            </div>
                            <div class="instrument-form-group">
                                <label>"Incert. calibración (abs)"</label>
                                <input
                                    type="text"
                                    value=move || escala_calibracion.get()
                                    on:input=move |ev| set_escala_calibracion.set(input_value(&ev))
                                />
                            </div>
                        </Show>

                        <Show
                            when=move || escala_unc_kind.get() == UiUncertaintyKind::PorcentajeMasDigitos
                        >
                            <div class="instrument-form-group">
                                <label>"p (%) – porcentaje del valor medido"</label>
                                <input
                                    type="text"
                                    placeholder="Ej: 1.0 para ±1%"
                                    value=move || escala_porcentaje.get()
                                    on:input=move |ev| set_escala_porcentaje.set(input_value(&ev))
                                />
                            </div>
                            <div class="instrument-form-group">
                                <label>"d (dígitos) – múltiplos de la resolución"</label>
                                <input
                                    type="text"
                                    placeholder="Ej: 5 para ±5 dígitos"
                                    value=move || escala_digitos.get()
                                    on:input=move |ev| set_escala_digitos.set(input_value(&ev))
                                />
                            </div>
                        </Show>

                        <Show
                            when=move || escala_unc_kind.get() == UiUncertaintyKind::AbsolutaFija
                        >
                            <div class="instrument-form-group">
                                <label>"Incertidumbre absoluta"</label>
                                <input
                                    type="text"
                                    value=move || escala_lectura.get()
                                    on:input=move |ev| set_escala_lectura.set(input_value(&ev))
                                />
                            </div>
                        </Show>
                    </div>

                    <div class="instrument-actions">
                        <button class="secondary-button" on:click=on_add_scale>
                            "Agregar escala"
                        </button>
                    </div>

                    <Show
                        when=move || !escalas_en_edicion.get().is_empty()
                    >
                        <div class="escalas-preview">
                            <h6>"Escalas agregadas a este instrumento"</h6>
                            <ul>
                                <For
                                    each=move || escalas_en_edicion.get()
                                    key=|s: &UiScale| s.id
                                    children=move |s: UiScale| {
                                        let unit_str = format!(
                                            "{}{}",
                                            s.prefix.label(),
                                            unit_label(s.unit)
                                        );
                                        view! {
                                            <li>
                                                <span class="scale-label">{s.label.clone()}</span>
                                                <span class="scale-unit">{unit_str}</span>
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                        </div>
                    </Show>

                    <div class="instrument-actions">
                        <button class="primary-button" on:click=on_save_instrument>
                            "Guardar instrumento"
                        </button>
                    </div>
                </section>
            </div>
        </div>
    }
}
