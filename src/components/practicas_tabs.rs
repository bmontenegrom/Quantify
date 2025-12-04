use leptos::prelude::*;
use crate::models::practica::{Practica, practicas_iniciales};
use crate::components::practica_pendulo::PracticaPendulo;

#[component]
pub fn PracticasTabs() -> impl IntoView {
    let practicas = Memo::new(move |_| practicas_iniciales());
    let (selected_id, set_selected_id) = signal(1u32);

    let selected_practica = move || {
        practicas
            .get()
            .into_iter()
            .find(|p| p.id == selected_id.get())
    };

    view! {
        <div class="practicas-root">
            <div class="practicas-tabs">
                <For
                    each=move || practicas.get()
                    key=|p| p.id
                    children=move |p: Practica| {
                        let active = move || p.id == selected_id.get();
                        view! {
                            <button
                                class=move || if active() { "tab tab-activa" } else { "tab" }
                                on:click=move |_| set_selected_id.set(p.id)
                            >
                                {p.nombre}
                            </button>
                        }
                    }
                />
            </div>

            <div class="practica-content">
                {move || -> AnyView {
                    if let Some(p) = selected_practica() {
                        // Si es la práctica del péndulo (por ejemplo id == 1):
                        if p.id == 1 {
                            view! {
                                <div class="practica-detalle">
                                    <h2>{p.nombre}</h2>
                                    <p>{p.descripcion}</p>
                                    <PracticaPendulo />
                                </div>
                            }.into_any()
                        } else {
                            // Otras prácticas, por ahora placeholder
                            view! {
                                <div class="practica-detalle">
                                    <h2>{p.nombre}</h2>
                                    <p>{p.descripcion}</p>
                                    <p>"(Contenido de esta práctica aún no implementado)"</p>
                                </div>
                            }.into_any()
                        }
                    } else {
                        view! {
                            <div class="practica-detalle">
                                <p>"Práctica no encontrada."</p>
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
