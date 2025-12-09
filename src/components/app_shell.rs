use leptos::prelude::*;

use crate::components::practicas_tabs::PracticasTabs;
use crate::components::instrumentos_panel::InstrumentosPanel;
// Más adelante: use crate::components::instrumentos_panel::InstrumentosPanel;

#[derive(Clone, Copy, PartialEq)]
enum SidebarSection {
    Practicas,
    Instrumentos,
    Ayuda,
    Sesion,
}

#[component]
pub fn AppShell() -> impl IntoView {
    let (section, set_section) = signal(SidebarSection::Practicas);

    view! {
        <div class="app-shell">
            // --------- SIDEBAR ----------
            <aside class="sidebar">
                <div class="sidebar-header">
                    <h2>"Quantify"</h2>
                    <p>"Laboratorio de Física"</p>
                </div>

                <nav class="sidebar-nav">
                    <button
                        class=move || {
                            if section.get() == SidebarSection::Practicas {
                                "sidebar-item active".to_string()
                            } else {
                                "sidebar-item".to_string()
                            }
                        }
                        on:click=move |_| set_section.set(SidebarSection::Practicas)
                    >
                        "Prácticas"
                    </button>

                    <button
                        class=move || {
                            if section.get() == SidebarSection::Instrumentos {
                                "sidebar-item active".to_string()
                            } else {
                                "sidebar-item".to_string()
                            }
                        }
                        on:click=move |_| set_section.set(SidebarSection::Instrumentos)
                    >
                        "Instrumentos"
                    </button>

                    <button
                        class=move || {
                            if section.get() == SidebarSection::Ayuda {
                                "sidebar-item active".to_string()
                            } else {
                                "sidebar-item".to_string()
                            }
                        }
                        on:click=move |_| set_section.set(SidebarSection::Ayuda)
                    >
                        "Ayuda"
                    </button>

                    <button
                        class=move || {
                            if section.get() == SidebarSection::Sesion {
                                "sidebar-item active".to_string()
                            } else {
                                "sidebar-item".to_string()
                            }
                        }
                        on:click=move |_| set_section.set(SidebarSection::Sesion)
                    >
                        "Sesión"
                    </button>
                </nav>

                <div class="sidebar-footer">
                    <span class="sidebar-version">"v0.1.0"</span>
                </div>
            </aside>

            // --------- CONTENIDO PRINCIPAL ----------
            <main class="main-content">
                {move || match section.get() {
                    SidebarSection::Practicas => view! {
                        <PracticasTabs />
                    }.into_any(),

                    SidebarSection::Instrumentos => view! {
                        <InstrumentosPanel />
                    }.into_any(),

                    SidebarSection::Ayuda => view! {
                        <AyudaPlaceholder />
                    }.into_any(),

                    SidebarSection::Sesion => view! {
                        <SesionPlaceholder />
                    }.into_any(),
                }}
            </main>
        </div>
    }
}

// ------- Placeholders por ahora -------

#[component]
fn InstrumentosPlaceholder() -> impl IntoView {
    view! {
        <div>
            <h3>"Gestión de instrumentos"</h3>
            <p>
                "Aquí vas a poder agregar, editar y revisar instrumentos, "
                "sus escalas, unidades y modelos de incertidumbre."
            </p>
            <p class="placeholder-note">
                "Por ahora esta sección es un placeholder. "
                "Más adelante, acá conectamos el modelo Instrument/Scale/Measurement."
            </p>
        </div>
    }
}

#[component]
fn AyudaPlaceholder() -> impl IntoView {
    view! {
        <div>
            <h3>"Ayuda y guía rápida"</h3>
            <ul>
                <li>"Cómo usar el cronómetro y registrar marcas."</li>
                <li>"Cómo interpretar s, Sₙ y el test de normalidad."</li>
                <li>"Cómo exportar los datos para el informe."</li>
            </ul>
            <p class="placeholder-note">
                "Más adelante podemos poner aquí texto introductorio para cada práctica, "
                "videos cortos o enlaces a PDFs del laboratorio."
            </p>
        </div>
    }
}

#[component]
fn SesionPlaceholder() -> impl IntoView {
    view! {
        <div>
            <h3>"Sesión / Grupo"</h3>
            <p>
                "En una versión futura, acá se podría configurar:"
            </p>
            <ul>
                <li>"Grupo de laboratorio (número, horario, docente)."</li>
                <li>"Nombre de los integrantes del grupo."</li>
                <li>"Opciones de envío/exportación (por ejemplo, enviar resumen por email)."</li>
            </ul>
            <p class="placeholder-note">
                "Esto podría integrarse con el envío de datos por correo, o guardar "
                "metadatos para incluirlos en los JSON/PDF."
            </p>
        </div>
    }
}
