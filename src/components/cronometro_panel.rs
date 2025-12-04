use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::window;

/// Formatea ms a HH:MM:SS.mmm
fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let millis = ms % 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

#[component]
pub fn CronometroPanel() -> impl IntoView {
    // Tiempo acumulado del cronómetro
    let (elapsed_ms, set_elapsed_ms) = signal(0u64);

    // Estado: corriendo o no
    let (running, set_running) = signal(false);

    // -------- INTERVALO GLOBAL ----------
    // Se crea una única vez y se ejecuta cada 10 ms
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

        // No dropear la closure
        cb.forget();
    }
    // -------------------------------------

    // Handlers
    let on_start = move |_| set_running.set(true);
    let on_pause = move |_| set_running.set(false);
    let on_reset = move |_| {
        set_running.set(false);
        set_elapsed_ms.set(0);
    };

    view! {
        <div class="cronometro-panel">
            <div class="cronometro-display">
                {move || format_time(elapsed_ms.get())}
            </div>

            <div class="cronometro-buttons">
                // PAUSAR
                <button
                    on:click=on_pause
                    style=move || if running.get() { "" } else { "display:none" }
                >
                    "Pausar"
                </button>

                // INICIAR / REANUDAR
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

                // REINICIAR
                <button on:click=on_reset>"Reiniciar"</button>
            </div>
        </div>
    }
}
