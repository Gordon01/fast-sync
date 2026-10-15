mod stats;
mod sync;

use std::net::{IpAddr, SocketAddr};

use clap::{Parser, Subcommand};
use fast_sync::wft::Wft;
use tracing::debug;
use tracing_subscriber::EnvFilter;

/// Syncs files from a Wi‑Fi File Transfer server to the local filesystem.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// Server IP address.
    #[arg(long, short)]
    ip: IpAddr,

    /// Server port.
    #[arg(long, short, default_value_t = 1234, global = true)]
    port: u16,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Download files from a server directory.
    Pull(sync::PullArgs),
    /// Show server-reported storage and media sizes.
    StorageInfo,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    debug!("CL arguments: {cli:#?}");
    let mut wft = Wft::new(SocketAddr::new(cli.ip, cli.port)).await?;
    match cli.command {
        Command::Pull(args) => {
            if let Some(storage) = args.storage {
                wft.select_storage(storage)?;
            }
            wft.set_output_path(args.output.clone())?;
            sync::pull(&wft, args.directory).await?;
            Ok(())
        }
        Command::StorageInfo => {
            println!("{}", wft.storages());
            Ok(())
        }
    }
}
