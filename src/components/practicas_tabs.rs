use leptos::prelude::*;

use crate::components::practica_pendulo::PracticaPendulo; // ajusta el path si hace falta

#[derive(Clone, PartialEq)]
struct PracticaInfo {
    id: u32,
    nombre: &'static str,
    descripcion: &'static str,
}

/// Lista inicial de prácticas (10 por ahora).
fn practicas_iniciales() -> Vec<PracticaInfo> {
    vec![
        PracticaInfo {
            id: 1,
            nombre: "Péndulo simple",
            descripcion: "Medición del período de un péndulo y análisis estadístico.",
        },
        PracticaInfo {
            id: 2,
            nombre: "MRU/MRUV",
            descripcion: "Estudio de movimiento rectilíneo uniforme y uniformemente variado.",
        },
        PracticaInfo {
            id: 3,
            nombre: "Tiro oblicuo",
            descripcion: "Medición de alcance y altura máxima de proyectiles.",
        },
        PracticaInfo {
            id: 4,
            nombre: "Fuerzas y rozamiento",
            descripcion: "Estudio de fuerza neta, coeficiente de rozamiento y dinámica.",
        },
        PracticaInfo {
            id: 5,
            nombre: "Oscilaciones amortiguadas",
            descripcion: "Osciladores con amortiguamiento y ajuste de parámetros.",
        },
        PracticaInfo {
            id: 6,
            nombre: "Ondas en cuerda",
            descripcion: "Relación entre frecuencia, longitud de onda y velocidad de propagación.",
        },
        PracticaInfo {
            id: 7,
            nombre: "Ley de Hooke",
            descripcion: "Estudio de resortes, constante elástica y energía potencial.",
        },
        PracticaInfo {
            id: 8,
            nombre: "Circuitos DC",
            descripcion: "Medición de tensiones, corrientes y resistencias en circuitos sencillos.",
        },
        PracticaInfo {
            id: 9,
            nombre: "Circuitos AC",
            descripcion: "Estudio de respuesta a señales alternas en circuitos RC/RL/RLC.",
        },
        PracticaInfo {
            id: 10,
            nombre: "Termometría",
            descripcion: "Medición de temperatura y calibración básica de sensores.",
        },
    ]
}

#[component]
pub fn PracticasTabs() -> impl IntoView {
    // Lista de prácticas (fija por ahora)
    let practicas = Memo::new(|_| practicas_iniciales());

    // Práctica seleccionada (por defecto la 1: péndulo)
    let (selected_id, set_selected_id) = signal(1u32);

    // Objeto práctica seleccionada
    let selected_practica = Memo::new({
        let practicas = practicas.clone();
        move |_| {
            let current = selected_id.get();
            practicas
                .get()
                .into_iter()
                .find(|p| p.id == current)
        }
    });

    view! {
        <div class="practicas-layout">
            // --------- Barra de pestañas ----------
            <div class="practicas-tabs">
                <For
                    each=move || practicas.get()
                    key=|p: &PracticaInfo| p.id
                    children=move |p: PracticaInfo| {
                        let id = p.id;
                        let set_selected_id = set_selected_id.clone();
                        let selected_id = selected_id.clone();
                        view! {
                            <button
                                class=move || {
                                    if selected_id.get() == id {
                                        "tab-button active".to_string()
                                    } else {
                                        "tab-button".to_string()
                                    }
                                }
                                on:click=move |_| set_selected_id.set(id)
                            >
                                {p.nombre}
                            </button>
                        }
                    }
                />
            </div>

            // --------- Contenido de la práctica seleccionada ----------
            <div class="tab-content">
                {move || {
                    if let Some(p) = selected_practica.get() {
                        match p.id {
                            // aquí enchufamos la interfaz real de la práctica del péndulo
                            1 => view! {
                                <PracticaPendulo />
                            }.into_any(),

                            // resto: placeholders por ahora
                            _ => view! {
                                <div class="practica-placeholder">
                                    <h3>{p.nombre}</h3>
                                    <p class="practica-description">
                                        {p.descripcion}
                                    </p>
                                    <p class="placeholder-note">
                                        "Interfaz aún no implementada en la aplicación. "
                                        "Esta práctica se cargará en futuras versiones."
                                    </p>
                                </div>
                            }.into_any(),
                        }
                    } else {
                        view! {
                            <p>"Práctica no encontrada."</p>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
