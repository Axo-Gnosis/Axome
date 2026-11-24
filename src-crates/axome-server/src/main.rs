mod pairing;

use anyhow::Context;
use axome_core::{run_supervisor, HostDirs, SUPERVISOR_ROUTE};
use axome_remote::Identity;
use axome_server::{serve, ServeOptions};
use clap::{Parser, Subcommand};
use std::{ffi::OsStr, net::SocketAddr, path::PathBuf, process::ExitCode};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "axome-server")]
struct Cli {
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
    /// Server config; defaults to `<config-dir>/server.jsonc`.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Web frontend used when `web.assetsDir` is unset; packaged launchers set this.
    #[arg(long, global = true)]
    web_assets_dir: Option<PathBuf>,
    /// Bun executable; packaged launchers set this.
    #[arg(long, global = true, value_name = "PATH")]
    bun: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long)]
        listen: Option<SocketAddr>,
    },
    Pair {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        listen: Option<SocketAddr>,
    },
    /// Print the server's fingerprint, or that of an identity file (like `ssh-keygen -lf`).
    Fingerprint { file: Option<PathBuf> },
    /// Write a new identity to FILE and print its fingerprint (like `ssh-keygen -f`).
    /// Works for desktops too: provision it as `~/.config/axome/client.pem`.
    Keygen { file: PathBuf },
}

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    if args.nth(1).as_deref() == Some(OsStr::new(SUPERVISOR_ROUTE)) {
        return run_supervisor(args);
    }

    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build server runtime")
        .and_then(|runtime| runtime.block_on(run()));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:?}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut dirs = HostDirs::from_env().map_err(anyhow::Error::msg)?;
    if let Some(config) = cli.config_dir {
        dirs.config = config;
    }
    if let Some(data) = cli.data_dir {
        dirs.data = data;
    }
    if let Some(cache) = cli.cache_dir {
        dirs.cache = cache;
    }
    let config_file = cli
        .config
        .unwrap_or_else(|| axome_server::config::default_path(&dirs.config));

    match cli.command {
        Command::Serve { listen } => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    EnvFilter::new("axome_server=info,axome_core=info,axome_remote=info")
                }))
                .init();
            let handle = serve(ServeOptions {
                dirs,
                config_file: Some(config_file),
                listen,
                web_assets_dir: cli.web_assets_dir,
                bun: cli.bun,
            })
            .await?;
            tracing::info!("listening on {}", handle.local_addr);
            if let Some(addr) = handle.web_addr {
                tracing::warn!(%addr, "web has no authentication and grants the server OS user's filesystem and process authority");
                if !addr.ip().is_loopback() {
                    tracing::warn!(%addr, "web is exposed on a non-loopback address without authentication");
                }
                tracing::info!("web listening on http://{addr}");
            }
            tracing::info!("fingerprint {}", handle.fingerprint);
            let signal_result = wait_for_shutdown().await;
            handle.shutdown().await;
            signal_result?;
        }
        Command::Pair { host, listen } => pairing::run(&dirs.config, &config_file, host, listen)?,
        Command::Fingerprint { file } => {
            let identity = match file {
                Some(file) => Identity::load(&file)?,
                None => axome_server::config::identity(
                    &dirs.config,
                    &axome_server::config::load(&config_file)?,
                )?,
            };
            println!("{}", identity.fingerprint());
        }
        Command::Keygen { file } => println!("{}", Identity::create(&file)?.fingerprint()),
    }
    Ok(())
}

#[cfg(unix)]
async fn wait_for_shutdown() -> anyhow::Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("install SIGTERM handler")?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.context("wait for Ctrl-C")?,
        _ = terminate.recv() => {},
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> anyhow::Result<()> {
    tokio::signal::ctrl_c().await.context("wait for Ctrl-C")
}
