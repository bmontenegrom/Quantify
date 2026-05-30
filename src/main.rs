//! Binario de Quantify: arranca el servidor HTTP usando la biblioteca `quantify`.

use anyhow::Context;
use axum::Router;
use quantify::db::{self, AppState};
use quantify::{instruments, practices, routes};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Punto de entrada del binario: configura logging, prepara la base de datos
/// (migraciones + seeds) y arranca el servidor Axum escuchando en `APP_BIND_ADDR`.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "quantify=debug,tower_http=info,axum::rejection=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let bind_addr = env::var("APP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data/quantify.db".to_string());
    let upload_dir =
        PathBuf::from(env::var("UPLOAD_DIR").unwrap_or_else(|_| "data/uploads".into()));

    tokio::fs::create_dir_all(&upload_dir)
        .await
        .with_context(|| format!("creating upload directory {}", upload_dir.display()))?;

    if let Some(path) = database_url.strip_prefix("sqlite:") {
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating database directory {}", parent.display()))?;
        }
    }

    // `foreign_keys(true)` hace que SQLite respete las claves foráneas y los `ON DELETE CASCADE`
    // declarados en el esquema (por defecto SQLite las ignora).
    let connect_options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(connect_options)
        .await
        .with_context(|| format!("connecting to database {database_url}"))?;

    db::migrate(&pool).await?;
    db::seed_practices(&pool).await?;
    db::seed_users(&pool).await?;
    db::seed_academic(&pool).await?;
    instruments::seed_instruments(&pool, "fisica-experimental-i-2026").await?;
    practices::seed_definitions(&pool).await?;

    let state = Arc::new(AppState { pool, upload_dir });
    let app = Router::new()
        .nest("/api", routes::api_router(state))
        .fallback_service(ServeDir::new("static").append_index_html_on_directories(true))
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = bind_addr.parse().context("invalid APP_BIND_ADDR")?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Quantify listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
