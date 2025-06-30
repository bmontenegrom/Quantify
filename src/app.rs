use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use std::time::{Duration};

use wasm_timer::Instant; 

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> {
    name: &'a str,
}

#[component]
pub fn App() -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (greet_msg, set_greet_msg) = signal(String::new());

    let update_name = move |ev| {
        let v = event_target_value(&ev);
        set_name.set(v);
    };

    let greet = move |ev: SubmitEvent| {
        ev.prevent_default();
        spawn_local(async move {
            let name = name.get_untracked();
            if name.is_empty() {
                return;
            }

            let args = serde_wasm_bindgen::to_value(&GreetArgs { name: &name }).unwrap();
            // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
            let new_msg = invoke("greet", args).await.as_string().unwrap();
            set_greet_msg.set(new_msg);
        });
    };

    view! {
        <main class="container">
            <h1>"Welcome to Tauri + Leptos"</h1>

            <div class="row">
                <a href="https://tauri.app" target="_blank">
                    <img src="public/tauri.svg" class="logo tauri" alt="Tauri logo"/>
                </a>
                <a href="https://docs.rs/leptos/" target="_blank">
                    <img src="public/leptos.svg" class="logo leptos" alt="Leptos logo"/>
                </a>
            </div>
            <p>"Click on the Tauri and Leptos logos to learn more."</p>

            <form class="row" on:submit=greet>
                <input
                    id="greet-input"
                    placeholder="Enter a name..."
                    on:input=update_name
                />
                <button type="submit">"Greet"</button>
            </form>
            <p>{ move || greet_msg.get() }</p>
        </main>
    }
}


#[component]
pub fn Stopwatch() -> impl IntoView {
    /* ─────────── Señales ─────────── */
    let running     = RwSignal::new(false);
    let elapsed     = RwSignal::new(Duration::ZERO);
    let last_tick   = RwSignal::new(None::<Instant>);
    let interval_hd = RwSignal::new(None::<IntervalHandle>); // handle cancelable

    /* ────────── Botón Start/Stop ────────── */
    let toggle = move |_| {
        if running.get() {
            // ---------- STOP ----------
            running.set(false);
            if let Some(h) = interval_hd.get() {
                h.clear();          // cancela el setInterval
                interval_hd.set(None);
            }
        } else {
            // ---------- START ----------
            elapsed.set(Duration::ZERO);
            last_tick.set(Some(Instant::now()));
            running.set(true);

            // Clones para el cierre
            let el = elapsed.clone();
            let lt = last_tick.clone();

            // 10 ms entre ticks
            let hd = set_interval_with_handle(
                move || {
                    if let Some(prev) = lt.get() {
                        let now   = Instant::now();
                        let delta = now.duration_since(prev);
                        el.update(|d| *d += delta);
                        lt.set(Some(now));
                    }
                },
                Duration::from_millis(10),
            )
            .expect("interval supported");

            interval_hd.set(Some(hd));
        }
    };

    /* ─────────── Botón Reset ─────────── */
    let reset = move |_| {
        running.set(false);
        if let Some(h) = interval_hd.get() {
            h.clear();
            interval_hd.set(None);
        }
        elapsed.set(Duration::ZERO);
        last_tick.set(None);
    };

    /* ─────────── Vista ─────────── */
    view! {
        <div class="stopwatch flex flex-col items-center gap-3 p-4 rounded bg-slate-800 text-white">
            <h3 class="text-lg font-semibold text-emerald-400">"Stopwatch"</h3>

            <p class="text-3xl font-mono">{ move || {
                let d = elapsed.get();
                format!("{:02}:{:02}.{:03}",
                    d.as_secs() / 60,
                    d.as_secs() % 60,
                    d.subsec_millis())
            }}</p>

            <div class="flex gap-4">
                <button class="px-4 py-2 rounded bg-emerald-600 hover:bg-emerald-700"
                        on:click=toggle>
                    { move || if running.get() { "Stop" } else { "Start" } }
                </button>

                <button class="px-4 py-2 rounded bg-red-600 hover:bg-red-700"
                        on:click=reset>
                    "Reset"
                </button>
            </div>
        </div>
    }
}