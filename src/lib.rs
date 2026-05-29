//! Quantify: aplicación de laboratorio de física para Física 103.
//!
//! Esta biblioteca agrupa los módulos que componen la aplicación:
//! - [`analysis`]: análisis de CSV (estadísticos y ajustes).
//! - [`uncertainty`]: motor de incertidumbres (tipo A/B, combinada, expandida, propagación).
//! - [`db`]: persistencia y lógica de dominio (usuarios, cursos, entregas, notas).
//! - [`routes`]: handlers HTTP de la API bajo `/api`.
//! - [`error`]: tipo de error de la aplicación y su mapeo a respuestas HTTP.
//!
//! El binario (`main.rs`) es un shim delgado que arranca el servidor usando estos módulos.

pub mod analysis;
pub mod db;
pub mod error;
pub mod routes;
pub mod uncertainty;
