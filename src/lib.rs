//! Quantify: aplicación de laboratorio de física para Física 103.
//!
//! Esta biblioteca agrupa los módulos que componen la aplicación:
//! - [`analysis`]: análisis de CSV (estadísticos y ajustes).
//! - [`uncertainty`]: motor de incertidumbres (tipo A/B, combinada, expandida, propagación).
//! - [`db`]: persistencia y lógica de dominio (usuarios, cursos, entregas, notas).
//! - [`instruments`]: catálogo de instrumentos por curso (CRUD y export/import).
//! - [`practices`]: definición de prácticas (magnitudes de entrada y mensurandos derivados).
//! - [`computation`]: cálculo de incertidumbres de una entrega por formulario.
//! - [`routes`]: handlers HTTP de la API bajo `/api`.
//! - [`error`]: tipo de error de la aplicación y su mapeo a respuestas HTTP.
//!
//! El binario (`main.rs`) es un shim delgado que arranca el servidor usando estos módulos.

pub mod analysis;
pub mod computation;
pub mod courses;
pub mod db;
pub mod error;
pub mod instruments;
pub mod practices;
pub mod routes;
pub mod sessions;
pub mod submissions;
pub mod uncertainty;
pub mod users;
