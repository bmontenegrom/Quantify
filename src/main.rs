use leptos::*;
use leptos::prelude::*;
use leptos::mount:: mount_to_body;
mod components;

//use components::practicas_tabs::PracticasTabs;
mod models;

fn main() {
    // En CSR esto se ejecuta en el navegador
    mount_to_body(|| view! { <App/> });
}



#[component]
fn App() -> impl IntoView {
    view! {
        <main class="app-root">
            <header class="app-header">
                <h1>"Laboratorio de Física"</h1>
            </header>

            <section class="app-main">
                <components::practicas_tabs::PracticasTabs />
            </section>
        </main>
    }
}