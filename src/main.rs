
mod components;
use crate::components::app_shell::AppShell;
//use components::practicas_tabs::PracticasTabs;
mod models;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(AppShell);

}
