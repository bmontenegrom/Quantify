use std::{fs, path::PathBuf};
use tauri::Manager;

fn instrumentos_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    // app.path() es la API nueva de Tauri 2 para paths específicos de la app
    let base = app
        .path()
        .app_local_data_dir()
        .or_else(|_| Err("No se pudo obtener app_data_dir de Tauri".to_string()))?;

    // aseguramos que el directorio exista
    if let Err(e) = fs::create_dir_all(&base) {
        return Err(format!("No se pudo crear app_data_dir: {e}"));
    }

    Ok(base.join("instrumentos.json"))
}

#[tauri::command]
async fn load_instruments_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let path = instrumentos_file_path(&app)?;
    if !path.exists() {
        // primera vez: no hay archivo
        return Ok(None);
    }

    match fs::read_to_string(&path) {
        Ok(s) => Ok(Some(s)),
        Err(e) => Err(format!("Error leyendo {path:?}: {e}")),
    }
}

#[tauri::command]
async fn save_instruments_file(app: tauri::AppHandle, json: String) -> Result<(), String> {
    let path = instrumentos_file_path(&app)?;
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(format!("No se pudo crear carpeta para {path:?}: {e}"));
        }
    }

    fs::write(&path, json).map_err(|e| format!("Error escribiendo {path:?}: {e}"))
}

// entrypoint típico de Tauri 2:
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_instruments_file,
            save_instruments_file,
            // ... aquí van otros comandos que ya tengas
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
