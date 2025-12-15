use leptos::ev;
use leptos::prelude::*;
use serde::{Serialize, Deserialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use js_sys::Array;
use web_sys::{
    window, console, HtmlAnchorElement, HtmlInputElement, HtmlSelectElement,
    HtmlTextAreaElement,
};
use tauri_wasm::{self, args};

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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum UiInstrumentKind {
    Analogico,
    Digital,
}

/// Magnitud física principal que mide la escala
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
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
    Longitud,
}

/// Unidad básica (sin prefijo)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum UiUnit {
    Volt,
    Ampere,
    Ohm,
    Farad,
    Henry,
    Celsius,
    Gramo,              // masa en g como unidad base
    Litro,
    Metro,
    Segundo,
    KgPorMetroCubico,
}

/// Prefijo independiente de la unidad (m, µ, k, etc.)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
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

    /// Factor de conversión del prefijo a unidad base.
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum UiUncertaintyKind {
    /// Incertidumbre combinada en lectura y calibración (en unidades de la escala)
    LecturaCalibracion,
    /// ±(p% del valor medido + d dígitos de resolución)
    PorcentajeMasDigitos,
    /// Incertidumbre absoluta fija (en unidades de la escala)
    AbsolutaFija,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiScale {
    pub id: u32,
    pub label: String,
    pub quantity: UiQuantity,
    pub unit: UiUnit,
    pub prefix: UiPrefix,
    pub range_min: f64,
    pub range_max: f64,
    /// Resolución declarada en unidades con prefijo (ej: 0.01 V, 0.1 mA, etc.)
    pub resolution: f64,
    /// Resolución expresada en unidad base (aplicando el factor del prefijo)
    pub resolution_si: f64,
    pub uncertainty_kind: UiUncertaintyKind,
    /// Para Lectura/Calibración y AbsolutaFija
    pub lectura_abs: Option<f64>,
    pub calibracion_abs: Option<f64>,
    /// Para Porcentaje+Digitos
    pub porcentaje: Option<f64>,
    pub digitos: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiInstrument {
    pub id: u32,
    pub nombre: String,
    pub kind: UiInstrumentKind,
    pub notas: String,
    pub escalas: Vec<UiScale>,
}

/// Objeto raíz para exportar a JSON manualmente
#[derive(Debug, Serialize, Deserialize)]
pub struct UiInstrumentCollection {
    pub version: String,
    pub instrumentos: Vec<UiInstrument>,
}

/// Estructura para persistencia en archivo (incluye contadores)
#[derive(Clone, Debug, Serialize, Deserialize)]
struct InstrumentStore {
    instrumentos: Vec<UiInstrument>,
    next_instrument_id: u32,
    next_scale_id: u32,
}

// ─────────────────────────────────────────────────────────────
// Utilidades varias
// ─────────────────────────────────────────────────────────────

/// Descargar un JSON como archivo desde el WebView (export manual)
fn download_json(filename: &str, contents: &str) {
    if let Some(win) = window() {
        if let Some(doc) = win.document() {
            let array = Array::new();
            array.push(&JsValue::from_str(contents));

            let blob = web_sys::Blob::new_with_str_sequence(&array)
                .expect("no se pudo crear Blob");

            let url = web_sys::Url::create_object_url_with_blob(&blob)
                .expect("no se pudo crear object URL");

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
        UiUnit::Gramo => "g",
        UiUnit::Litro => "L",
        UiUnit::Metro => "m",
        UiUnit::Segundo => "s",
        UiUnit::KgPorMetroCubico => "kg/m³",
    }
}

/// Unidad base por defecto para cada magnitud
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
        UiQuantity::Longitud => UiUnit::Metro,
    }
}

// ─────────────────────────────────────────────────────────────
// Helpers de UI (sub-vistas)
// ─────────────────────────────────────────────────────────────

fn render_header() -> impl IntoView {
    view! {
        <div class="instrumentos-header">
            <h3>"Instrumentos"</h3>
            <p class="instrumentos-subtitle">
                "Definí instrumentos y sus escalas (unidad, prefijo, rango, resolución, incertidumbre)."
            </p>
        </div>
    }
}

/// Sección de lista de instrumentos + export + eliminar + modificar
fn render_list_section(
    instrumentos: ReadSignal<Vec<UiInstrument>>,
    set_instrumentos: WriteSignal<Vec<UiInstrument>>,
    set_nuevo_nombre: WriteSignal<String>,
    set_nuevo_kind: WriteSignal<UiInstrumentKind>,
    set_nuevo_notas: WriteSignal<String>,
    set_escalas_en_edicion: WriteSignal<Vec<UiScale>>,
    set_escala_quantity: WriteSignal<UiQuantity>,
    set_escala_unit: WriteSignal<UiUnit>,
    set_escala_prefix: WriteSignal<UiPrefix>,
    set_escala_min: WriteSignal<String>,
    set_escala_max: WriteSignal<String>,
    set_escala_res: WriteSignal<String>,
    set_escala_unc_kind: WriteSignal<UiUncertaintyKind>,
    set_escala_lectura: WriteSignal<String>,
    set_escala_calibracion: WriteSignal<String>,
    set_escala_porcentaje: WriteSignal<String>,
    set_escala_digitos: WriteSignal<String>,
    set_editing_instrument_id: WriteSignal<Option<u32>>,
) -> impl IntoView {
    // Exportar JSON manualmente (colección completa)
    let on_export_json = move |_| {
        let snapshot = instrumentos.get_untracked();
        console::log_1(&format!("Exportando instrumentos: {:?}", snapshot).into());

        let collection = UiInstrumentCollection {
            version: "0.1.0".to_string(),
            instrumentos: snapshot,
        };

        match serde_json::to_string_pretty(&collection) {
            Ok(json_str) => {
                download_json("instrumentos.json", &json_str);
            }
            Err(e) => {
                console::error_1(
                    &format!("Error serializando instrumentos: {}", e).into(),
                );
            }
        }
    };

    view! {
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
                            let set_instrumentos = set_instrumentos.clone();
                            let set_nuevo_nombre = set_nuevo_nombre.clone();
                            let set_nuevo_kind = set_nuevo_kind.clone();
                            let set_nuevo_notas = set_nuevo_notas.clone();
                            let set_escalas_en_edicion = set_escalas_en_edicion.clone();
                            let set_escala_quantity = set_escala_quantity.clone();
                            let set_escala_unit = set_escala_unit.clone();
                            let set_escala_prefix = set_escala_prefix.clone();
                            let set_escala_min = set_escala_min.clone();
                            let set_escala_max = set_escala_max.clone();
                            let set_escala_res = set_escala_res.clone();
                            let set_escala_unc_kind = set_escala_unc_kind.clone();
                            let set_escala_lectura = set_escala_lectura.clone();
                            let set_escala_calibracion = set_escala_calibracion.clone();
                            let set_escala_porcentaje = set_escala_porcentaje.clone();
                            let set_escala_digitos = set_escala_digitos.clone();
                            let set_editing_instrument_id = set_editing_instrument_id.clone();

                            let inst_id = inst.id;

                            // Eliminar con confirmación
                            let on_delete = move |_| {
                                console::log_1(
                                    &format!("Intentando eliminar instrumento id={}", inst_id).into()
                                );

                                let confirmed: bool;

                                if let Some(win) = window() {
                                    match win.confirm_with_message(
                                        "¿Eliminar este instrumento? Esta acción no se puede deshacer.",
                                    ) {
                                        Ok(c) => { confirmed = c; }
                                        Err(e) => {
                                            console::error_1(
                                                &format!("Error mostrando confirm(): {:?}", e).into(),
                                            );
                                            confirmed = false;
                                        }
                                    }
                                } else {
                                    console::warn_1(
                                        &"window() es None, no se puede mostrar confirm(). No se elimina."
                                            .into(),
                                    );
                                    confirmed = false;
                                }

                                if !confirmed {
                                    console::log_1(
                                        &"El usuario canceló la eliminación del instrumento."
                                            .into(),
                                    );
                                    return;
                                }

                                set_instrumentos.update(|lista| {
                                    lista.retain(|i| i.id != inst_id);
                                });

                                // si se estaba editando este instrumento, salir del modo edición
                                set_editing_instrument_id.set(None);
                                set_escalas_en_edicion.set(Vec::new());
                            };

                            // Modificar: carga datos al formulario
                            let inst_clone = inst.clone();
                            let on_edit = move |_| {
                                console::log_1(
                                    &format!("Editando instrumento: {:?}", inst_clone).into(),
                                );

                                let i = inst_clone.clone();
                                set_editing_instrument_id.set(Some(i.id));

                                set_nuevo_nombre.set(i.nombre.clone());
                                set_nuevo_kind.set(i.kind);
                                set_nuevo_notas.set(i.notas.clone());
                                set_escalas_en_edicion.set(i.escalas.clone());

                                // si hay al menos una escala, la usamos para precargar el form de escala
                                if let Some(first) = i.escalas.first() {
                                    set_escala_quantity.set(first.quantity);
                                    set_escala_unit.set(first.unit);
                                    set_escala_prefix.set(first.prefix);
                                    set_escala_min.set(first.range_min.to_string());
                                    set_escala_max.set(first.range_max.to_string());
                                    set_escala_res.set(first.resolution.to_string());
                                    set_escala_unc_kind.set(first.uncertainty_kind);

                                    match first.uncertainty_kind {
                                        UiUncertaintyKind::LecturaCalibracion => {
                                            set_escala_lectura.set(
                                                first
                                                    .lectura_abs
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_default(),
                                            );
                                            set_escala_calibracion.set(
                                                first
                                                    .calibracion_abs
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_default(),
                                            );
                                            set_escala_porcentaje.set(String::new());
                                            set_escala_digitos.set(String::new());
                                        }
                                        UiUncertaintyKind::PorcentajeMasDigitos => {
                                            set_escala_porcentaje.set(
                                                first
                                                    .porcentaje
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_default(),
                                            );
                                            set_escala_digitos.set(
                                                first
                                                    .digitos
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_default(),
                                            );
                                            set_escala_lectura.set(String::new());
                                            set_escala_calibracion.set(String::new());
                                        }
                                        UiUncertaintyKind::AbsolutaFija => {
                                            set_escala_lectura.set(
                                                first
                                                    .lectura_abs
                                                    .map(|v| v.to_string())
                                                    .unwrap_or_default(),
                                            );
                                            set_escala_calibracion.set(String::new());
                                            set_escala_porcentaje.set(String::new());
                                            set_escala_digitos.set(String::new());
                                        }
                                    }
                                }
                            };

                            view! {
                                <div class="instrument-card">
                                    <div class="instrument-card-header">
                                        <div class="instrument-card-title">
                                            <span class="instrument-name">
                                                {inst.nombre.clone()}
                                            </span>
                                            <span class="instrument-kind">
                                                {
                                                    match inst.kind {
                                                        UiInstrumentKind::Analogico => "Analógico",
                                                        UiInstrumentKind::Digital => "Digital",
                                                    }
                                                }
                                            </span>
                                        </div>
                                        <div class="instrument-card-actions">
                                            <button
                                                class="edit-button"
                                                title="Modificar instrumento"
                                                on:click=on_edit
                                            >
                                                "Modificar"
                                            </button>
                                            <button
                                                class="delete-button"
                                                title="Eliminar instrumento"
                                                on:click=on_delete
                                            >
                                                "Eliminar"
                                            </button>
                                        </div>
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
    }
}

#[allow(clippy::too_many_arguments)]
fn render_form_section(
    set_instrumentos: WriteSignal<Vec<UiInstrument>>,
    nuevo_nombre: ReadSignal<String>,
    set_nuevo_nombre: WriteSignal<String>,
    nuevo_kind: ReadSignal<UiInstrumentKind>,
    set_nuevo_kind: WriteSignal<UiInstrumentKind>,
    nuevo_notas: ReadSignal<String>,
    set_nuevo_notas: WriteSignal<String>,
    escala_label: ReadSignal<String>,
    set_escala_label: WriteSignal<String>,
    escala_quantity: ReadSignal<UiQuantity>,
    set_escala_quantity: WriteSignal<UiQuantity>,
    escala_unit: ReadSignal<UiUnit>,
    set_escala_unit: WriteSignal<UiUnit>,
    escala_prefix: ReadSignal<UiPrefix>,
    set_escala_prefix: WriteSignal<UiPrefix>,
    escala_min: ReadSignal<String>,
    set_escala_min: WriteSignal<String>,
    escala_max: ReadSignal<String>,
    set_escala_max: WriteSignal<String>,
    escala_res: ReadSignal<String>,
    set_escala_res: WriteSignal<String>,
    escala_unc_kind: ReadSignal<UiUncertaintyKind>,
    set_escala_unc_kind: WriteSignal<UiUncertaintyKind>,
    escala_lectura: ReadSignal<String>,
    set_escala_lectura: WriteSignal<String>,
    escala_calibracion: ReadSignal<String>,
    set_escala_calibracion: WriteSignal<String>,
    escala_porcentaje: ReadSignal<String>,
    set_escala_porcentaje: WriteSignal<String>,
    escala_digitos: ReadSignal<String>,
    set_escala_digitos: WriteSignal<String>,
    escalas_en_edicion: ReadSignal<Vec<UiScale>>,
    set_escalas_en_edicion: WriteSignal<Vec<UiScale>>,
    next_instrument_id: ReadSignal<u32>,
    set_next_instrument_id: WriteSignal<u32>,
    next_scale_id: ReadSignal<u32>,
    set_next_scale_id: WriteSignal<u32>,
    editing_instrument_id: ReadSignal<Option<u32>>,
    set_editing_instrument_id: WriteSignal<Option<u32>>,
) -> impl IntoView {
    // Handler: agregar escala al instrumento en edición
    let on_add_scale = {
        let escala_label = escala_label.clone();
        let escala_min = escala_min.clone();
        let escala_max = escala_max.clone();
        let escala_res = escala_res.clone();
        let escala_prefix = escala_prefix.clone();
        let escala_unit = escala_unit.clone();
        let escala_quantity = escala_quantity.clone();
        let escala_unc_kind = escala_unc_kind.clone();
        let escala_lectura = escala_lectura.clone();
        let escala_calibracion = escala_calibracion.clone();
        let escala_porcentaje = escala_porcentaje.clone();
        let escala_digitos = escala_digitos.clone();
        let set_escala_label = set_escala_label.clone();
        let set_escalas_en_edicion = set_escalas_en_edicion.clone();
        let set_next_scale_id = set_next_scale_id.clone();

        move |_| {
            let id = next_scale_id.get_untracked();
            set_next_scale_id.set(id + 1);

            let prefix = escala_prefix.get_untracked();
            let unit = escala_unit.get_untracked();

            let label = if !escala_label.get_untracked().is_empty() {
                escala_label.get_untracked()
            } else {
                format!(
                    "{} – {} {}{}",
                    escala_min.get_untracked(),
                    escala_max.get_untracked(),
                    prefix.label(),
                    unit_label(unit)
                )
            };

            let range_min = escala_min.get_untracked().parse::<f64>().unwrap_or(0.0);
            let range_max = escala_max.get_untracked().parse::<f64>().unwrap_or(0.0);
            let resolution = escala_res.get_untracked().parse::<f64>().unwrap_or(0.0);

            let factor = prefix.factor();
            let resolution_si = resolution * factor;

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
                unit,
                prefix,
                range_min,
                range_max,
                resolution,
                resolution_si,
                uncertainty_kind: unc_kind,
                lectura_abs,
                calibracion_abs,
                porcentaje: porcentaje_opt,
                digitos: digitos_opt,
            };

            set_escalas_en_edicion.update(|v| v.push(scale));
            set_escala_label.set(String::new());
        }
    };

    // Handler: guardar instrumento (nuevo o modificación)
    let on_save_instrument = move |_| {
        let nombre = nuevo_nombre.get_untracked().trim().to_string();
        if nombre.is_empty() {
            console::warn_1(&"Nombre de instrumento vacío".into());
            return;
        }

        let editing = editing_instrument_id.get_untracked();
        console::log_1(
            &format!("Guardando instrumento. editing = {:?}", editing).into(),
        );

        if let Some(edit_id) = editing {
            // Actualizar instrumento existente
            let nuevo_nombre_val = nombre.clone();
            let nuevo_kind_val = nuevo_kind.get_untracked();
            let nuevo_notas_val = nuevo_notas.get_untracked();
            let nuevas_escalas = escalas_en_edicion.get_untracked();

            set_instrumentos.update(|lista| {
                if let Some(inst) = lista.iter_mut().find(|i| i.id == edit_id) {
                    inst.nombre = nuevo_nombre_val.clone();
                    inst.kind = nuevo_kind_val;
                    inst.notas = nuevo_notas_val.clone();
                    inst.escalas = nuevas_escalas.clone();
                }
            });
        } else {
            // Crear instrumento nuevo
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
        }

        // Resetear formulario y salir del modo edición
        set_nuevo_nombre.set(String::new());
        set_nuevo_notas.set(String::new());
        set_escalas_en_edicion.set(Vec::new());
        set_editing_instrument_id.set(None);
    };

    view! {
        <section class="instrumentos-form-section">
            <Show
                when=move || editing_instrument_id.get().is_some()
                fallback=|| view! { <h4>"Nuevo instrumento"</h4> }
            >
                <h4>"Modificar instrumento"</h4>
            </Show>

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
                    <option value="digital" selected={move || nuevo_kind.get() == UiInstrumentKind::Digital}>
                        { "Digital" }
                    </option>
                    <option value="analogico" selected={move || nuevo_kind.get() == UiInstrumentKind::Analogico}>
                        { "Analógico" }
                    </option>
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
                                "longitud" => UiQuantity::Longitud,
                                _ => UiQuantity::Tension,
                            };
                            let u = default_unit_for_quantity(q);
                            set_escala_quantity.set(q);
                            set_escala_unit.set(u);
                        }
                    >
                        <option
                            value="tension"
                            selected={move || escala_quantity.get() == UiQuantity::Tension}
                        >
                            { "Tensión" }
                        </option>
                        <option
                            value="corriente"
                            selected={move || escala_quantity.get() == UiQuantity::Corriente}
                        >
                            { "Corriente" }
                        </option>
                        <option
                            value="resistencia"
                            selected={move || escala_quantity.get() == UiQuantity::Resistencia}
                        >
                            { "Resistencia" }
                        </option>
                        <option
                            value="capacitancia"
                            selected={move || escala_quantity.get() == UiQuantity::Capacitancia}
                        >
                            { "Capacitancia" }
                        </option>
                        <option
                            value="inductancia"
                            selected={move || escala_quantity.get() == UiQuantity::Inductancia}
                        >
                            { "Inductancia" }
                        </option>
                        <option
                            value="temperatura"
                            selected={move || escala_quantity.get() == UiQuantity::Temperatura}
                        >
                            { "Temperatura" }
                        </option>
                        <option
                            value="masa"
                            selected={move || escala_quantity.get() == UiQuantity::Masa}
                        >
                            { "Masa" }
                        </option>
                        <option
                            value="volumen"
                            selected={move || escala_quantity.get() == UiQuantity::Volumen}
                        >
                            { "Volumen" }
                        </option>
                        <option
                            value="tiempo"
                            selected={move || escala_quantity.get() == UiQuantity::Tiempo}
                        >
                            { "Tiempo" }
                        </option>
                        <option
                            value="densidad"
                            selected={move || escala_quantity.get() == UiQuantity::Densidad}
                        >
                            { "Densidad" }
                        </option>
                        <option
                            value="longitud"
                            selected={move || escala_quantity.get() == UiQuantity::Longitud}
                        >
                            { "Longitud" }
                        </option>
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
                        <option
                            value="none"
                            selected={move || escala_prefix.get() == UiPrefix::Ninguno}
                        >
                            { " (sin prefijo)" }
                        </option>
                        <option
                            value="mili"
                            selected={move || escala_prefix.get() == UiPrefix::Mili}
                        >
                            { "m" }
                        </option>
                        <option
                            value="micro"
                            selected={move || escala_prefix.get() == UiPrefix::Micro}
                        >
                            { "µ" }
                        </option>
                        <option
                            value="nano"
                            selected={move || escala_prefix.get() == UiPrefix::Nano}
                        >
                            { "n" }
                        </option>
                        <option
                            value="kilo"
                            selected={move || escala_prefix.get() == UiPrefix::Kilo}
                        >
                            { "k" }
                        </option>
                        <option
                            value="mega"
                            selected={move || escala_prefix.get() == UiPrefix::Mega}
                        >
                            { "M" }
                        </option>
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
                    <label>"Resolución (en unidad con prefijo)"</label>
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
                                "p_digitos" => UiUncertaintyKind::PorcentajeMasDigitos,
                                "absoluta" => UiUncertaintyKind::AbsolutaFija,
                                _ => UiUncertaintyKind::LecturaCalibracion,
                            };
                            set_escala_unc_kind.set(k);
                        }
                    >
                        <option
                            value="lectura_cal"
                            selected={move || escala_unc_kind.get() == UiUncertaintyKind::LecturaCalibracion}
                        >
                            "Lectura + Calibración (absolutas)"
                        </option>
                        <option
                            value="p_digitos"
                            selected={move || escala_unc_kind.get() == UiUncertaintyKind::PorcentajeMasDigitos}
                        >
                            "±(p% del valor + d dígitos)"
                        </option>
                        <option
                            value="absoluta"
                            selected={move || escala_unc_kind.get() == UiUncertaintyKind::AbsolutaFija}
                        >
                            "Absoluta fija"
                        </option>
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
                        <label>"p (%) — porcentaje del valor medido"</label>
                        <input
                            type="number"
                            step="0.1"
                            placeholder="Ej: 1.0 para ±1%"
                            value=move || escala_porcentaje.get()
                            on:input=move |ev| set_escala_porcentaje.set(input_value(&ev))
                        />
                    </div>
                    <div class="instrument-form-group">
                        <label>"d (dígitos) — múltiplos de la resolución"</label>
                        <input
                            type="number"
                            step="1"
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
                    {move || {
                        if editing_instrument_id.get().is_some() {
                            "Guardar cambios".to_string()
                        } else {
                            "Guardar instrumento".to_string()
                        }
                    }}
                </button>
            </div>
        </section>
    }
}

