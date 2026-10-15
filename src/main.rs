mod pull;
mod stats;

use std::net::{IpAddr, SocketAddr};

use clap::{Parser, Subcommand};
use fast_sync::wft::{Storage, Wft};
use readable::byte::Byte;
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
    Pull(pull::PullArgs),
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
    let wft = Wft::new(SocketAddr::new(cli.ip, cli.port)).await?;
    match cli.command {
        Command::Pull(args) => pull::run(&wft, &args).await,
        Command::StorageInfo => {
            let storages = wft.get_storage_details().await?;
            print_storage_info(&storages);
            Ok(())
        }
    }
}

fn print_storage_info(storages: &[Storage]) {
    if storages.is_empty() {
        println!("No storage information returned.");
        return;
    }

    let mut roots: Vec<(&str, Vec<&Storage>)> = Vec::new();
    for storage in storages {
        if let Some((_, entries)) = roots
            .iter_mut()
            .find(|(root_path, _)| *root_path == storage.root_path)
        {
            entries.push(storage);
        } else {
            roots.push((&storage.root_path, vec![storage]));
        }
    }

    for (root_path, entries) in roots {
        let total_bytes = entries[0].total_bytes;
        let available_bytes = entries[0].available_bytes;
        println!(
            "{root_path}: {} used of {} ({} free)",
            Byte::from(total_bytes.saturating_sub(available_bytes)),
            Byte::from(total_bytes),
            Byte::from(available_bytes)
        );
        for storage in entries {
            println!("  {storage}");
        }
    }
}
