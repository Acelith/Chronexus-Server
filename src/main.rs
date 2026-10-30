use chronexus_server::{AppState, config::Config, db::Database, routes};
use std::{io::Write, net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Chronexus Server: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str).unwrap_or("serve") {
        "openapi" if args.len() == 1 => {
            println!(
                "{}",
                serde_json::to_string_pretty(&routes::contract())
                    .map_err(|_| "Cannot serialize contract.")?
            );
        }
        "invite" | "recover"
            if args.len() == 1
                || (args[0] == "invite"
                    && args.get(1).is_some_and(|s| s == "--gm")
                    && args.len() == 2)
                || (args[0] == "recover" && args.len() == 2) =>
        {
            if args[0] == "recover" && args.len() != 2 {
                return Err("Usage: chronexus-server recover USERNAME".into());
            }
            let path = PathBuf::from(
                std::env::var("CHRONEXUS_DATABASE")
                    .unwrap_or_else(|_| "data/chronexus.sqlite3".into()),
            );
            let db = Database::open(&path)?;
            let code = db
                .issue_invite_with_gm(
                    if args[0] == "recover" {
                        args.get(1).map(String::as_str)
                    } else {
                        None
                    },
                    args.get(1).is_some_and(|s| s == "--gm"),
                )
                .map_err(|e| e.1.to_string())?;
            // Only this operator-invoked command reveals the secret; serve never logs it.
            println!("{code}");
        }
        "revoke-sessions" if args.len() == 1 => {
            // After restoring an older backup: every session ends, everyone signs in again.
            let path = PathBuf::from(
                std::env::var("CHRONEXUS_DATABASE")
                    .unwrap_or_else(|_| "data/chronexus.sqlite3".into()),
            );
            let db = Database::open(&path)?;
            let ended = db.revoke_all_sessions().map_err(|e| e.1.to_string())?;
            println!("{ended} sessions revoked.");
        }
        "compact" if args.len() <= 2 => {
            // Drops tombstones older than the given number of days (default 180).
            let days = args
                .get(1)
                .map(|d| {
                    d.parse::<i64>()
                        .map_err(|_| "Usage: chronexus-server compact [DAYS]")
                })
                .transpose()?
                .unwrap_or(180);
            let path = PathBuf::from(
                std::env::var("CHRONEXUS_DATABASE")
                    .unwrap_or_else(|_| "data/chronexus.sqlite3".into()),
            );
            let db = Database::open(&path)?;
            let removed = db.purge_tombstones(days).map_err(|e| e.1.to_string())?;
            println!("{removed} tombstones removed.");
        }
        "backup" if args.len() == 2 => {
            let database = PathBuf::from(
                std::env::var("CHRONEXUS_DATABASE")
                    .unwrap_or_else(|_| "data/chronexus.sqlite3".into()),
            );
            let report = chronexus_server::backup::create(&database, &PathBuf::from(&args[1]))?;
            println!("Backup written and verified: {report}");
        }
        "verify" if args.len() == 2 => {
            let report = chronexus_server::backup::verify(&PathBuf::from(&args[1]))?;
            println!("Backup is healthy: {report}");
        }
        "restore" if args.len() == 2 => {
            // The server must be stopped: the restored file replaces the live database.
            let database = PathBuf::from(
                std::env::var("CHRONEXUS_DATABASE")
                    .unwrap_or_else(|_| "data/chronexus.sqlite3".into()),
            );
            let (report, ended, kept) =
                chronexus_server::backup::restore(&database, &PathBuf::from(&args[1]))?;
            println!("Restored: {report}");
            println!("{ended} sessions revoked; everyone must sign in again.");
            if let Some(kept) = kept {
                println!("The previous database was kept as {}", kept.display());
            }
        }
        "healthcheck" if args.len() == 1 => {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let listen: SocketAddr = std::env::var("CHRONEXUS_LISTEN")
                .unwrap_or_else(|_| "127.0.0.1:8080".into())
                .parse()
                .map_err(|_| "Invalid listener.")?;
            let host = if listen.is_ipv4() { "127.0.0.1" } else { "::1" };
            tokio::time::timeout(std::time::Duration::from_secs(3),async {
                let mut socket=tokio::net::TcpStream::connect((host,listen.port())).await.map_err(|_|"Health connection failed.")?;
                socket.write_all(b"GET /health/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.map_err(|_|"Health request failed.")?;
                let mut buffer=[0;13]; socket.read_exact(&mut buffer).await.map_err(|_|"Health reply failed.")?;
                if &buffer != b"HTTP/1.1 200 "{return Err("Server not ready.");} Ok(())
            }).await.map_err(|_|"Health timeout.")?.map_err(String::from)?;
        }
        "serve" if args.len() <= 1 => {
            let config = Config::from_env()?;
            let listen = config.listen;
            let state = AppState::new(config)?;
            let listener = tokio::net::TcpListener::bind(listen)
                .await
                .map_err(|_| "Cannot bind listener; check address and port availability.")?;
            writeln!(
                std::io::stderr(),
                "Chronexus Server {} listening on {listen}; selective sharing and managed attachments; continuous replication pending.",
                env!("CARGO_PKG_VERSION")
            )
            .map_err(|_| "Cannot write startup status.")?;
            axum::serve(
                listener,
                routes::router(state).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown())
            .await
            .map_err(|_| "HTTP service stopped unexpectedly.")?;
        }
        _ => {
            return Err(
                "Usage: chronexus-server [serve|invite [--gm]|recover USERNAME|revoke-sessions|compact [DAYS]|backup FILE|verify FILE|restore FILE|openapi|healthcheck]"
                    .into(),
            );
        }
    }
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=terminate.recv()=>{} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