// ─────────────────────────────────────────────────────────────
// Componente principal
// ─────────────────────────────────────────────────────────────

#[component]
pub fn InstrumentosPanel() -> impl IntoView {
    // Lista de instrumentos definidos en la sesión
    let (instrumentos, set_instrumentos) = signal(Vec::<UiInstrument>::new());

    // Campos del formulario de "nuevo instrumento" / edición
    let (nuevo_nombre, set_nuevo_nombre) = signal(String::new());
    let (nuevo_kind, set_nuevo_kind) = signal(UiInstrumentKind::Digital);
    let (nuevo_notas, set_nuevo_notas) = signal(String::new());

    // Escalas del instrumento en edición
    let (escala_label, set_escala_label) = signal(String::new());
    let (escala_quantity, set_escala_quantity) = signal(UiQuantity::Tension);
    let (escala_unit, set_escala_unit) =
        signal(default_unit_for_quantity(UiQuantity::Tension));
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

    // Modo edición: id del instrumento que se está editando (o None)
    let (editing_instrument_id, set_editing_instrument_id) = signal::<Option<u32>>(None);

    // -------- CARGA INICIAL DESDE TAURI (archivo instrumentos.json) --------
    {
        let instrumentos = instrumentos.clone();
        let set_instrumentos = set_instrumentos.clone();
        let set_next_instrument_id = set_next_instrument_id.clone();
        let set_next_scale_id = set_next_scale_id.clone();
        let set_escalas_en_edicion = set_escalas_en_edicion.clone();
        let set_editing_instrument_id = set_editing_instrument_id.clone();

        Effect::new(move |_| {
            if !instrumentos.get().is_empty() {
                return;
            }

            spawn_local(async move {
                if !tauri_wasm::is_tauri() {
                    console::log_1(
                        &"No se detectó Tauri (modo navegador), no se cargan instrumentos desde disco"
                            .into(),
                    );
                    return;
                }

                match tauri_wasm::invoke("load_instruments_file").await {
                    Ok(js_val) => {
                        if js_val.is_null() || js_val.is_undefined() {
                            console::log_1(&"No hay archivo de instrumentos todavía".into());
                            return;
                        }

                        if let Some(json_str) = js_val.as_string() {
                            console::log_1(
                                &format!("JSON de instrumentos cargado: {}", json_str)
                                    .into(),
                            );
                            match serde_json::from_str::<InstrumentStore>(&json_str) {
                                Ok(store) => {
                                    console::log_1(
                                        &format!("InstrumentStore cargado: {:?}", store)
                                            .into(),
                                    );
                                    set_instrumentos.set(store.instrumentos);
                                    set_next_instrument_id.set(store.next_instrument_id);
                                    set_next_scale_id.set(store.next_scale_id);
                                    set_escalas_en_edicion.set(Vec::new());
                                    set_editing_instrument_id.set(None);
                                    console::log_1(
                                        &"Instrumentos cargados desde disco".into(),
                                    );
                                }
                                Err(e) => {
                                    console::error_1(
                                        &format!(
                                            "Error parseando instrumentos.json: {e}"
                                        )
                                        .into(),
                                    );
                                }
                            }
                        } else {
                            console::error_1(
                                &"load_instruments_file devolvió algo que no es string"
                                    .into(),
                            );
                        }
                    }
                    Err(e) => {
                        console::error_1(
                            &format!(
                                "Error al invocar load_instruments_file: {e:?}"
                            )
                            .into(),
                        );
                    }
                }
            });
        });
    }

    // -------- GUARDADO AUTOMÁTICO AL CAMBIAR INSTRUMENTOS / IDS --------
    {
        let instrumentos = instrumentos.clone();
        let next_instrument_id = next_instrument_id.clone();
        let next_scale_id = next_scale_id.clone();

        Effect::new(move |_| {
            let store = InstrumentStore {
                instrumentos: instrumentos.get(),
                next_instrument_id: next_instrument_id.get(),
                next_scale_id: next_scale_id.get(),
            };

            spawn_local(async move {
                if !tauri_wasm::is_tauri() {
                    return;
                }

                let json = match serde_json::to_string(&store) {
                    Ok(s) => s,
                    Err(e) => {
                        console::error_1(
                            &format!(
                                "Error serializando instrumentos para guardar: {e}"
                            )
                            .into(),
                        );
                        return;
                    }
                };

                #[derive(Serialize)]
                struct SaveArgs<'a> {
                    json: &'a str,
                }
                let binding = SaveArgs { json: &json };
                let args_js = match args(&binding) {
                    Ok(a) => a,
                    Err(e) => {
                        console::error_1(
                            &format!(
                                "Error preparando args para save_instruments_file: {e}"
                            )
                            .into(),
                        );
                        return;
                    }
                };

                if let Err(e) = tauri_wasm::invoke("save_instruments_file")
                    .with_args(args_js)
                    .await
                {
                    console::error_1(
                        &format!("Error al invocar save_instruments_file: {e:?}")
                            .into(),
                    );
                } else {
                    console::log_1(
                        &"instrumentos.json guardado correctamente desde la UI"
                            .into(),
                    );
                }
            });
        });
    }

    view! {
        <div class="instrumentos-panel">
            { render_header() }
            <div class="instrumentos-layout">
                {
                    render_list_section(
                        instrumentos,
                        set_instrumentos,
                        set_nuevo_nombre,
                        set_nuevo_kind,
                        set_nuevo_notas,
                        set_escalas_en_edicion,
                        set_escala_quantity,
                        set_escala_unit,
                        set_escala_prefix,
                        set_escala_min,
                        set_escala_max,
                        set_escala_res,
                        set_escala_unc_kind,
                        set_escala_lectura,
                        set_escala_calibracion,
                        set_escala_porcentaje,
                        set_escala_digitos,
                        set_editing_instrument_id,
                    )
                }
                {
                    render_form_section(
                        set_instrumentos,
                        nuevo_nombre,
                        set_nuevo_nombre,
                        nuevo_kind,
                        set_nuevo_kind,
                        nuevo_notas,
                        set_nuevo_notas,
                        escala_label,
                        set_escala_label,
                        escala_quantity,
                        set_escala_quantity,
                        escala_unit,
                        set_escala_unit,
                        escala_prefix,
                        set_escala_prefix,
                        escala_min,
                        set_escala_min,
                        escala_max,
                        set_escala_max,
                        escala_res,
                        set_escala_res,
                        escala_unc_kind,
                        set_escala_unc_kind,
                        escala_lectura,
                        set_escala_lectura,
                        escala_calibracion,
                        set_escala_calibracion,
                        escala_porcentaje,
                        set_escala_porcentaje,
                        escala_digitos,
                        set_escala_digitos,
                        escalas_en_edicion,
                        set_escalas_en_edicion,
                        next_instrument_id,
                        set_next_instrument_id,
                        next_scale_id,
                        set_next_scale_id,
                        editing_instrument_id,
                        set_editing_instrument_id,
                    )
                }
            </div>
        </div>
    }
}
