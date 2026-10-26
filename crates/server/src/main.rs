use nested_chess_server::{
    model::{Error, Result},
    web::{App, router},
};
use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let import = match args.as_slice() {
        [] => None,
        [flag, path] if flag == "--import-sqlite" => Some(path.clone()),
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "Nested chess online server\n\nUsage: nested-chess-server [--import-sqlite EXPORT.json]\n\nEnvironment:\n  LISTEN_ADDR       127.0.0.1:4180\n  PUBLIC_URL        http://localhost:4180\n  DATABASE_URL      postgres://...\n  STATIC_DIR        dist\n  GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET\n  GITHUB_CLIENT_ID / GITHUB_CLIENT_SECRET\n  MICROSOFT_CLIENT_ID / MICROSOFT_CLIENT_SECRET\n\nSee docs/server.md for provider setup and API details."
            );
            return Ok(());
        }
        _ => return Err(Error::Invalid("Unknown arguments. Use --help.".into())),
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nested_chess_server=info".into()),
        )
        .init();
    let database =
        env::var("DATABASE_URL").map_err(|_| Error::Invalid("DATABASE_URL is required.".into()))?;
    if let Some(path) = import {
        let data =
            std::fs::read_to_string(path).map_err(|error| Error::Invalid(error.to_string()))?;
        nested_chess_server::store::Store::connect(&database)
            .await?
            .import_sqlite(&data)
            .await?;
        println!(
            "Imported SQLite data. Timed active matches are paused until both players resume."
        );
        return Ok(());
    }
    let public_url = env::var("PUBLIC_URL").unwrap_or_else(|_| "http://localhost:4180".into());
    let address = env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:4180".into());
    let static_dir = PathBuf::from(env::var("STATIC_DIR").unwrap_or_else(|_| "dist".into()));
    let mut credentials = Vec::new();
    for (name, prefix) in [
        ("google", "GOOGLE"),
        ("github", "GITHUB"),
        ("microsoft", "MICROSOFT"),
    ] {
        match (
            env::var(format!("{prefix}_CLIENT_ID")),
            env::var(format!("{prefix}_CLIENT_SECRET")),
        ) {
            (Ok(id), Ok(secret)) if id.is_empty() && secret.is_empty() => {}
            (Ok(id), Ok(secret)) => credentials.push((name.into(), id, secret)),
            (Err(env::VarError::NotPresent), Err(env::VarError::NotPresent)) => {}
            _ => {
                return Err(Error::Invalid(format!(
                    "Set both {prefix}_CLIENT_ID and {prefix}_CLIENT_SECRET."
                )));
            }
        }
    }
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .map_err(|error| Error::Internal(format!("Could not listen on {address}: {error}")))?;
    let app = App::new(&database, &public_url, credentials).await?;
    app.start_clock();
    tracing::info!(%address,"server listening");
    let signal_app = app.clone();
    tokio::spawn(async move {
        if shutdown_signal().await.is_ok() {
            let _ = signal_app.stop().await;
        }
    });
    let mut stopped = app.stopped();
    axum::serve(listener, router(app, static_dir))
        .with_graceful_shutdown(async move {
            let _ = stopped.changed().await;
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))
}

async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result,_=terminate.recv()=>Ok(())}
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
