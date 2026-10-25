use nested_chess_server::{
    model::{Error, Result},
    web::{App, router},
};
use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "Nested chess online server\n\nUsage: nested-chess-server\n\nEnvironment:\n  LISTEN_ADDR       127.0.0.1:4180\n  PUBLIC_URL        http://localhost:4180\n  DATABASE_PATH     nested-chess.sqlite\n  STATIC_DIR        dist\n  GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET\n  GITHUB_CLIENT_ID / GITHUB_CLIENT_SECRET\n  MICROSOFT_CLIENT_ID / MICROSOFT_CLIENT_SECRET\n\nSee docs/server.md for provider setup and API details."
            );
            return Ok(());
        }
        _ => return Err(Error::Invalid("Unknown arguments. Use --help.".into())),
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nested_chess_server=info".into()),
        )
        .init();
    let database =
        PathBuf::from(env::var("DATABASE_PATH").unwrap_or_else(|_| "nested-chess.sqlite".into()));
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
    let app = App::new(&database, &public_url, credentials)?;
    app.start_clock();
    tracing::info!(%address,"server listening");
    let signal_app = app.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
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
